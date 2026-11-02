//! One isolated host-owned Selfie still cutout, published only after validation.
use super::{decode, digest, publish, publish_bytes, regular};
use anyhow::{ensure, Result};
use image::ImageEncoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use video_kadr_vision_runtime::{
    mask_apply::{self, Mapping},
    mask_edit::{self, Correction},
    selfie_matting::{SelfieSegmenter, MODEL_BYTES, MODEL_SHA256},
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    model: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    output: PathBuf,
    source_sha256: String,
    #[serde(default)]
    mask_output: Option<PathBuf>,
    #[serde(default)]
    correction: Option<Correction>,
}
struct OwnedMask(Option<PathBuf>);
impl Drop for OwnedMask {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}
fn model_hash(path: &Path) -> Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == MODEL_BYTES,
        "invalid object model file"
    );
    let mut reader = std::fs::File::open(path)?.take(MODEL_BYTES + 1);
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes = 0u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= MODEL_BYTES, "object model grew");
        hash.update(&buffer[..n]);
    }
    ensure!(bytes == MODEL_BYTES, "object model size changed");
    Ok(format!("{:x}", hash.finalize()))
}
pub(super) fn run(request: Request) -> Result<()> {
    ensure!(
        request.schema_version == 6
            && [
                &request.model,
                &request.runtime_library,
                &request.input,
                &request.output
            ]
            .iter()
            .all(|p| p.is_absolute()),
        "invalid object request schema/paths"
    );
    ensure!(
        !request.output.try_exists()?,
        "cutout output already exists"
    );
    if let Some(path) = &request.mask_output {
        ensure!(
            path.is_absolute() && path != &request.output && !path.try_exists()?,
            "invalid or existing mask output"
        );
    }
    ensure!(
        request
            .correction
            .as_ref()
            .is_none_or(|c| c.strokes.iter().all(|s| s.region.is_none())),
        "stored learned regions require cached correction protocol"
    );
    let source = regular(&request.input, 256 * 1024 * 1024)?;
    let source_hash = digest(&source);
    ensure!(
        source_hash == request.source_sha256,
        "cutout source digest mismatch"
    );
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load host object runtime: {e}"))?
        .commit();
    let image = decode(source)?;
    let mut segmenter = SelfieSegmenter::load(&request.model)?;
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":0,"total":2})
    );
    let cancel = AtomicBool::new(false);
    let learned = segmenter.mask(&image, &cancel)?;
    let mask = video_kadr_vision_runtime::object_matting::Mask {
        width: learned.width,
        height: learned.height,
        bytes: learned.bytes,
    };
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":1,"total":2})
    );
    // Uncorrected blank masks leave the picture whole, like MaskStore::resolved.
    let blank = mask.is_blank();
    let resolved = if let Some(edit) = &request.correction {
        mask_edit::resolve(
            &mask,
            edit.custom,
            &edit.strokes,
            &[],
            image.width as f32 / image.height as f32,
            edit.feather,
            &cancel,
        )?
    } else if blank {
        None
    } else {
        Some(mask.clone())
    };
    let resolved_hash = resolved.as_ref().map(|m| digest(&m.bytes));
    let output = if let Some(resolved) = &resolved {
        mask_apply::apply(&image, resolved, Mapping::IDENTITY, &cancel)?
    } else {
        image
    };
    ensure!(
        digest(&regular(&request.input, 256 * 1024 * 1024)?) == source_hash
            && model_hash(&request.model)? == MODEL_SHA256
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "cutout source/model/runtime changed"
    );
    let mut owned_mask = OwnedMask(None);
    let mask_file_hash = if let Some(path) = &request.mask_output {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            &mask.bytes,
            mask.width as u32,
            mask.height as u32,
            image::ExtendedColorType::L8,
        )?;
        let hash = publish_bytes(path, &png)?;
        owned_mask.0 = Some(path.clone());
        Some(hash)
    } else {
        None
    };
    let output_hash = publish(&request.output, &output)?;
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":2,"total":2})
    );
    owned_mask.0 = None;
    let mut metadata = serde_json::json!({"schemaVersion":6,"width":output.width,"height":output.height,"maskWidth":mask.width,"maskHeight":mask.height,"blankMask":blank,"maskHash":digest(&mask.bytes),"sourceHash":source_hash,"modelHash":MODEL_SHA256,"runtimeHash":runtime_hash,"outputHash":output_hash});
    if request.mask_output.is_some() {
        metadata["maskFileHash"] = serde_json::json!(mask_file_hash);
    }
    if request.correction.is_some() {
        metadata["resolvedMaskHash"] = serde_json::json!(resolved_hash);
    }
    println!("{}", metadata);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_selfie_protocol() {
        let value = serde_json::json!({"schemaVersion":6,"model":"/model","runtimeLibrary":"/runtime","input":"/input","output":"/output","sourceSha256":"hash"});
        assert!(serde_json::from_value::<Request>(value.clone()).is_ok());
        let mut bad = value;
        bad["url"] = serde_json::json!("https://invalid");
        assert!(serde_json::from_value::<Request>(bad).is_err());
    }
}
