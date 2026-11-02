//! Host-controlled Chatterbox subprocess adapter. Caller owns source/model access
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
}
pub struct Input<'a> {
    pub text: &'a str,
    pub reference: &'a Path,
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
    chunk_index: usize,
    chunk_count: usize,
    tokens: usize,
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
            && config.runtime_library.is_absolute(),
        "Chatterbox host paths must be absolute"
    );
    let bundle = super::tts_job_identity::fingerprint(&super::local_tts::TtsConfig {
        executable: config.executable.clone(),
        model_dir: Some(config.model_dir.clone()),
        threads: 2,
    })
    .await?;
    let (size, hash) = super::local_speech::fingerprint_model(&config.runtime_library).await?;
    let mut digest = Sha256::new();
    digest.update(bundle.as_bytes());
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
        !input.text.trim().is_empty() && input.text.len() <= 4096 && !input.text.contains('\0'),
        "invalid Chatterbox narration text"
    );
    ensure!(
        destination.is_absolute() && input.reference.is_absolute(),
        "Chatterbox host paths must be absolute"
    );
    ensure!(!cancel.is_cancelled(), "Chatterbox cancelled");
    ensure!(!destination.try_exists()?, "destination already exists");
    let parent = destination.parent().context("missing output parent")?;
    ensure!(parent.is_dir(), "output parent is unavailable");
    let work = Owned(parent.join(format!(".chatterbox-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&work.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let deadline = tokio::time::Instant::now() + limit;
    let before = tokio::time::timeout_at(deadline, identity(config)).await??;
    let meta = tokio::fs::symlink_metadata(input.reference).await?;
    ensure!(
        meta.file_type().is_file() && meta.len() <= 8 * 1024 * 1024,
        "invalid reference file"
    );
    let reference_before = super::local_speech::fingerprint_model(input.reference).await?;
    let (wav, duration) =
        super::local_tts::normalize_reference(tokio::fs::read(input.reference).await?)?;
    ensure!(
        duration >= 5000000,
        "Chatterbox requires at least5seconds of reference"
    );
    let prepared = work.0.join("reference.wav");
    private_file(&prepared, &wav).await?;
    let result = work.0.join("result.wav");
    let request = work.0.join("request.json");
    private_file(&request,&serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"modelDir":config.model_dir,"runtimeLibrary":config.runtime_library,"reference":prepared,"output":result,"text":input.text}))?).await?;
    ensure!(!cancel.is_cancelled(), "Chatterbox cancelled");
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    ensure!(!remaining.is_zero(), "Chatterbox deadline exceeded");
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
            || !(1..=32).contains(&value.chunk_count)
            || value.chunk_index >= value.chunk_count
            || !(1..=1024).contains(&value.tokens)
        {
            return None;
        }
        let percent = (value.chunk_index as f64 + value.tokens as f64 / 1024.)
            / value.chunk_count as f64
            * 95.;
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
        "Chatterbox process did not complete"
    );
    ensure!(!cancel.is_cancelled(), "Chatterbox cancelled");
    let (state, duplicate) = metadata
        .into_inner()
        .map_err(|_| anyhow::anyhow!("invalid Chatterbox metadata"))?;
    ensure!(!duplicate, "duplicate Chatterbox result");
    let state = state.context("missing Chatterbox result")?;
    let meta = tokio::fs::symlink_metadata(&result).await?;
    ensure!(
        meta.file_type().is_file() && (44..=32 * 1024 * 1024).contains(&meta.len()),
        "invalid Chatterbox WAV"
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
        "Chatterbox metadata/audio mismatch"
    );
    let after = tokio::time::timeout_at(deadline, identity(config)).await??;
    ensure!(
        before == after
            && reference_before == super::local_speech::fingerprint_model(input.reference).await?,
        "Chatterbox model/runtime/reference changed during inference"
    );
    let (_, model_hash) = super::local_speech::fingerprint_model(
        &config
            .model_dir
            .join("onnx/language_model_quantized.onnx_data"),
    )
    .await?;
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "Chatterbox cancelled or expired"
    );
    tokio::fs::hard_link(&result, destination)
        .await
        .context("publish Chatterbox without replacement")?;
    let _ = progress.send(100.);
    Ok(super::local_tts::SynthesizedAudio {
        engine: "chatterbox-onnx",
        voice_id: 2099,
        sample_rate: 24000,
        frames: audio.frames(),
        duration_us: audio.frames() as u64 * 1_000_000 / 24000,
        model_hash,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub executable: PathBuf,
    pub library: PathBuf,
}
impl Runtime {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Option<Self>> {
        let executable = lookup("CHATTERBOX_TTS_BIN").filter(|s| !s.trim().is_empty());
        let library = lookup("CHATTERBOX_ORT_LIBRARY").filter(|s| !s.trim().is_empty());
        match (executable, library) {
            (None, None) => Ok(None),
            (Some(executable), Some(library)) => {
                let value = Self {
                    executable: executable.into(),
                    library: library.into(),
                };
                ensure!(
                    value.executable.is_absolute() && value.library.is_absolute(),
                    "Chatterbox runtime paths must be absolute"
                );
                Ok(Some(value))
            }
            _ => anyhow::bail!("configure both CHATTERBOX_TTS_BIN and CHATTERBOX_ORT_LIBRARY"),
        }
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    #[test]
    fn chatterbox_runtime_settings_require_a_complete_absolute_pair() {
        assert!(Runtime::from_lookup(&mut |_| None).unwrap().is_none());
        assert!(
            Runtime::from_lookup(&mut |key| if key == "CHATTERBOX_TTS_BIN" {
                Some("/bin/chatterbox".into())
            } else {
                None
            })
            .is_err()
        );
        assert!(Runtime::from_lookup(&mut |key| Some(
            if key == "CHATTERBOX_TTS_BIN" {
                "relative"
            } else {
                "/lib/ort"
            }
            .into()
        ))
        .is_err());
        let runtime = Runtime::from_lookup(&mut |key| {
            Some(
                if key == "CHATTERBOX_TTS_BIN" {
                    "/bin/chatterbox"
                } else {
                    "/lib/ort"
                }
                .into(),
            )
        })
        .unwrap()
        .unwrap();
        assert_eq!(runtime.executable, PathBuf::from("/bin/chatterbox"));
        assert_eq!(runtime.library, PathBuf::from("/lib/ort"));
    }
}
