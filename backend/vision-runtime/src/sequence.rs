//! Streaming RGBA sequence protocol for host-owned video decode/encode stages.
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
use video_kadr_vision_runtime::{Enhancer, Image, MODEL_SHA256};
pub(super) const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    model: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    output: PathBuf,
    source_sha256: String,
    width: usize,
    height: usize,
    frames: u64,
    factor: usize,
}
pub(super) fn hash_file(path: &Path, limit: u64) -> Result<String> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= limit,
        "invalid bounded sequence file"
    );
    let mut file = File::open(path)?.take(limit + 1);
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        ensure!(bytes <= limit, "sequence grew beyond limit");
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl Request {
    fn dimensions(&self) -> Result<(usize, usize, usize, u64)> {
        ensure!(
            self.schema_version == 2
                && [1, 2, 4].contains(&self.factor)
                && self.frames > 0
                && self.frames <= 360000,
            "invalid sequence schema/factor/count"
        );
        ensure!(
            self.width > 0
                && self.height > 0
                && self
                    .width
                    .checked_mul(self.height)
                    .is_some_and(|p| p <= 64 * 1024 * 1024),
            "invalid sequence dimensions"
        );
        let w = self.width * self.factor;
        let h = self.height * self.factor;
        ensure!(
            w.checked_mul(h).is_some_and(|p| p <= 64 * 1024 * 1024),
            "enhanced frame exceeds pixel limit"
        );
        let (w, h) = (w & !1, h & !1);
        ensure!(w > 0 && h > 0, "video frame is too small");
        let input_bytes = (self.width as u64) * (self.height as u64) * 4;
        let output_bytes = (w as u64) * (h as u64) * 4;
        ensure!(
            input_bytes
                .checked_mul(self.frames)
                .is_some_and(|b| b <= MAX_BYTES)
                && output_bytes
                    .checked_mul(self.frames)
                    .is_some_and(|b| b <= MAX_BYTES),
            "sequence exceeds staging budget"
        );
        ensure!(
            [
                &self.input,
                &self.output,
                &self.model,
                &self.runtime_library
            ]
            .iter()
            .all(|p| p.is_absolute()),
            "host paths must be absolute"
        );
        ensure!(
            self.source_sha256.len() == 64
                && self.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid sequence digest"
        );
        Ok((w, h, input_bytes as usize, output_bytes * self.frames))
    }
}
pub(super) fn run(request: Request) -> Result<()> {
    let (width, height, frame_bytes, output_bytes) = request.dimensions()?;
    ensure!(!request.output.try_exists()?, "output already exists");
    ensure!(
        std::fs::symlink_metadata(&request.input)?.len() == frame_bytes as u64 * request.frames,
        "raw sequence size mismatch"
    );
    ensure!(
        hash_file(&request.input, MAX_BYTES)? == request.source_sha256,
        "sequence source digest mismatch"
    );
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load host ONNX Runtime: {e}"))?
        .commit();
    let mut enhancer = Enhancer::load(&request.model)?;
    let parent = request
        .output
        .parent()
        .context("sequence output parent missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = Temporary(parent.join(format!(
        ".enhance-sequence-{}-{nonce}.rgba",
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
    let mut reader = File::open(&request.input)?;
    let mut input_hash = Sha256::new();
    let mut output_hash = Sha256::new();
    let tiles = request.width.div_ceil(256) * request.height.div_ceil(256);
    let total = tiles as u64 * request.frames;
    for ordinal in 0..request.frames {
        let mut rgba = vec![0u8; frame_bytes];
        reader.read_exact(&mut rgba)?;
        input_hash.update(&rgba);
        let image = Image::new(request.width, request.height, rgba)?;
        let enhanced=enhancer.enhance(&image,request.factor,&AtomicBool::new(false),|completed,_|{
            eprintln!("{}",serde_json::json!({"event":"progress","completed":ordinal*tiles as u64+completed as u64,"total":total}));
        })?;
        // Concat crops only the right/bottom stray pixel for even video sides.
        for y in 0..height {
            let row = &enhanced.rgba[y * enhanced.width * 4..y * enhanced.width * 4 + width * 4];
            writer.write_all(row)?;
            output_hash.update(row);
        }
    }
    let mut extra = [0u8; 1];
    ensure!(
        reader.read(&mut extra)? == 0,
        "sequence source gained extra frames"
    );
    ensure!(
        format!("{:x}", input_hash.finalize()) == request.source_sha256
            && hash_file(&request.input, MAX_BYTES)? == request.source_sha256,
        "sequence source changed during inference"
    );
    ensure!(
        digest(&regular(&request.model, 4866417)?) == MODEL_SHA256
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "sequence engine changed during inference"
    );
    writer.sync_all()?;
    ensure!(
        writer.metadata()?.len() == output_bytes,
        "sequence output size mismatch"
    );
    drop(writer);
    std::fs::hard_link(&temporary.0, &request.output)
        .context("publish sequence without replacement")?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":2,"width":width,"height":height,"frames":request.frames,"factor":request.factor,"bytes":output_bytes,"sourceHash":request.source_sha256,"modelHash":MODEL_SHA256,"runtimeHash":runtime_hash,"outputHash":format!("{:x}",output_hash.finalize())})
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        serde_json::from_value(serde_json::json!({"schemaVersion":2,"model":"/model","runtimeLibrary":"/runtime","input":"/in","output":"/out","sourceSha256":"0".repeat(64),"width":257,"height":9,"frames":2,"factor":1})).unwrap()
    }
    #[test]
    fn even_crop_and_sequence_budget_admission() {
        let mut r = request();
        assert_eq!(
            r.dimensions().unwrap(),
            (256, 8, 257 * 9 * 4, 256 * 8 * 4 * 2)
        );
        r.factor = 4;
        assert_eq!(r.dimensions().unwrap().0, 1028);
        r.frames = 360001;
        assert!(r.dimensions().is_err());
        r.frames = 360000;
        r.width = 512;
        assert!(r.dimensions().is_err());
        r.frames = 1;
        r.width = 1;
        r.height = 1;
        r.factor = 1;
        assert!(r.dimensions().is_err());
        r = request();
        r.input = "relative".into();
        assert!(r.dimensions().is_err());
    }
    #[test]
    fn strict_sequence_refuses_extra_fields() {
        let mut v=serde_json::to_value(serde_json::json!({"schemaVersion":2,"model":"/model","runtimeLibrary":"/runtime","input":"/in","output":"/out","sourceSha256":"0".repeat(64),"width":257,"height":9,"frames":2,"factor":2})).unwrap();
        v["url"] = serde_json::json!("https://invalid");
        assert!(serde_json::from_value::<Request>(v).is_err());
    }
}
