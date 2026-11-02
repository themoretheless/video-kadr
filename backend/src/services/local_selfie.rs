//! Host-only learned Selfie person cutout. Caller owns source ACL and Analysis permit.
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
pub(super) const MODEL_HASH: &str = "3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub runtime_library: PathBuf,
}
impl Config {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        match (
            lookup("SELFIE_BIN").filter(|s| !s.trim().is_empty()),
            lookup("SELFIE_MODEL").filter(|s| !s.trim().is_empty()),
            lookup("SELFIE_ORT_LIBRARY").filter(|s| !s.trim().is_empty()),
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
            _ => anyhow::bail!("configure SELFIE_BIN/SELFIE_MODEL/SELFIE_ORT_LIBRARY together"),
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.executable.is_absolute()
                && self.model.is_absolute()
                && self.runtime_library.is_absolute(),
            "cutout host paths must be absolute"
        );
        Ok(())
    }
}
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub mask_destination: Option<&'a Path>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub blank_mask: bool,
    pub mask_hash: String,
    pub source_hash: String,
    pub model_hash: String,
    pub runtime_hash: String,
    pub output_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_file_hash: Option<String>,
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
    identity.update(b"selfie256-rgb01-byte256-straight-alpha-v1");
    let mut runtime_hash = String::new();
    for (index, path) in [&config.executable, &config.model, &config.runtime_library]
        .iter()
        .enumerate()
    {
        regular(path, 256 * 1024 * 1024).await?;
        let (size, hash) = super::local_speech::fingerprint_model(path).await?;
        if index == 1 {
            ensure!(
                size == 462352 && hash == MODEL_HASH,
                "cutout model inventory mismatch"
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
pub async fn cutout(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    ensure!(
        input.source.is_absolute() && destination.is_absolute(),
        "invalid cutout paths"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(3600) && !cancel.is_cancelled(),
        "cutout cancelled or invalid deadline"
    );
    ensure!(!destination.try_exists()?, "output already exists");
    if let Some(mask) = input.mask_destination {
        ensure!(
            mask.is_absolute()
                && mask != destination
                && !mask.try_exists()?
                && mask.parent().is_some_and(|p| p.is_dir()),
            "invalid or existing mask destination"
        );
    }

    let parent = destination.parent().context("output parent missing")?;
    ensure!(parent.is_dir(), "output directory missing");
    let deadline = tokio::time::Instant::now() + limit;
    let prepare = async {
        let before = identity(config).await?;
        regular(input.source, 256 * 1024 * 1024).await?;
        let source_before = super::local_speech::fingerprint_model(input.source).await?;
        ensure!(
            source_before.1 == input.source_hash,
            "cutout source digest mismatch"
        );
        let (width, height) =
            super::local_enhance::dimensions(input.source.to_owned(), true).await?;
        anyhow::Ok((before, source_before, width, height))
    };
    let (before, source_before, width, height) = tokio::select! {_=cancel.cancelled()=>anyhow::bail!("cutout cancelled"),result=tokio::time::timeout_at(deadline,prepare)=>result??};
    let work = Owned(parent.join(format!(".selfie-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let result = work.0.join("result.png");
    let request = work.0.join("request.json");
    let mask_output = work.0.join("mask.png");
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
    file.write_all(&serde_json::to_vec(&serde_json::json!({"schemaVersion":6,"model":config.model,"runtimeLibrary":config.runtime_library,"input":input.source,"output":result,"sourceSha256":input.source_hash,"maskOutput":input.mask_destination.map(|_|&mask_output)}))?).await?;
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
            || value.total != 2
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
        "cutout cancelled or expired"
    );
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg("--selfie").arg(&request);
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
        "cutout did not complete"
    );
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid cutout metadata"))?;
    ensure!(!duplicate, "duplicate cutout metadata");
    let state = state.context("missing cutout metadata")?;
    regular(&result, 256 * 1024 * 1024).await?;
    let output_hash = super::local_speech::fingerprint_model(&result).await?.1;
    ensure!(
        state.schema_version == 6
            && state.mask_width == 256
            && state.mask_height == 256
            && state.mask_hash.len() == 64
            && state
                .mask_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && state.width == width
            && state.height == height
            && state.source_hash == input.source_hash
            && state.model_hash == MODEL_HASH
            && state.runtime_hash == before.runtime_hash
            && state.output_hash == output_hash,
        "cutout result identity mismatch"
    );
    use tokio::io::AsyncReadExt;
    let mut signature = [0u8; 8];
    tokio::fs::File::open(&result)
        .await?
        .read_exact(&mut signature)
        .await?;
    ensure!(
        signature == *b"\x89PNG\r\n\x1a\n",
        "cutout output is not PNG"
    );
    super::local_cutout::verify_pixels(
        input.source.to_owned(),
        result.clone(),
        width,
        height,
        state.blank_mask,
        cancel.clone(),
        deadline,
    )
    .await?;
    if input.mask_destination.is_some() {
        read_verified_mask(&mask_output, &state).await?;
    } else {
        ensure!(state.mask_file_hash.is_none(), "unexpected mask sidecar");
    }
    let recheck = async {
        let after = identity(config).await?;
        ensure!(
            before.hash == after.hash
                && source_before == super::local_speech::fingerprint_model(input.source).await?,
            "cutout source/runtime changed"
        );
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("cutout cancelled"),result=tokio::time::timeout_at(deadline,recheck)=>result??};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "cutout cancelled or expired"
    );
    struct PublishedMask(Option<PathBuf>);
    impl Drop for PublishedMask {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut published_mask = PublishedMask(None);
    if let Some(mask) = input.mask_destination {
        tokio::fs::hard_link(&mask_output, mask)
            .await
            .context("publish mask without replacement")?;
        published_mask.0 = Some(mask.to_owned());
    }
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "cutout cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish cutout without replacement")?;
    published_mask.0 = None;
    let _ = progress.send(100.);
    Ok(state)
}

/// Recheck both encoded and raw probability digests before reuse or private serving.
pub async fn read_verified_mask(path: &Path, metadata: &Metadata) -> Result<Vec<u8>> {
    ensure!(
        metadata.schema_version == 6,
        "invalid mask metadata version"
    );
    regular(path, 1024 * 1024).await?;
    let bytes = tokio::fs::read(path).await?;
    ensure!(
        bytes.len() <= 1024 * 1024
            && metadata.mask_file_hash.as_deref()
                == Some(format!("{:x}", Sha256::digest(&bytes)).as_str()),
        "mask PNG digest mismatch"
    );
    let expected = metadata.clone();
    tokio::task::spawn_blocking(move || {
        ensure!(
            bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n") && bytes.get(24..26) == Some(&[8, 0]),
            "mask is not grayscale8 PNG"
        );
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(256);
        limits.max_image_height = Some(256);
        limits.max_alloc = Some(4 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode()?.to_luma8();
        ensure!(
            image.dimensions() == (256, 256)
                && expected.mask_width == 256
                && expected.mask_height == 256
                && format!("{:x}", Sha256::digest(image.as_raw())) == expected.mask_hash
                && image.as_raw().iter().all(|v| *v < 10) == expected.blank_mask,
            "mask probability identity mismatch"
        );
        Ok(bytes)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn raw_mask_checks_encoded_raw_shape_blank_and_symlink_identity() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("mask.png");
        image::GrayImage::from_pixel(256, 256, image::Luma([128]))
            .save(&path)
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let metadata = Metadata {
            schema_version: 6,
            width: 37,
            height: 23,
            mask_width: 256,
            mask_height: 256,
            blank_mask: false,
            mask_hash: format!("{:x}", Sha256::digest(vec![128; 256 * 256])),
            mask_file_hash: Some(format!("{:x}", Sha256::digest(&bytes))),
            source_hash: String::new(),
            model_hash: String::new(),
            runtime_hash: String::new(),
            output_hash: String::new(),
        };
        assert_eq!(read_verified_mask(&path, &metadata).await.unwrap(), bytes);
        let mut wrong = metadata.clone();
        wrong.mask_hash = "0".repeat(64);
        assert!(read_verified_mask(&path, &wrong).await.is_err());
        wrong = metadata.clone();
        wrong.blank_mask = true;
        assert!(read_verified_mask(&path, &wrong).await.is_err());
        wrong = metadata.clone();
        wrong.mask_width = 511;
        assert!(read_verified_mask(&path, &wrong).await.is_err());
        #[cfg(unix)]
        {
            let linked = root.path().join("linked.png");
            std::os::unix::fs::symlink(&path, &linked).unwrap();
            assert!(read_verified_mask(&linked, &metadata).await.is_err());
        }
        std::fs::write(&path, b"corrupt").unwrap();
        assert!(read_verified_mask(&path, &metadata).await.is_err());
        let mut legacy = serde_json::to_value(&metadata).unwrap();
        legacy.as_object_mut().unwrap().remove("maskFileHash");
        assert!(serde_json::from_value::<Metadata>(legacy)
            .unwrap()
            .mask_file_hash
            .is_none());
    }
}
