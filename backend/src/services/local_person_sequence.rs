//! Host adapter for one loaded model over a raw video-frame sequence.
//! Caller owns source ACL, Analysis permit and raw decode/encode staging.
use super::local_person::{identity, Config};
use super::local_person_sequence_contract::{verify_frames, MAX_BYTES};
pub use super::local_person_sequence_contract::{Input, Metadata};
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
const MODEL_HASH: &str = super::local_person::MODEL_HASH;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    event: String,
    completed: u64,
    total: u64,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn regular(path: &Path, limit: u64) -> Result<()> {
    let m = tokio::fs::symlink_metadata(path).await?;
    ensure!(
        m.file_type().is_file() && m.len() <= limit,
        "invalid bounded sequence file"
    );
    Ok(())
}
/// A single loaded model for an ordered decoded source run.
#[derive(Clone,Copy)]
pub enum Engine<'a>{Person(&'a Config),Selfie(&'a super::local_selfie::Config),Object(&'a super::local_cutout::Config)}
impl Engine<'_>{
 fn config(&self)->Config{match self{Self::Person(c)=>(*c).clone(),Self::Selfie(c)=>Config{executable:c.executable.clone(),model:c.model.clone(),runtime_library:c.runtime_library.clone()},Self::Object(c)=>Config{executable:c.executable.clone(),model:c.model.clone(),runtime_library:c.runtime_library.clone()}}}
 fn family(&self)->Option<&'static str>{match self{Self::Person(_)=>None,Self::Selfie(_)=>Some("selfie"),Self::Object(_)=>Some("object")}}
 pub(crate) fn model_hash(&self)->&'static str{match self{Self::Person(_)=>MODEL_HASH,Self::Selfie(_)=>super::local_selfie::MODEL_HASH,Self::Object(_)=>super::local_cutout::MODEL_HASH}}
 pub(crate) async fn identity(&self)->Result<super::local_person::Identity>{match self{Self::Person(c)=>identity(c).await,Self::Selfie(c)=>{let i=super::local_selfie::identity(c).await?;Ok(super::local_person::Identity{hash:i.hash,runtime_hash:i.runtime_hash})},Self::Object(c)=>{let i=super::local_cutout::identity(c).await?;Ok(super::local_person::Identity{hash:i.hash,runtime_hash:i.runtime_hash})}}}
}
pub async fn segment(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    segment_with_engine(runtime,Engine::Person(config),input,destination,cancel,limit,progress).await
}

