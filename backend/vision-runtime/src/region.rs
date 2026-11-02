//! Host-owned single source-UV SlimSAM region, with verified immutable inputs.
use super::{decode, digest, publish_bytes, regular};
use anyhow::{ensure, Result};
use image::ImageEncoder;
use serde::Deserialize;
use std::{path::PathBuf, sync::atomic::AtomicBool};
use video_kadr_vision_runtime::slim_sam::{
    Brush, DECODER_BYTES, DECODER_SHA256, ENCODER_BYTES, ENCODER_SHA256,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    schema_version: u32,
    encoder: PathBuf,
    decoder: PathBuf,
    runtime_library: PathBuf,
    input: PathBuf,
    output: PathBuf,
    source_sha256: String,
    points: Vec<[f64; 2]>,
}
pub(super) fn run(request: Request) -> Result<()> {
    ensure!(
        request.schema_version == 5
            && [
                &request.encoder,
                &request.decoder,
                &request.runtime_library,
                &request.input,
                &request.output
            ]
            .iter()
            .all(|p| p.is_absolute()),
        "invalid SAM request paths/schema"
    );
    ensure!(!request.output.try_exists()?, "SAM output already exists");
    ensure!(
        request.source_sha256.len() == 64
            && request
                .source_sha256
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
            && !request.points.is_empty()
            && request.points.len() <= 65536
            && request
                .points
                .iter()
                .flatten()
                .all(|v| v.is_finite() && (-0.5..=1.5).contains(v)),
        "invalid SAM identity/prompts"
    );
    let source = regular(&request.input, 256 * 1024 * 1024)?;
    let source_hash = digest(&source);
    ensure!(
        source_hash == request.source_sha256,
        "SAM source digest mismatch"
    );
    let prompt_hash = digest(&serde_json::to_vec(&request.points)?);
    let runtime_hash = digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?);
    ort::init_from(&request.runtime_library)
        .map_err(|e| anyhow::anyhow!("load SAM runtime: {e}"))?
        .commit();
    let image = decode(source)?;
    let mut engine = Brush::load(&request.encoder, &request.decoder)?;
    let cancel = AtomicBool::new(false);
    let progress = |completed| {
        eprintln!(
            "{}",
            serde_json::json!({"event":"progress","completed":completed,"total":3})
        )
    };
    progress(0);
    let embedding = engine.embed(&image, &cancel)?;
    progress(1);
    let mask = engine.region(&embedding, &request.points, &cancel)?;
    progress(2);
    ensure!(
        digest(&regular(&request.input, 256 * 1024 * 1024)?) == source_hash
            && digest(&regular(&request.encoder, ENCODER_BYTES)?) == ENCODER_SHA256
            && digest(&regular(&request.decoder, DECODER_BYTES)?) == DECODER_SHA256
            && digest(&regular(&request.runtime_library, 256 * 1024 * 1024)?) == runtime_hash,
        "SAM source/model/runtime changed"
    );
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(
        &mask.bytes,
        mask.width as u32,
        mask.height as u32,
        image::ExtendedColorType::L8,
    )?;
    let output_hash = publish_bytes(&request.output, &png)?;
    progress(3);
    println!(
        "{}",
        serde_json::json!({"schemaVersion":5,"width":image.width,"height":image.height,"maskWidth":mask.width,"maskHeight":mask.height,"sourceHash":source_hash,"encoderHash":ENCODER_SHA256,"decoderHash":DECODER_SHA256,"runtimeHash":runtime_hash,"promptHash":prompt_hash,"maskHash":digest(&mask.bytes),"outputHash":output_hash})
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_region_protocol() {
        let input = serde_json::json!({"schemaVersion":5,"encoder":"/e","decoder":"/d","runtimeLibrary":"/r","input":"/i","output":"/o","sourceSha256":"a".repeat(64),"points":[[0.5,0.5]]});
        assert!(serde_json::from_value::<Request>(input.clone()).is_ok());
        let mut bad = input;
        bad["url"] = serde_json::json!("https://invalid");
        assert!(serde_json::from_value::<Request>(bad).is_err());
    }
}
