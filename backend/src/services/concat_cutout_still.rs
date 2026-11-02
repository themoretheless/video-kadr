//! Native still analysis for Concat policy. Caller owns ACL and Analysis permit.
use std::{future::Future,path::{Path,PathBuf},sync::{Arc,Mutex},time::Duration};
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::{domain::concat_cutout_import::Subject,process_control::ProcessRuntime};
use super::{concat_cutout_selection::{self,Model},local_cutout,local_selfie,concat_cutout_asset::{load_still,load_verified_image,verify_copy_pixels,ExpectedIdentity}};
#[derive(Clone)]
pub enum Engine {Person(super::local_person::Config),Selfie(local_selfie::Config),Object(local_cutout::Config)}
impl Engine{pub(super) fn model(&self)->Model{match self{Self::Person(_)=>Model::Person,Self::Selfie(_)=>Model::Selfie,Self::Object(_)=>Model::Object}}}
impl Engine{pub(crate) fn sequence(&self)->super::local_person_sequence::Engine<'_>{match self{Self::Person(c)=>super::local_person_sequence::Engine::Person(c),Self::Selfie(c)=>super::local_person_sequence::Engine::Selfie(c),Self::Object(c)=>super::local_person_sequence::Engine::Object(c)}}}
/// Owns private temporary artifacts until the caller registers/copies them.
/// Dropping a failed or completed unaccepted analysis removes its staging files.
pub struct AnalysedStill {
 pub subject:Subject,pub model:Model,pub metadata:local_cutout::Metadata,
 pub copy:PathBuf,pub mask:PathBuf,
 directory:PathBuf,
 executable:PathBuf,
}
impl Drop for AnalysedStill{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.directory);}}
struct Stage(PathBuf);
impl Drop for Stage{fn drop(&mut self){if !self.0.as_os_str().is_empty(){let _=std::fs::remove_dir_all(&self.0);}}}
/// `load` must resolve host-controlled, authorized model inventory.
/// Person acquisition failure follows the original Selfie fallback.
pub async fn analyse<L,LF>(runtime:&ProcessRuntime,subject:Subject,cached:Option<Model>,source:&Path,source_hash:&str,staging_root:&Path,mut load:L,cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>)->Result<AnalysedStill>
where L:FnMut(Model)->LF,LF:Future<Output=Result<Engine>>{
 ensure!(source.is_absolute()&&staging_root.is_absolute()&&staging_root.is_dir()&&!cancel.is_cancelled(),"invalid still analysis paths/cancelled");
 ensure!(!limit.is_zero()&&limit<=Duration::from_secs(3600),"invalid still analysis deadline");
 let directory=staging_root.join(format!("concat-still-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let mut stage=Stage(directory.clone());
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
 let artifacts=Arc::new(Mutex::new(None));let written=artifacts.clone();let deadline=tokio::time::Instant::now()+limit;
 let(model,_)=concat_cutout_selection::analyse(subject,cached,|requested|{
  let acquired=load(requested);async move{let engine=acquired.await?;ensure!(engine.model()==requested,"acquired cutout model differs from requested family");Ok(engine)}
 },|engine:Engine|{
  let directory=directory.clone();let written=written.clone();
  async move{
   let executable=match &engine{Engine::Person(c)=>c.executable.clone(),Engine::Selfie(c)=>c.executable.clone(),Engine::Object(c)=>c.executable.clone()};
   let model=engine.model();let copy=directory.join(format!("{model:?}.png"));let mask=directory.join(format!("{model:?}.mask.png"));
   let remaining=deadline.saturating_duration_since(tokio::time::Instant::now());ensure!(!remaining.is_zero(),"still analysis deadline exceeded");
   let metadata=match engine{
    Engine::Person(config)=>{
     let result=super::local_person_still::analyse(runtime,&config,source,source_hash,&directory,cancel,remaining,progress).await?;
     tokio::fs::copy(&result.copy,&copy).await?;tokio::fs::copy(&result.mask,&mask).await?;
     local_cutout::Metadata{schema_version:8,width:result.sequence.width,height:result.sequence.height,mask_width:result.sequence.mask_width,mask_height:result.sequence.mask_height,blank_mask:result.sequence.blank_frames==1,mask_hash:result.sequence.mask_hash.clone(),source_hash:result.encoded_source_hash.clone(),model_hash:result.sequence.model_hash.clone(),runtime_hash:result.sequence.runtime_hash.clone(),output_hash:result.copy_file_hash.clone(),mask_file_hash:Some(result.mask_file_hash.clone())}
    },
    Engine::Selfie(config)=>{let metadata=local_selfie::cutout(runtime,&config,local_selfie::Input{source,source_hash,mask_destination:Some(&mask)},&copy,cancel,remaining,progress).await?;serde_json::from_value(serde_json::to_value(metadata)?)?},
    Engine::Object(config)=>local_cutout::cutout(runtime,&config,local_cutout::Input{source,source_hash,mask_destination:Some(&mask)},&copy,cancel,remaining,progress).await?,
   };
   let automatic=load_still(&mask,&metadata,ExpectedIdentity{source_hash,model_hash:&metadata.model_hash,runtime_hash:&metadata.runtime_hash},cancel).await?;
   let original=load_verified_image(source,Some(source_hash),cancel).await?;
   let output=load_verified_image(&copy,Some(&metadata.output_hash),cancel).await?;
   verify_copy_pixels(&original,&output,if metadata.blank_mask{None}else{Some(&automatic)},cancel)?;
   let plane=crate::domain::concat_cutout_clock::ResolvedMask{width:automatic.width,height:automatic.height,probabilities:automatic.probabilities.clone()};
   *written.lock().map_err(|_|anyhow::anyhow!("still artifact state poisoned"))?=Some((metadata,copy,mask,executable));Ok(plane)
  }
 },cancel).await?;
 ensure!(tokio::time::Instant::now()<=deadline&&!cancel.is_cancelled(),"still analysis deadline/cancelled");
 let(metadata,copy,mask,executable)=artifacts.lock().map_err(|_|anyhow::anyhow!("still artifact state poisoned"))?.take().ok_or_else(||anyhow::anyhow!("still analysis result missing"))?;
 stage.0=PathBuf::new();Ok(AnalysedStill{subject,model,metadata,copy,mask,directory,executable})
}

/// Owns the checked temporary copy until project/library publication completes.
#[derive(Default)]
pub struct SavedRegions {
 pub correction:Option<crate::domain::cutout::Correction>,
 pub sources:Vec<crate::domain::composition::CompositionSource>,
 pub paths:std::collections::BTreeMap<String,PathBuf>,
}
pub struct PreparedStill {
 pub binding:crate::domain::concat_project_import::ReadyStillBinding,
 pub copy:PathBuf,pub mask:PathBuf,
 pub(crate) region_paths:std::collections::BTreeMap<String,PathBuf>,
 _analysis:AnalysedStill,
}
impl AnalysedStill {
 /// Descriptors must come from the caller's authorized source/ID reservation.
 /// Timed learned regions need their own verified provider and are not fabricated.
 pub async fn prepare_binding(self,runtime:&ProcessRuntime,source:&Path,plan:crate::domain::concat_cutout_import::Plan,original:crate::domain::composition::CompositionSource,copy:crate::domain::composition::CompositionSource,mask:crate::domain::composition::CompositionSource,cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>)->Result<PreparedStill>{
  self.prepare_binding_with_regions(runtime,source,plan,original,copy,mask,SavedRegions::default(),cancel,limit,progress).await
 }
 pub async fn prepare_binding_with_regions(self,runtime:&ProcessRuntime,source:&Path,plan:crate::domain::concat_cutout_import::Plan,original:crate::domain::composition::CompositionSource,copy:crate::domain::composition::CompositionSource,mask:crate::domain::composition::CompositionSource,regions:SavedRegions,cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>)->Result<PreparedStill>{
  use crate::domain::{composition::SourceKind,cutout::Attachment,concat_cutout_binding::StillAssets,concat_project_import::ReadyStillBinding};
  ensure!(plan.subject==self.subject,"cutout subject changed since analysis");
  let correction=regions.correction.as_ref().unwrap_or(&plan.correction);
  let mut plain=correction.clone();for stroke in &mut plain.strokes{stroke.region=None;}
  ensure!(plain==plan.correction,"saved region recipe changed");
  let mut inputs=Vec::with_capacity(correction.strokes.len());
  for stroke in &correction.strokes{
   if let Some(region)=&stroke.region{let path=regions.paths.get(&region.mask_source_id).ok_or_else(||anyhow::anyhow!("saved region path missing"))?;inputs.push(Some(super::local_mask_edit::RegionInput{path,attachment:region}));}
   else{ensure!(!(correction.custom&&matches!(stroke.tool,crate::domain::cutout::Tool::SmartBrush|crate::domain::cutout::Tool::SmartEraser)&&stroke.at.is_some()),"verified learned region required");inputs.push(None);}
  }
  ensure!([&original,&copy,&mask].iter().all(|s|s.kind==SourceKind::Image&&!s.has_audio),"cutout descriptors must be silent images");
  ensure!(original.id!=copy.id&&original.id!=mask.id&&copy.id!=mask.id,"cutout asset ID collision");
  ensure!((original.width,original.height)==(self.metadata.width,self.metadata.height)&&(copy.width,copy.height)==(original.width,original.height)&&(mask.width,mask.height)==(self.metadata.mask_width,self.metadata.mask_height),"cutout descriptor dimensions changed");
  let corrected=self.directory.join("prepared.png");
  super::local_mask_edit::correct_with_regions(runtime,&self.executable,super::local_mask_edit::Input{source,mask:&self.mask,original:&self.metadata,correction},&inputs,&corrected,cancel,limit,progress).await?;
  let snapshot=Attachment{subject:Some(self.subject),source_id:original.id.as_str().into(),mask_source_id:mask.id.as_str().into(),source_hash:self.metadata.source_hash.clone(),mask_hash:self.metadata.mask_hash.clone(),mask_file_hash:self.metadata.mask_file_hash.clone().ok_or_else(||anyhow::anyhow!("analysis mask identity missing"))?,model_hash:self.metadata.model_hash.clone(),runtime_hash:self.metadata.runtime_hash.clone(),correction:correction.clone()};
  let assets=StillAssets{original,copy,mask,regions:regions.sources,snapshot};
  super::concat_cutout_asset::verify_still_bundle_with_regions(source,&corrected,&self.mask,&assets,&plan,ExpectedIdentity{source_hash:&self.metadata.source_hash,model_hash:&self.metadata.model_hash,runtime_hash:&self.metadata.runtime_hash},&regions.paths,cancel).await?;
  let binding=ReadyStillBinding{assets,source_settings:plan,source_hash:self.metadata.source_hash.clone(),model_hash:self.metadata.model_hash.clone(),runtime_hash:self.metadata.runtime_hash.clone()};
  Ok(PreparedStill{binding,copy:corrected,mask:self.mask.clone(),region_paths:regions.paths,_analysis:self})
 }
}

/// Persistent private files; caller registers these paths with the media library
/// and claims descriptor IDs for its authorized project before exposing a result.
pub struct PublishedStill {
 pub binding:crate::domain::concat_project_import::ReadyStillBinding,
 pub copy:PathBuf,pub mask:PathBuf,
 pub copy_file_hash:String,pub mask_file_hash:String,
}
impl PreparedStill {
 /// Publishes both checked files into a fresh private directory without replacing
 /// existing files. An incomplete publication is rolled back before returning.
 pub async fn publish_files(self,root:&Path,source:&Path,cancel:&CancellationToken)->Result<PublishedStill>{
  use tokio::io::{AsyncReadExt,AsyncWriteExt};
  ensure!(root.is_absolute()&&tokio::fs::symlink_metadata(root).await?.file_type().is_dir()&&!cancel.is_cancelled(),"invalid still publication root/cancelled");
  let directory=root.join(format!("concat-cutout-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let mut stage=Stage(directory.clone());
  #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
  let copy=directory.join("copy.png");let mask=directory.join("mask.png");
  for(from,to,limit)in [(&self.copy,&copy,256*1024*1024u64),(&self.mask,&mask,1024*1024)]{
   ensure!(!cancel.is_cancelled(),"still publication cancelled");
   let input=tokio::fs::File::open(from).await?;let mut input=input.take(limit+1);
   let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);
   #[cfg(unix)]{options.mode(0o600);}
   let mut output=options.open(to).await?;
   let bytes=tokio::io::copy(&mut input,&mut output).await?;ensure!(bytes<=limit,"still publication file exceeds limit");output.flush().await?;output.sync_all().await?;
  }
  // Check the destination snapshots, not just the temporary paths that were read.
  let binding=&self.binding;
  super::concat_cutout_asset::verify_still_bundle_with_regions(source,&copy,&mask,&binding.assets,&binding.source_settings,ExpectedIdentity{source_hash:&binding.source_hash,model_hash:&binding.model_hash,runtime_hash:&binding.runtime_hash},&self.region_paths,cancel).await?;
  let copy_file_hash=super::local_speech::fingerprint_model(&copy).await?.1;
  let mask_file_hash=super::local_speech::fingerprint_model(&mask).await?.1;
  ensure!(!cancel.is_cancelled(),"still publication cancelled");
  stage.0=PathBuf::new();Ok(PublishedStill{binding:self.binding.clone(),copy,mask,copy_file_hash,mask_file_hash})
 }
}
