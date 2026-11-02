//! Project-level video subject selection, cache fill and committed family choice.
use std::{path::Path,time::Duration};
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::{state::AppState,domain::{concat_cutout_import::Subject,concat_cutout_clock::MaskTimeline},services::{concat_cutout_selection::{self,Model},concat_cutout_still::Engine,concat_video_selection,concat_video_analysis::{RunCache,FillReport},concat_video_sample::Input,local_enhance_video::Tools}};
pub struct Report{pub fill:FillReport,pub cache_reused:bool,pub model_hash:Option<String>}
async fn load(state:&AppState,actor:&str,model:Model)->Result<Engine>{match model{
 Model::Person=>Ok(Engine::Person(state.person.as_ref().as_ref().ok_or_else(||anyhow::anyhow!("Person model unavailable"))?.clone())),
 Model::Selfie=>Ok(Engine::Selfie(state.selfie.as_ref().as_ref().ok_or_else(||anyhow::anyhow!("Selfie model unavailable"))?.clone())),
 Model::Object=>{let(config,_)=super::cutout_models::config_for_owner(state,actor).await.map_err(|_|anyhow::anyhow!("Object model unavailable"))?;Ok(Engine::Object(config))},
}}
fn remaining(deadline:tokio::time::Instant,cancel:&CancellationToken)->Result<Duration>{ensure!(!cancel.is_cancelled(),"video analysis cancelled");let limit=deadline.saturating_duration_since(tokio::time::Instant::now());ensure!(!limit.is_zero(),"video analysis deadline exceeded");Ok(limit)}
/// Caller holds Analysis admission and model-operation serialization.
pub async fn analyse(state:&AppState,actor:&str,project:&str,source_id:&str,revision:u64,path:&Path,ranges:&[(f64,f64)],subject:Subject,cancel:&CancellationToken)->Result<Report>{
 let deadline=tokio::time::Instant::now()+Duration::from_secs(1800);
 let choice=super::concat_video_cache::choice(state,actor,project,source_id,revision,cancel).await?;
 let hash=crate::project_archive::fingerprint_file_with_limit(path,64*1024*1024*1024).await?.1;
 let choice=choice.filter(|choice|choice.video_hash==hash);
 let old_model=choice.as_ref().map(|choice|choice.model);let stale=concat_cutout_selection::stale(subject,old_model);let cached=if stale{None}else{old_model};
 let codecs=crate::services::local_enhance_video::resolve_tools().await?;let root=state.storage.join("concat-video-analysis");tokio::fs::create_dir_all(&root).await?;let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 let mut fill=FillReport::default();
 let (mut cache,engine,cache_reused)=if subject==Subject::Auto&&cached.is_none(){
  let runs=MaskTimeline::new(vec![])?.missing_runs(ranges)?;let Some(run)=runs.first()else{return Ok(Report{fill,cache_reused:false,model_hash:None})};
  let selected=concat_video_selection::analyse(&state.process_runtime,Tools{ffmpeg:&codecs.ffmpeg,ffprobe:&codecs.ffprobe},Input{source:path,source_hash:&hash,run:crate::domain::concat_cutout_clock::AnalysisRun{start_millis:run.start_millis,frames:1},staging_bytes:64*1024*1024*1024},subject,None,&root,|model|load(state,actor,model),cancel,remaining(deadline,cancel)?,&tx).await?;
  let Some(selected)=selected else{fill.empty_runs=1;return Ok(Report{fill,cache_reused:false,model_hash:None})};
  let cache=RunCache::new_for_model(hash.clone(),selected.run.metadata.runtime_hash.clone(),selected.run.metadata.model_hash.clone())?;(cache,selected.engine,false)
 }else{
  let(model,engine)=concat_video_selection::acquire_decided(subject,cached,|model|load(state,actor,model),cancel,remaining(deadline,cancel)?).await?;let identity=engine.sequence().identity().await?;
  let reusable=!stale&&old_model.is_none_or(|old|old==model);
  let existing=if reusable{super::concat_video_cache::restore_for_model(state,actor,project,source_id,revision,&identity.runtime_hash,engine.sequence().model_hash(),cancel).await?}else{None};let reused=existing.is_some();
  let cache=if let Some(cache)=existing{cache}else{RunCache::new_for_model(hash.clone(),identity.runtime_hash,engine.sequence().model_hash().into())?};(cache,engine,reused)
 };
 let session=super::concat_video_cache::PublicationSession::begin(state,actor,project,source_id,revision,&cache,cancel).await?;
 let publication_cancel=CancellationToken::new();let mut drain_deadline=None;
 for run in cache.timeline()?.missing_runs(ranges)?{
  let sampled=crate::services::concat_video_sample::sample(&state.process_runtime,Tools{ffmpeg:&codecs.ffmpeg,ffprobe:&codecs.ffprobe},Input{source:path,source_hash:&hash,run,staging_bytes:64*1024*1024*1024},&root,cancel,remaining(deadline,cancel)?).await?;
  let Some(sampled)=sampled else{fill.empty_runs+=1;continue};
  let operation_cancel=cancel.child_token();let(frames,mut receiver)=tokio::sync::mpsc::channel(1);
  let operation=crate::services::concat_video_analysis::stream_checkpoints(&state.process_runtime,engine.sequence(),sampled,&root,&operation_cancel,remaining(deadline,cancel)?,&tx,&frames);tokio::pin!(operation);
  let mut terminal=None;let mut failure=None;let mut received=0u64;
  loop{
   let frame=if terminal.is_some(){receiver.try_recv().ok()}else{tokio::select!{biased;frame=receiver.recv()=>frame,result=&mut operation=>{terminal=Some(result);receiver.try_recv().ok()}}};let Some(frame)=frame else{if terminal.is_some(){break}else{continue}};
   if failure.is_some(){continue}
   if cancel.is_cancelled()||tokio::time::Instant::now()>=deadline{drain_deadline.get_or_insert_with(||tokio::time::Instant::now()+Duration::from_secs(30));}
   let result:Result<()>=async{
    if drain_deadline.is_some_and(|limit|tokio::time::Instant::now()>=limit){publication_cancel.cancel();}
    ensure!(!publication_cancel.is_cancelled(),"video prefix publication grace expired");cache.insert(frame)?;session.append(state,&mut cache,&publication_cancel).await?;Ok(())
   }.await;
   match result{Ok(())=>received+=1,Err(error)=>{failure=Some(error);operation_cancel.cancel();}}
  }
  // Always observe native termination before leaving the request's model/Analysis
  // admission. Successfully committed prefixes survive an error or client abort.
  if let Some(error)=failure{return Err(error)}
  let produced=terminal.ok_or_else(||anyhow::anyhow!("missing progressive video status"))??;ensure!(produced==received,"video prefix delivery count mismatch");
  if received>0{fill.analysed_runs+=1;fill.analysed_frames+=received;}
 }
 session.finish(state,&cache,cancel).await?;
 Ok(Report{model_hash:Some(cache.model_hash().into()),fill,cache_reused})
}
