//! Bounded offline decoding of an explicitly authorized source range.
use crate::{
    process_control::{capture_output_cancellable, ProcessRuntime},
    services::local_tts,
};
use anyhow::{ensure, Context, Result};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn seconds(us: u64) -> String {
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}
pub async fn extract(
    runtime: &ProcessRuntime,
    input: &Path,
    start: u64,
    end: u64,
    staging: &Path,
    cancel: &CancellationToken,
) -> Result<(Vec<u8>, u64)> {
    ensure!(
        end.checked_sub(start)
            .is_some_and(|n| (500000..=10000000).contains(&n)),
        "invalid reference range"
    );
    ensure!(!cancel.is_cancelled(), "reference extraction cancelled");
    let work = Owned(staging.join(uuid::Uuid::new_v4().to_string()));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let wav = work.0.join("reference.wav");
    let mut command = tokio::process::Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-threads",
            "1",
            "-protocol_whitelist",
            "file,pipe,crypto",
            "-ss",
        ])
        .arg(seconds(start))
        .arg("-i")
        .arg(input)
        .arg("-t")
        .arg(seconds(end - start))
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-map_metadata",
            "-1",
            "-ar",
            "24000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-fs",
            "1048576",
            "-f",
            "wav",
        ])
        .arg(&wav);
    let output = capture_output_cancellable(
        runtime,
        command,
        runtime.render_policy(vec![input.to_path_buf()], &wav),
        Duration::from_secs(120),
        cancel,
    )
    .await?;
    ensure!(output.status.success(), "source audio decoding failed");
    ensure!(!cancel.is_cancelled(), "reference extraction cancelled");
    let metadata = tokio::fs::symlink_metadata(&wav).await?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= 1024 * 1024,
        "invalid decoded reference"
    );
    local_tts::normalize_reference(
        tokio::fs::read(&wav)
            .await
            .context("read decoded reference")?,
    )
}