pub async fn segment_with_engine(
    runtime: &ProcessRuntime,
    engine: Engine<'_>,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    segment_with_checkpoints(runtime,engine,input,destination,cancel,limit,progress,None).await
}
/// Caller owns this private directory until the native child has been reaped.
/// Notifications identify committed frames; they are not verified mask data.
pub struct CheckpointOutput<'a>{pub directory:&'a Path,pub completed:&'a tokio::sync::mpsc::UnboundedSender<u64>}
pub async fn segment_with_checkpoints(
    runtime:&ProcessRuntime,engine:Engine<'_>,input:Input<'_>,destination:&Path,
    cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>,
    checkpoints:Option<CheckpointOutput<'_>>,
)->Result<Metadata>{
    let config=engine.config();let schema=if engine.family().is_some(){10}else{7};let model_hash=engine.model_hash();

    ensure!(
        input.source.is_absolute() && destination.is_absolute(),
        "invalid person sequence paths"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(3600) && !cancel.is_cancelled(),
        "person sequence cancelled or invalid deadline"
    );
    ensure!(!destination.try_exists()?, "output already exists");
    if let Some(mask) = input.mask_destination {
        ensure!(
            mask.is_absolute() && mask != destination && !mask.try_exists()?,
            "invalid person mask destination"
        );
    }
    let parent = destination.parent().context("output parent missing")?;
    ensure!(parent.is_dir(), "output directory missing");
    let deadline = tokio::time::Instant::now() + limit;
    let prepare = async {
        let before = engine.identity().await?;
        regular(input.source, MAX_BYTES).await?;
        let source_before =
            crate::project_archive::fingerprint_file_with_limit(input.source, MAX_BYTES).await?;
        anyhow::Ok((before, source_before))
    };
    let (before, source_before) = tokio::select! {_=cancel.cancelled()=>anyhow::bail!("sequence cancelled"),result=tokio::time::timeout_at(deadline,prepare)=>result??};
    ensure!(
        source_before.1 == input.source_hash,
        "source digest mismatch"
    );
    let (w, h) = (input.width, input.height);
    let (mask_width, mask_height, bytes, mask_bytes) = super::local_person_sequence_contract::layout_for(model_hash,w,h,input.frames)?;
    if let Some(checkpoints)=&checkpoints{
        ensure!(checkpoints.directory.is_absolute()&&bytes.checked_add(mask_bytes).is_some_and(|n|n<=MAX_BYTES),"invalid checkpoint path/staging budget");
        let metadata=tokio::fs::symlink_metadata(checkpoints.directory).await?;ensure!(metadata.is_dir()&&tokio::fs::read_dir(checkpoints.directory).await?.next_entry().await?.is_none(),"checkpoint root must be an empty real directory");
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;ensure!(metadata.permissions().mode()&0o777==0o700,"checkpoint root is not private");}
    }
    let (width, height) = (w, h);
    ensure!(
        source_before.0 == u64::from(w) * u64::from(h) * 4 * input.frames,
        "raw sequence size mismatch"
    );
    let work = Owned(parent.join(format!(".person-sequence-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let result = work.0.join("result.rgba");
    let request = work.0.join("request.json");
    let masks = work.0.join("masks.raw");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&request)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await?;
    }
    let mut request_value=serde_json::json!({"schemaVersion":schema,"model":config.model,"runtimeLibrary":config.runtime_library,"input":input.source,"output":result,"maskOutput":masks,"sourceSha256":input.source_hash,"width":w,"height":h,"frames":input.frames});
    if let Some(family)=engine.family(){request_value["family"]=serde_json::json!(family);}
    if let Some(checkpoints)=&checkpoints{request_value["checkpointOutput"]=serde_json::json!(checkpoints.directory);}
    file.write_all(&serde_json::to_vec(&request_value)?).await?;
    file.sync_all().await?;
    drop(file);
    let metadata = Mutex::new((None, false));
    let maximum = Mutex::new(0f64);
    let completed=Mutex::new(0u64);
    let process_cancel=cancel.child_token();
    let parse = |line: &str| {
        if let Ok(value) = serde_json::from_str::<Metadata>(line) {
            let mut state = metadata.lock().ok()?;
            if state.0.is_some() {
                state.1 = true
            }
            state.0 = Some(value);
            return None;
        }
        let value = serde_json::from_str::<Progress>(line).ok()?;
        if value.event=="checkpoint"{
            if let Some(checkpoints)=&checkpoints{
                let mut seen=completed.lock().ok()?;
                if value.total!=input.frames||value.completed!=*seen+1||value.completed>input.frames||checkpoints.completed.send(value.completed-1).is_err(){process_cancel.cancel();return None;}
                *seen=value.completed;
            }
            return None;
        }
        if value.event != "progress"
            || value.total == 0
            || value.total != input.frames
            || value.completed > value.total
        {
            return None;
        }
        let mut maximum = maximum.lock().ok()?;
        *maximum = maximum.max(value.completed as f64 / value.total as f64 * 95.);
        Some(*maximum)
    };
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    ensure!(
        !remaining.is_zero() && !cancel.is_cancelled(),
        "person sequence cancelled or expired"
    );
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg(if engine.family().is_some(){"--cutout-sequence"}else{"--person-sequence"}).arg(&request);
    // Always await kill/reap before releasing owned staging.
    let mut reads=vec![config.model.clone(),config.runtime_library.clone(),input.source.to_owned(),work.0.clone()];
    if let Some(checkpoints)=&checkpoints{reads.push(checkpoints.directory.to_owned());}
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            reads,
            &result,
        ),
        parse,
        progress,
        &process_cancel,
        remaining,
    )
    .await?;
    ensure!(
        status == ProcessStatus::Ok && !cancel.is_cancelled(),
        "person sequence did not complete"
    );
    if checkpoints.is_some(){ensure!(*completed.lock().map_err(|_|anyhow::anyhow!("invalid checkpoint notification state"))?==input.frames,"missing native frame checkpoints");}
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid person sequence metadata"))?;
    ensure!(!duplicate, "duplicate person sequence metadata");
    let state = state.context("missing person sequence metadata")?;
    let validate = async {
        regular(&result, MAX_BYTES).await?;
        regular(&masks, MAX_BYTES).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&result, &masks] {
                ensure!(
                    tokio::fs::metadata(path).await?.permissions().mode() & 0o777 == 0o600,
                    "person result is not private"
                );
            }
        }
        let (output_hash, mask_hash, blanks) = verify_frames(
            &input,
            &result,
            &masks,
            mask_width,
            mask_height,
            cancel,
            deadline,
        )
        .await?;
        ensure!(
            state.schema_version == schema
                && state.frames == input.frames
                && state.bytes == bytes
                && state.mask_width == mask_width
                && state.mask_height == mask_height
                && state.mask_bytes == mask_bytes
                && state.mask_hash == mask_hash
                && state.blank_frames == blanks
                && state.width == width
                && state.height == height
                && state.source_hash == input.source_hash
                && state.model_hash == model_hash
                && state.runtime_hash == before.runtime_hash
                && state.output_hash == output_hash,
            "sequence result identity mismatch"
        );
        let after = engine.identity().await?;
        ensure!(
            before.hash == after.hash
                && source_before
                    == crate::project_archive::fingerprint_file_with_limit(input.source, MAX_BYTES)
                        .await?,
            "sequence source/runtime changed"
        );
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("sequence cancelled"),result=tokio::time::timeout_at(deadline,validate)=>result??};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "person sequence cancelled or expired"
    );
    struct PublishedMask(Option<PathBuf>);
    impl Drop for PublishedMask {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut published = PublishedMask(None);
    if let Some(path) = input.mask_destination {
        tokio::fs::hard_link(&masks, path).await?;
        published.0 = Some(path.to_owned());
    }
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "person publication cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish person sequence without replacement")?;
    published.0 = None;
    let _ = progress.send(100.);
    Ok(state)
}
