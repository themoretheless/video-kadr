//! Reapply cached probabilities to the original source; no model/runtime required.
use super::{decode, digest, publish, regular};
use anyhow::{ensure, Result};
use serde::Deserialize;
use std::{io::Cursor, path::PathBuf, sync::atomic::AtomicBool};
use video_kadr_vision_runtime::{
    mask_apply::{self, Mapping},
    mask_edit::{self, Correction},
    object_matting::Mask,
};
fn default_mask_size() -> u32 {
    512
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    #[serde(default = "default_mask_size")]
    mask_size: u32,
    #[serde(default)]
    mask_width: Option<u32>,
    #[serde(default)]
    mask_height: Option<u32>,
    input: PathBuf,
    mask: PathBuf,
    output: PathBuf,
    source_sha256: String,
    mask_file_sha256: String,
    mask_sha256: String,
    correction: Correction,
    #[serde(default)]
    regions: Vec<Option<CachedRegion>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CachedRegion {
    path: PathBuf,
    file_sha256: String,
    mask_sha256: String,
}
fn read_region(region: &CachedRegion) -> Result<Mask> {
    ensure!(region.path.is_absolute(), "region path must be absolute");
    let bytes = regular(&region.path, 1024 * 1024)?;
    ensure!(
        digest(&bytes) == region.file_sha256,
        "region file digest mismatch"
    );
    ensure!(
        bytes.get(..8) == Some(b"\x89PNG\r\n\x1a\n") && bytes.get(24..26) == Some(&[8, 0]),
        "expected grayscale8 region PNG"
    );
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(256);
    limits.max_image_height = Some(256);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_luma8();
    ensure!(
        image.dimensions() == (256, 256) && digest(image.as_raw()) == region.mask_sha256,
        "region probability identity mismatch"
    );
    Ok(Mask {
        width: 256,
        height: 256,
        bytes: image.into_raw(),
    })
}
fn progress(completed: u32) {
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":completed,"total":2})
    );
}
pub(super) fn run(request: Request) -> Result<()> {
    let (mask_width, mask_height) = match (
        request.schema_version,
        request.mask_width,
        request.mask_height,
    ) {
        (4, None, None) if [256, 512].contains(&request.mask_size) => {
            (request.mask_size, request.mask_size)
        }
        (9, Some(w), Some(h))
            if [w, h]
                .iter()
                .all(|edge| *edge >= 64 && *edge <= 512 && *edge % 16 == 0) =>
        {
            (w, h)
        }
        _ => anyhow::bail!("invalid mask correction grid schema"),
    };
    ensure!(
        [&request.input, &request.mask, &request.output]
            .iter()
            .all(|p| p.is_absolute()),
        "invalid mask correction schema/paths"
    );
    ensure!(
        !request.output.try_exists()?,
        "correction output already exists"
    );
    let source = regular(&request.input, 256 * 1024 * 1024)?;
    ensure!(
        digest(&source) == request.source_sha256,
        "correction source digest mismatch"
    );
    let encoded = regular(&request.mask, 1024 * 1024)?;
    ensure!(
        digest(&encoded) == request.mask_file_sha256,
        "correction mask file digest mismatch"
    );
    ensure!(
        encoded.get(..8) == Some(b"\x89PNG\r\n\x1a\n") && encoded.get(24..26) == Some(&[8, 0]),
        "expected grayscale8 mask PNG"
    );
    let mut reader =
        image::ImageReader::with_format(Cursor::new(&encoded), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(512);
    limits.max_image_height = Some(512);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode()?.to_luma8();
    ensure!(
        decoded.dimensions() == (mask_width, mask_height),
        "invalid cached mask dimensions"
    );
    let mask = Mask {
        width: mask_width as usize,
        height: mask_height as usize,
        bytes: decoded.into_raw(),
    };
    ensure!(
        digest(&mask.bytes) == request.mask_sha256,
        "correction probability digest mismatch"
    );
    ensure!(
        request.correction.strokes.len() <= 1024 && request.regions.len() <= 1024,
        "too many cached correction strokes/regions"
    );
    ensure!(
        request.regions.is_empty() || request.regions.len() == request.correction.strokes.len(),
        "region/stroke count mismatch"
    );
    let mut regions = Vec::with_capacity(request.regions.len());
    for (index, region) in request.regions.iter().enumerate() {
        if let Some(region) = region {
            ensure!(
                request.correction.strokes[index].is_smart(),
                "learned region requires smart tool"
            );
            if let Some(attached) = &request.correction.strokes[index].region {
                attached.validate()?;
                ensure!(
                    attached.source_hash == request.source_sha256
                        && attached.mask_hash == region.mask_sha256
                        && attached.mask_file_hash == region.file_sha256
                        && attached.points == request.correction.strokes[index].points,
                    "cached region provenance mismatch"
                );
            }
            regions.push(Some(read_region(region)?));
        } else {
            regions.push(None);
        }
    }
    for (index, stroke) in request.correction.strokes.iter().enumerate() {
        ensure!(
            stroke.region.is_none() || request.regions.get(index).is_some_and(Option::is_some),
            "missing stored learned region"
        );
    }
    let region_refs: Vec<_> = regions.iter().map(Option::as_ref).collect();
    let image = decode(source)?;
    progress(0);
    let cancel = AtomicBool::new(false);
    let settings = &request.correction;
    let resolved = mask_edit::resolve(
        &mask,
        settings.custom,
        &settings.strokes,
        &region_refs,
        image.width as f32 / image.height as f32,
        settings.feather,
        &cancel,
    )?;
    let resolved_hash = resolved.as_ref().map(|m| digest(&m.bytes));
    let output = if let Some(resolved) = resolved {
        mask_apply::apply(&image, &resolved, Mapping::IDENTITY, &cancel)?
    } else {
        image
    };
    progress(1);
    ensure!(
        digest(&regular(&request.input, 256 * 1024 * 1024)?) == request.source_sha256
            && digest(&regular(&request.mask, 1024 * 1024)?) == request.mask_file_sha256,
        "correction source/mask changed"
    );
    for region in request.regions.iter().flatten() {
        ensure!(
            digest(&regular(&region.path, 1024 * 1024)?) == region.file_sha256,
            "correction region changed"
        );
    }
    let output_hash = publish(&request.output, &output)?;
    progress(2);
    let mut metadata = serde_json::json!({"schemaVersion":4,"width":output.width,"height":output.height,"maskWidth":mask_width,"maskHeight":mask_height,"blankMask":mask.is_blank(),"sourceHash":request.source_sha256,"maskFileHash":request.mask_file_sha256,"maskHash":request.mask_sha256,"correctionHash":digest(&serde_json::to_vec(settings)?),"resolvedMaskHash":resolved_hash,"outputHash":output_hash});
    if !request.regions.is_empty() {
        metadata["regionHashes"] = serde_json::json!(request
            .regions
            .iter()
            .map(|region| region.as_ref().map(|region| &region.mask_sha256))
            .collect::<Vec<_>>());
    }
    println!("{metadata}");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_fraction_points_roundtrip_without_changing_settings_identity() {
        let points: Vec<[f64; 2]> = (0..40).map(|i| [i as f64 / 39., 0.3]).collect();
        let json = serde_json::to_vec(&points).unwrap();
        let decoded: Vec<[f64; 2]> = serde_json::from_slice(&json).unwrap();
        assert_eq!(points, decoded);
        assert_eq!(
            digest(&json),
            digest(&serde_json::to_vec(&decoded).unwrap())
        );
    }
    #[test]
    fn cached_protocol_accepts_no_model_and_refuses_extra_inputs() {
        let mut value = serde_json::json!({"schemaVersion":4,"input":"/source","mask":"/mask","output":"/output","sourceSha256":"source","maskFileSha256":"encoded","maskSha256":"raw","correction":{"custom":true,"strokes":[],"feather":0.01}});
        assert!(serde_json::from_value::<Request>(value.clone()).is_ok());
        value["model"] = serde_json::json!("/model");
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
}
