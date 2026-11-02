//! Host-controlled request-file protocol; model/text/user paths are never inferred.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};
use video_kadr_chatterbox::generation::{synthesize_long, Options};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: u32,
    model_dir: PathBuf,
    reference: PathBuf,
    output: PathBuf,
    text: String,
    #[serde(default = "seed")]
    seed: u64,
    #[serde(default)]
    runtime_library: Option<PathBuf>,
}
fn seed() -> u64 {
    42
}
fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= limit,
        "input must be a bounded regular file"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "input grew beyond limit");
    Ok(bytes)
}
fn reference(path: &Path) -> Result<Vec<f32>> {
    let bytes = read_regular(path, 480044)?;
    ensure!(
        bytes.len() >= 44
            && &bytes[0..4] == b"RIFF"
            && &bytes[8..16] == b"WAVEfmt "
            && &bytes[36..40] == b"data",
        "reference must use canonical PCM16 WAV"
    );
    let u16_at = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    ensure!(
        u64::from(u32_at(4)) + 8 == bytes.len() as u64
            && u32_at(16) == 16
            && u16_at(20) == 1
            && u16_at(22) == 1
            && u32_at(24) == 24000
            && u32_at(28) == 48000
            && u16_at(32) == 2
            && u16_at(34) == 16
            && u64::from(u32_at(40)) + 44 == bytes.len() as u64
            && (bytes.len() - 44) % 2 == 0,
        "reference must be mono24kHz PCM16"
    );
    let samples: Vec<_> = bytes[44..]
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.)
        .collect();
    ensure!(
        (120000..=240000).contains(&samples.len()),
        "reference must last5..10seconds"
    );
    Ok(samples)
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn publish(path: &Path, samples: &[f32]) -> Result<()> {
    ensure!(
        !samples.is_empty()
            && samples.len() <= 24000 * 600
            && samples.iter().all(|v| v.is_finite()),
        "invalid output waveform"
    );
    let parent = path.parent().context("output has no parent")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let staging = parent.join(format!(".chatterbox-{}-{nonce}.wav", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&staging)?;
    let temporary = Temporary(staging);
    let size = (samples.len() * 2) as u32;
    file.write_all(b"RIFF")?;
    file.write_all(&(size + 36).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&24000u32.to_le_bytes())?;
    file.write_all(&48000u32.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&size.to_le_bytes())?;
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for value in samples {
        pcm.extend_from_slice(&((value.clamp(-1., 1.) * 32767.).round() as i16).to_le_bytes());
    }
    file.write_all(&pcm)?;
    file.sync_all()?;
    drop(file);
    std::fs::hard_link(&temporary.0, path).context("publish without replacing existing output")?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2 && args[0] == "--request",
        "usage: video-kadr-chatterbox --request <host JSON file>"
    );
    let request_path = PathBuf::from(&args[1]);
    ensure!(request_path.is_absolute(), "request path must be absolute");
    let request: Request = serde_json::from_slice(&read_regular(&request_path, 32768)?)?;
    ensure!(request.schema_version == 1, "unsupported request schema");
    ensure!(
        request.model_dir.is_absolute()
            && request.reference.is_absolute()
            && request.output.is_absolute(),
        "host paths must be absolute"
    );
    ensure!(
        !request.text.trim().is_empty() && request.text.len() <= 4096,
        "invalid narration text"
    );
    ensure!(!request.output.try_exists()?, "output already exists");
    if let Some(library) = &request.runtime_library {
        ensure!(
            library.is_absolute(),
            "runtime library path must be absolute"
        );
        ensure!(
            std::fs::metadata(library)?.is_file(),
            "runtime library is unavailable"
        );
        ort::init_from(library)
            .map_err(|e| anyhow::anyhow!("load host ONNX runtime: {e}"))?
            .commit();
    }
    let samples = reference(&request.reference)?;
    let wave = synthesize_long(
        &request.model_dir,
        &request.text,
        &samples,
        Options {
            seed: request.seed,
            ..Default::default()
        },
        &AtomicBool::new(false),
        |p| {
            if p.tokens == 1 || p.tokens % 25 == 0 {
                eprintln!(
                    "{}",
                    serde_json::json!({"event":"progress","chunkIndex":p.chunk_index,"chunkCount":p.chunk_count,"tokens":p.tokens})
                );
            }
        },
    )?;
    publish(&request.output, &wave)?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":1,"sampleRate":24000,"channels":1,"frames":wave.len()})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Chatterbox request failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owned() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "chatterbox-protocol-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        dir
    }
    #[test]
    fn publication_is_pcm16_private_and_never_replaces() {
        let root = owned();
        let output = root.join("sample.wav");
        publish(&output, &vec![0.25; 120000]).unwrap();
        let samples = reference(&output).unwrap();
        assert_eq!(samples.len(), 120000);
        assert!((samples[0] - 0.25).abs() < 0.0001);
        let before = std::fs::read(&output).unwrap();
        assert!(publish(&output, &[0.5; 8]).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), before);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn strict_request_rejects_unknown_fields() {
        let value = serde_json::json!({"schemaVersion":1,"modelDir":"/model","reference":"/sample.wav","output":"/out.wav","text":"Hello","url":"https://example.com"});
        assert!(serde_json::from_value::<Request>(value).is_err());
    }
}
