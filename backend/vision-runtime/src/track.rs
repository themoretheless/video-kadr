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
}
impl Request {
    fn layout(&self) -> Result<(usize, u64, u64)> {
        ensure!(
            self.schema_version == 9
                && self.frames > 0
                && self.frames <= 360000
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
    let temporary =
        Temporary(parent.join(format!(".sam-track-{}-{nonce}.masks", std::process::id())));
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
    let mut done = 1;
    let mut tracked = 1;
    let mut forward_lost = None;
    let mut backward_lost = None;
    progress(0, request.frames);
    progress(done, request.frames);
    for forward in [true, false] {
        let mut previous = Some(Mask {
            width: 256,
            height: 256,
            bytes: seed.clone(),
        });
        let ordinals: Box<dyn Iterator<Item = u64>> = if forward {
            Box::new(request.seed_frame + 1..request.frames)
        } else {
            Box::new((0..request.seed_frame).rev())
        };
        for ordinal in ordinals {
            if let Some(region) = previous.take() {
                reader.seek(SeekFrom::Start(ordinal * frame_bytes as u64))?;
                let mut rgba = vec![0u8; frame_bytes];
                reader.read_exact(&mut rgba)?;
                let image = Image::new(request.width, request.height, rgba)?;
                let embedding = brush.embed(&image, &cancel)?;
                previous = brush.track(&embedding, &region, &cancel)?;
                if let Some(mask) = &previous {
                    ensure!(
                        mask.width == 256 && mask.height == 256 && mask.bytes.len() == PLANE,
                        "tracked region shape changed"
                    );
                    writer.seek(SeekFrom::Start(ordinal * PLANE as u64))?;
                    writer.write_all(&mask.bytes)?;
                    tracked += 1;
                } else if forward {
                    forward_lost = Some(ordinal);
                } else {
                    backward_lost = Some(ordinal);
                }
            }
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
    std::fs::hard_link(&temporary.0, &request.output)
        .context("publish tracked regions without replacement")?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":9,"width":request.width,"height":request.height,"frames":request.frames,"seedFrame":request.seed_frame,"maskWidth":256,"maskHeight":256,"trackedFrames":tracked,"forwardLostAt":forward_lost,"backwardLostAt":backward_lost,"sourceHash":request.source_sha256,"seedHash":request.seed_sha256,"encoderHash":ENCODER_SHA256,"decoderHash":DECODER_SHA256,"runtimeHash":runtime_hash,"maskBytes":mask_bytes,"outputHash":output_hash})
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_tracking_schema_and_layout() {
        let value = serde_json::json!({"schemaVersion":9,"encoder":"/e","decoder":"/d","runtimeLibrary":"/r","input":"/i","seed":"/s","output":"/not-present-sam-track-output","sourceSha256":"a".repeat(64),"seedSha256":"b".repeat(64),"width":37,"height":23,"frames":4,"seedFrame":1});
        assert_eq!(
            serde_json::from_value::<Request>(value.clone())
                .unwrap()
                .layout()
                .unwrap(),
            (37 * 23 * 4, 37 * 23 * 4 * 4, 4 * 65536)
        );
        for (field, v) in [
            ("seedFrame", serde_json::json!(4)),
            ("width", serde_json::json!(0)),
            ("schemaVersion", serde_json::json!(8)),
            ("frames", serde_json::json!(360001)),
        ] {
            let mut bad = value.clone();
            bad[field] = v;
            assert!(serde_json::from_value::<Request>(bad)
                .unwrap()
                .layout()
                .is_err());
        }
        let mut bad = value;
        bad["url"] = serde_json::json!("https://invalid");
        assert!(serde_json::from_value::<Request>(bad).is_err());
    }
}
