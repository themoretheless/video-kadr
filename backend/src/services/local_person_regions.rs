//! Trusted raw tracked-region bindings for model-free sequence correction.
//! Caller verifies encoded movie/region files, ACL and decoder ordinal clock.
use super::local_person_correct::Input;
use crate::domain::{
    composition::FrameRateRatio, cutout::RegionAttachment, video_region::Attachment,
};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
pub struct RegionInput<'a> {
    pub path: &'a Path,
    pub attachment: &'a Attachment,
}
pub struct Context<'a> {
    pub encoded_source_hash: &'a str,
    pub frame_rate: FrameRateRatio,
    pub regions: &'a [RegionInput<'a>],
}
fn provenance(region: &Attachment) -> RegionAttachment {
    RegionAttachment {
        source_id: region.source_id.clone(),
        mask_source_id: region.mask_source_id.clone(),
        source_hash: region.source_hash.clone(),
        mask_hash: region.mask_hash.clone(),
        mask_file_hash: region.mask_file_hash.clone(),
        encoder_hash: region.encoder_hash.clone(),
        decoder_hash: region.decoder_hash.clone(),
        runtime_hash: region.runtime_hash.clone(),
        prompt_hash: region.prompt_hash.clone(),
        points: region.points.clone(),
    }
}
pub(super) fn validate(input: &Input<'_>, context: &Context<'_>) -> Result<String> {
    ensure!(
        input.frames > 0 && input.frames <= 360000 && input.corrections.len() <= 1024,
        "invalid tracked correction count"
    );
    let mut strokes = 0usize;
    let mut points = 0usize;
    for edit in input.corrections {
        strokes = strokes
            .checked_add(edit.correction.strokes.len())
            .ok_or_else(|| anyhow::anyhow!("stroke budget overflow"))?;
        for stroke in &edit.correction.strokes {
            points = points
                .checked_add(stroke.points.len())
                .ok_or_else(|| anyhow::anyhow!("point budget overflow"))?;
        }
        ensure!(
            strokes <= 1024 && points <= 65536,
            "tracked correction stroke/point budget"
        );
        edit.correction.validate()?;
    }
    ensure!(
        !context.regions.is_empty()
            && context.regions.len() <= 1024
            && 65536u64 * input.frames * context.regions.len() as u64 <= 64 * 1024 * 1024 * 1024,
        "tracked region inventory budget"
    );
    // Reuse ordinal/stroke budgets, then validate the ORIGINAL bound annotations.
    // Only this validation copy loses annotations; authored request/hash retain them.
    let mut plain = input.corrections.to_vec();
    for edit in &mut plain {
        for stroke in &mut edit.correction.strokes {
            stroke.region = None;
        }
    }
    super::local_person_correct::validate_edits(&plain, input.frames)?;
    let mut supplied = BTreeSet::new();
    for region in context.regions {
        let a = region.attachment;
        a.validate()?;
        ensure!(
            region.path.is_absolute()
                && a.source_hash == context.encoded_source_hash
                && (a.width, a.height, a.frames) == (input.width, input.height, input.frames)
                && a.frame_rate == context.frame_rate
                && supplied.insert(a.mask_source_id.as_str()),
            "tracked region source/layout/clock binding changed"
        );
    }
    let mut used = BTreeSet::new();
    for edit in input.corrections {
        edit.correction.validate()?;
        for stroke in &edit.correction.strokes {
            if let Some(plane) = &stroke.region {
                let stream = context
                    .regions
                    .iter()
                    .find(|r| r.attachment.mask_source_id == plane.mask_source_id)
                    .ok_or_else(|| anyhow::anyhow!("unbound tracked region stroke"))?;
                let mut binding = plane.clone();
                binding.mask_hash = stream.attachment.mask_hash.clone();
                ensure!(
                    binding == provenance(stream.attachment),
                    "tracked region provenance changed"
                );
                used.insert(plane.mask_source_id.as_str());
            }
        }
    }
    ensure!(used == supplied, "unused tracked region stream");
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(input.corrections)?)
    ))
}
pub(super) fn hashes(context: &Context<'_>) -> BTreeMap<String, String> {
    context
        .regions
        .iter()
        .map(|r| {
            (
                r.attachment.mask_source_id.clone(),
                r.attachment.mask_hash.clone(),
            )
        })
        .collect()
}
pub(super) fn payload(context: &Context<'_>) -> serde_json::Value {
    serde_json::json!(context
        .regions
        .iter()
        .map(|r| serde_json::json!({"input":r.path,"attachment":provenance(r.attachment)}))
        .collect::<Vec<_>>())
}
pub(super) async fn verify(input: &Input<'_>, context: &Context<'_>) -> Result<()> {
    for stream in context.regions {
        let meta = tokio::fs::symlink_metadata(stream.path).await?;
        let expected = 65536 * input.frames;
        ensure!(
            meta.file_type().is_file() && meta.len() == expected,
            "invalid raw tracked region file"
        );
        let digest =
            crate::project_archive::fingerprint_file_with_limit(stream.path, expected).await?;
        ensure!(
            digest.0 == expected && digest.1 == stream.attachment.mask_hash,
            "tracked region stream changed"
        );
        let mut frames = BTreeMap::<u64, Vec<&str>>::new();
        for edit in input.corrections {
            for stroke in &edit.correction.strokes {
                if let Some(plane) = stroke
                    .region
                    .as_ref()
                    .filter(|r| r.mask_source_id == stream.attachment.mask_source_id)
                {
                    frames.entry(edit.frame).or_default().push(&plane.mask_hash);
                }
            }
        }
        let mut file = tokio::fs::File::open(stream.path).await?;
        let mut bytes = vec![0; 65536];
        for (ordinal, hashes) in frames {
            file.seek(std::io::SeekFrom::Start(ordinal * 65536)).await?;
            file.read_exact(&mut bytes).await?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            ensure!(
                hashes.iter().all(|h| *h == hash),
                "tracked region frame binding changed"
            );
        }
    }
    Ok(())
}
