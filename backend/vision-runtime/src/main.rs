//! Host-owned still-image or streaming video-frame enhancement per process.
mod correct;
mod object;
mod person_checkpoint;
mod person_correct;
mod person_sequence;
mod region;
mod reidentify;
mod selfie;
mod sequence;
mod track;
use anyhow::{ensure, Context, Result};
use image::{ImageDecoder, ImageEncoder};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use video_kadr_vision_runtime::{Enhancer, Image, MODEL_SHA256};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: u32,
    model: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    output: PathBuf,
    source_sha256: String,
    factor: usize,
}
fn regular(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit,
        "input must be bounded regular file"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "input grew beyond limit");
    Ok(bytes)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn decode(bytes: Vec<u8>) -> Result<Image> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 64 * 1024 * 1024,
        "image exceeds pixel limit"
    );
    let orientation = decoder.orientation()?;
    let mut image = image::DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    let rgba = image.to_rgba8();
    Image::new(
        rgba.width() as usize,
        rgba.height() as usize,
        rgba.into_raw(),
    )
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn publish(path: &Path, image: &Image) -> Result<String> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        &image.rgba,
        image.width as u32,
        image.height as u32,
        image::ExtendedColorType::Rgba8,
    )?;
    publish_bytes(path, &png)
}
fn publish_bytes(path: &Path, png: &[u8]) -> Result<String> {
    let parent = path.parent().context("output parent missing")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = Temporary(parent.join(format!(".enhance-{}-{nonce}.png", std::process::id())));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(&temporary.0)?;
    output.write_all(png)?;
    output.sync_all()?;
    drop(output);
    std::fs::hard_link(&temporary.0, path).context("publish enhancement without replacement")?;
    Ok(digest(png))
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--capabilities" {
        println!(
            "{}",
            serde_json::json!({"schemaVersion":1,"samReidentificationSchema":14,"samReidentificationLatestSchema":15,"samReidentificationSchemas":[14,15],"samTrackingSchema":9,"samRegionSchema":5})
        );
        return Ok(());
    }

    ensure!(
        args.len() == 2
            && (args[0] == "--request"
                || args[0] == "--sequence"
                || args[0] == "--sam-track"
                || args[0] == "--sam-reidentify"
                || args[0] == "--person-sequence"
                || args[0] == "--cutout-sequence"
                || args[0] == "--person-correct"
                || args[0] == "--selfie"
                || args[0] == "--object"
                || args[0] == "--mask"
                || args[0] == "--region"),
        "usage: video-kadr-vision-runtime (--request|--sam-track|--sequence|--person-sequence|--person-correct|--selfie|--object|--mask|--region) <host JSON file>"
    );
    let request_path = PathBuf::from(&args[1]);
    ensure!(request_path.is_absolute(), "request path must be absolute");
    let bytes = regular(
        &request_path,
        if args[0] == "--person-correct"
            || args[0] == "--selfie"
            || args[0] == "--object"
            || args[0] == "--mask"
            || args[0] == "--region"
        {
            4 * 1024 * 1024
        } else {
            32768
        },
    )?;
    if args[0] == "--sam-reidentify" {
        return reidentify::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--sam-track" {
        return track::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--region" {
        return region::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--mask" {
        return correct::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--person-correct" {
        return person_correct::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--person-sequence" || args[0] == "--cutout-sequence" {
        return person_sequence::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--sequence" {
        return sequence::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--selfie" {
        return selfie::run(serde_json::from_slice(&bytes)?);
    }
    if args[0] == "--object" {
        return object::run(serde_json::from_slice(&bytes)?);
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    ensure!(
        request.schema_version == 1 && [1, 2, 4].contains(&request.factor),
        "unsupported schema or enhancement factor"
    );
    ensure!(
        request.model.is_absolute()
            && request.runtime_library.is_absolute()
            && request.input.is_absolute()
            && request.output.is_absolute(),
        "host paths must be absolute"
    );
    ensure!(!request.output.try_exists()?, "output already exists");
    let bytes = regular(&request.input, 256 * 1024 * 1024)?;
    let source_hash = digest(&bytes);
    ensure!(
        source_hash == request.source_sha256,
        "source digest mismatch"
    );
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load host ONNX Runtime: {e}"))?
        .commit();
    let image = decode(bytes)?;
    let mut enhancer = Enhancer::load(&request.model)?;
    let output = enhancer.enhance(
        &image,
        request.factor,
        &AtomicBool::new(false),
        |completed, total| {
            eprintln!(
                "{}",
                serde_json::json!({"event":"progress","completed":completed,"total":total})
            );
        },
    )?;
    ensure!(
        digest(&regular(&request.input, 256 * 1024 * 1024)?) == source_hash
            && digest(&regular(&request.model, 4866417)?) == MODEL_SHA256
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "source/model/runtime changed during enhancement"
    );
    let output_hash = publish(&request.output, &output)?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":1,"width":output.width,"height":output.height,"factor":request.factor,"sourceHash":source_hash,"modelHash":MODEL_SHA256,"runtimeHash":runtime_hash,"outputHash":output_hash})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Image enhancement failed: {error}");
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_protocol_refuses_unknown_fields() {
        let text = r#"{"schemaVersion":1,"model":"/model","runtimeLibrary":"/ort","input":"/input","output":"/output","sourceSha256":"hash","factor":2}"#;
        assert!(serde_json::from_str::<Request>(text).is_ok());
        assert!(serde_json::from_str::<Request>(
            &text.replace("\"factor\":2", "\"factor\":2,\"url\":\"https://invalid\"")
        )
        .is_err());
    }
}
