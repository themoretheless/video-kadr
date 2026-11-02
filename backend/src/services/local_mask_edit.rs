//! Cached-mask correction. Caller owns source/job ACL and Analysis admission.
pub use crate::domain::cutout::{Correction, Stroke, Tool};
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
pub struct Input<'a> {
    pub source: &'a Path,
    pub mask: &'a Path,
    pub original: &'a super::local_cutout::Metadata,
    pub correction: &'a Correction,
}
/// Server-resolved region; caller must authorize both its source and mask media.
pub struct RegionInput<'a> {
    pub path: &'a Path,
    pub attachment: &'a crate::domain::cutout::RegionAttachment,
}
pub(crate) async fn verify_region_file(
    original: &super::local_cutout::Metadata,
    region: &RegionInput<'_>,
) -> Result<()> {
    let a = region.attachment;
    super::local_sam::verify_png(
        region.path.to_owned(),
        super::local_sam::Metadata {
            schema_version: 5,
            width: original.width,
            height: original.height,
            mask_width: 256,
            mask_height: 256,
            source_hash: a.source_hash.clone(),
            encoder_hash: a.encoder_hash.clone(),
            decoder_hash: a.decoder_hash.clone(),
            runtime_hash: a.runtime_hash.clone(),
            prompt_hash: a.prompt_hash.clone(),
            mask_hash: a.mask_hash.clone(),
            output_hash: a.mask_file_hash.clone(),
        },
    )
    .await?;
    Ok(())
}
async fn verify_regions(input: &Input<'_>, regions: &[Option<RegionInput<'_>>]) -> Result<()> {
    ensure!(
        regions.is_empty() || regions.len() == input.correction.strokes.len(),
        "correction region/stroke count mismatch"
    );
    ensure!(regions.len() <= 1024, "too many correction regions");
    for (index, stroke) in input.correction.strokes.iter().enumerate() {
        if let Some(attached) = &stroke.region {
            ensure!(
                regions
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some_and(|r| r.attachment == attached),
                "missing or mismatched stored learned region"
            );
        }
    }
    for (index, region) in regions.iter().enumerate() {
        if let Some(region) = region {
            let stroke = &input.correction.strokes[index];
            region.attachment.validate()?;
            ensure!(
                matches!(stroke.tool, Tool::SmartBrush | Tool::SmartEraser)
                    && region.path.is_absolute()
                    && region.attachment.source_hash == input.original.source_hash
                    && region.attachment.points == stroke.points,
                "correction region source/prompts/tool mismatch"
            );
            verify_region_file(input.original, region).await?;
        }
    }
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
    pub blank_mask: bool,
    pub source_hash: String,
    pub mask_file_hash: String,
    pub mask_hash: String,
    pub correction_hash: String,
    pub resolved_mask_hash: Option<String>,
    pub output_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_hashes: Option<Vec<Option<String>>>,
}
pub async fn identity(executable: &Path) -> Result<String> {
    ensure!(
        executable.is_absolute(),
        "mask executable path must be absolute"
    );
    let meta = tokio::fs::symlink_metadata(executable).await?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= 256 * 1024 * 1024,
        "invalid mask executable"
    );
    Ok(super::local_speech::fingerprint_model(executable).await?.1)
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Progress {
    event: String,
    completed: u32,
    total: u32,
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
    correct_with_regions(
        runtime,
        executable,
        input,
        &[],
        destination,
        cancel,
        limit,
        progress,
    )
    .await
}
pub async fn correct_with_regions(
    runtime: &ProcessRuntime,
    executable: &Path,
    input: Input<'_>,
    regions: &[Option<RegionInput<'_>>],
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Metadata> {
    input.correction.validate()?;
    ensure!(
        destination.is_absolute()
            && input.source.is_absolute()
            && input.mask.is_absolute()
            && !destination.try_exists()?,
        "invalid correction paths/existing output"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(3600) && !cancel.is_cancelled(),
        "invalid correction deadline/cancelled"
    );
    let parent = destination
        .parent()
        .context("correction output parent missing")?;
    ensure!(parent.is_dir(), "correction output directory missing");
    let deadline = tokio::time::Instant::now() + limit;
    let correction_hash = input.correction.hash()?;
    let prepare = async {
        let executable_hash = identity(executable).await?;
        verify_regions(&input, regions).await?;
        super::local_cutout::read_verified_mask(input.mask, input.original).await?;
        let source =
            crate::project_archive::fingerprint_file_with_limit(input.source, 256 * 1024 * 1024)
                .await?;
        ensure!(
            source.1 == input.original.source_hash,
            "correction source changed"
        );
        let size = super::local_enhance::dimensions(input.source.to_owned(), true).await?;
        ensure!(
            size == (input.original.width, input.original.height),
            "correction source dimensions changed"
        );
        anyhow::Ok(executable_hash)
    };
    let executable_hash = tokio::select! {_=cancel.cancelled()=>anyhow::bail!("mask correction cancelled"),r=tokio::time::timeout_at(deadline,prepare)=>r??};
    let work = Owned(parent.join(format!(".mask-edit-{}", uuid::Uuid::new_v4())));
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
    let mut payload = serde_json::json!({"schemaVersion":4,"maskSize":input.original.mask_width,"input":input.source,"mask":input.mask,"output":result,"sourceSha256":input.original.source_hash,"maskFileSha256":input.original.mask_file_hash,"maskSha256":input.original.mask_hash,"correction":input.correction});
    if input.original.schema_version==8 {
        payload["schemaVersion"]=serde_json::json!(9);
        payload["maskWidth"]=serde_json::json!(input.original.mask_width);
        payload["maskHeight"]=serde_json::json!(input.original.mask_height);
        payload.as_object_mut().unwrap().remove("maskSize");
    }
    if !regions.is_empty() {
        payload["regions"]=serde_json::json!(regions.iter().map(|region|region.as_ref().map(|r|serde_json::json!({"path":r.path,"fileSha256":r.attachment.mask_file_hash,"maskSha256":r.attachment.mask_hash}))).collect::<Vec<_>>());
    }
    file.write_all(&serde_json::to_vec(&payload)?).await?;
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
        if value.event != "progress" || value.total != 2 || value.completed > 2 {
            return None;
        }
        let mut max = maximum.lock().ok()?;
        *max = max.max(if value.completed == 0 {
            1.
        } else {
            f64::from(value.completed) / 2. * 95.
        });
        Some(*max)
    };
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    ensure!(
        !remaining.is_zero() && !cancel.is_cancelled(),
        "correction cancelled/expired"
    );
    let mut command = tokio::process::Command::new(executable);
    command.arg("--mask").arg(&request);
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            [
                input.source.to_owned(),
                input.mask.to_owned(),
                work.0.clone(),
            ]
            .into_iter()
            .chain(regions.iter().flatten().map(|r| r.path.to_owned()))
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
        "mask correction did not complete"
    );
    let (metadata, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid correction metadata"))?;
    ensure!(!duplicate, "duplicate correction metadata");
    let metadata = metadata.context("missing correction metadata")?;
    let original = input.original;
    ensure!(
        metadata.schema_version == 4
            && (metadata.width, metadata.height) == (original.width, original.height)
            && (metadata.mask_width, metadata.mask_height)
                == (original.mask_width, original.mask_height)
            && metadata.blank_mask == original.blank_mask
            && metadata.source_hash == original.source_hash
            && Some(&metadata.mask_file_hash) == original.mask_file_hash.as_ref()
            && metadata.mask_hash == original.mask_hash
            && metadata.correction_hash == correction_hash,
        "correction result identity mismatch"
    );
    let expected_regions = if regions.is_empty() {
        None
    } else {
        Some(
            regions
                .iter()
                .map(|r| r.as_ref().map(|r| r.attachment.mask_hash.clone()))
                .collect::<Vec<_>>(),
        )
    };
    ensure!(
        metadata.region_hashes == expected_regions,
        "correction region result identity mismatch"
    );
    let expect_none =
        original.blank_mask && (!input.correction.custom || input.correction.strokes.is_empty());
    ensure!(
        if expect_none {
            metadata.resolved_mask_hash.is_none()
        } else {
            metadata.resolved_mask_hash.as_ref().is_some_and(|h| {
                h.len() == 64
                    && h.bytes()
                        .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            })
        },
        "invalid resolved mask identity"
    );
    ensure!(
        crate::project_archive::fingerprint_file_with_limit(&result, 256 * 1024 * 1024)
            .await?
            .1
            == metadata.output_hash,
        "correction PNG digest mismatch"
    );
    use tokio::io::AsyncReadExt;
    let mut signature = [0u8; 8];
    tokio::fs::File::open(&result)
        .await?
        .read_exact(&mut signature)
        .await?;
    ensure!(
        signature == *b"\x89PNG\r\n\x1a\n",
        "correction output is not PNG"
    );
    super::local_cutout::verify_pixels(
        input.source.to_owned(),
        result.clone(),
        metadata.width,
        metadata.height,
        expect_none,
        cancel.clone(),
        deadline,
    )
    .await?;
    let recheck = async {
        verify_regions(&input, regions).await?;
        ensure!(
            identity(executable).await? == executable_hash
                && crate::project_archive::fingerprint_file_with_limit(
                    input.source,
                    256 * 1024 * 1024
                )
                .await?
                .1 == original.source_hash,
            "correction source/executable changed"
        );
        super::local_cutout::read_verified_mask(input.mask, original).await?;
        anyhow::Ok(())
    };
    tokio::select! {_=cancel.cancelled()=>anyhow::bail!("mask correction cancelled"),r=tokio::time::timeout_at(deadline,recheck)=>r??};
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "correction cancelled/expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish correction without replacement")?;
    let _ = progress.send(100.);
    Ok(metadata)
}
