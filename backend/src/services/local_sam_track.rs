//! Owned raw SAM sequence process. Caller owns source ACL, Analysis and movie staging.
use super::{
    local_enhance_video::{guard, remaining},
    local_sam::{identity, Config, DECODER_HASH, ENCODER_HASH},
};
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
const PLANE: usize = 256 * 256;
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub seed: &'a Path,
    pub seed_hash: &'a str,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub seed_frame: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    pub seed_frame: u64,
    pub mask_width: u32,
    pub mask_height: u32,
    pub mask_bytes: u64,
    pub tracked_frames: u64,
    pub forward_lost_at: Option<u64>,
    pub backward_lost_at: Option<u64>,
    pub source_hash: String,
    pub seed_hash: String,
    pub encoder_hash: String,
    pub decoder_hash: String,
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
fn layout(input: &Input<'_>) -> Result<(u64, u64)> {
    ensure!(
        input.source.is_absolute()
            && input.seed.is_absolute()
            && input.width > 0
            && input.height > 0
            && u64::from(input.width) * u64::from(input.height) <= 64 * 1024 * 1024
            && input.frames > 0
            && input.frames <= 360000
            && input.seed_frame < input.frames,
        "invalid SAM sequence layout"
    );
    ensure!(
        [input.source_hash, input.seed_hash]
            .iter()
            .all(|v| v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
        "invalid SAM sequence identity"
    );
    let bytes = (u64::from(input.width) * u64::from(input.height) * 4)
        .checked_mul(input.frames)
        .context("SAM source overflow")?;
    let masks = (PLANE as u64) * input.frames;
    ensure!(
        bytes.checked_add(masks).is_some_and(|n| n <= MAX_BYTES),
        "SAM sequence staging budget"
    );
    Ok((bytes, masks))
}
async fn fingerprint(path: &Path, limit: u64) -> Result<(u64, String)> {
    let info = tokio::fs::symlink_metadata(path).await?;
    ensure!(
        info.file_type().is_file() && info.len() <= limit,
        "invalid SAM sequence regular file"
    );
    Ok(crate::project_archive::fingerprint_file_with_limit(path, limit).await?)
}
fn temporal_step(previous: &[u8], candidate: &[u8]) -> Result<()> {
    ensure!(
        previous.iter().any(|v| *v >= 230),
        "tracked region has no confident prior prompt"
    );
    let (mut both, mut either) = (0u32, 0u32);
    for (&a, &b) in previous.iter().zip(candidate) {
        both += u32::from(a >= 128 && b >= 128);
        either += u32::from(a >= 128 || b >= 128);
    }
    ensure!(
        either > 0 && both as f32 / either as f32 >= 0.25,
        "tracked region overlap below admission"
    );
    Ok(())
}
async fn verify(
    path: &Path,
    input: &Input<'_>,
    seed: &[u8],
    state: &Metadata,
    cancel: &CancellationToken,
    deadline: tokio::time::Instant,
) -> Result<String> {
    let lower = if let Some(n) = state.backward_lost_at {
        ensure!(n < input.seed_frame, "invalid backward loss ordinal");
        n + 1
    } else {
        0
    };
    let upper = if let Some(n) = state.forward_lost_at {
        ensure!(
            n > input.seed_frame && n < input.frames,
            "invalid forward loss ordinal"
        );
        n
    } else {
        input.frames
    };
    ensure!(
        state.tracked_frames == upper - lower,
        "SAM tracked-frame counter mismatch"
    );
    let mut file = tokio::fs::File::open(path).await?;
    let mut previous = vec![0; PLANE];
    let mut current = vec![0; PLANE];
    let mut hash = Sha256::new();
    for frame in 0..input.frames {
        remaining(deadline, cancel)?;
        file.read_exact(&mut current).await?;
        hash.update(&current);
        if frame == input.seed_frame {
            ensure!(current == seed, "SAM seed plane changed");
        }
        if frame < lower || frame >= upper {
            ensure!(
                current.iter().all(|v| *v == 0),
                "SAM region continued beyond declared loss"
            );
        } else if frame > lower && frame <= input.seed_frame {
            temporal_step(&current, &previous)?;
        } else if frame > input.seed_frame {
            temporal_step(&previous, &current)?;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    let mut extra = [0];
    ensure!(file.read(&mut extra).await? == 0, "extra SAM region bytes");
    Ok(format!("{:x}", hash.finalize()))
}
#[allow(clippy::too_many_arguments)]
pub async fn track(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    let (source_bytes, mask_bytes) = layout(&input)?;
    ensure!(
        destination.is_absolute()
            && !destination.try_exists()?
            && !cancel.is_cancelled()
            && !limit.is_zero()
            && limit <= Duration::from_secs(3600),
        "invalid SAM track output/deadline"
    );
    let deadline = tokio::time::Instant::now() + limit;
    let (before, source, seed_id, seed) = guard(
        async {
            let identity = identity(config).await?;
            let source = fingerprint(input.source, MAX_BYTES).await?;
            let seed_id = fingerprint(input.seed, PLANE as u64).await?;
            let mut seed = Vec::with_capacity(PLANE);
            tokio::fs::File::open(input.seed)
                .await?
                .take(PLANE as u64 + 1)
                .read_to_end(&mut seed)
                .await?;
            Ok((identity, source, seed_id, seed))
        },
        deadline,
        cancel,
    )
    .await?;
    ensure!(
        source == (source_bytes, input.source_hash.to_owned())
            && seed_id == (PLANE as u64, input.seed_hash.to_owned())
            && seed.len() == PLANE
            && format!("{:x}", Sha256::digest(&seed)) == input.seed_hash,
        "SAM sequence source/seed mismatch"
    );
    let parent = destination.parent().context("SAM track parent missing")?;
    ensure!(parent.is_dir(), "SAM track parent missing");
    let work_path = parent.join(format!(".sam-track-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&work_path)?;
    let work = Owned(work_path);
    remaining(deadline, cancel)?;
    let result = work.0.join("result.masks");
    let request = work.0.join("request.json");
    guard(async{let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&request).await?;#[cfg(unix)]{use std::os::unix::fs::PermissionsExt;file.set_permissions(std::fs::Permissions::from_mode(0o600)).await?;}file.write_all(&serde_json::to_vec(&serde_json::json!({"schemaVersion":9,"encoder":config.encoder,"decoder":config.decoder,"runtimeLibrary":config.runtime_library,"input":input.source,"seed":input.seed,"output":result,"sourceSha256":input.source_hash,"seedSha256":input.seed_hash,"width":input.width,"height":input.height,"frames":input.frames,"seedFrame":input.seed_frame}))?).await?;file.sync_all().await?;Ok(())},deadline,cancel).await?;
    let metadata = Mutex::new((None, false));
    let maximum = Mutex::new(0f64);
    let parse = |line: &str| {
        if let Ok(value) = serde_json::from_str::<Metadata>(line) {
            let mut state = metadata.lock().ok()?;
            if state.0.is_some() {
                state.1 = true;
            }
            state.0 = Some(value);
            return None;
        }
        if serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .is_some_and(|value| value.get("schemaVersion").is_some())
        {
            metadata.lock().ok()?.1 = true;
            return None;
        }
        let value = serde_json::from_str::<Progress>(line).ok()?;
        if value.event != "progress" || value.total != input.frames || value.completed > value.total
        {
            return None;
        }
        let mut maximum = maximum.lock().ok()?;
        *maximum = maximum.max(value.completed as f64 / input.frames as f64 * 95.);
        Some(*maximum)
    };
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg("--sam-track").arg(&request);
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            vec![
                config.encoder.clone(),
                config.decoder.clone(),
                config.runtime_library.clone(),
                input.source.to_owned(),
                input.seed.to_owned(),
                work.0.clone(),
            ],
            &result,
        ),
        parse,
        progress,
        cancel,
        remaining(deadline, cancel)?,
    )
    .await?;
    ensure!(
        status == ProcessStatus::Ok && !cancel.is_cancelled(),
        "SAM tracking process did not complete"
    );
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid SAM metadata lock"))?;
    ensure!(!duplicate, "duplicate or invalid SAM tracking metadata");
    let state = state.context("missing SAM tracking metadata")?;
    guard(
        async {
            let file = fingerprint(&result, mask_bytes).await?;
            ensure!(file.0 == mask_bytes, "SAM mask stream length mismatch");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                ensure!(
                    tokio::fs::metadata(&result).await?.permissions().mode() & 0o777 == 0o600,
                    "SAM output is not private"
                );
            }
            ensure!(
                state.schema_version == 9
                    && state.width == input.width
                    && state.height == input.height
                    && state.frames == input.frames
                    && state.seed_frame == input.seed_frame
                    && state.mask_width == 256
                    && state.mask_height == 256
                    && state.mask_bytes == mask_bytes
                    && state.source_hash == input.source_hash
                    && state.seed_hash == input.seed_hash
                    && state.encoder_hash == ENCODER_HASH
                    && state.decoder_hash == DECODER_HASH
                    && state.runtime_hash == before.runtime_hash,
                "SAM tracking metadata identity mismatch"
            );
            let actual = verify(&result, &input, &seed, &state, cancel, deadline).await?;
            ensure!(
                actual == state.output_hash && actual == file.1,
                "SAM stream digest mismatch"
            );
            ensure!(
                identity(config).await? == before
                    && fingerprint(input.source, MAX_BYTES).await? == source
                    && fingerprint(input.seed, PLANE as u64).await? == seed_id,
                "SAM track source/seed/engine changed"
            );
            tokio::fs::File::open(&result).await?.sync_all().await?;
            Ok(())
        },
        deadline,
        cancel,
    )
    .await?;
    remaining(deadline, cancel)?;
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish SAM track without replacement")?;
    if cancel.is_cancelled() || tokio::time::Instant::now() >= deadline {
        tokio::fs::remove_file(destination).await?;
        anyhow::bail!("SAM track publication cancelled/expired");
    }
    let _ = progress.send(100.);
    Ok(state)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_grid_overlap_is_independent_of_child_metadata() {
        let full = vec![255; PLANE];
        let mut candidate = vec![0; PLANE];
        candidate[..PLANE / 4].fill(128);
        assert!(temporal_step(&full, &candidate).is_ok());
        candidate[PLANE / 4 - 1] = 0;
        assert!(temporal_step(&full, &candidate).is_err());
        assert!(temporal_step(&vec![229; PLANE], &full).is_err());
    }
}
