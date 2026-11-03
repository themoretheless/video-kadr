//! Enable/disable verified automatic cutout on one saved video clip.
pub const MAX_BODY_BYTES:usize=crate::db::MAX_VIDEO_CUTOUT_SETTINGS_BYTES;
use anyhow::{ensure,Result};
use axum::{extract::{Path,State},http::HeaderMap,Json};
use serde::Deserialize;
use crate::{state::AppState,error::{ApiJson,AppError,AppResult},domain::composition::{Composition,CompositionTrack,CompositionClipId}};
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Request{region_tracks:Option<Vec<crate::domain::concat_video_binding::RegionTrack>>,correction:Option<crate::domain::cutout::Correction>,feather:Option<f64>,expected_model_hash:Option<String>,subject:Option<crate::domain::concat_cutout_import::Subject>,composition_id:Option<String>,clip_id:String,expected_revision:u64,enabled:bool}
pub async fn handler(State(state):State<AppState>,Path(project):Path<String>,headers:HeaderMap,ApiJson(request):ApiJson<Request>)->AppResult<Json<crate::db::CompositionProject>>{
 if request.feather.is_some_and(|value|!value.is_finite()||!(0.0..=0.1).contains(&value)){return Err(AppError::bad_request("Неверная растушёвка видео"))}
 if let Some(correction)=&request.correction{correction.validate().map_err(|e|AppError::bad_request(e.to_string()))?;if request.feather.is_some_and(|f|f!=correction.feather){return Err(AppError::bad_request("Conflicting video correction feather"))}}
 let actor=super::publish::authenticated_actor(&state,&headers).await?;
 super::cutout_assets::authorize(&state,&project,&actor).await?;
 let token=tokio_util::sync::CancellationToken::new();let _cancel=token.clone().drop_guard();
 let work=tokio::spawn(async move{let result:Result<crate::db::CompositionProject>=async{
  let _permit=if request.enabled{Some(super::concat_cutout_analyse::acquire_analysis(&state,&token).await?)}else{None};
  let saved=state.db.get_composition_project_for(&project,&actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;ensure!(saved.revision==request.expected_revision,"stale project revision");
  let mut document=saved.document.clone();
  let content=if document.get("compositions").is_some(){let id=request.composition_id.as_deref().ok_or_else(||anyhow::anyhow!("timeline required"))?;document.get_mut("compositions").and_then(|nodes|nodes.get_mut(id)).and_then(|node|node.get_mut("content")).ok_or_else(||anyhow::anyhow!("timeline unavailable"))?}else{ensure!(request.composition_id.is_none(),"unexpected timeline");&mut document};
  let mut composition:Composition=serde_json::from_value(content.clone())?;composition.validate_native_surface()?;
  let id=CompositionClipId::parse(request.clip_id)?;
  ensure!(!composition.metadata.as_ref().is_some_and(|m|m.media_versions.contains_key(&id)),"clip contains another processed version");
  let clip=composition.tracks.iter_mut().find_map(|track|match track{CompositionTrack::Video{locked:false,clips,..}=>clips.iter_mut().find(|clip|clip.id==id&&clip.enabled),_=>None}).ok_or_else(||anyhow::anyhow!("unlocked video required"))?;
  let desired=if request.enabled{
   let choice=super::concat_video_cache::choice(&state,&actor,&project,clip.source_id.as_str(),request.expected_revision,&token).await?.ok_or_else(||anyhow::anyhow!("analyse video before enabling cutout"))?;
   ensure!(request.expected_model_hash.as_ref().is_none_or(|expected|expected==&choice.model_hash),"video analysis model changed; analyse again before applying");
   let cache=super::concat_video_cache::restore_for_model(&state,&actor,&project,clip.source_id.as_str(),request.expected_revision,&choice.runtime_hash,&choice.model_hash,&token).await?.ok_or_else(||anyhow::anyhow!("selected video mask cache missing"))?;
   let source=super::concat_cutout_access::source_for_document(&state,&actor,&saved.document,clip.source_id.as_str()).await?;let source_hash=crate::project_archive::fingerprint_file_with_limit(&source,64*1024*1024*1024).await?.1;ensure!(source_hash==cache.video_hash()&&source_hash==choice.video_hash,"video changed after cache verification");
   let subject=match request.subject{Some(crate::domain::concat_cutout_import::Subject::Auto)=>"auto",Some(crate::domain::concat_cutout_import::Subject::Person)=>"person",Some(crate::domain::concat_cutout_import::Subject::Object)=>"object",None=>if choice.model==crate::services::concat_cutout_selection::Model::Object{"object"}else{"person"}};
   let mut correction=request.correction.clone().or_else(||clip.concat_cutout.as_ref().and_then(|b|b.correction.clone()));
   let feather=request.feather.or_else(||correction.as_ref().map(|c|c.feather)).or_else(||clip.concat_cutout.as_ref().and_then(|b|b.feather));
   if let (Some(c),Some(f))=(&mut correction,feather){c.feather=f;}
   Some(crate::domain::concat_video_binding::Binding{origin:None,source_hash,model_hash:choice.model_hash,runtime_hash:choice.runtime_hash,subject:subject.into(),feather,correction,region_tracks:request.region_tracks.clone().unwrap_or_else(||clip.concat_cutout.as_ref().map(|b|b.region_tracks.clone()).unwrap_or_default())})
  }else{None};
  ensure!(!token.is_cancelled(),"video cutout activation cancelled");
  if clip.concat_cutout==desired{return Ok(saved)}
  clip.concat_cutout=desired;composition.validate_native_surface()?;*content=serde_json::to_value(composition)?;
  let sources=crate::db::extract_document_source_ids(&document)?;
  state.db.update_composition_project_for(&project,&actor,&saved.name,&document,&sources,request.expected_revision).await?.ok_or_else(||anyhow::anyhow!("project changed during video cutout activation"))
 }.await;result});
 let result=work.await.map_err(|e|AppError::internal("join video cutout activation",e))?;
 result.map(Json).map_err(|e|AppError::conflict(e.to_string()))
}
