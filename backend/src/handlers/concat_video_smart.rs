//! Start the existing durable SAM tracker from an authorized saved clip clock.
pub const MAX_START_BODY_BYTES:usize=4*1024*1024;
use axum::{extract::{Path,State},http::{HeaderMap,StatusCode},Json};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use crate::{state::AppState,error::{ApiJson,AppError,AppResult},domain::composition::{Composition,CompositionClipId,CompositionTrack}};
#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct StartRequest{#[serde(default,skip_serializing_if="Option::is_none")]operation_id:Option<String>,composition_id:Option<String>,clip_id:String,expected_revision:u64,time_us:u64,points:Vec<[f64;2]>}
pub async fn start(State(state):State<AppState>,Path(project):Path<String>,headers:HeaderMap,ApiJson(request):ApiJson<StartRequest>)->AppResult<(StatusCode,Json<Value>)>{
 if let Some(operation)=&request.operation_id{super::concat_video_operation::validate_id(operation)?;}
 use sha2::{Digest,Sha256};let request_hash=format!("{:x}",Sha256::digest(serde_json::to_vec(&request).map_err(|e|AppError::internal("hash smart operation",e))?));
 crate::services::local_sam::validate_points(&request.points).map_err(|e|AppError::bad_request(e.to_string()))?;
 let actor=super::publish::authenticated_actor(&state,&headers).await?;
 super::cutout_assets::authorize(&state,&project,&actor).await?;
 let cancel=tokio_util::sync::CancellationToken::new();let _guard=cancel.clone().drop_guard();
 let worker=tokio::spawn(async move{
  if let Some(operation)=&request.operation_id{if let Some(receipt)=super::concat_video_operation::peek(&state,&actor,&project,operation,&request_hash,&headers).await?{if state.get_job(&receipt.job_id).await.map_err(|e|AppError::internal("read smart operation job",e))?.is_some(){if super::concat_video_operation::stop_if_cancelled(&state,&receipt.job_id).await?{return Err(AppError::conflict("Операция SAM отменена"))}super::jobs::dispatch_job(&state,&receipt.job_id).await;return Ok((StatusCode::ACCEPTED,Json(super::concat_video_operation::accepted(&receipt)?)))}}}
  let _permit=super::concat_cutout_analyse::acquire_analysis(&state,&cancel).await.map_err(|e|AppError::conflict(e.to_string()))?;
  let saved=state.db.get_composition_project_for(&project,&actor).await.map_err(|e|AppError::internal("read smart video target",e))?.ok_or_else(||AppError::not_found("Проект не найден"))?;
  if saved.revision!=request.expected_revision{return Err(AppError::conflict("Проект изменился"))}
  let content=if let Some(nodes)=saved.document.get("compositions"){&nodes[request.composition_id.as_deref().ok_or_else(||AppError::bad_request("Нужен таймлайн"))?]["content"]}else{if request.composition_id.is_some(){return Err(AppError::bad_request("Лишний таймлайн"))} &saved.document};
  let composition:Composition=serde_json::from_value(content.clone()).map_err(|e|AppError::bad_request(e.to_string()))?;composition.validate_native_surface().map_err(|e|AppError::bad_request(e.to_string()))?;
  let id=CompositionClipId::parse(request.clip_id).map_err(|e|AppError::bad_request(e.to_string()))?;
  if composition.metadata.as_ref().is_some_and(|m|m.media_versions.contains_key(&id)){return Err(AppError::conflict("Клип содержит обработанную версию"))}
  let clip=composition.tracks.iter().find_map(|t|match t{CompositionTrack::Video{locked:false,clips,..}=>clips.iter().find(|c|c.id==id&&c.enabled),_=>None}).ok_or_else(||AppError::bad_request("Нужно незаблокированное видео"))?;
  let physical=super::concat_cutout_access::library_id(&saved.document,clip.source_id.as_str()).map_err(|e|AppError::conflict(e.to_string()))?;
  if !saved.source_ids.iter().any(|id|id==&physical){return Err(AppError::forbidden("Исходник не принадлежит проекту"))}
  let source_us=crate::domain::video_clock::source_tick(&clip.placement,clip.playback_mode,request.time_us).map_err(|e|AppError::bad_request(e.to_string()))?.ok_or_else(||AppError::bad_request("Кадр вне клипа"))?;
  // Source authorization can relocate legacy files into private Space storage.
  // Complete that move before concurrent seed probes or receipt authorization.
  super::library::authorize_source_access(&state,&physical,&headers).await?;
  let source_id=clip.source_id.as_str().to_owned();let path=super::concat_cutout_access::source_for_document(&state,&actor,&saved.document,&source_id).await.map_err(|e|AppError::forbidden(e.to_string()))?;
  let before=crate::project_archive::fingerprint_file_with_limit(&path,64*1024*1024*1024).await.map_err(|e|AppError::conflict(e.to_string()))?;
  let seed=crate::services::concat_video_sam_seed::resolve(&state.process_runtime,&path,source_us,&cancel).await.map_err(|e|AppError::bad_request(e.to_string()))?;
  super::cutout_assets::authorize(&state,&project,&actor).await?;
  let current=state.db.get_composition_project_for(&project,&actor).await.map_err(|e|AppError::internal("recheck smart video target",e))?.ok_or_else(||AppError::conflict("Проект недоступен"))?;
  if current.revision!=saved.revision||current.document!=saved.document||cancel.is_cancelled()||crate::project_archive::fingerprint_file_with_limit(&path,64*1024*1024*1024).await.map_err(|e|AppError::conflict(e.to_string()))?!=before{return Err(AppError::conflict("Исходник или проект изменился"))}
  let mut accepted=json!({"sourceId":source_id,"sourceUs":seed.source_us,"seedFrame":seed.seed_frame,"frames":seed.frames,"frameRate":seed.frame_rate,"expectedRevision":saved.revision});
  if let Some(times)=seed.frame_times_us{accepted["frameTimesUs"]=json!(times)}
  let receipt=if let Some(operation)=&request.operation_id{Some(super::concat_video_operation::reserve(&state,&actor,&project,operation,&request_hash,&physical,&before.1,accepted.clone(),&headers).await?)}else{None};
  let body:super::sam_jobs::Request=serde_json::from_value(json!({"sourceId":physical,"points":request.points,"seedFrame":seed.seed_frame})).map_err(|e|AppError::bad_request(e.to_string()))?;
  let(status,Json(submitted))=super::sam_jobs::submit_named(state.clone(),headers,body,receipt.as_ref().map(|r|r.job_id.clone()),true,Some(before.1.clone())).await?;
  let id=submitted["jobId"].as_str().ok_or_else(||AppError::conflict("Нет задачи SAM"))?;
  if super::concat_video_operation::stop_if_cancelled(&state,id).await?{return Err(AppError::conflict("Операция SAM отменена"))}
  super::jobs::dispatch_job(&state,id).await;
  if let Some(receipt)=receipt{accepted=super::concat_video_operation::accepted(&receipt)?;}else{accepted["jobId"]=json!(id);}
  Ok((status,Json(accepted)))
 });
 worker.await.map_err(|e|AppError::internal("join smart video submit",e))?
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct AttachRequest{composition_id:Option<String>,clip_id:String,expected_revision:u64,time_us:u64,job_id:String,tool:crate::domain::cutout::Tool,size:f64}
pub async fn attach(State(state):State<AppState>,Path(project):Path<String>,headers:HeaderMap,ApiJson(request):ApiJson<AttachRequest>)->AppResult<Json<crate::db::CompositionProject>>{
 use anyhow::ensure;
 if !matches!(request.tool,crate::domain::cutout::Tool::SmartBrush|crate::domain::cutout::Tool::SmartEraser)||!request.size.is_finite()||!(0.005..=0.5).contains(&request.size){return Err(AppError::bad_request("Неверная умная видеокисть"))}
 let actor=super::publish::authenticated_actor(&state,&headers).await?;super::cutout_assets::authorize(&state,&project,&actor).await?;
 let cancel=tokio_util::sync::CancellationToken::new();let _guard=cancel.clone().drop_guard();
 let worker=tokio::spawn(async move{let result:anyhow::Result<_>=async{
  let _permit=super::concat_cutout_analyse::acquire_analysis(&state,&cancel).await?;
  ensure!(!super::concat_video_operation::is_cancelled(&state,&request.job_id).await?,"smart region operation cancelled");
  let saved=state.db.get_composition_project_for(&project,&actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;ensure!(saved.revision==request.expected_revision,"stale smart region target");
  let mut document=saved.document.clone();let content=if document.get("compositions").is_some(){document.get_mut("compositions").and_then(|n|n.get_mut(request.composition_id.as_deref()?)).and_then(|n|n.get_mut("content")).ok_or_else(||anyhow::anyhow!("timeline unavailable"))?}else{ensure!(request.composition_id.is_none(),"unexpected timeline");&mut document};
  let mut composition:Composition=serde_json::from_value(content.clone())?;composition.validate_native_surface()?;let id=CompositionClipId::parse(request.clip_id)?;
  ensure!(!composition.metadata.as_ref().is_some_and(|m|m.media_versions.contains_key(&id)),"clip contains another processed version");
  let clip=composition.tracks.iter_mut().find_map(|t|match t{CompositionTrack::Video{locked:false,clips,..}=>clips.iter_mut().find(|c|c.id==id&&c.enabled),_=>None}).ok_or_else(||anyhow::anyhow!("unlocked video required"))?;
  let source_id=clip.source_id.clone();let physical=super::concat_cutout_access::library_id(&saved.document,source_id.as_str())?;ensure!(saved.source_ids.iter().any(|s|s==&physical),"unrelated clip source");
  let source_us=crate::domain::video_clock::source_tick(&clip.placement,clip.playback_mode,request.time_us)?.ok_or_else(||anyhow::anyhow!("cursor outside clip"))?;
  let binding=clip.concat_cutout.as_mut().ok_or_else(||anyhow::anyhow!("enable automatic video cutout before attaching a smart region"))?;
  let cache=super::concat_video_cache::restore_for_model(&state,&actor,&project,source_id.as_str(),saved.revision,&binding.runtime_hash,&binding.model_hash,&cancel).await?.ok_or_else(||anyhow::anyhow!("automatic video cache missing"))?;ensure!(cache.video_hash()==binding.source_hash,"automatic source changed");
  let(movie,_)=super::sam_jobs::checked_video(&state,&request.job_id,&headers).await.map_err(|_|anyhow::anyhow!("SAM video job unavailable"))?;
  let recipe=super::sam_jobs::points(State(state.clone()),Path(request.job_id.clone()),headers.clone()).await.map_err(|_|anyhow::anyhow!("SAM points unavailable"))?.0;
  let seed=crate::domain::video_region::frame_ordinal(source_us,movie.frames,movie.frame_rate,movie.frame_times_us.as_deref())?;
  ensure!(movie.source_hash==binding.source_hash&&recipe["sourceId"].as_str()==Some(physical.as_str())&&seed==movie.seed_frame,"SAM job source/seed does not match clip cursor");
  let mut correction=binding.settings();let stroke=crate::domain::cutout::Stroke{tool:request.tool,size:request.size,points:serde_json::from_value(recipe["points"].clone())?,at:Some(source_us as f64/1_000_000.),region:None};let index=correction.strokes.len();correction.strokes.push(stroke);correction.custom=true;correction.validate()?;
  let(mask,snapshot)=super::sam_video_assets::mask_for_original(&state,&request.job_id,&project,&actor,&headers,source_id.as_str(),saved.revision).await.map_err(|_|anyhow::anyhow!("SAM region asset unavailable"))?;
  ensure!(!cancel.is_cancelled(),"smart region attachment cancelled");
  binding.correction=Some(correction);binding.region_tracks.push(crate::domain::concat_video_binding::RegionTrack{stroke:index,snapshot:snapshot.clone()});binding.validate()?;
  let duration=snapshot.frame_times_us.as_ref().and_then(|t|t.last()).map(|v|u128::from(*v)).unwrap_or_else(||(u128::from(snapshot.frames)*u128::from(snapshot.frame_rate.denominator)*1_000_000).div_ceil(u128::from(snapshot.frame_rate.numerator)));
  let descriptor:crate::domain::composition::CompositionSource=serde_json::from_value(json!({"id":mask.id,"kind":"video","durationTicks":u64::try_from(duration)?,"width":256,"height":256,"hasAudio":false}))?;
  if let Some(existing)=composition.sources.get(&descriptor.id){ensure!(serde_json::to_value(existing)?==serde_json::to_value(&descriptor)?,"region source descriptor collision");}else{composition.sources.insert(descriptor.id.clone(),descriptor);}
  composition.validate_native_surface()?;*content=serde_json::to_value(composition)?;let sources=crate::db::extract_document_source_ids(&document)?;
  super::cutout_assets::authorize(&state,&project,&actor).await.map_err(|_|anyhow::anyhow!("project editor access revoked"))?;ensure!(!cancel.is_cancelled(),"smart region attachment cancelled");
  ensure!(!super::concat_video_operation::is_cancelled(&state,&request.job_id).await?,"smart region operation cancelled");
  state.db.update_composition_project_for(&project,&actor,&saved.name,&document,&sources,saved.revision).await?.ok_or_else(||anyhow::anyhow!("project changed during smart region attachment"))
 }.await;result.map(Json).map_err(|e|AppError::conflict(e.to_string()))});
 worker.await.map_err(|e|AppError::internal("join smart region attachment",e))?
}
