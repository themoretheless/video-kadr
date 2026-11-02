//! One owned Concat video analysis run. Caller owns source ACL and Analysis admission.
use std::{path::{Path, PathBuf}, time::Duration};
use anyhow::{ensure, Result};
use tokio_util::sync::CancellationToken;
use crate::process_control::ProcessRuntime;
use super::{concat_video_masks::{self, ExpectedRun, VerifiedMaskRun}, concat_video_sample::SampledRun, local_person, local_person_sequence};

struct Stage(PathBuf);
impl Drop for Stage { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
/// Mask file handles close before their private directory is removed.
pub struct AnalysedRun {
    pub masks: VerifiedMaskRun,
    pub metadata: local_person_sequence::Metadata,
    pub video_hash: String,
    pub start_millis: u64,
    _stage: Stage,
}

pub async fn analyse(
    runtime: &ProcessRuntime,
    config: &local_person::Config,
    sampled: SampledRun,
    root: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<AnalysedRun> {
    analyse_with_engine(runtime,local_person_sequence::Engine::Person(config),sampled,root,cancel,limit,progress).await
}

pub async fn analyse_with_engine(
    runtime: &ProcessRuntime,
    engine: local_person_sequence::Engine<'_>,
    sampled: SampledRun,
    root: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<AnalysedRun> {
    ensure!(root.is_absolute() && root.is_dir() && !limit.is_zero() && limit <= Duration::from_secs(3600), "invalid video analysis root/deadline");
    let deadline = tokio::time::Instant::now() + limit;
    super::local_enhance_video::remaining(deadline, cancel)?;
    let identity = engine.identity().await?;
    let stage = Stage(root.join(format!("concat-video-analysis-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&stage.0).await?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&stage.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let output = stage.0.join("result.rgba");
    let masks = stage.0.join("masks.raw");
    let metadata = local_person_sequence::segment_with_engine(runtime, engine, local_person_sequence::Input {
        source: &sampled.rgba, source_hash: &sampled.rgba_hash,
        mask_destination: Some(&masks), width: sampled.width, height: sampled.height, frames: sampled.frames,
    }, &output, cancel, super::local_enhance_video::remaining(deadline, cancel)?, progress).await?;
    let verified = concat_video_masks::open(&masks, sampled.start_millis, &metadata, ExpectedRun {
        decoded_source_hash: &sampled.rgba_hash, model_hash: engine.model_hash(), runtime_hash: &identity.runtime_hash,
    }, cancel).await?;
    super::local_enhance_video::remaining(deadline, cancel)?;
    // The copied RGBA result is not needed for mask lookup; retain only verified masks.
    tokio::fs::remove_file(output).await?;
    Ok(AnalysedRun { masks: verified, metadata, video_hash: sampled.video_hash.clone(), start_millis: sampled.start_millis, _stage: stage })
}

/// An admitted source's disjoint runs. Nearest lookup is global across run boundaries.
/// This lease is in-memory; persistent project cache publication is a separate step.
pub struct RunCache {
    model_hash: String,
    video_hash: String,
    runtime_hash: String,
    runs: Vec<AnalysedRun>,
    frames: std::collections::BTreeMap<u64, usize>,
    publication_id:uuid::Uuid,
    published:Option<PublishedPrefix>,
}
impl RunCache {
    pub fn video_hash(&self) -> &str { &self.video_hash }
    pub fn model_hash(&self)->&str{&self.model_hash}
    pub fn runtime_hash(&self)->&str{&self.runtime_hash}
    pub fn is_empty(&self)->bool{self.frames.is_empty()}
    pub fn identity(&self)->Result<String>{
        use sha2::Digest;
        let runs:Vec<_>=self.runs.iter().map(|run|(run.start_millis,&run.metadata)).collect();
        let bytes=if self.model_hash==local_person::MODEL_HASH{serde_json::to_vec(&(&self.video_hash,&self.runtime_hash,runs))?}else{serde_json::to_vec(&(&self.video_hash,&self.runtime_hash,&self.model_hash,runs))?};
        Ok(format!("{:x}",sha2::Sha256::digest(bytes)))
    }
    pub fn new(video_hash: String, runtime_hash: String) -> Result<Self> {
        Self::new_for_model(video_hash,runtime_hash,local_person::MODEL_HASH.into())
    }
    pub fn new_for_model(video_hash:String,runtime_hash:String,model_hash:String)->Result<Self>{
        super::local_person_sequence_contract::layout_for(&model_hash,1,1,1)?;
        for hash in [&video_hash, &runtime_hash] {
            ensure!(hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "invalid video cache identity");
        }
        Ok(Self { model_hash, video_hash, runtime_hash, runs: vec![], frames: Default::default(), publication_id:uuid::Uuid::new_v4(), published:None })
    }
    pub fn timeline(&self) -> Result<crate::domain::concat_cutout_clock::MaskTimeline> {
        crate::domain::concat_cutout_clock::MaskTimeline::new(self.frames.keys().copied().collect())
    }
    /// Validate all indices before changing the cache. Failed insertion drops only the incoming lease.
    pub fn insert(&mut self, run: AnalysedRun) -> Result<()> {
        ensure!(run.video_hash == self.video_hash && run.metadata.runtime_hash == self.runtime_hash && run.metadata.model_hash == self.model_hash, "video cache identity mismatch");
        ensure!(self.frames.len().checked_add(usize::try_from(run.metadata.frames)?).is_some_and(|n| n <= 360000), "video cache frame budget exceeded");
        let times: Vec<u64> = (0..run.metadata.frames).map(|i| run.start_millis.checked_add(i.checked_mul(100)?)).collect::<Option<_>>().ok_or_else(|| anyhow::anyhow!("video cache clock overflow"))?;
        let timeline = run.masks.timeline()?;
        for at in &times {
            ensure!(!self.frames.contains_key(at), "overlapping video analysis runs");
            ensure!(timeline.nearest_millis(*at as f64 / 1000.)? == Some(*at), "video run clock identity mismatch");
        }
        let index = self.runs.len();
        for at in times { self.frames.insert(at, index); }
        self.runs.push(run);
        Ok(())
    }
    pub fn automatic(&mut self, millis: u64, cancel: &CancellationToken) -> Result<Option<std::sync::Arc<crate::domain::concat_cutout_clock::ResolvedMask>>> {
        ensure!(!cancel.is_cancelled(), "video cache read cancelled");
        let index = *self.frames.get(&millis).ok_or_else(|| anyhow::anyhow!("video cache mask missing"))?;
        self.runs[index].masks.automatic(millis, cancel)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FillReport {
    pub analysed_runs: usize,
    pub analysed_frames: u64,
    pub empty_runs: usize,
}
impl RunCache {
    /// Fill only absent 100 ms instants. Completed runs survive a later error/cancel,
    /// matching Concat's incremental store. Each run starts a fresh model recurrence.
    pub async fn fill_missing(
        &mut self,
        runtime: &ProcessRuntime,
        tools: super::local_enhance_video::Tools<'_>,
        config: &local_person::Config,
        source: &Path,
        ranges: &[(f64, f64)],
        root: &Path,
        staging_bytes: u64,
        cancel: &CancellationToken,
        limit: Duration,
        progress: &tokio::sync::mpsc::UnboundedSender<f64>,
    ) -> Result<FillReport> {
        self.fill_missing_with_engine(runtime,tools,local_person_sequence::Engine::Person(config),source,ranges,root,staging_bytes,cancel,limit,progress).await
    }
    pub async fn fill_missing_with_engine(
        &mut self,
        runtime: &ProcessRuntime,
        tools: super::local_enhance_video::Tools<'_>,
        engine: local_person_sequence::Engine<'_>,
        source: &Path,
        ranges: &[(f64, f64)],
        root: &Path,
        staging_bytes: u64,
        cancel: &CancellationToken,
        limit: Duration,
        progress: &tokio::sync::mpsc::UnboundedSender<f64>,
    ) -> Result<FillReport> {
        ensure!(source.is_absolute() && root.is_absolute() && root.is_dir() && !limit.is_zero() && limit <= Duration::from_secs(3600), "invalid video range analysis paths/deadline");
        let deadline = tokio::time::Instant::now() + limit;
        super::local_enhance_video::remaining(deadline, cancel)?;
        let wanted = self.timeline()?.missing_runs(ranges)?;
        let identity = engine.identity().await?;
        ensure!(engine.model_hash()==self.model_hash,"video range model family mismatch");
        ensure!(identity.runtime_hash == self.runtime_hash, "video range runtime identity mismatch");
        let before = super::local_enhance_video::fingerprint(source).await?;
        ensure!(before.1 == self.video_hash, "video range source identity mismatch");
        let mut report = FillReport::default();
        for run in wanted {
            let sampled = super::concat_video_sample::sample(runtime, super::local_enhance_video::Tools { ffmpeg: tools.ffmpeg, ffprobe: tools.ffprobe }, super::concat_video_sample::Input {
                source, source_hash: &self.video_hash, run, staging_bytes,
            }, root, cancel, super::local_enhance_video::remaining(deadline, cancel)?).await?;
            let Some(sampled) = sampled else { report.empty_runs += 1; continue; };
            let analysed = analyse_with_engine(runtime, engine, sampled, root, cancel, super::local_enhance_video::remaining(deadline, cancel)?, progress).await?;
            let frames = analysed.metadata.frames;
            self.insert(analysed)?;
            report.analysed_runs += 1;
            report.analysed_frames += frames;
        }
        ensure!(super::local_enhance_video::fingerprint(source).await? == before, "video changed during range analysis");
        super::local_enhance_video::remaining(deadline, cancel)?;
        Ok(report)
    }
}

const MAX_CACHE_MANIFEST: u64 = 64 * 1024 * 1024;
fn person_model_hash()->String{local_person::MODEL_HASH.into()}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheManifest {
    #[serde(default="person_model_hash")]
    model_hash:String,
    schema_version: u32,
    video_hash: String,
    runtime_hash: String,
    runs: Vec<StoredRun>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    parent:Option<CacheParent>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    total_runs:Option<usize>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheParent{generation:String,manifest_hash:String}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct StoredRun { start_millis: u64, metadata: local_person_sequence::Metadata }
/// Server-owned receipt. Persist this identity with project state; never accept client paths.
/// Dropping a receipt or restored cache does not delete published files.
#[derive(Clone)]
pub struct PublishedCache {
    pub model_hash:String,
    pub directory: PathBuf,
    pub manifest_hash: String,
    pub video_hash: String,
    pub runtime_hash: String,
}
struct PublishedPrefix{receipt:PublishedCache,run_count:usize,total_bytes:u64}
pub(crate) struct PreparedPublication{pub receipt:PublishedCache,prefix:PublishedPrefix,owner:uuid::Uuid}
impl RunCache {
    /// Standalone full generation, preserving the original schema and file layout.
    pub async fn publish(&mut self,root:&Path,budget:u64,cancel:&CancellationToken)->Result<PublishedCache>{
        Ok(self.prepare_generation(root,budget,cancel,0,None,0).await?.receipt)
    }
    /// Prepare only newly appended runs. The caller accepts this cursor only
    /// after project registration succeeds, so failed commits cannot lose data.
    pub(crate) async fn prepare_incremental(&mut self,root:&Path,budget:u64,cancel:&CancellationToken)->Result<PreparedPublication>{
        if let Some(prefix)=&self.published{
            ensure!(prefix.run_count<=self.runs.len(),"invalid published cache prefix");
            if prefix.run_count<self.runs.len()&&prefix.receipt.directory.parent()==Some(root){
                let generation=prefix.receipt.directory.file_name().and_then(|s|s.to_str()).ok_or_else(||anyhow::anyhow!("invalid parent cache generation"))?.to_owned();
                valid_generation(&generation)?;
                let parent=CacheParent{generation,manifest_hash:prefix.receipt.manifest_hash.clone()};
                let first=prefix.run_count;let base=prefix.total_bytes;
                return self.prepare_generation(root,budget,cancel,first,Some(parent),base).await;
            }
        }
        // Explicit unchanged re-publication remains a fresh full generation.
        self.prepare_generation(root,budget,cancel,0,None,0).await
    }
    pub(crate) fn accept_publication(&mut self,publication:PreparedPublication)->Result<()>{
        ensure!(publication.owner==self.publication_id&&publication.prefix.run_count==self.runs.len(),"cache changed before accepting publication");
        self.published=Some(publication.prefix);Ok(())
    }
    async fn prepare_generation(&mut self,root:&Path,budget:u64,cancel:&CancellationToken,first:usize,parent:Option<CacheParent>,base:u64)->Result<PreparedPublication>{
        use tokio::io::AsyncWriteExt;
        ensure!(root.is_absolute()&&root.is_dir()&&budget>0&&budget<=64*1024*1024*1024&&!cancel.is_cancelled()&&first<=self.runs.len(),"invalid video cache publication");
        let manifest=CacheManifest{model_hash:self.model_hash.clone(),schema_version:if parent.is_some(){2}else{1},video_hash:self.video_hash.clone(),runtime_hash:self.runtime_hash.clone(),total_runs:parent.as_ref().map(|_|self.runs.len()),parent,runs:self.runs[first..].iter().map(|run|StoredRun{start_millis:run.start_millis,metadata:run.metadata.clone()}).collect()};
        let bytes=serde_json::to_vec(&manifest)?;ensure!(bytes.len()as u64<=MAX_CACHE_MANIFEST,"video cache manifest too large");
        let total=self.runs[first..].iter().try_fold(base.checked_add(bytes.len()as u64).ok_or_else(||anyhow::anyhow!("cache budget overflow"))?,|n,run|n.checked_add(run.metadata.mask_bytes).ok_or_else(||anyhow::anyhow!("cache size overflow")))?;ensure!(total<=budget,"video cache publication budget exceeded");
        let directory=root.join(format!("concat-video-cache-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let mut stage=Stage(directory.clone());
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
        for(index,run)in self.runs[first..].iter_mut().enumerate(){
            let path=directory.join(format!("{index}.masks"));let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{options.mode(0o600);}let mut file=options.open(&path).await?;
            for frame in 0..run.metadata.frames{let mask=run.masks.read(run.start_millis+frame*100,cancel)?;file.write_all(&mask.probabilities).await?;}
            file.sync_all().await?;drop(file);ensure!(super::local_enhance_video::fingerprint(&path).await?==(run.metadata.mask_bytes,run.metadata.mask_hash.clone()),"published video mask identity mismatch");
        }
        ensure!(!cancel.is_cancelled(),"video cache publication cancelled");let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{options.mode(0o600);}let mut file=options.open(directory.join("manifest.json")).await?;file.write_all(&bytes).await?;file.sync_all().await?;drop(file);ensure!(!cancel.is_cancelled(),"video cache publication cancelled");
        #[cfg(unix)]{tokio::fs::File::open(&directory).await?.sync_all().await?;tokio::fs::File::open(root).await?.sync_all().await?;}
        use sha2::Digest;let receipt=PublishedCache{model_hash:self.model_hash.clone(),directory,manifest_hash:format!("{:x}",sha2::Sha256::digest(&bytes)),video_hash:self.video_hash.clone(),runtime_hash:self.runtime_hash.clone()};stage.0=PathBuf::new();
        Ok(PreparedPublication{prefix:PublishedPrefix{receipt:receipt.clone(),run_count:self.runs.len(),total_bytes:total},receipt,owner:self.publication_id})
    }
    /// Iterative restoration validates the complete parent chain before exposure.
    pub async fn restore(receipt:&PublishedCache,budget:u64,cancel:&CancellationToken)->Result<Self>{
        ensure!(receipt.directory.is_absolute()&&budget>0&&budget<=64*1024*1024*1024&&!cancel.is_cancelled(),"invalid video cache restore");
        let root=receipt.directory.parent().ok_or_else(||anyhow::anyhow!("cache root missing"))?;let mut directory=receipt.directory.clone();let mut hash=receipt.manifest_hash.clone();let mut visited=std::collections::HashSet::new();let mut chunks=vec![];let mut total=0u64;let mut expected_count=None;
        loop{
            ensure!(!cancel.is_cancelled()&&visited.len()<=360000&&visited.insert(directory.clone()),"cancelled/cyclic/oversized cache parent chain");
            let(manifest,length)=read_manifest(&directory,&hash,budget.saturating_sub(total)).await?;
            ensure!(manifest.model_hash==receipt.model_hash&&manifest.video_hash==receipt.video_hash&&manifest.runtime_hash==receipt.runtime_hash&&manifest.runs.len()<=360000,"video cache provenance mismatch");
            let count=match manifest.schema_version{1=>{ensure!(manifest.parent.is_none()&&manifest.total_runs.is_none(),"invalid legacy cache parent");manifest.runs.len()},2=>{ensure!(manifest.parent.is_some()&&!manifest.runs.is_empty(),"invalid incremental cache parent/runs");manifest.total_runs.ok_or_else(||anyhow::anyhow!("incremental run count missing"))?},_=>anyhow::bail!("unsupported video cache schema")};
            ensure!(count<=360000&&count>=manifest.runs.len()&&expected_count.is_none_or(|n|n==count),"video cache parent coverage mismatch");
            total=manifest.runs.iter().try_fold(total.checked_add(length).ok_or_else(||anyhow::anyhow!("cache manifest budget overflow"))?,|n,run|n.checked_add(run.metadata.mask_bytes).ok_or_else(||anyhow::anyhow!("cache restore size overflow")))?;ensure!(total<=budget,"video cache restore budget exceeded");
            expected_count=Some(count-manifest.runs.len());let parent=manifest.parent;chunks.push((directory,manifest.runs));
            if let Some(parent)=parent{valid_generation(&parent.generation)?;directory=root.join(parent.generation);hash=parent.manifest_hash;}else{ensure!(expected_count==Some(0),"incomplete video cache parent coverage");break}
        }
        let mut cache=Self::new_for_model(receipt.video_hash.clone(),receipt.runtime_hash.clone(),receipt.model_hash.clone())?;
        for(directory,runs)in chunks.into_iter().rev(){for(index,run)in runs.into_iter().enumerate(){let masks=concat_video_masks::open(&directory.join(format!("{index}.masks")),run.start_millis,&run.metadata,ExpectedRun{decoded_source_hash:&run.metadata.source_hash,model_hash:&cache.model_hash,runtime_hash:&cache.runtime_hash},cancel).await?;cache.insert(AnalysedRun{masks,metadata:run.metadata,video_hash:cache.video_hash.clone(),start_millis:run.start_millis,_stage:Stage(PathBuf::new())})?;}}
        ensure!(!cancel.is_cancelled(),"video cache restore cancelled");cache.published=Some(PublishedPrefix{receipt:receipt.clone(),run_count:cache.runs.len(),total_bytes:total});Ok(cache)
    }
}
fn valid_generation(value:&str)->Result<()>{let suffix=value.strip_prefix("concat-video-cache-").ok_or_else(||anyhow::anyhow!("invalid parent cache generation"))?;ensure!(uuid::Uuid::parse_str(suffix)?.to_string()==suffix,"invalid parent cache generation");Ok(())}
async fn read_manifest(directory:&Path,hash:&str,budget:u64)->Result<(CacheManifest,u64)>{
    use tokio::io::AsyncReadExt;use sha2::Digest;
    ensure!(tokio::fs::symlink_metadata(directory).await?.is_dir(),"video cache directory must be real");let path=directory.join("manifest.json");ensure!(tokio::fs::symlink_metadata(&path).await?.is_file(),"video cache manifest must be regular");let file=tokio::fs::File::open(path).await?;let mut bytes=vec![];file.take(MAX_CACHE_MANIFEST.min(budget)+1).read_to_end(&mut bytes).await?;ensure!(bytes.len()as u64<=MAX_CACHE_MANIFEST.min(budget),"video cache manifest budget exceeded");ensure!(format!("{:x}",sha2::Sha256::digest(&bytes))==hash,"video cache manifest identity mismatch");Ok((serde_json::from_slice(&bytes)?,bytes.len()as u64))
}



/// Trusted per-stroke planes for one exact decoded source instant.
/// Providers must verify encoded-region ownership and content before construction.
pub struct SourceRegions {
    pub video_hash:String,
    pub source_us:u64,
    pub planes:Vec<Option<std::sync::Arc<crate::domain::concat_cutout_clock::ResolvedMask>>>,
}

impl RunCache {
    /// Filter an unprepared decoded source frame at its actual source timestamp.
    /// The renderer subsequently performs crop/flips, transforms and clip effects.
    pub fn apply_decoded(&mut self, frame: &mut crate::domain::group_surface::GroupSurface, source_us: u64, cancel: &CancellationToken) -> Result<crate::domain::concat_cutout_clock::Dispatch> {
        self.apply_decoded_with_feather(frame,source_us,0.,cancel)
    }
    pub fn apply_decoded_with_feather(&mut self,frame:&mut crate::domain::group_surface::GroupSurface,source_us:u64,feather:f64,cancel:&CancellationToken)->Result<crate::domain::concat_cutout_clock::Dispatch>{
        self.apply_decoded_with_correction(frame,source_us,&crate::domain::cutout::Correction{custom:false,strokes:vec![],feather},cancel)
    }
    /// Resolve source-coordinate authored strokes before source preparation.
    /// Attached learned regions require a source-time region provider.
    pub fn apply_decoded_with_correction(&mut self,frame:&mut crate::domain::group_surface::GroupSurface,source_us:u64,correction:&crate::domain::cutout::Correction,cancel:&CancellationToken)->Result<crate::domain::concat_cutout_clock::Dispatch>{
        crate::domain::concat_video_binding::validate_correction(correction)?;
        self.apply_decoded_with_regions(frame,source_us,correction,None,cancel)
    }
    /// Region planes are resolved by an authorized provider for this exact source instant.
    /// A lost track is an explicit zero plane; a missing timed region is not a seed mask.
    pub fn apply_decoded_with_regions(&mut self,frame:&mut crate::domain::group_surface::GroupSurface,source_us:u64,correction:&crate::domain::cutout::Correction,regions:Option<&SourceRegions>,cancel:&CancellationToken)->Result<crate::domain::concat_cutout_clock::Dispatch>{
        use crate::domain::{concat_cutout_clock::Dispatch, concat_cutout_mask::{apply, MaskPlane}, concat_picture::Preparation};
        correction.validate()?;
        if let Some(regions)=regions{
            ensure!(regions.video_hash==self.video_hash&&regions.source_us==source_us,"video regions source/clock mismatch");
            ensure!(regions.planes.len()==correction.strokes.len(),"video regions stroke count mismatch");
            for (stroke,region) in correction.strokes.iter().zip(&regions.planes){
                let smart=matches!(stroke.tool,crate::domain::cutout::Tool::SmartBrush|crate::domain::cutout::Tool::SmartEraser);
                ensure!(smart||region.is_none(),"region attached to ordinary video brush");
                ensure!(!(smart&&stroke.at.is_some())||region.is_some(),"timed smart video region missing");
            }
        }else{crate::domain::concat_video_binding::validate_correction(correction)?;}
        let planes=regions.map(|r|r.planes.as_slice()).unwrap_or(&[]);
        ensure!(frame.width>0&&frame.height>0,"invalid video correction source dimensions");
        ensure!(!cancel.is_cancelled(), "video cutout render cancelled");
        let Some(mask_millis) = self.timeline()?.nearest_millis(source_us as f64 / 1_000_000.)? else { return Ok(Dispatch::Unanalysed { source_us }); };
        let index=*self.frames.get(&mask_millis).ok_or_else(||anyhow::anyhow!("video cache mask missing"))?;
        // Keep blank automatic planes: custom strokes may deliberately paint or erase them.
        let automatic=self.runs[index].masks.read(mask_millis,cancel)?;
        let plan=crate::domain::concat_cutout_import::Plan{subject:crate::domain::concat_cutout_import::Subject::Auto,correction:correction.clone(),region_source_seconds:vec![]};
        let Some(mask)=crate::domain::concat_cutout_resolve::resolve(&plan,&automatic,planes,frame.width as f32/frame.height as f32,&std::sync::atomic::AtomicBool::new(cancel.is_cancelled()))? else{return Ok(Dispatch::Missing{source_us,mask_millis})};
        let geometry = Preparation { crop: [0.;4], canvas: [frame.width,frame.height], flip_h: false, flip_v: false };
        apply(frame, MaskPlane { width: mask.width, height: mask.height, probabilities: &mask.probabilities }, &geometry, &mut || { ensure!(!cancel.is_cancelled(), "video cutout render cancelled"); Ok(()) })?;
        Ok(Dispatch::Applied { source_us, mask_millis })
    }
}

/// Import one independently validated completed frame without resetting the
/// native producer's recurrence. Caller owns encoded-source ACL and decode proof.
pub async fn import_checkpoint(directory:&Path,expected:super::concat_video_checkpoint::Expected<'_>,video_hash:&str,start_millis:u64,root:&Path,cancel:&CancellationToken,limit:Duration)->Result<AnalysedRun>{
    ensure!(video_hash.len()==64&&video_hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))&&start_millis%100==0,"invalid checkpoint video identity/grid");
    let verified=super::concat_video_checkpoint::verify(directory,&expected,root,cancel,limit).await?;
    let start_millis=start_millis.checked_add(verified.ordinal.checked_mul(100).ok_or_else(||anyhow::anyhow!("checkpoint clock overflow"))?).ok_or_else(||anyhow::anyhow!("checkpoint clock overflow"))?;
    let stage=Stage(root.join(format!("concat-video-analysis-{}",uuid::Uuid::new_v4())));tokio::fs::create_dir(&stage.0).await?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&stage.0,std::fs::Permissions::from_mode(0o700)).await?;}
    let path=stage.0.join("masks.raw");{use tokio::io::AsyncWriteExt;let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{options.mode(0o600);}let mut file=options.open(&path).await?;file.write_all(&verified.mask).await?;file.flush().await?;}
    let masks=concat_video_masks::open(&path,start_millis,&verified.metadata,ExpectedRun{decoded_source_hash:&verified.metadata.source_hash,model_hash:expected.model_hash,runtime_hash:expected.runtime_hash},cancel).await?;
    Ok(AnalysedRun{masks,metadata:verified.metadata,video_hash:video_hash.into(),start_millis,_stage:stage})
}

/// Deliver independently verified completed frames while one native recurrence
/// is live. On cancellation/error, already delivered leases remain caller-owned.
/// This future must be awaited to terminal before the caller releases staging.
pub async fn stream_checkpoints(
    runtime:&ProcessRuntime,engine:local_person_sequence::Engine<'_>,sampled:SampledRun,
    root:&Path,cancel:&CancellationToken,limit:Duration,
    progress:&tokio::sync::mpsc::UnboundedSender<f64>,frames:&tokio::sync::mpsc::Sender<AnalysedRun>,
)->Result<u64>{
    use tokio::io::{AsyncReadExt,AsyncSeekExt};
    ensure!(root.is_absolute()&&root.is_dir()&&!limit.is_zero()&&limit<=Duration::from_secs(3600)&&!cancel.is_cancelled(),"invalid progressive analysis root/deadline");
    let deadline=tokio::time::Instant::now()+limit;let identity=engine.identity().await?;
    let stage=Stage(root.join(format!("concat-progressive-{}",uuid::Uuid::new_v4())));tokio::fs::create_dir(&stage.0).await?;
    let checkpoints=stage.0.join("checkpoints");tokio::fs::create_dir(&checkpoints).await?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;for path in [&stage.0,&checkpoints]{tokio::fs::set_permissions(path,std::fs::Permissions::from_mode(0o700)).await?;}}
    let output=stage.0.join("result.rgba");let masks=stage.0.join("masks.raw");let(events,mut receiver)=tokio::sync::mpsc::unbounded_channel();
    let operation_cancel=cancel.child_token();
    let operation=local_person_sequence::segment_with_checkpoints(runtime,engine,local_person_sequence::Input{source:&sampled.rgba,source_hash:&sampled.rgba_hash,mask_destination:Some(&masks),width:sampled.width,height:sampled.height,frames:sampled.frames},&output,&operation_cancel,super::local_enhance_video::remaining(deadline,cancel)?,progress,Some(local_person_sequence::CheckpointOutput{directory:&checkpoints,completed:&events}));tokio::pin!(operation);
    let mut input=tokio::fs::File::open(&sampled.rgba).await?;let size=u64::from(sampled.width)*u64::from(sampled.height)*4;let mut pixels=vec![0u8;usize::try_from(size)?];let mut delivered=0u64;let mut failure=None;
    // Completed native frames can be drained after cancellation under a separate
    // bounded verification token. Never detach/drop the native operation.
    let verify_cancel=CancellationToken::new();
    let mut terminal=None;let mut drain_deadline=None;
    loop{
        let ordinal=if terminal.is_some(){receiver.try_recv().ok()}else{tokio::select!{biased;event=receiver.recv()=>event,result=&mut operation=>{terminal=Some(result);receiver.try_recv().ok()}}};
        let Some(ordinal)=ordinal else{if terminal.is_some(){break}else{continue}};
        if failure.is_some(){continue}
        if cancel.is_cancelled()||tokio::time::Instant::now()>=deadline{drain_deadline.get_or_insert_with(||tokio::time::Instant::now()+Duration::from_secs(30));}
        let verification_deadline=drain_deadline.unwrap_or(deadline).min(tokio::time::Instant::now()+Duration::from_secs(30));
        let imported=async{
            ensure!(tokio::time::Instant::now()<verification_deadline,"checkpoint drain deadline exceeded");
            ensure!(ordinal==delivered,"out-of-order verified checkpoint delivery");
            input.seek(std::io::SeekFrom::Start(ordinal.checked_mul(size).ok_or_else(||anyhow::anyhow!("checkpoint input offset overflow"))?)).await?;input.read_exact(&mut pixels).await?;
            let frame=import_checkpoint(&checkpoints,super::concat_video_checkpoint::Expected{sequence_hash:&sampled.rgba_hash,model_hash:engine.model_hash(),runtime_hash:&identity.runtime_hash,source_frame:&pixels,width:sampled.width,height:sampled.height,ordinal,total:sampled.frames},&sampled.video_hash,sampled.start_millis,root,&verify_cancel,verification_deadline.saturating_duration_since(tokio::time::Instant::now())).await?;
            tokio::time::timeout_at(verification_deadline,frames.send(frame)).await?.map_err(|_|anyhow::anyhow!("checkpoint consumer closed"))?;
            anyhow::Ok(())
        }.await;
        match imported{Ok(())=>delivered+=1,Err(error)=>{failure=Some(error);operation_cancel.cancel();}}
    }
    // The select loop observed the operation terminal, including owned kill/reap.
    if let Some(error)=failure{return Err(error)}
    terminal.ok_or_else(||anyhow::anyhow!("missing checkpoint producer status"))??;
    ensure!(delivered==sampled.frames,"incomplete checkpoint delivery");Ok(delivered)
}
