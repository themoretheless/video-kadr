//! Host-only learned still enhancement. Caller owns source ACL and Analysis permit.
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use image::ImageDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
const MODEL_HASH: &str = "1940a93ee08283a0a7286183186357b1688fe9fa8ede74604b424586aaddf112";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub runtime_library: PathBuf,
}
impl Config {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        match (
            lookup("ENHANCE_BIN").filter(|s| !s.trim().is_empty()),
            lookup("ENHANCE_MODEL").filter(|s| !s.trim().is_empty()),
            lookup("ENHANCE_ORT_LIBRARY").filter(|s| !s.trim().is_empty()),
        ) {
            (None, None, None) => Ok(None),
            (Some(executable), Some(model), Some(runtime_library)) => {
                let config = Self {
                    executable: executable.into(),
                    model: model.into(),
                    runtime_library: runtime_library.into(),
                };
                config.validate()?;
                Ok(Some(config))
            }
            _ => anyhow::bail!("configure ENHANCE_BIN/ENHANCE_MODEL/ENHANCE_ORT_LIBRARY together"),
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.executable.is_absolute()
                && self.model.is_absolute()
                && self.runtime_library.is_absolute(),
            "enhancement host paths must be absolute"
        );
        Ok(())
    }
}
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub factor: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub factor: u32,
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
#[derive(Clone, Debug)]
pub struct Identity {
    pub hash: String,
    pub runtime_hash: String,
}
async fn regular(path: &Path, limit: u64) -> Result<()> {
    let meta = tokio::fs::symlink_metadata(path).await?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= limit,
        "invalid bounded regular file"
    );
    Ok(())
}
pub async fn identity(config: &Config) -> Result<Identity> {
    config.validate()?;
    let mut identity = Sha256::new();
    identity.update(b"realesr-x4v3-core256-pad16-rgba8-box-v1");
    let mut runtime_hash = String::new();
    for (index, path) in [&config.executable, &config.model, &config.runtime_library]
        .iter()
        .enumerate()
    {
        regular(path, 256 * 1024 * 1024).await?;
        let (size, hash) = super::local_speech::fingerprint_model(path).await?;
        if index == 1 {
            ensure!(
                size == 4866417 && hash == MODEL_HASH,
                "enhancement model inventory mismatch"
            );
        }
        if index == 2 {
            runtime_hash = hash.clone();
        }
        identity.update(size.to_le_bytes());
        identity.update(hash.as_bytes());
    }
    Ok(Identity {
        hash: format!("{:x}", identity.finalize()),
        runtime_hash,
    })
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) async fn dimensions(path: PathBuf, orientation: bool) -> Result<(u32, u32)> {
    tokio::task::spawn_blocking(move || -> Result<_> {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(512 * 1024 * 1024);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let (w, h) = decoder.dimensions();
        ensure!(
            w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024,
            "image dimensions exceed limit"
        );
        if orientation {
            use image::metadata::Orientation;
            return Ok(
                if matches!(
                    decoder.orientation()?,
                    Orientation::Rotate90
                        | Orientation::Rotate270
                        | Orientation::Rotate90FlipH
                        | Orientation::Rotate270FlipH
                ) {
                    (h, w)
                } else {
                    (w, h)
                },
            );
        }
        let pixels = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
        ensure!(
            pixels.pixels().all(|p| p.0[3] == 255),
            "enhancement output must be opaque"
        );
        Ok((w, h))
    })
    .await?
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
    let before = tokio::time::timeout_at(deadline, identity(config)).await??;
    regular(input.source, 256 * 1024 * 1024).await?;
    let source_before = super::local_speech::fingerprint_model(input.source).await?;
    ensure!(
        source_before.1 == input.source_hash,
        "source digest mismatch"
    );
    let (w, h) = dimensions(input.source.to_owned(), true).await?;
    let width = w
        .checked_mul(input.factor)
        .context("output dimension overflow")?;
    let height = h
        .checked_mul(input.factor)
        .context("output dimension overflow")?;
    ensure!(
        u64::from(width) * u64::from(height) <= 64 * 1024 * 1024,
        "output pixel limit exceeded"
    );
    let work = Owned(parent.join(format!(".enhance-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let result = work.0.join("result.png");
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
    file.write_all(&serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"model":config.model,"runtimeLibrary":config.runtime_library,"input":input.source,"output":result,"sourceSha256":input.source_hash,"factor":input.factor}))?).await?;
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
            || value.total != u64::from(w.div_ceil(256)) * u64::from(h.div_ceil(256))
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
    command.arg("--request").arg(&request);
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
    regular(&result, 256 * 1024 * 1024).await?;
    let output_hash = super::local_speech::fingerprint_model(&result).await?.1;
    ensure!(
        state.schema_version == 1
            && state.factor == input.factor
            && state.width == width
            && state.height == height
            && state.source_hash == input.source_hash
            && state.model_hash == MODEL_HASH
            && state.runtime_hash == before.runtime_hash
            && state.output_hash == output_hash,
        "enhancement result identity mismatch"
    );
    let signature = tokio::fs::read(&result).await?;
    ensure!(
        signature.starts_with(b"\x89PNG\r\n\x1a\n"),
        "enhancement result is not PNG"
    );
    drop(signature);
    ensure!(
        dimensions(result.clone(), false).await? == (width, height),
        "enhancement result dimensions mismatch"
    );
    let after = tokio::time::timeout_at(deadline, identity(config)).await??;
    ensure!(
        before.hash == after.hash
            && source_before == super::local_speech::fingerprint_model(input.source).await?,
        "enhancement source/runtime changed"
    );
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
