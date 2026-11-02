//! Bidirectional source-ordinal SAM region stream; seed is immutable host input.
use super::{
    digest, regular,
    sequence::{hash_file, MAX_BYTES},
    Temporary,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::AtomicBool,
};
use video_kadr_vision_runtime::{
    object_matting::Mask,
    slim_sam::{Brush, DECODER_BYTES, DECODER_SHA256, ENCODER_BYTES, ENCODER_SHA256},
    Image,
};
const PLANE: usize = 256 * 256;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Exemplar {
    frame: u64,
    mask: PathBuf,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    encoder: PathBuf,
    decoder: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    seed: PathBuf,
    output: PathBuf,
    source_sha256: String,
    seed_sha256: String,
    width: usize,
    height: usize,
    frames: u64,
    seed_frame: u64,
    #[serde(default)]
    exemplars: Vec<Exemplar>,
}
impl Request {
    fn layout(&self) -> Result<(usize, u64, u64)> {
        ensure!(
            matches!(self.schema_version, 14 | 15)
                && self.frames > 0
                && self.frames <= 300
                && self.seed_frame < self.frames,
            "invalid SAM tracking schema/frame count"
        );
        ensure!(
            [
                &self.encoder,
                &self.decoder,
                &self.runtime_library,
                &self.input,
                &self.seed,
                &self.output
            ]
            .iter()
            .all(|p| p.is_absolute())
                && !self.output.try_exists()?,
            "invalid SAM tracking paths/output"
        );
        ensure!(
            [&self.source_sha256, &self.seed_sha256]
                .iter()
                .all(|v| v.len() == 64
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "invalid SAM tracking identities"
        );
        let frame = self
            .width
            .checked_mul(self.height)
            .filter(|n| self.width > 0 && self.height > 0 && *n <= 64 * 1024 * 1024)
            .and_then(|n| n.checked_mul(4))
            .context("invalid SAM tracking source shape")?;
        let source = (frame as u64)
            .checked_mul(self.frames)
            .context("SAM source bytes overflow")?;
        let masks = (PLANE as u64)
            .checked_mul(self.frames)
            .context("SAM masks overflow")?;
        ensure!(
            source.checked_add(masks).is_some_and(|n| n <= MAX_BYTES),
            "SAM tracking staging budget"
        );
        ensure!(
            self.exemplars.len() <= 12 && (self.schema_version == 15 || self.exemplars.is_empty()),
            "invalid identity exemplar schema/count"
        );
        let mut previous = self.seed_frame;
        for view in &self.exemplars {
            ensure!(
                view.frame > previous
                    && view.frame < self.frames
                    && view.mask.is_absolute()
                    && view.sha256.len() == 64
                    && view
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid source-owned exemplar"
            );
            previous = view.frame;
        }
        Ok((frame, source, masks))
    }
}
fn progress(done: u64, total: u64) {
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":done,"total":total})
    );
}
pub(super) fn run(request: Request) -> Result<()> {
    let (frame_bytes, source_bytes, mask_bytes) = request.layout()?;
    ensure!(
        std::fs::symlink_metadata(&request.input)?
            .file_type()
            .is_file()
            && std::fs::metadata(&request.input)?.len() == source_bytes,
        "invalid SAM tracking source length"
    );
    ensure!(
        hash_file(&request.input, MAX_BYTES)? == request.source_sha256,
        "SAM tracking source identity mismatch"
    );
    let seed = regular(&request.seed, PLANE as u64)?;
    ensure!(
        seed.len() == PLANE && digest(&seed) == request.seed_sha256,
        "SAM tracking seed identity mismatch"
    );
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load SAM tracking runtime: {e}"))?
        .commit();
    let mut brush = Brush::load(&request.encoder, &request.decoder)?;
    let cancel = AtomicBool::new(false);
    let parent = request
        .output
        .parent()
        .context("SAM tracking output parent missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = Temporary(parent.join(format!(
        ".sam-reidentify-{}-{nonce}.masks",
        std::process::id()
    )));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut writer = options.open(&temporary.0)?;
    writer.set_len(mask_bytes)?;
    writer.seek(SeekFrom::Start(request.seed_frame * PLANE as u64))?;
    writer.write_all(&seed)?;
    let mut reader = File::open(&request.input)?;
    reader.seek(SeekFrom::Start(request.seed_frame * frame_bytes as u64))?;
    let mut seed_rgba = vec![0; frame_bytes];
    reader.read_exact(&mut seed_rgba)?;
    let seed_image = Image::new(request.width, request.height, seed_rgba)?;
    let seed_embedding = brush.embed(&seed_image, &cancel)?;
    let seed_mask = Mask {
        width: 256,
        height: 256,
        bytes: seed.clone(),
    };
    let mut identity = video_kadr_vision_runtime::sam_reidentification::Identity::from_seed(
        &seed_embedding,
        &seed_image,
        &seed_mask,
        &cancel,
    )?;
    // These are propagated saved masks, not human identity annotations. Content
    // integrity is necessary but insufficient: only a contiguous unique neural
    // match agreeing with the saved mask can enter the bounded frozen bank.
    let mut admitted_views = vec![];
    let mut previous_mask = seed_mask.clone();
    let mut previous_image = seed_image.clone();
    let mut previous_frame = request.seed_frame;
    for view in &request.exemplars {
        let bytes = regular(&view.mask, PLANE as u64)?;
        ensure!(
            bytes.len() == PLANE && digest(&bytes) == view.sha256,
            "identity exemplar content changed"
        );
        if view.frame != previous_frame + 1 {
            break;
        }
        let saved = Mask {
            width: 256,
            height: 256,
            bytes,
        };
        if video_kadr_vision_runtime::sam_tracking::overlap(&previous_mask, &saved, &cancel)? < 0.25
        {
            break;
        }
        reader.seek(SeekFrom::Start(view.frame * frame_bytes as u64))?;
        let mut rgba = vec![0; frame_bytes];
        reader.read_exact(&mut rgba)?;
        let image = Image::new(request.width, request.height, rgba)?;
        let embedding = brush.embed(&image, &cancel)?;
        let (state, found) = brush.reidentify_visible(
            &embedding,
            &image,
            &identity,
            Some((&previous_image, &previous_mask)),
            &cancel,
        )?;
        if state != video_kadr_vision_runtime::sam_reidentification::State::Tracked {
            break;
        }
        let Some(found) = found else {
            break;
        };
        if video_kadr_vision_runtime::sam_tracking::overlap(&found, &saved, &cancel)? < 0.5 {
            break;
        }
        if let Some(slot) = identity.enroll_admitted_view(&embedding, &image, &found, &cancel)? {
            if slot < admitted_views.len() {
                admitted_views[slot] = view.frame;
            } else {
                admitted_views.push(view.frame);
            }
        }
        previous_mask = saved;
        previous_image = image;
        previous_frame = view.frame;
    }
    use video_kadr_vision_runtime::sam_reidentification::State;
    let mut states = vec![State::Occluded; request.frames as usize];
    states[request.seed_frame as usize] = State::Seed;
    let mut done = 1;
    progress(done, request.frames);
    for forward in [true, false] {
        let mut previous = Some(seed_mask.clone());
        let mut previous_image = Some(seed_image.clone());
        let ordinals: Box<dyn Iterator<Item = u64>> = if forward {
            Box::new(request.seed_frame + 1..request.frames)
        } else {
            Box::new((0..request.seed_frame).rev())
        };
        for ordinal in ordinals {
            reader.seek(SeekFrom::Start(ordinal * frame_bytes as u64))?;
            let mut rgba = vec![0; frame_bytes];
            reader.read_exact(&mut rgba)?;
            let image = Image::new(request.width, request.height, rgba)?;
            let embedding = brush.embed(&image, &cancel)?;
            let (state, mask) = brush.reidentify_visible(
                &embedding,
                &image,
                &identity,
                previous_image.as_ref().zip(previous.as_ref()),
                &cancel,
            )?;
            states[ordinal as usize] = state;
            if let Some(mask) = &mask {
                writer.seek(SeekFrom::Start(ordinal * PLANE as u64))?;
                writer.write_all(&mask.bytes)?;
            }
            previous_image = mask.as_ref().map(|_| image);
            previous = mask;
            done += 1;
            progress(done, request.frames);
        }
    }
    writer.sync_all()?;
    drop(writer);
    let output_hash = hash_file(&temporary.0, MAX_BYTES)?;
    ensure!(
        hash_file(&request.input, MAX_BYTES)? == request.source_sha256
            && digest(&regular(&request.seed, PLANE as u64)?) == request.seed_sha256
            && digest(&regular(&request.encoder, ENCODER_BYTES)?) == ENCODER_SHA256
            && digest(&regular(&request.decoder, DECODER_BYTES)?) == DECODER_SHA256
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "SAM tracking source/seed/model/runtime changed"
    );
    for view in &request.exemplars {
        ensure!(
            digest(&regular(&view.mask, PLANE as u64)?) == view.sha256,
            "identity exemplar changed during inference"
        );
    }
    std::fs::hard_link(&temporary.0, &request.output)
        .context("publish tracked regions without replacement")?;
    admitted_views.sort_unstable();
    let mut metadata = serde_json::json!({"schemaVersion":request.schema_version,"width":request.width,"height":request.height,"frames":request.frames,"seedFrame":request.seed_frame,"maskWidth":256,"maskHeight":256,"states":states,"exemplarHashes":request.exemplars.iter().map(|v| &v.sha256).collect::<Vec<_>>(),"admittedViews":admitted_views,"sourceHash":request.source_sha256,"seedHash":request.seed_sha256,"encoderHash":ENCODER_SHA256,"decoderHash":DECODER_SHA256,"runtimeHash":runtime_hash,"maskBytes":mask_bytes,"outputHash":output_hash});
    if request.schema_version == 14 {
        metadata.as_object_mut().unwrap().remove("exemplarHashes");
        metadata.as_object_mut().unwrap().remove("admittedViews");
    }
    println!("{}", metadata);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload(schema: u32) -> serde_json::Value {
        serde_json::json!({"schemaVersion":schema,"encoder":"/tmp/encoder","decoder":"/tmp/decoder","runtimeLibrary":"/tmp/ort","input":"/tmp/rgba","seed":"/tmp/seed","output":format!("/tmp/unpublished-reid-{}",std::process::id()),"sourceSha256":"a".repeat(64),"seedSha256":"b".repeat(64),"width":64,"height":32,"frames":5,"seedFrame":0})
    }
    #[test]
    fn schema14_is_preserved_and_multiview_inputs_are_bounded_and_ordered() {
        assert!(serde_json::from_value::<Request>(payload(14))
            .unwrap()
            .layout()
            .is_ok());
        let mut value = payload(15);
        value["exemplars"] = serde_json::json!([{"frame":1,"mask":"/tmp/view1","sha256":"c".repeat(64)},{"frame":2,"mask":"/tmp/view2","sha256":"d".repeat(64)}]);
        assert!(serde_json::from_value::<Request>(value.clone())
            .unwrap()
            .layout()
            .is_ok());
        value["schemaVersion"] = serde_json::json!(14);
        assert!(serde_json::from_value::<Request>(value.clone())
            .unwrap()
            .layout()
            .is_err());
        value["schemaVersion"] = serde_json::json!(15);
        value["exemplars"][1]["frame"] = serde_json::json!(1);
        assert!(serde_json::from_value::<Request>(value.clone())
            .unwrap()
            .layout()
            .is_err());
        value["exemplars"] = serde_json::json!((0..13)
            .map(|i| serde_json::json!({"frame":i+1,"mask":"/tmp/view","sha256":"c".repeat(64)}))
            .collect::<Vec<_>>());
        assert!(serde_json::from_value::<Request>(value)
            .unwrap()
            .layout()
            .is_err());
    }
}
