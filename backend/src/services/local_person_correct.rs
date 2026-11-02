//! Model-free cached video-mask correction through the owned ProcessRuntime.
//! Caller owns source ACL, Analysis permit and raw decode/encode staging.
use super::local_mask_edit::identity;
use super::local_person_sequence_contract::{layout, verify_frames_with_fallbacks, MAX_BYTES};
pub use crate::domain::video_cutout::FrameCorrection;
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub mask: &'a Path,
    pub mask_hash: &'a str,
    pub mask_destination: Option<&'a Path>,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub corrections: &'a [FrameCorrection],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub frames: u64,
    pub bytes: u64,
    pub mask_bytes: u64,
    pub blank_frames: u64,
    pub source_hash: String,
    pub source_mask_hash: String,
    pub correction_hash: String,
    pub mask_hash: String,
    pub output_hash: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub region_hashes: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_source_hash: Option<String>,
}
pub(crate) fn validate_edits(edits: &[FrameCorrection], frames: u64) -> Result<String> {
    crate::domain::video_cutout::validate_corrections(edits, frames)
}
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
pub async fn correct(
    runtime: &ProcessRuntime,
    executable: &Path,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    correct_inner(
        runtime,
        executable,
        input,
        None,
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
pub async fn correct_with_regions(
    runtime: &ProcessRuntime,
    executable: &Path,
    input: Input<'_>,
    regions: &super::local_person_regions::Context<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    correct_inner(
        runtime,
        executable,
        input,
        Some(regions),
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn correct_inner(
    runtime: &ProcessRuntime,
    executable: &Path,
    input: Input<'_>,
    regions: Option<&super::local_person_regions::Context<'_>>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        input.source.is_absolute() && input.mask.is_absolute() && destination.is_absolute(),
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
    let correction_hash = match regions {
        Some(regions) => super::local_person_regions::validate(&input, regions)?,
        None => validate_edits(input.corrections, input.frames)?,
    };
    let prepare = async {
        let before = identity(executable).await?;
        regular(input.source, MAX_BYTES).await?;
        regular(input.mask, MAX_BYTES).await?;
        let source_before =
            crate::project_archive::fingerprint_file_with_limit(input.source, MAX_BYTES).await?;
        let mask_before =
            crate::project_archive::fingerprint_file_with_limit(input.mask, MAX_BYTES).await?;
        if let Some(regions) = regions {
            super::local_person_regions::verify(&input, regions).await?;
        }
        anyhow::Ok((before, source_before, mask_before))
    };
    let (before, source_before, mask_before) = tokio::select! {_=cancel.cancelled()=>anyhow::bail!("sequence cancelled"),result=tokio::time::timeout_at(deadline,prepare)=>result??};
    ensure!(
        source_before.1 == input.source_hash,
        "source digest mismatch"
    );
    let (w, h) = (input.width, input.height);
    let (mask_width, mask_height, bytes, mask_bytes) = layout(w, h, input.frames)?;
    let (width, height) = (w, h);
    ensure!(
        mask_before.0 == mask_bytes && mask_before.1 == input.mask_hash,
        "original mask identity mismatch"
    );
    ensure!(
        source_before.0 == u64::from(w) * u64::from(h) * 4 * input.frames,
        "raw sequence size mismatch"
    );
    let directory = parent.join(format!(".person-correct-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let work = Owned(directory);
    let result = work.0.join("result.rgba");
    let request = work.0.join("request.json");
    let masks = work.0.join("masks.raw");
    let staging = async {
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
        let mut payload = serde_json::json!({"schemaVersion":8,"input":input.source,"mask":input.mask,"output":result,"maskOutput":masks,"sourceSha256":input.source_hash,"maskSha256":input.mask_hash,"width":w,"height":h,"frames":input.frames,"corrections":input.corrections});
        if let Some(regions) = regions {
            payload["regions"] = super::local_person_regions::payload(regions);
            payload["regionSourceSha256"] = serde_json::json!(regions.encoded_source_hash);
        }
        let request_bytes = serde_json::to_vec(&payload)?;
        ensure!(
            request_bytes.len() <= 4 * 1024 * 1024,
            "correction request budget"
        );
        file.write_all(&request_bytes).await?;
        file.sync_all().await?;
        drop(file);
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("correction staging cancelled"),result=tokio::time::timeout_at(deadline,staging)=>result??};
    let metadata = Mutex::new((None, false));
    let maximum = Mutex::new(0f64);
    let parse = |line: &str| {
        if let Ok(value) = serde_json::from_str::<Metadata>(line) {
            let mut state = metadata.lock().ok()?;
            if state.0.is_some() {
                state.1 = true
            }
            state.0 = Some(value);
            return None;
        }
        if serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .is_some_and(|v| v.get("schemaVersion").is_some())
        {
            metadata.lock().ok()?.1 = true;
            return None;
        }
        let value = serde_json::from_str::<Progress>(line).ok()?;
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
    let mut command = tokio::process::Command::new(executable);
    command.arg("--person-correct").arg(&request);
    // Always await kill/reap before releasing owned staging.
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            vec![
                executable.to_owned(),
                input.mask.to_owned(),
                input.source.to_owned(),
                work.0.clone(),
            ]
            .into_iter()
            .chain(
                regions
                    .into_iter()
                    .flat_map(|c| c.regions.iter().map(|r| r.path.to_owned())),
            )
            .collect(),
            &result,
        ),
        parse,
        progress,
        cancel,
        remaining,
    )
    .await?;
    ensure!(
        status == ProcessStatus::Ok && !cancel.is_cancelled(),
        "person sequence did not complete"
    );
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
        // Original automatic blank fallback differs from an authored erase-all.
        let mut original = tokio::fs::File::open(input.mask).await?;
        let mut corrected = tokio::fs::File::open(&masks).await?;
        let mut auto = vec![0u8; mask_width as usize * mask_height as usize];
        let mut changed = auto.clone();
        let mut fallbacks = Vec::with_capacity(input.frames as usize);
        let edit_index: std::collections::BTreeMap<_, _> =
            input.corrections.iter().map(|e| (e.frame, e)).collect();
        for ordinal in 0..input.frames {
            ensure!(
                !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
                "correction verification expired"
            );
            original.read_exact(&mut auto).await?;
            corrected.read_exact(&mut changed).await?;
            let edit = edit_index.get(&ordinal);
            if edit.is_none() {
                ensure!(auto == changed, "unedited mask frame changed");
            }
            fallbacks.push(
                auto.iter().all(|v| *v < 10)
                    && edit.is_none_or(|e| !e.correction.custom || e.correction.strokes.is_empty()),
            );
        }
        let base = super::local_person_sequence_contract::Input {
            source: input.source,
            source_hash: input.source_hash,
            mask_destination: None,
            width: w,
            height: h,
            frames: input.frames,
        };
        let (output_hash, mask_hash, blanks) = verify_frames_with_fallbacks(
            &base,
            &result,
            &masks,
            mask_width,
            mask_height,
            cancel,
            deadline,
            Some(&fallbacks),
        )
        .await?;
        ensure!(
            state.schema_version == 8
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
                && state.source_mask_hash == input.mask_hash
                && state.correction_hash == correction_hash
                && state.output_hash == output_hash
                && state.region_hashes
                    == regions
                        .map(super::local_person_regions::hashes)
                        .unwrap_or_default()
                && state.region_source_hash.as_deref() == regions.map(|r| r.encoded_source_hash),
            "sequence result identity mismatch"
        );
        let after = identity(executable).await?;
        ensure!(
            before == after
                && source_before
                    == crate::project_archive::fingerprint_file_with_limit(input.source, MAX_BYTES)
                        .await?,
            "sequence source/runtime changed"
        );
        ensure!(
            mask_before
                == crate::project_archive::fingerprint_file_with_limit(input.mask, MAX_BYTES)
                    .await?,
            "cached masks changed"
        );
        if let Some(regions) = regions {
            super::local_person_regions::verify(&input, regions).await?;
        }
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("sequence cancelled"),result=tokio::time::timeout_at(deadline,validate)=>result??};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "person sequence cancelled or expired"
    );
    struct Published(Vec<PathBuf>);
    impl Drop for Published {
        fn drop(&mut self) {
            for path in &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut published = Published(Vec::new());
    if let Some(path) = input.mask_destination {
        tokio::fs::hard_link(&masks, path).await?;
        published.0.push(path.to_owned());
    }
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "person publication cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish person sequence without replacement")?;
    published.0.push(destination.to_owned());
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "person publication cancelled or expired"
    );
    published.0.clear();
    let _ = progress.send(100.);
    Ok(state)
}
