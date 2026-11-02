//! Ordered recurrent person masks and original-size alpha RGBA, one loaded model.
use super::sequence::{hash_file, MAX_BYTES};
use super::{digest, regular, Temporary};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use video_kadr_vision_runtime::{
    mask_apply::{self, Mapping},
    person_matting::{working_size, PersonSegmenter, MODEL_BYTES, MODEL_SHA256},
    Image,
};
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Family {
    Selfie,
    Object,
}
enum Engine {
    Person(PersonSegmenter),
    Selfie(video_kadr_vision_runtime::selfie_matting::SelfieSegmenter),
    Object(video_kadr_vision_runtime::object_matting::ObjectSegmenter),
}
impl Engine {
    fn load(family: Option<Family>, model: &Path) -> Result<Self> {
        Ok(match family {
            None => Self::Person(PersonSegmenter::load(model)?),
            Some(Family::Selfie) => Self::Selfie(
                video_kadr_vision_runtime::selfie_matting::SelfieSegmenter::load(model)?,
            ),
            Some(Family::Object) => Self::Object(
                video_kadr_vision_runtime::object_matting::ObjectSegmenter::load(model)?,
            ),
        })
    }
    fn begin(&mut self) {
        if let Self::Person(engine) = self {
            engine.begin()
        }
    }
    fn mask(
        &mut self,
        image: &Image,
        ordinal: u64,
        cancel: &AtomicBool,
    ) -> Result<video_kadr_vision_runtime::object_matting::Mask> {
        match self {
            Self::Person(engine) => engine.mask(image, ordinal, cancel),
            Self::Object(engine) => engine.mask(image, cancel),
            Self::Selfie(engine) => {
                let mask = engine.mask(image, cancel)?;
                Ok(video_kadr_vision_runtime::object_matting::Mask {
                    width: mask.width,
                    height: mask.height,
                    bytes: mask.bytes,
                })
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    #[serde(default)]
    family: Option<Family>,
    model: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    output: PathBuf,
    source_sha256: String,
    width: usize,
    height: usize,
    frames: u64,
    #[serde(default)]
    mask_output: Option<PathBuf>,
    #[serde(default)]
    checkpoint_output: Option<PathBuf>,
}
impl Request {
    fn dimensions(&self) -> Result<(usize, usize, usize, u64, u64)> {
        ensure!(
            ((self.schema_version == 7 && self.family.is_none())
                || (self.schema_version == 10 && self.family.is_some()))
                && self.frames > 0
                && self.frames <= 360000,
            "invalid person sequence schema/count"
        );
        let (mw, mh) = working_size(self.width, self.height)?;
        let (mw, mh) = match self.family {
            None => (mw, mh),
            Some(Family::Selfie) => (256, 256),
            Some(Family::Object) => (512, 512),
        };
        let frame = self
            .width
            .checked_mul(self.height)
            .and_then(|n| n.checked_mul(4))
            .context("person frame overflow")?;
        let bytes = (frame as u64)
            .checked_mul(self.frames)
            .filter(|n| *n <= MAX_BYTES)
            .context("person sequence exceeds staging budget")?;
        let masks = (mw as u64 * mh as u64)
            .checked_mul(self.frames)
            .filter(|n| *n <= MAX_BYTES)
            .context("person masks exceed staging budget")?;
        ensure!(
            [
                &self.model,
                &self.runtime_library,
                &self.input,
                &self.output
            ]
            .iter()
            .all(|p| p.is_absolute()),
            "person host paths must be absolute"
        );
        ensure!(
            self.source_sha256.len() == 64
                && self
                    .source_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "invalid person sequence digest"
        );
        if let Some(path) = &self.mask_output {
            ensure!(
                path.is_absolute() && path != &self.output && !path.try_exists()?,
                "invalid person mask destination"
            );
        }
        if let Some(root) = &self.checkpoint_output {
            super::person_checkpoint::validate_root(root)?;
            ensure!(
                bytes.checked_add(masks).is_some_and(|n| n <= MAX_BYTES),
                "checkpoint staging budget exceeded"
            );
        }
        Ok((mw, mh, frame, bytes, masks))
    }
}
pub(super) fn private_file(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn progress(completed: u64, total: u64) {
    eprintln!(
        "{}",
        serde_json::json!({"event":"progress","completed":completed,"total":total})
    );
}
pub(super) fn run(request: Request) -> Result<()> {
    let (mw, mh, frame_bytes, bytes, mask_bytes) = request.dimensions()?;
    ensure!(
        !request.output.try_exists()?,
        "person output already exists"
    );
    ensure!(
        std::fs::symlink_metadata(&request.input)?.len() == bytes,
        "raw person sequence size mismatch"
    );
    ensure!(
        hash_file(&request.input, MAX_BYTES)? == request.source_sha256,
        "person source digest mismatch"
    );
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load person runtime: {e}"))?
        .commit();
    let (model_bytes, model_hash) = match request.family {
        None => (MODEL_BYTES, MODEL_SHA256),
        Some(Family::Selfie) => (
            video_kadr_vision_runtime::selfie_matting::MODEL_BYTES,
            video_kadr_vision_runtime::selfie_matting::MODEL_SHA256,
        ),
        Some(Family::Object) => (
            video_kadr_vision_runtime::object_matting::MODEL_BYTES,
            video_kadr_vision_runtime::object_matting::MODEL_SHA256,
        ),
    };
    let mut engine = Engine::load(request.family, &request.model)?;
    engine.begin();
    let parent = request
        .output
        .parent()
        .context("person output parent missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = Temporary(parent.join(format!(
        ".person-sequence-{}-{nonce}.rgba",
        std::process::id()
    )));
    let mask_temporary = Temporary(parent.join(format!(
        ".person-sequence-{}-{nonce}.mask",
        std::process::id()
    )));
    let mut writer = private_file(&temporary.0)?;
    let mut mask_writer = if request.mask_output.is_some() {
        Some(private_file(&mask_temporary.0)?)
    } else {
        None
    };
    let mut reader = File::open(&request.input)?;
    let mut input_hash = Sha256::new();
    let mut output_hash = Sha256::new();
    let mut mask_hash = Sha256::new();
    let mut blanks = 0u64;
    let cancel = AtomicBool::new(false);
    progress(0, request.frames);
    for ordinal in 0..request.frames {
        let mut rgba = vec![0u8; frame_bytes];
        reader.read_exact(&mut rgba)?;
        input_hash.update(&rgba);
        let frame_hash = digest(&rgba);
        let image = Image::new(request.width, request.height, rgba)?;
        let mask = engine.mask(&image, ordinal, &cancel)?;
        ensure!(
            (mask.width, mask.height) == (mw, mh),
            "person working size changed"
        );
        mask_hash.update(&mask.bytes);
        if let Some(writer) = &mut mask_writer {
            writer.write_all(&mask.bytes)?;
        }
        let output = if mask.is_blank() {
            blanks += 1;
            image
        } else {
            mask_apply::apply(&image, &mask, Mapping::IDENTITY, &cancel)?
        };
        writer.write_all(&output.rgba)?;
        output_hash.update(&output.rgba);
        if let Some(root) = &request.checkpoint_output {
            super::person_checkpoint::publish(
                root,
                ordinal,
                super::person_checkpoint::Context {
                    source_hash: &request.source_sha256,
                    frame_hash: &frame_hash,
                    model_hash,
                    runtime_hash: &runtime_hash,
                    width: request.width,
                    height: request.height,
                    mask_width: mw,
                    mask_height: mh,
                    total: request.frames,
                },
                &output.rgba,
                &mask.bytes,
            )?;
            eprintln!(
                "{}",
                serde_json::json!({"event":"checkpoint","completed":ordinal+1,"total":request.frames})
            );
        }
        progress(ordinal + 1, request.frames);
    }
    let mut extra = [0u8; 1];
    ensure!(reader.read(&mut extra)? == 0, "person source gained frames");
    ensure!(
        format!("{:x}", input_hash.finalize()) == request.source_sha256
            && hash_file(&request.input, MAX_BYTES)? == request.source_sha256,
        "person source changed"
    );
    ensure!(
        hash_file(&request.model, model_bytes)? == model_hash
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "person engine changed"
    );
    writer.sync_all()?;
    ensure!(
        writer.metadata()?.len() == bytes,
        "person output size mismatch"
    );
    drop(writer);
    if let Some(writer) = mask_writer {
        writer.sync_all()?;
        ensure!(
            writer.metadata()?.len() == mask_bytes,
            "person mask size mismatch"
        );
    }
    struct OwnedMask(Option<PathBuf>);
    impl Drop for OwnedMask {
        fn drop(&mut self) {
            if let Some(path) = &self.0 {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let mut owned_mask = OwnedMask(None);
    if let Some(path) = &request.mask_output {
        std::fs::hard_link(&mask_temporary.0, path)
            .context("publish person masks without replacement")?;
        owned_mask.0 = Some(path.clone());
    }
    std::fs::hard_link(&temporary.0, &request.output)
        .context("publish person RGBA without replacement")?;
    owned_mask.0 = None;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":request.schema_version,"width":request.width,"height":request.height,"frames":request.frames,"bytes":bytes,"maskWidth":mw,"maskHeight":mh,"maskBytes":mask_bytes,"blankFrames":blanks,"sourceHash":request.source_sha256,"modelHash":model_hash,"runtimeHash":runtime_hash,"outputHash":format!("{:x}",output_hash.finalize()),"maskHash":format!("{:x}",mask_hash.finalize())})
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        serde_json::from_value(serde_json::json!({"schemaVersion":7,"model":"/model","runtimeLibrary":"/runtime","input":"/input","output":"/output","sourceSha256":"a".repeat(64),"width":37,"height":23,"frames":4})).unwrap()
    }
    #[test]
    fn independent_family_admission() {
        let mut r = request();
        r.schema_version = 10;
        assert!(r.dimensions().is_err());
        r.family = Some(Family::Selfie);
        assert_eq!(r.dimensions().unwrap().0, 256);
        r.family = Some(Family::Object);
        assert_eq!(r.dimensions().unwrap().0, 512);
        r.schema_version = 7;
        assert!(r.dimensions().is_err());
    }
    #[test]
    fn ordered_sequence_admission() {
        let mut r = request();
        assert_eq!(
            r.dimensions().unwrap(),
            (512, 320, 37 * 23 * 4, 37 * 23 * 4 * 4, 512 * 320 * 4)
        );
        r.frames = 0;
        assert!(r.dimensions().is_err());
        r.frames = 360001;
        assert!(r.dimensions().is_err());
        r = request();
        r.width = 0;
        assert!(r.dimensions().is_err());
        r = request();
        r.frames = 360000;
        r.width = 4096;
        r.height = 4096;
        assert!(r.dimensions().is_err());
        r = request();
        r.output = "relative".into();
        assert!(r.dimensions().is_err());
        r = request();
        r.mask_output = Some(r.output.clone());
        assert!(r.dimensions().is_err());
    }
    #[test]
    fn refuses_unknown_host_inputs() {
        let mut value=serde_json::to_value(serde_json::json!({"schemaVersion":7,"model":"/model","runtimeLibrary":"/runtime","input":"/input","output":"/output","sourceSha256":"a".repeat(64),"width":37,"height":23,"frames":4})).unwrap();
        value["url"] = serde_json::json!("https://invalid");
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
}
