//! Host adapter for one loaded model over a raw video-frame sequence.
//! Caller owns source ACL, Analysis permit and raw decode/encode staging.
use super::local_enhance::{identity, Config};
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MODEL_HASH: &str = super::enhance_model::SHA256;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub factor: u32,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub factor: u32,
    pub frames: u64,
    pub bytes: u64,
    pub source_hash: String,
    pub model_hash: String,
    pub runtime_hash: String,
    pub output_hash: String,
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
fn layout(w: u32, h: u32, frames: u64, factor: u32) -> Result<(u32, u32, u64)> {
    ensure!(
        w > 0
            && h > 0
            && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024
            && frames > 0
            && frames <= 360000
            && [1, 2, 4].contains(&factor),
        "invalid sequence layout"
    );
    let width = w.checked_mul(factor).context("sequence width overflow")?;
    let height = h.checked_mul(factor).context("sequence height overflow")?;
    ensure!(
        u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "sequence frame pixel limit"
    );
    let (width, height) = (width & !1, height & !1);
    ensure!(width > 0 && height > 0, "sequence frame too small");
    let bytes = u64::from(width) * u64::from(height) * 4;
    ensure!(
        (u64::from(w) * u64::from(h) * 4)
            .checked_mul(frames)
            .is_some_and(|b| b <= MAX_BYTES),
        "sequence input staging budget"
    );
    let bytes = bytes
        .checked_mul(frames)
        .context("sequence size overflow")?;
    ensure!(bytes <= MAX_BYTES, "sequence output staging budget");
    Ok((width, height, bytes))
}
async fn opaque_hash(path: &Path, expected: u64) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut buffer = [0u8; 65536];
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        ensure!(bytes + n as u64 <= expected, "sequence output grew");
        for (i, value) in buffer[..n].iter().enumerate() {
            ensure!(
                (bytes + i as u64) % 4 != 3 || *value == 255,
                "sequence output must be opaque"
            );
        }
        bytes += n as u64;
        hash.update(&buffer[..n]);
    }
    ensure!(bytes == expected, "sequence output size mismatch");
    Ok(format!("{:x}", hash.finalize()))
}
pub async fn enhance(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        input.source.is_absolute()
            && destination.is_absolute()
            && [1, 2, 4].contains(&input.factor),
        "invalid enhancement paths/factor"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(3600) && !cancel.is_cancelled(),
        "enhancement cancelled or invalid deadline"
    );
    ensure!(!destination.try_exists()?, "output already exists");
    let parent = destination.parent().context("output parent missing")?;
    ensure!(parent.is_dir(), "output directory missing");
    let deadline = tokio::time::Instant::now() + limit;
    let prepare = async {
        let before = identity(config).await?;
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
    let (width, height, bytes) = layout(w, h, input.frames, input.factor)?;
    ensure!(
        source_before.0 == u64::from(w) * u64::from(h) * 4 * input.frames,
        "raw sequence size mismatch"
    );
    let work = Owned(parent.join(format!(".enhance-sequence-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let result = work.0.join("result.rgba");
    let request = work.0.join("request.json");
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
    file.write_all(&serde_json::to_vec(&serde_json::json!({"schemaVersion":2,"model":config.model,"runtimeLibrary":config.runtime_library,"input":input.source,"output":result,"sourceSha256":input.source_hash,"factor":input.factor,"width":w,"height":h,"frames":input.frames}))?).await?;
    file.sync_all().await?;
    drop(file);
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
        let value = serde_json::from_str::<Progress>(line).ok()?;
        if value.event != "progress"
            || value.total == 0
            || value.total != u64::from(w.div_ceil(256)) * u64::from(h.div_ceil(256)) * input.frames
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
        "enhancement cancelled or expired"
    );
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg("--sequence").arg(&request);
    // Always await kill/reap before releasing owned staging.
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            vec![
                config.model.clone(),
                config.runtime_library.clone(),
                input.source.to_owned(),
                work.0.clone(),
            ],
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
        "enhancement did not complete"
    );
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid enhancement metadata"))?;
    ensure!(!duplicate, "duplicate enhancement metadata");
    let state = state.context("missing enhancement metadata")?;
    let validate = async {
        regular(&result, MAX_BYTES).await?;
        let output_hash = opaque_hash(&result, bytes).await?;
        ensure!(
            state.schema_version == 2
                && state.frames == input.frames
                && state.bytes == bytes
                && state.factor == input.factor
                && state.width == width
                && state.height == height
                && state.source_hash == input.source_hash
                && state.model_hash == MODEL_HASH
                && state.runtime_hash == before.runtime_hash
                && state.output_hash == output_hash,
            "sequence result identity mismatch"
        );
        let after = identity(config).await?;
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
        "enhancement cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish enhancement without replacement")?;
    let _ = progress.send(100.);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_retains_even_crop_and_staging_limits() {
        assert_eq!(layout(257, 9, 2, 1).unwrap(), (256, 8, 16384));
        assert!(layout(1, 1, 1, 1).is_err());
        assert!(layout(512, 9, 360000, 4).is_err());
        assert!(layout(257, 9, 0, 2).is_err());
    }
    #[tokio::test]
    async fn raw_output_opacity_and_size_are_verified() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("output");
        let mut bytes = vec![255; 65540];
        tokio::fs::write(&path, &bytes).await.unwrap();
        assert!(opaque_hash(&path, 65540).await.is_ok());
        bytes[65539] = 0;
        tokio::fs::write(&path, &bytes).await.unwrap();
        assert!(opaque_hash(&path, 65540).await.is_err());
        assert!(opaque_hash(&path, 65536).await.is_err());
    }
}
