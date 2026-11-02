//! Model-free ordered replay of cached RVM masks with sparse ordinal corrections.
use super::{
    sequence::{hash_file, MAX_BYTES},
    Temporary,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use video_kadr_vision_runtime::{
    mask_apply::{self, Mapping},
    mask_edit::{self, Correction},
    object_matting::Mask,
    person_matting::working_size,
    Image,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    input: PathBuf,
    mask: PathBuf,
    output: PathBuf,
    mask_output: PathBuf,
    source_sha256: String,
    mask_sha256: String,
    width: usize,
    height: usize,
    frames: u64,
    corrections: Vec<FrameCorrection>,
    #[serde(default)]
    regions: Vec<RegionStream>,
    #[serde(default)]
    region_source_sha256: Option<String>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FrameCorrection {
    frame: u64,
    correction: Correction,
}
/// Attachment mask_hash identifies the COMPLETE raw stream; authored strokes
/// retain individual plane hashes. Encoded/source provenance is supplied by host.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegionStream {
    input: PathBuf,
    attachment: mask_edit::RegionAttachment,
}
impl Request {
    fn validate_regions(&self) -> Result<()> {
        ensure!(
            self.regions.len() <= 1024
                && 65536u64 * self.frames * self.regions.len() as u64 <= MAX_BYTES,
            "region stream inventory budget"
        );
        ensure!(
            self.regions.is_empty() == self.region_source_sha256.is_none(),
            "missing/unexpected encoded source identity"
        );
        let mut supplied = std::collections::BTreeSet::new();
        for stream in &self.regions {
            stream.attachment.validate()?;
            ensure!(
                stream.input.is_absolute()
                    && supplied.insert(stream.attachment.mask_source_id.as_str())
                    && Some(&stream.attachment.source_hash) == self.region_source_sha256.as_ref(),
                "invalid region stream/source binding"
            );
        }
        let mut used = std::collections::BTreeSet::new();
        for frame in &self.corrections {
            for stroke in &frame.correction.strokes {
                if let Some(plane) = &stroke.region {
                    plane.validate()?;
                    ensure!(
                        stroke.is_smart() && plane.points == stroke.points,
                        "invalid region stroke tool/prompts"
                    );
                    let stream = self
                        .regions
                        .iter()
                        .find(|s| s.attachment.mask_source_id == plane.mask_source_id)
                        .context("unbound tracked region")?;
                    let mut identity = plane.clone();
                    identity.mask_hash = stream.attachment.mask_hash.clone();
                    ensure!(
                        identity == stream.attachment,
                        "tracked region provenance changed"
                    );
                    used.insert(plane.mask_source_id.as_str());
                }
            }
        }
        ensure!(used == supplied, "unused tracked region stream");
        Ok(())
    }
    fn verify_regions(&self) -> Result<()> {
        for stream in &self.regions {
            verify(
                &stream.input,
                65536 * self.frames,
                &stream.attachment.mask_hash,
            )?;
        }
        Ok(())
    }
    fn load_regions(
        &self,
        correction: &Correction,
        ordinal: u64,
    ) -> Result<BTreeMap<String, Mask>> {
        let mut loaded = BTreeMap::new();
        for stroke in &correction.strokes {
            if let Some(region) = &stroke.region {
                if !loaded.contains_key(&region.mask_source_id) {
                    let stream = self
                        .regions
                        .iter()
                        .find(|s| s.attachment.mask_source_id == region.mask_source_id)
                        .context("unbound tracked region")?;
                    let mut file = File::open(&stream.input)?;
                    file.seek(SeekFrom::Start(ordinal * 65536))?;
                    let mut bytes = vec![0; 65536];
                    file.read_exact(&mut bytes)?;
                    loaded.insert(
                        region.mask_source_id.clone(),
                        Mask {
                            width: 256,
                            height: 256,
                            bytes,
                        },
                    );
                }
                let mask = &loaded[&region.mask_source_id];
                ensure!(
                    format!("{:x}", Sha256::digest(&mask.bytes)) == region.mask_hash,
                    "tracked region plane identity changed"
                );
            }
        }
        Ok(loaded)
    }
}
fn digest_ok(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
impl Request {
    fn layout(&self) -> Result<(usize, usize, usize, u64, u64)> {
        ensure!(
            self.schema_version == 8 && self.frames > 0 && self.frames <= 360000,
            "invalid cached person sequence schema/count"
        );
        ensure!(
            [&self.input, &self.mask, &self.output, &self.mask_output]
                .iter()
                .all(|p| p.is_absolute())
                && self.output != self.mask_output,
            "invalid cached sequence paths"
        );
        ensure!(
            digest_ok(&self.source_sha256) && digest_ok(&self.mask_sha256),
            "invalid cached sequence identity"
        );
        let (mw, mh) = working_size(self.width, self.height)?;
        let frame = self
            .width
            .checked_mul(self.height)
            .and_then(|n| n.checked_mul(4))
            .context("RGBA overflow")?;
        let bytes = (frame as u64)
            .checked_mul(self.frames)
            .filter(|n| *n <= MAX_BYTES)
            .context("RGBA budget")?;
        let masks = (mw as u64 * mh as u64)
            .checked_mul(self.frames)
            .filter(|n| *n <= MAX_BYTES)
            .context("mask budget")?;
        ensure!(self.corrections.len() <= 1024, "too many corrected frames");
        let mut seen = std::collections::BTreeSet::new();
        let mut points = 0usize;
        let mut strokes = 0usize;
        for entry in &self.corrections {
            ensure!(
                entry.frame < self.frames && seen.insert(entry.frame),
                "duplicate/out-of-range corrected frame"
            );
            let c = &entry.correction;
            ensure!(
                c.feather.is_finite() && (0.0..=0.1).contains(&c.feather),
                "invalid feather"
            );
            strokes = strokes
                .checked_add(c.strokes.len())
                .context("stroke overflow")?;
            for stroke in &c.strokes {
                points = points
                    .checked_add(stroke.points.len())
                    .context("point overflow")?;
                ensure!(
                    stroke.at.is_none(),
                    "ordinal replay requires untimed strokes"
                );
                ensure!(
                    stroke.size.is_finite()
                        && (0.005..=0.5).contains(&stroke.size)
                        && !stroke.points.is_empty()
                        && stroke
                            .points
                            .iter()
                            .flatten()
                            .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
                    "invalid correction stroke"
                );
            }
        }
        ensure!(
            points <= 65536 && strokes <= 1024,
            "sequence correction budget"
        );
        self.validate_regions()?;
        Ok((mw, mh, frame, bytes, masks))
    }
}
fn private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn verify(path: &Path, size: u64, hash: &str) -> Result<()> {
    let info = std::fs::symlink_metadata(path)?;
    ensure!(
        info.file_type().is_file() && info.len() == size && hash_file(path, MAX_BYTES)? == hash,
        "cached sequence input changed"
    );
    Ok(())
}
pub(super) fn run(request: Request) -> Result<()> {
    let (mw, mh, frame, bytes, mask_bytes) = request.layout()?;
    let correction_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&request.corrections)?)
    );
    ensure!(
        !request.output.try_exists()? && !request.mask_output.try_exists()?,
        "cached output already exists"
    );
    verify(&request.input, bytes, &request.source_sha256)?;
    verify(&request.mask, mask_bytes, &request.mask_sha256)?;
    request.verify_regions()?;
    let parent = request.output.parent().context("output parent missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let rgba_tmp = Temporary(parent.join(format!(
        ".person-correct-{}-{nonce}.rgba",
        std::process::id()
    )));
    let mask_tmp = Temporary(parent.join(format!(
        ".person-correct-{}-{nonce}.masks",
        std::process::id()
    )));
    let (mut rgba_out, mut mask_out) = (private(&rgba_tmp.0)?, private(&mask_tmp.0)?);
    let (mut input, mut masks) = (File::open(&request.input)?, File::open(&request.mask)?);
    let corrections: BTreeMap<_, _> = request
        .corrections
        .iter()
        .map(|v| (v.frame, &v.correction))
        .collect();
    let mut source_hash = Sha256::new();
    let mut original_masks = Sha256::new();
    let mut output_hash = Sha256::new();
    let mut mask_hash = Sha256::new();
    let mut blanks = 0u64;
    let cancel = AtomicBool::new(false);
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":0,"total":request.frames})
    );
    for ordinal in 0..request.frames {
        let mut rgba = vec![0; frame];
        input.read_exact(&mut rgba)?;
        source_hash.update(&rgba);
        let image = Image::new(request.width, request.height, rgba)?;
        let mut pixels = vec![0; mw * mh];
        masks.read_exact(&mut pixels)?;
        original_masks.update(&pixels);
        let auto = Mask {
            width: mw,
            height: mh,
            bytes: pixels,
        };
        let resolved = if let Some(c) = corrections.get(&ordinal) {
            let loaded = request.load_regions(c, ordinal)?;
            let regions = c
                .strokes
                .iter()
                .map(|stroke| stroke.region.as_ref().map(|r| &loaded[&r.mask_source_id]))
                .collect::<Vec<_>>();
            mask_edit::resolve(
                &auto,
                c.custom,
                &c.strokes,
                &regions,
                request.width as f32 / request.height as f32,
                c.feather,
                &cancel,
            )?
        } else if auto.is_blank() {
            None
        } else {
            Some(auto.clone())
        };
        let effective = resolved.as_ref().unwrap_or(&auto);
        mask_out.write_all(&effective.bytes)?;
        mask_hash.update(&effective.bytes);
        let result = if let Some(mask) = resolved {
            mask_apply::apply(&image, &mask, Mapping::IDENTITY, &cancel)?
        } else {
            blanks += 1;
            image
        };
        rgba_out.write_all(&result.rgba)?;
        output_hash.update(&result.rgba);
        eprintln!(
            "{}",
            serde_json::json!({"event":"progress","completed":ordinal+1,"total":request.frames})
        );
    }
    let mut extra = [0];
    ensure!(
        input.read(&mut extra)? == 0 && masks.read(&mut extra)? == 0,
        "cached inputs gained frames"
    );
    ensure!(
        format!("{:x}", source_hash.finalize()) == request.source_sha256
            && format!("{:x}", original_masks.finalize()) == request.mask_sha256,
        "cached inputs changed while reading"
    );
    verify(&request.input, bytes, &request.source_sha256)?;
    verify(&request.mask, mask_bytes, &request.mask_sha256)?;
    request.verify_regions()?;
    rgba_out.sync_all()?;
    mask_out.sync_all()?;
    drop(rgba_out);
    drop(mask_out);
    struct Published(Option<PathBuf>);
    impl Drop for Published {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut published = Published(None);
    std::fs::hard_link(&mask_tmp.0, &request.mask_output)?;
    published.0 = Some(request.mask_output.clone());
    std::fs::hard_link(&rgba_tmp.0, &request.output)?;
    published.0 = None;
    let mut metadata = serde_json::json!({"schemaVersion":8,"width":request.width,"height":request.height,"frames":request.frames,"bytes":bytes,"maskWidth":mw,"maskHeight":mh,"maskBytes":mask_bytes,"blankFrames":blanks,"sourceHash":request.source_sha256,"sourceMaskHash":request.mask_sha256,"correctionHash":correction_hash,"outputHash":format!("{:x}",output_hash.finalize()),"maskHash":format!("{:x}",mask_hash.finalize())});
    if !request.regions.is_empty() {
        metadata["regionHashes"] = serde_json::to_value(
            request
                .regions
                .iter()
                .map(|s| (&s.attachment.mask_source_id, &s.attachment.mask_hash))
                .collect::<BTreeMap<_, _>>(),
        )?;
        metadata["regionSourceHash"] = serde_json::to_value(&request.region_source_sha256)?;
    }
    println!("{metadata}");
    Ok(())
}
