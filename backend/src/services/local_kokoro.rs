//! Host-controlled Native Kokoro subprocess adapter. Caller owns source/model access
//! and Analysis admission. No request-supplied executable/model/filesystem paths.
use crate::{
    domain::filmcraft_wav_audio::WavSource,
    process_control::{stream_with_progress, ProcessRuntime, ProcessStatus},
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
#[derive(Clone, Debug)]
pub struct Config {
    pub executable: PathBuf,
    pub model_dir: PathBuf,
    pub runtime_library: PathBuf,
    pub ort_library: PathBuf,
    pub threads: usize,
}
pub struct Input<'a> {
    pub request: &'a super::local_tts::SynthesisRequest,
    pub pauses: f32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub executable: PathBuf,
    pub library: PathBuf,
    pub ort_library: PathBuf,
}
impl Runtime {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        let executable = lookup("KOKORO_NATIVE_BIN").filter(|s| !s.trim().is_empty());
        let library = lookup("KOKORO_SHERPA_LIBRARY").filter(|s| !s.trim().is_empty());
        let ort_library = lookup("KOKORO_ORT_LIBRARY").filter(|s| !s.trim().is_empty());
        match (executable, library, ort_library) {
            (None, None, None) => Ok(None),
            (Some(executable), Some(library), Some(ort_library)) => {
                let value = Self {
                    executable: executable.into(),
                    library: library.into(),
                    ort_library: ort_library.into(),
                };
                ensure!(
                    value.executable.is_absolute()
                        && value.library.is_absolute()
                        && value.ort_library.is_absolute(),
                    "native Kokoro paths must be absolute"
                );
                Ok(Some(value))
            }
            _ => anyhow::bail!(
                "configure all KOKORO_NATIVE_BIN/KOKORO_SHERPA_LIBRARY/KOKORO_ORT_LIBRARY"
            ),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Metadata {
    schema_version: u32,
    sample_rate: u32,
    channels: u32,
    frames: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Progress {
    event: String,
    progress: f64,
}
struct Owned(PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub async fn identity(config: &Config) -> Result<String> {
    ensure!(
        config.executable.is_absolute()
            && config.model_dir.is_absolute()
            && config.runtime_library.is_absolute()
            && config.ort_library.is_absolute()
            && (1..=16).contains(&config.threads),
        "Native Kokoro host paths must be absolute"
    );
    let bundle = super::tts_job_identity::fingerprint(&super::local_tts::TtsConfig {
        executable: config.executable.clone(),
        model_dir: Some(config.model_dir.clone()),
        threads: config.threads,
    })
    .await?;
    let (size, hash) = super::local_speech::fingerprint_model(&config.runtime_library).await?;
    let mut digest = Sha256::new();
    digest.update(bundle.as_bytes());
    digest.update(config.threads.to_le_bytes());
    digest.update(size.to_le_bytes());
    digest.update(hash.as_bytes());
    let (size, hash) = super::local_speech::fingerprint_model(&config.ort_library).await?;
    digest.update(size.to_le_bytes());
    digest.update(hash.as_bytes());
    Ok(format!("{:x}", digest.finalize()))
}
async fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await?;
    }
    file.write_all(bytes).await?;
    file.sync_all().await?;
    Ok(())
}
pub async fn synthesize(
    runtime: &ProcessRuntime,
    config: &Config,
    input: Input<'_>,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<super::local_tts::SynthesizedAudio> {
    ensure!(
        !input.request.text.trim().is_empty()
            && input.request.text.len() <= 4096
            && !input.request.text.contains('\0'),
        "invalid Native Kokoro narration text"
    );
    ensure!(
        destination.is_absolute(),
        "Native Kokoro host paths must be absolute"
    );
    ensure!(!cancel.is_cancelled(), "Native Kokoro cancelled");
    ensure!(!destination.try_exists()?, "destination already exists");
    let parent = destination.parent().context("missing output parent")?;
    ensure!(parent.is_dir(), "output parent is unavailable");
    let work = Owned(parent.join(format!(".kokoro-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let deadline = tokio::time::Instant::now() + limit;
    let before = tokio::time::timeout_at(deadline, identity(config)).await??;
    ensure!(
        super::local_tts::family(&super::local_tts::TtsConfig {
            executable: config.executable.clone(),
            model_dir: Some(config.model_dir.clone()),
            threads: config.threads
        })? == super::local_tts::TtsFamily::Kokoro,
        "native runtime requires Kokoro"
    );
    input
        .request
        .validate_for_model(&super::local_tts::TtsConfig {
            executable: config.executable.clone(),
            model_dir: Some(config.model_dir.clone()),
            threads: config.threads,
        })?;
    ensure!(
        input.pauses.is_finite() && (0.0..=1.).contains(&input.pauses),
        "invalid sentence pauses"
    );
    let result = work.0.join("result.wav");
    let request = work.0.join("request.json");
    private_file(&request,&serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"modelDir":config.model_dir,"output":result,"text":input.request.text,"voiceId":input.request.voice_id,"speed":input.request.speed,"pauses":input.pauses,"threads":config.threads,"runtimeLibrary":config.runtime_library,"ortLibrary":config.ort_library}))?).await?;
    ensure!(!cancel.is_cancelled(), "Native Kokoro cancelled");
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    ensure!(!remaining.is_zero(), "Native Kokoro deadline exceeded");
    let metadata = Mutex::new((None, false));
    let maximum = Mutex::new(0f64);
    let parse = |line: &str| {
        if let Ok(value) = serde_json::from_str::<Metadata>(line) {
            if let Ok(mut state) = metadata.lock() {
                if state.0.is_some() {
                    state.1 = true;
                }
                state.0 = Some(value);
            }
            return None;
        }
        let value = serde_json::from_str::<Progress>(line).ok()?;
        if value.event != "progress"
            || !value.progress.is_finite()
            || !(0.0..=1.).contains(&value.progress)
        {
            return None;
        }
        let percent = value.progress * 95.;
        let mut max = maximum.lock().ok()?;
        *max = max.max(percent);
        Some(*max)
    };
    let mut command = tokio::process::Command::new(&config.executable);
    command.arg("--request").arg(&request);
    // Await the runner even on cancellation: it kills/reaps the complete child
    // group before the owned request/reference/output staging can be dropped.
    let (status, _) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(
            vec![
                config.model_dir.clone(),
                config.runtime_library.clone(),
                config.ort_library.clone(),
                work.0.clone(),
            ],
            &result,
        ),
        parse,
        progress,
        cancel,
        remaining,
    )
    .await?;
    ensure!(
        status == ProcessStatus::Ok,
        "Native Kokoro process did not complete"
    );
    ensure!(!cancel.is_cancelled(), "Native Kokoro cancelled");
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid Native Kokoro metadata"))?;
    ensure!(!duplicate, "duplicate Native Kokoro result");
    let state = state.context("missing Native Kokoro result")?;
    let meta = tokio::fs::symlink_metadata(&result).await?;
    ensure!(
        meta.file_type().is_file() && (44..=32 * 1024 * 1024).contains(&meta.len()),
        "invalid Native Kokoro WAV"
    );
    let audio = WavSource::parse(tokio::fs::read(&result).await?.into())?;
    ensure!(
        state.schema_version == 1
            && state.sample_rate == 24000
            && state.channels == 1
            && state.frames == audio.frames()
            && audio.sample_rate() == 24000
            && audio.channels() == 1
            && audio.frames() > 0
            && audio.frames() <= 24000 * 600,
        "Native Kokoro metadata/audio mismatch"
    );
    let after = tokio::time::timeout_at(deadline, identity(config)).await??;
    ensure!(
        before == after,
        "Native Kokoro model/runtime changed during inference"
    );
    let network = config
        .model_dir
        .join(if config.model_dir.join("model.int8.onnx").is_file() {
            "model.int8.onnx"
        } else {
            "model.onnx"
        });
    let (_, model_hash) = super::local_speech::fingerprint_model(&network).await?;
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "Native Kokoro cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish Native Kokoro without replacement")?;
    let _ = progress.send(100.);
    Ok(super::local_tts::SynthesizedAudio {
        engine: "kokoro-sherpa-onnx",
        voice_id: input.request.voice_id,
        sample_rate: 24000,
        frames: audio.frames(),
        duration_us: audio.frames() as u64 * 1_000_000 / 24000,
        model_hash,
    })
}
