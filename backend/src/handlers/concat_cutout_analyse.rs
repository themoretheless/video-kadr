//! Authenticated source-free Concat cutout command for a saved image clip.
use anyhow::{ensure,Result};
use axum::{extract::{Path,State},http::HeaderMap,Json};
use serde::Deserialize;
use crate::{state::AppState,error::{ApiJson,AppError,AppResult},domain::{composition::{Composition,CompositionClipId,CompositionTrack},concat_cutout_import::plan},services::{concat_cutout_still::{self,Engine},concat_cutout_selection::Model},config::resource_classes::ResourceClass};
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Request{composition_id:Option<String>,clip_id:String,expected_revision:u64,cutout:serde_json::Value}
pub async fn handler(State(state):State<AppState>,Path(project):Path<String>,headers:HeaderMap,ApiJson(request):ApiJson<Request>)->AppResult<Json<crate::db::CompositionProject>>{
 let actor=super::publish::authenticated_actor(&state,&headers).await?;
 super::cutout_assets::authorize(&state,&project,&actor).await?;
 let settings=plan(&request.cutout).map_err(|e|AppError::bad_request(e.to_string()))?;
 let clip=CompositionClipId::parse(request.clip_id.clone()).map_err(|e|AppError::bad_request(e.to_string()))?;
 let saved=state.db.get_composition_project_for(&project,&actor).await.map_err(|e|AppError::internal("read cutout target",e))?.ok_or_else(||AppError::not_found("Проект не найден"))?;
 if saved.revision!=request.expected_revision{return Err(AppError::conflict("Проект изменился"))}
 let content=if let Some(nodes)=saved.document.get("compositions"){&nodes[request.composition_id.as_deref().ok_or_else(||AppError::bad_request("Нужен таймлайн"))?]["content"]}else{if request.composition_id.is_some(){return Err(AppError::bad_request("Лишний таймлайн"))}&saved.document};
 let composition:Composition=serde_json::from_value(content.clone()).map_err(|_|AppError::bad_request("Неверная композиция"))?;
 composition.validate_native_surface().map_err(|e|AppError::bad_request(e.to_string()))?;
 let source_id=composition.tracks.iter().find_map(|track|match track{CompositionTrack::Image{locked:false,clips,..}=>clips.iter().find(|c|c.id==clip).map(|c|c.source_id.clone()),_=>None}).ok_or_else(||AppError::bad_request("Нужно незаблокированное изображение"))?;
 let source_id=if let Some(pair)=composition.metadata.as_ref().and_then(|m|m.media_versions.get(&clip)){
  if pair.cutout.is_none(){return Err(AppError::conflict("Клип содержит другую обработанную версию"))}
  pair.original_source_id.clone()
 }else{source_id};
 let original=composition.sources[&source_id].clone();
 let previous=composition.metadata.as_ref().and_then(|m|m.media_versions.get(&clip)).and_then(|pair|pair.cutout.as_ref());
 let region_correction=crate::domain::concat_cutout_binding::match_saved_region_correction(previous,&settings).map_err(|e|AppError::conflict(e.to_string()))?;
 let mut regions=crate::services::concat_cutout_still::SavedRegions{correction:Some(region_correction.clone()),..Default::default()};
 for attached in region_correction.strokes.iter().filter_map(|s|s.region.as_ref()){
  let id=crate::domain::composition::SourceId::parse(attached.mask_source_id.clone()).map_err(|e|AppError::bad_request(e.to_string()))?;
  let descriptor=composition.sources.get(&id).ok_or_else(||AppError::conflict("Источник области не найден"))?.clone();
  let path=super::concat_cutout_access::source_for_document(&state,&actor,&saved.document,id.as_str()).await.map_err(|_|AppError::forbidden("Область недоступна"))?;
  if !regions.paths.contains_key(id.as_str()){regions.sources.push(descriptor);regions.paths.insert(id.as_str().into(),path);}
 }

 let source=super::concat_cutout_access::source_for_document(&state,&actor,&saved.document,source_id.as_str()).await.map_err(|_|AppError::forbidden("Исходник недоступен"))?;
 let token=tokio_util::sync::CancellationToken::new();let _cancel=token.clone().drop_guard();
 // The spawned task owns the permit and is not abandoned when HTTP is dropped.
 let work=tokio::spawn(async move{let mut generated=Vec::new();let result:Result<crate::db::CompositionProject>=async{
  let _permit=acquire_analysis(&state,&token).await?;
  ensure!(!token.is_cancelled(),"cutout request cancelled");
  let hash=crate::project_archive::fingerprint_file_with_limit(&source,256*1024*1024).await?.1;
  if let Some(correction)=regions.correction.as_mut(){
   for stroke in &mut correction.strokes{
    if correction.custom&&matches!(stroke.tool,crate::domain::cutout::Tool::SmartBrush|crate::domain::cutout::Tool::SmartEraser)&&stroke.at.is_some()&&stroke.region.is_none(){
     let(attachment,descriptor,path)=super::concat_smart_region::generate(&state,&actor,&project,request.expected_revision,&original,&source,&hash,&stroke.points,&token).await?;
     generated.push(descriptor.id.as_str().to_owned());regions.paths.insert(descriptor.id.as_str().into(),path);regions.sources.push(descriptor);stroke.region=Some(attachment);
    }
   }
  }
  let _models=tokio::select!{biased;_=token.cancelled()=>anyhow::bail!("cutout request cancelled"),guard=state.cutout_model_ops.lock()=>guard};
  let root=state.storage.join("concat-analysis");tokio::fs::create_dir_all(&root).await?;
  let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
  let analysed=concat_cutout_still::analyse(&state.process_runtime,settings.subject,None,&source,&hash,&root,|model|{let state=state.clone();let actor=actor.clone();async move{match model{
   Model::Person=>{let config=state.person.as_ref().as_ref().ok_or_else(||anyhow::anyhow!("Person model unavailable"))?.clone();crate::services::local_person::identity(&config).await?;Ok(Engine::Person(config))},
   Model::Selfie=>{let config=state.selfie.as_ref().as_ref().ok_or_else(||anyhow::anyhow!("Selfie model unavailable"))?.clone();crate::services::local_selfie::identity(&config).await?;Ok(Engine::Selfie(config))},
   Model::Object=>{let(config,_)=super::cutout_models::config_for_owner(&state,&actor).await.map_err(|_|anyhow::anyhow!("Object model unavailable"))?;crate::services::local_cutout::identity(&config).await?;Ok(Engine::Object(config))},
  }}},&token,std::time::Duration::from_secs(1800),&tx).await?;
  let mut copy=original.clone();copy.id=crate::domain::composition::SourceId::parse(uuid::Uuid::new_v4().to_string())?;
  let mut mask=copy.clone();mask.id=crate::domain::composition::SourceId::parse(uuid::Uuid::new_v4().to_string())?;mask.width=analysed.metadata.mask_width;mask.height=analysed.metadata.mask_height;
  let prepared=analysed.prepare_binding_with_regions(&state.process_runtime,&source,settings,original,copy,mask,regions,&token,std::time::Duration::from_secs(1800),&tx).await?;
  let registered=super::concat_cutout_publish::register(&state,&actor,&project,request.expected_revision,prepared,&token).await?;
  super::concat_cutout_publish::apply_registered(&state,&actor,&project,request.composition_id.as_deref(),&clip,request.expected_revision,&registered,&token).await
 }.await;
 if result.is_err(){for id in generated{if state.db.composition_source_reference_count(&id).await.unwrap_or(1)==0{let _=state.library.remove(&id).await;if state.library.get(&id).await.is_none(){let _=state.db.release_space_source_claim(&id).await;}}}}
 result});
 work.await.map_err(|e|AppError::internal("join cutout analysis",e))?.map(Json).map_err(|e|AppError::conflict(e.to_string()))
}

/// Cancellation wins even when a resource becomes available simultaneously.
pub async fn acquire_analysis(state:&AppState,cancel:&tokio_util::sync::CancellationToken)->Result<crate::jobs::JobPermit>{
 tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("cutout request cancelled"),permit=state.acquire_job_slot(ResourceClass::Analysis)=>Ok(permit?)}
}
