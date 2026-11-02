//! Bounded host request-file protocol for the native Kokoro engine.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use video_kadr_sherpa_runtime::Kokoro;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    schema_version: u32,
    model_dir: PathBuf,
    output: PathBuf,
    text: String,
    voice_id: i32,
    speed: f32,
    pauses: f32,
    threads: i32,
    runtime_library: PathBuf,
    ort_library: PathBuf,
}
#[cfg(unix)]
fn verify_loaded(symbol: &std::ffi::CStr, expected: &Path) -> Result<()> {
    ensure!(
        expected.is_absolute(),
        "runtime library path must be absolute"
    );
    // Both calls inspect already loaded symbols; no user code is loaded here.
    let actual = unsafe {
        let address = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr());
        ensure!(!address.is_null(), "runtime symbol unavailable");
        let mut info: libc::Dl_info = std::mem::zeroed();
        ensure!(
            libc::dladdr(address, &mut info) != 0 && !info.dli_fname.is_null(),
            "runtime path unavailable"
        );
        PathBuf::from(std::ffi::CStr::from_ptr(info.dli_fname).to_str()?)
    };
    ensure!(
        actual.canonicalize()? == expected.canonicalize()?,
        "loaded runtime differs from host configuration"
    );
    Ok(())
}
#[cfg(not(unix))]
fn verify_loaded(_: &std::ffi::CStr, _: &Path) -> Result<()> {
    anyhow::bail!("runtime identity verification unsupported on this platform")
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
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn publish(path: &Path, samples: &[f32], rate: u32) -> Result<()> {
    ensure!(
        !samples.is_empty()
            && (8000..=96000).contains(&rate)
            && samples.len() <= rate as usize * 600
            && samples.iter().all(|v| v.is_finite()),
        "invalid output waveform"
    );
    let parent = path.parent().context("output has no parent")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let staging = parent.join(format!(".sherpa-{}-{nonce}.wav", std::process::id()));
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
    file.write_all(&rate.to_le_bytes())?;
    file.write_all(&(rate * 2).to_le_bytes())?;
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
        "usage: video-kadr-sherpa-runtime --request <host JSON file>"
    );
    let path = PathBuf::from(&args[1]);
    ensure!(path.is_absolute(), "request must be absolute");
    let request: Request = serde_json::from_slice(&read_regular(&path, 32768)?)?;
    ensure!(request.schema_version == 1, "unsupported request schema");
    ensure!(
        request.model_dir.is_absolute() && request.output.is_absolute(),
        "host paths must be absolute"
    );
    ensure!(!request.output.try_exists()?, "output already exists");
    ensure!(
        !request.text.trim().is_empty()
            && request.text.len() <= 4096
            && !request.text.contains('\0'),
        "invalid narration text"
    );
    ensure!(
        request.speed.is_finite()
            && (0.5..=2.).contains(&request.speed)
            && request.pauses.is_finite()
            && (0.0..=1.).contains(&request.pauses),
        "invalid narration controls"
    );
    verify_loaded(c"SherpaOnnxCreateOfflineTts", &request.runtime_library)?;
    verify_loaded(c"OrtGetApiBase", &request.ort_library)?;
    let engine = Kokoro::load(&request.model_dir, request.threads)?;
    let wave = engine.speak(
        &request.text,
        request.voice_id,
        request.speed,
        request.pauses,
        Arc::new(AtomicBool::new(false)),
        |progress| {
            eprintln!(
                "{}",
                serde_json::json!({"event":"progress","progress":progress})
            );
        },
    )?;
    publish(&request.output, &wave.samples, wave.sample_rate)?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":1,"sampleRate":wave.sample_rate,"channels":1,"frames":wave.samples.len()})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Native Kokoro request failed: {error}");
        std::process::exit(1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_request() {
        let input = r#"{"schemaVersion":1,"modelDir":"/model","output":"/output","text":"Hello","voiceId":3,"speed":1,"pauses":0,"threads":2,"runtimeLibrary":"/lib/sherpa","ortLibrary":"/lib/ort"}"#;
        assert!(serde_json::from_str::<Request>(input).is_ok());
        let unknown = input.replace("\"threads\":2", "\"threads\":2,\"url\":\"https://invalid\"");
        assert!(serde_json::from_str::<Request>(&unknown).is_err());
    }
}
