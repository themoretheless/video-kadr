//! Original Concat subject policy over an actual source-PTS video run.
//! Caller owns source/model ACL, Analysis admission and persistence of the chosen family.
use std::{path::{Path,PathBuf},time::Duration,sync::{Arc,Mutex},future::Future};
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::{process_control::ProcessRuntime,domain::{concat_cutout_import::Subject,concat_cutout_clock::{AnalysisRun,ResolvedMask}}};
use super::{concat_cutout_selection::{self,Model},concat_cutout_still::Engine,concat_video_analysis::{self,AnalysedRun},concat_video_sample::{self,Input},local_enhance_video::{Tools,remaining}};
pub fn model_from_hash(hash:&str)->Result<Model>{match hash{super::local_person::MODEL_HASH=>Ok(Model::Person),super::local_selfie::MODEL_HASH=>Ok(Model::Selfie),super::local_cutout::MODEL_HASH=>Ok(Model::Object),_=>anyhow::bail!("unsupported saved video model")}}
pub struct SelectedRun{pub subject:Subject,pub model:Model,pub engine:Engine,pub run:AnalysedRun}
struct Stage(PathBuf);impl Drop for Stage{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
pub async fn analyse<L,LF>(runtime:&ProcessRuntime,tools:Tools<'_>,input:Input<'_>,subject:Subject,cached:Option<Model>,root:&Path,mut load:L,cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>)->Result<Option<SelectedRun>>
where L:FnMut(Model)->LF,LF:Future<Output=Result<Engine>>{
 ensure!(!limit.is_zero()&&limit<=Duration::from_secs(3600)&&!cancel.is_cancelled(),"invalid video selection deadline/cancelled");
 ensure!(input.source.is_absolute()&&root.is_absolute()&&root.is_dir()&&input.staging_bytes>0&&input.staging_bytes<=64*1024*1024*1024,"invalid video selection paths/budget");
 let _=crate::domain::concat_video_pacer::VideoPacer::<(),_>::new(std::iter::empty::<Result<crate::domain::concat_video_pacer::DecodedFrame<()>>>(),input.run)?;

 ensure!(!concat_cutout_selection::stale(subject,cached),"cached video model conflicts with subject");
 let deadline=tokio::time::Instant::now()+limit;
 let mut acquire=|requested|{let acquired=load(requested);async move{
  let resolve=async{let engine=acquired.await?;ensure!(engine.model()==requested,"acquired video model differs from requested family");engine.sequence().identity().await?;Ok(engine)};
  tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("video model acquisition cancelled"),result=tokio::time::timeout_at(deadline,resolve)=>result?}
 }};
 let undecided=subject==Subject::Auto&&cached.is_none();
 let (model,engine)=if !undecided{
  if subject==Subject::Object||(subject==Subject::Auto&&cached==Some(Model::Object)){(Model::Object,acquire(Model::Object).await?)}else{concat_cutout_selection::person(&mut acquire,cancel).await?}
 }else{
  let first=concat_video_sample::sample(runtime,Tools{ffmpeg:tools.ffmpeg,ffprobe:tools.ffprobe},Input{source:input.source,source_hash:input.source_hash,run:AnalysisRun{start_millis:input.run.start_millis,frames:1},staging_bytes:input.staging_bytes},root,cancel,remaining(deadline,cancel)?).await?;
  let Some(first)=first else{return Ok(None)};let first=Arc::new(first);
  let directory=root.join(format!("concat-video-choice-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let _stage=Stage(directory.clone());
  #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
  let selected=Arc::new(Mutex::new(None));let written=selected.clone();
  let (probe_progress,_)=tokio::sync::mpsc::unbounded_channel();
  let (model,_)=concat_cutout_selection::analyse(subject,cached,&mut acquire,|engine:Engine|{
   let first=first.clone();let directory=directory.clone();let written=written.clone();let progress=probe_progress.clone();
   async move{
    let name=format!("{:?}",engine.model());let output=directory.join(format!("{name}.rgba"));let masks=directory.join(format!("{name}.masks"));
    let metadata=super::local_person_sequence::segment_with_engine(runtime,engine.sequence(),super::local_person_sequence::Input{source:&first.rgba,source_hash:&first.rgba_hash,mask_destination:Some(&masks),width:first.width,height:first.height,frames:1},&output,cancel,remaining(deadline,cancel)?,&progress).await?;
    let mut verified=super::concat_video_masks::open(&masks,first.start_millis,&metadata,super::concat_video_masks::ExpectedRun{decoded_source_hash:&first.rgba_hash,model_hash:engine.sequence().model_hash(),runtime_hash:&metadata.runtime_hash},cancel).await?;
    let mask=verified.read(first.start_millis,cancel)?;
    *written.lock().map_err(|_|anyhow::anyhow!("video selection state poisoned"))?=Some(engine);
    Ok(ResolvedMask{width:mask.width,height:mask.height,probabilities:mask.probabilities.clone()})
   }
  },cancel).await?;
  let engine=selected.lock().map_err(|_|anyhow::anyhow!("video selection state poisoned"))?.take().ok_or_else(||anyhow::anyhow!("missing selected video engine"))?;
  (model,engine)
 };
 let sampled=concat_video_sample::sample(runtime,Tools{ffmpeg:tools.ffmpeg,ffprobe:tools.ffprobe},input,root,cancel,remaining(deadline,cancel)?).await?;let Some(sampled)=sampled else{return Ok(None)};
 let run=concat_video_analysis::analyse_with_engine(runtime,engine.sequence(),sampled,root,cancel,remaining(deadline,cancel)?,progress).await?;
 Ok(Some(SelectedRun{subject,model,engine,run}))
}

/// Acquire a decided subject/family without decoding or running inference.
pub async fn acquire_decided<L,LF>(subject:Subject,cached:Option<Model>,mut load:L,cancel:&CancellationToken,limit:Duration)->Result<(Model,Engine)>
where L:FnMut(Model)->LF,LF:Future<Output=Result<Engine>>{
 ensure!(!limit.is_zero()&&limit<=Duration::from_secs(3600)&&!cancel.is_cancelled(),"invalid video model acquisition deadline");
 ensure!(!(subject==Subject::Auto&&cached.is_none())&&!concat_cutout_selection::stale(subject,cached),"video model needs a new first-frame decision");
 let deadline=tokio::time::Instant::now()+limit;
 let mut acquire=|requested|{let acquired=load(requested);async move{let resolve=async{let engine=acquired.await?;ensure!(engine.model()==requested,"acquired video model differs from requested family");engine.sequence().identity().await?;Ok(engine)};tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("video model acquisition cancelled"),result=tokio::time::timeout_at(deadline,resolve)=>result?}}};
 if subject==Subject::Object||(subject==Subject::Auto&&cached==Some(Model::Object)){Ok((Model::Object,acquire(Model::Object).await?))}else{concat_cutout_selection::person(&mut acquire,cancel).await}
}
