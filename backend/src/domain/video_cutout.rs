//! Portable original-video/automatic-mask identity and sparse authored recipes.
use super::{composition::FrameRateRatio, cutout::Correction};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameCorrection {
    pub frame: u64,
    pub correction: Correction,
}
pub fn validate_corrections(edits: &[FrameCorrection], frames: u64) -> Result<String> {
    validate_inner(edits, frames, false)
}
/// Authored frame regions must resolve to the saved immutable inventory. Unused
/// inventory entries remain available for later editing and are not dispatched.
pub fn validate_corrections_with_regions(
    edits: &[FrameCorrection],
    frames: u64,
    regions: &[super::video_region::Attachment],
) -> Result<String> {
    let digest = validate_inner(edits, frames, true)?;
    ensure!(regions.len() <= 1024, "too many retained video regions");
    let mut ids = std::collections::BTreeMap::new();
    let mut points = 0usize;
    for region in regions {
        region.validate()?;
        points += region.points.len();
        ensure!(
            points <= 65536
                && region.frames == frames
                && ids.insert(region.mask_source_id.as_str(), region).is_none(),
            "invalid retained region inventory"
        );
    }
    ensure!(
        65536u64 * frames * regions.len() as u64 <= 64 * 1024 * 1024 * 1024,
        "retained region byte budget"
    );
    for edit in edits {
        for stroke in &edit.correction.strokes {
            if let Some(plane) = &stroke.region {
                let region = ids
                    .get(plane.mask_source_id.as_str())
                    .ok_or_else(|| anyhow::anyhow!("unbound tracked frame region"))?;
                let mut actual = plane.clone();
                actual.mask_hash = region.mask_hash.clone();
                let expected = super::cutout::RegionAttachment {
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
                };
                ensure!(
                    actual == expected,
                    "tracked frame region provenance changed"
                );
            }
        }
    }
    Ok(digest)
}
fn validate_inner(edits: &[FrameCorrection], frames: u64, allow_regions: bool) -> Result<String> {
    ensure!(
        frames > 0 && frames <= 360000 && edits.len() <= 1024,
        "invalid corrected frame count"
    );
    let mut seen = std::collections::BTreeSet::new();
    let (mut strokes, mut points) = (0usize, 0usize);
    for edit in edits {
        ensure!(
            edit.frame < frames && seen.insert(edit.frame),
            "duplicate/out-of-range correction frame"
        );
        edit.correction.validate()?;
        strokes += edit.correction.strokes.len();
        for s in &edit.correction.strokes {
            points += s.points.len();
            ensure!(
                s.at.is_none() && (allow_regions || s.region.is_none()),
                "unbound temporal/region correction"
            );
        }
    }
    ensure!(
        strokes <= 1024 && points <= 65536,
        "sequence correction budget"
    );
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(edits)?)))
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, try_from = "Wire")]
pub struct Attachment {
    pub source_id: String,
    pub mask_source_id: String,
    pub source_hash: String,
    pub mask_hash: String,
    pub mask_file_hash: String,
    pub width: u32,
    pub height: u32,
    pub mask_width: u32,
    pub mask_height: u32,
    pub frames: u64,
    pub frame_rate: FrameRateRatio,
    pub corrections: Vec<FrameCorrection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracked_regions: Vec<super::video_region::Attachment>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire {
    source_id: String,
    mask_source_id: String,
    source_hash: String,
    mask_hash: String,
    mask_file_hash: String,
    width: u32,
    height: u32,
    mask_width: u32,
    mask_height: u32,
    frames: u64,
    frame_rate: FrameRateRatio,
    corrections: Vec<FrameCorrection>,
    #[serde(default)]
    tracked_regions: Vec<super::video_region::Attachment>,
}
impl TryFrom<Wire> for Attachment {
    type Error = anyhow::Error;
    fn try_from(w: Wire) -> Result<Self> {
        let a = Self {
            source_id: w.source_id,
            mask_source_id: w.mask_source_id,
            source_hash: w.source_hash,
            mask_hash: w.mask_hash,
            mask_file_hash: w.mask_file_hash,
            width: w.width,
            height: w.height,
            mask_width: w.mask_width,
            mask_height: w.mask_height,
            frames: w.frames,
            frame_rate: w.frame_rate,
            corrections: w.corrections,
            tracked_regions: w.tracked_regions,
        };
        a.validate()?;
        Ok(a)
    }
}
impl Attachment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.source_id.is_empty()
                && !self.mask_source_id.is_empty()
                && self.source_id != self.mask_source_id,
            "invalid video mask source IDs"
        );
        for hash in [&self.source_hash, &self.mask_hash, &self.mask_file_hash] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid video mask digest"
            );
        }
        ensure!(
            self.width > 0
                && self.height > 0
                && u64::from(self.width) * u64::from(self.height) <= 64 * 1024 * 1024,
            "invalid video mask source dimensions"
        );
        let scale = 512. / f64::from(self.width.max(self.height));
        let edge = |v: u32| (((f64::from(v) * scale / 16.).round() as u32) * 16).max(64);
        ensure!(
            (self.mask_width, self.mask_height) == (edge(self.width), edge(self.height)),
            "invalid original video mask dimensions"
        );
        ensure!(
            self.frame_rate.numerator > 0 && self.frame_rate.denominator > 0,
            "invalid video mask frame clock"
        );
        validate_corrections_with_regions(&self.corrections, self.frames, &self.tracked_regions)?;
        ensure!(
            self.tracked_regions.len() <= 1024,
            "too many retained video regions"
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut points = 0usize;
        for region in &self.tracked_regions {
            region.validate()?;
            points += region.points.len();
            ensure!(
                region.source_hash == self.source_hash
                    && region.width == self.width
                    && region.height == self.height
                    && region.frames == self.frames
                    && region.frame_rate == self.frame_rate
                    && region.mask_source_id != self.mask_source_id
                    && region.mask_source_id != self.source_id
                    && ids.insert(&region.mask_source_id),
                "retained region belongs to another video/mask"
            );
        }
        ensure!(
            points <= 65536
                && 65536u64 * self.frames * self.tracked_regions.len() as u64
                    <= 64 * 1024 * 1024 * 1024,
            "retained video region budget"
        );

        ensure!(
            u64::from(self.width) * u64::from(self.height) * 4 * self.frames
                <= 64 * 1024 * 1024 * 1024,
            "video source byte budget"
        );
        ensure!(
            u64::from(self.mask_width) * u64::from(self.mask_height) * self.frames
                <= 64 * 1024 * 1024 * 1024,
            "video mask byte budget"
        );
        Ok(())
    }
}
