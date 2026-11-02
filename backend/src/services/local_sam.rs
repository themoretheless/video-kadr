//! Host-owned SlimSAM process adapter. Caller owns source ACL and Analysis admission.
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
pub const ENCODER_HASH: &str = "9f8433273a6750b587779baa0cf5508111001bf7e7acfcf585d370139fd366d0";
pub const DECODER_HASH: &str = "f4514391764fbd56e08e119060d874ecd7d52994bfb1968af159e12d4943b5bb";
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub executable: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub runtime_library: PathBuf,
}
impl Config {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        let keys = [
            "SAM_BIN",
            "SAM_ENCODER_MODEL",
            "SAM_DECODER_MODEL",
            "SAM_ORT_LIBRARY",
        ];
        let values: Vec<_> = keys
            .iter()
            .map(|key| lookup(key).filter(|value| !value.trim().is_empty()))
            .collect();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        ensure!(
            values.iter().all(Option::is_some),
            "configure SAM_BIN/SAM_ENCODER_MODEL/SAM_DECODER_MODEL/SAM_ORT_LIBRARY together"
        );
        let paths: Vec<PathBuf> = values
            .into_iter()
            .map(|v| PathBuf::from(v.unwrap()))
            .collect();
        ensure!(
            paths.iter().all(|v| v.is_absolute()),
            "SAM host paths must be absolute"
        );
        Ok(Some(Self {
            executable: paths[0].clone(),
            encoder: paths[1].clone(),
            decoder: paths[2].clone(),
            runtime_library: paths[3].clone(),
        }))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub hash: String,
    pub runtime_hash: String,
}
pub async fn identity(config: &Config) -> Result<Identity> {
    ensure!(
        [
            &config.executable,
            &config.encoder,
            &config.decoder,
            &config.runtime_library
        ]
        .iter()
        .all(|p| p.is_absolute()),
        "SAM host paths must be absolute"
    );
    let executable = super::local_mask_edit::identity(&config.executable).await?;
    for (path, size, hash) in [
        (&config.encoder, 23_276_014, ENCODER_HASH),
        (&config.decoder, 16_557_892, DECODER_HASH),
    ] {
        let meta = tokio::fs::symlink_metadata(path).await?;
        ensure!(
            meta.file_type().is_file() && meta.len() == size,
            "invalid SAM model inventory"
        );
        ensure!(
            crate::project_archive::fingerprint_file_with_limit(path, size)
                .await?
                .1
                == hash,
            "SAM model checksum mismatch"
        );
    }
    let runtime_meta = tokio::fs::symlink_metadata(&config.runtime_library).await?;
    ensure!(
        runtime_meta.file_type().is_file() && runtime_meta.len() <= 256 * 1024 * 1024,
        "invalid SAM runtime inventory"
    );
    let runtime_hash = crate::project_archive::fingerprint_file_with_limit(
        &config.runtime_library,
        256 * 1024 * 1024,
    )
    .await?
    .1;
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            executable,
            ENCODER_HASH,
            DECODER_HASH,
            &runtime_hash
        ))?)
    );
    Ok(Identity { hash, runtime_hash })
}
pub struct Input<'a> {
    pub source: &'a Path,
    pub source_hash: &'a str,
    pub points: &'a [[f64; 2]],
}
pub fn validate_points(points: &[[f64; 2]]) -> Result<()> {
    ensure!(
        !points.is_empty()
            && points.len() <= 65536
            && points
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
        "invalid SAM prompts"
    );
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub width: u32,
    pub height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub source_hash: String,
    pub encoder_hash: String,
    pub decoder_hash: String,
    pub runtime_hash: String,
    pub prompt_hash: String,
    pub mask_hash: String,
    pub output_hash: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    event: String,
    completed: u32,
    total: u32,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
pub(crate) async fn verify_png(path: PathBuf, metadata: Metadata) -> Result<()> {
    read_verified_probabilities(path, metadata).await.map(|_| ())
}
pub(crate) async fn read_verified_probabilities(path: PathBuf, metadata: Metadata) -> Result<Vec<u8>> {
    let meta = tokio::fs::symlink_metadata(&path).await?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= 1024 * 1024,
        "invalid SAM PNG inventory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            meta.permissions().mode() & 0o777 == 0o600,
            "SAM output must be private"
        );
    }
    let bytes = tokio::fs::read(path).await?;
    ensure!(
        bytes.len() <= 1024 * 1024
            && format!("{:x}", Sha256::digest(&bytes)) == metadata.output_hash
            && bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n")
            && bytes.get(24..26) == Some(&[8, 0]),
        "SAM PNG hash/type mismatch"
    );
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(256);
        limits.max_image_height = Some(256);
        limits.max_alloc = Some(4 * 1024 * 1024);
        reader.limits(limits);
        let mask = reader.decode()?.to_luma8();
        ensure!(
            mask.dimensions() == (256, 256)
                && format!("{:x}", Sha256::digest(mask.as_raw())) == metadata.mask_hash,
            "SAM raw region identity mismatch"
        );
        Ok(mask.into_raw())
    })
    .await?
}
pub async fn region(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    validate_points(input.points)?;
    ensure!(
        input.source.is_absolute()
            && destination.is_absolute()
            && !destination.try_exists()?
            && valid_hash(input.source_hash),
        "invalid SAM paths/source hash/existing destination"
    );
    ensure!(
        !cancel.is_cancelled() && !limit.is_zero() && limit <= Duration::from_secs(3600),
        "SAM cancelled/invalid deadline"
    );
    let parent = destination.parent().context("SAM output parent missing")?;
    ensure!(parent.is_dir(), "SAM output directory missing");
    let deadline = tokio::time::Instant::now() + limit;
    let prepare = async {
        let identity = identity(config).await?;
        ensure!(
            crate::project_archive::fingerprint_file_with_limit(input.source, 256 * 1024 * 1024)
                .await?
                .1
                == input.source_hash,
            "SAM source changed"
        );
        let dimensions = super::local_enhance::dimensions(input.source.to_owned(), true).await?;
        anyhow::Ok((identity, dimensions))
    };
    let (captured, dimensions) = tokio::select! {_=cancel.cancelled()=>anyhow::bail!("SAM cancelled"),result=tokio::time::timeout_at(deadline,prepare)=>result??};
    let prompt_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(input.points)?));
    let work = Owned(parent.join(format!(".sam-region-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let result = work.0.join("result.png");
    let request = work.0.join("request.json");
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options.open(&request).await?;
    file.write_all(&serde_json::to_vec(&serde_json::json!({"schemaVersion":5,"encoder":config.encoder,"decoder":config.decoder,"runtimeLibrary":config.runtime_library,"input":input.source,"output":result,"sourceSha256":input.source_hash,"points":input.points}))?).await?;
    file.sync_all().await?;
    drop(file);
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
        let value = serde_json::from_str::<Progress>(line).ok()?;
        if value.event != "progress" || value.total != 3 || value.completed > 3 {
            return None;
        }
        let mut maximum = maximum.lock().ok()?;
        *maximum = maximum.max(if value.completed == 0 {
            1.
        } else {
            f64::from(value.completed) / 3. * 95.
        });
        Some(*maximum)
    };
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    ensure!(
        !remaining.is_zero() && !cancel.is_cancelled(),
        "SAM cancelled/expired"
    );
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg("--region").arg(&request);
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            vec![
                input.source.to_owned(),
                config.encoder.clone(),
                config.decoder.clone(),
                config.runtime_library.clone(),
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
        "SAM region did not complete"
    );
    let (metadata, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid SAM metadata"))?;
    ensure!(!duplicate, "duplicate SAM metadata");
    let metadata = metadata.context("missing SAM metadata")?;
    ensure!(
        metadata.schema_version == 5
            && (metadata.width, metadata.height) == dimensions
            && (metadata.mask_width, metadata.mask_height) == (256, 256)
            && metadata.source_hash == input.source_hash
            && metadata.encoder_hash == ENCODER_HASH
            && metadata.decoder_hash == DECODER_HASH
            && metadata.runtime_hash == captured.runtime_hash
            && metadata.prompt_hash == prompt_hash
            && valid_hash(&metadata.mask_hash)
            && valid_hash(&metadata.output_hash),
        "SAM result identity mismatch"
    );
    verify_png(result.clone(), metadata.clone()).await?;
    let recheck = async {
        ensure!(
            identity(config).await? == captured
                && crate::project_archive::fingerprint_file_with_limit(
                    input.source,
                    256 * 1024 * 1024
                )
                .await?
                .1 == input.source_hash,
            "SAM source/engine changed"
        );
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("SAM cancelled"),result=tokio::time::timeout_at(deadline,recheck)=>result??};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "SAM cancelled/expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish SAM region without replacement")?;
    let _ = progress.send(100.);
    Ok(metadata)
}
