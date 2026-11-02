//! Local Kokoro synthesis through the official sherpa-onnx executable.
//! Model/executable/output paths are selected by the host, never by request text.
use crate::{
    domain::filmcraft_wav_audio::WavSource,
    process_control::{capture_output_cancellable, ProcessRuntime},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsFamily {
    #[default]
    Kokoro,
    Pocket,
    Chatterbox,
}
pub fn engine_name(family: TtsFamily) -> &'static str {
    match family {
        TtsFamily::Kokoro => "kokoro-sherpa-onnx",
        TtsFamily::Pocket => "pocket-sherpa-onnx",
        TtsFamily::Chatterbox => "chatterbox-onnx",
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TtsConfig {
    pub executable: PathBuf,
    pub model_dir: Option<PathBuf>,
    pub threads: usize,
}
impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            executable: "sherpa-onnx-offline-tts".into(),
            model_dir: None,
            threads: 2,
        }
    }
}
impl TtsConfig {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Self> {
        let model_dir = lookup("KOKORO_MODEL_DIR")
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);
        ensure!(
            model_dir.as_ref().is_none_or(|p| p.is_absolute()),
            "KOKORO_MODEL_DIR must be absolute"
        );
        let executable = lookup("SHERPA_TTS_BIN")
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| "sherpa-onnx-offline-tts".into());
        let threads = lookup("KOKORO_THREADS")
            .map(|s| s.parse::<usize>())
            .transpose()?
            .unwrap_or(2);
        ensure!((1..=16).contains(&threads), "KOKORO_THREADS must be 1..16");
        Ok(Self {
            executable,
            model_dir,
            threads,
        })
    }
}
pub fn family(config: &TtsConfig) -> Result<TtsFamily> {
    let Some(root) = &config.model_dir else {
        return Ok(TtsFamily::Kokoro);
    };
    let pocket = root.join("lm_main.int8.onnx").is_file();
    let kokoro = root.join("model.int8.onnx").is_file() || root.join("model.onnx").is_file();
    let chatterbox = root.join("onnx/language_model_quantized.onnx").is_file();
    ensure!(
        usize::from(pocket) + usize::from(kokoro) + usize::from(chatterbox) <= 1,
        "ambiguous TTS model family"
    );
    Ok(if chatterbox {
        TtsFamily::Chatterbox
    } else if pocket {
        TtsFamily::Pocket
    } else {
        TtsFamily::Kokoro
    })
}
pub fn model_voices(config: &TtsConfig) -> Result<Vec<Voice>> {
    Ok(match family(config)? {
        TtsFamily::Kokoro => voices(),
        TtsFamily::Chatterbox => vec![
            Voice {
                id: 2000,
                name: "chatterbox_bria",
                language: "en",
            },
            Voice {
                id: 2099,
                name: "chatterbox_clone",
                language: "en",
            },
        ],
        TtsFamily::Pocket => vec![
            Voice {
                id: 1000,
                name: "pocket_bria",
                language: "en",
            },
            Voice {
                id: 1001,
                name: "pocket_loona",
                language: "en",
            },
            Voice {
                id: 1099,
                name: "pocket_clone",
                language: "en",
            },
        ],
    })
}
#[derive(Clone, Debug, Serialize)]
pub struct Voice {
    pub id: u32,
    pub name: &'static str,
    pub language: &'static str,
}
// Kokoro v1.0 model metadata. Only languages supported by this bundle's frontend.
pub fn voices() -> Vec<Voice> {
    let en = [
        "af_alloy",
        "af_aoede",
        "af_bella",
        "af_heart",
        "af_jessica",
        "af_kore",
        "af_nicole",
        "af_nova",
        "af_river",
        "af_sarah",
        "af_sky",
        "am_adam",
        "am_echo",
        "am_eric",
        "am_fenrir",
        "am_liam",
        "am_michael",
        "am_onyx",
        "am_puck",
        "am_santa",
        "bf_alice",
        "bf_emma",
        "bf_isabella",
        "bf_lily",
        "bm_daniel",
        "bm_fable",
        "bm_george",
        "bm_lewis",
    ];
    let zh = [
        "zf_xiaobei",
        "zf_xiaoni",
        "zf_xiaoxiao",
        "zf_xiaoyi",
        "zm_yunjian",
        "zm_yunxi",
        "zm_yunxia",
        "zm_yunyang",
    ];
    en.into_iter()
        .enumerate()
        .map(|(id, name)| Voice {
            id: id as u32,
            name,
            language: "en",
        })
        .chain(zh.into_iter().enumerate().map(|(id, name)| Voice {
            id: id as u32 + 45,
            name,
            language: "zh",
        }))
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SynthesisRequest {
    pub text: String,
    pub voice_id: u32,
    pub speed: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pauses: Option<f32>,
}
impl SynthesisRequest {
    pub fn validate_for_model(&self, config: &TtsConfig) -> Result<()> {
        self.validate()?;
        ensure!(
            self.pauses.is_none() || family(config)? == TtsFamily::Kokoro,
            "sentence pauses require Kokoro"
        );
        ensure!(
            self.num_steps.is_none() || family(config)? == TtsFamily::Pocket,
            "quality steps require Pocket TTS"
        );
        if family(config)? == TtsFamily::Chatterbox {
            ensure!(self.speed == 1., "Chatterbox uses its own speaking speed");
        }
        ensure!(
            model_voices(config)?.iter().any(|v| v.id == self.voice_id),
            "voice does not belong to selected model family"
        );
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pauses
                .is_none_or(|value| value.is_finite() && (0.0..=1.).contains(&value)),
            "sentence pauses must be 0..1"
        );
        ensure!(
            self.num_steps
                .is_none_or(|steps| matches!(steps, 3 | 5 | 8)),
            "Pocket quality steps must be 3, 5 or 8"
        );
        ensure!(
            !self.text.trim().is_empty() && self.text.len() <= 4096,
            "text must contain 1..4096 UTF-8 bytes"
        );
        ensure!(
            !self
                .text
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
            "unsupported text control character"
        );
        ensure!(
            voices().iter().any(|v| v.id == self.voice_id)
                || matches!(self.voice_id, 1000 | 1001 | 1099 | 2000 | 2099),
            "unsupported local TTS voice"
        );
        ensure!(
            self.speed.is_finite() && (0.5..=2.).contains(&self.speed),
            "speed must be 0.5..2"
        );
        Ok(())
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizedAudio {
    pub engine: &'static str,
    pub voice_id: u32,
    pub sample_rate: u32,
    pub frames: usize,
    pub duration_us: u64,
    pub model_hash: String,
}
struct OwnedDir(PathBuf);
impl Drop for OwnedDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn cancelled(cancel: &CancellationToken) -> Result<()> {
    ensure!(!cancel.is_cancelled(), "speech synthesis cancelled");
    Ok(())
}
/// Atomic no-replace publication. Failure/cancellation removes only this call's staging.
/// Caller owns source/model authorization and must hold an Analysis admission permit.
pub async fn synthesize(
    runtime: &ProcessRuntime,
    config: &TtsConfig,
    request: &SynthesisRequest,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
) -> Result<SynthesizedAudio> {
    synthesize_with_reference(runtime, config, request, destination, cancel, limit, None).await
}
pub async fn synthesize_with_reference(
    runtime: &ProcessRuntime,
    config: &TtsConfig,
    request: &SynthesisRequest,
    destination: &Path,
    cancel: &CancellationToken,
    limit: Duration,
    reference: Option<&Path>,
) -> Result<SynthesizedAudio> {
    request.validate_for_model(config)?;
    ensure!(
        request.pauses.is_none(),
        "sentence pauses require the native Kokoro adapter"
    );
    let family = family(config)?;
    ensure!(
        family != TtsFamily::Chatterbox,
        "Chatterbox requires the host runtime adapter"
    );
    ensure!(
        (request.voice_id == 1099) == reference.is_some(),
        "custom voice requires its reference; stock voices refuse external references"
    );
    cancelled(cancel)?;
    ensure!(
        (1..=16).contains(&config.threads),
        "invalid synthesis thread count"
    );
    ensure!(
        !limit.is_zero() && limit <= Duration::from_secs(600),
        "invalid synthesis deadline"
    );
    let model_dir = config
        .model_dir
        .as_ref()
        .context("Local TTS model is not configured")?;
    ensure!(
        model_dir.is_absolute(),
        "Local TTS directory must be absolute"
    );
    let parent = destination
        .parent()
        .context("missing synthesis output directory")?;
    ensure!(parent.is_dir(), "synthesis output directory is missing");
    // Never spend inference time on a destination that already exists.
    ensure!(
        !destination.try_exists()?,
        "synthesis destination already exists"
    );
    let staging = OwnedDir(parent.join(format!(".tts-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&staging.0).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&staging.0, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let deadline = tokio::time::Instant::now() + limit;
    let operation = async {
        let network = match family {
            TtsFamily::Chatterbox => anyhow::bail!("Chatterbox requires the host adapter"),
            TtsFamily::Pocket => model_dir.join("lm_main.int8.onnx"),
            TtsFamily::Kokoro => {
                if model_dir.join("model.int8.onnx").is_file() {
                    model_dir.join("model.int8.onnx")
                } else {
                    model_dir.join("model.onnx")
                }
            }
        };
        let (model_size, model_hash) = super::local_speech::fingerprint_model(&network)
            .await
            .context("TTS model is unavailable")?;
        ensure!(model_size >= 1_000_000, "TTS network is too small");
        let mut networks = vec![(network.clone(), (model_size, model_hash.clone()))];
        if family == TtsFamily::Pocket {
            for name in [
                "lm_flow.int8.onnx",
                "encoder.onnx",
                "decoder.int8.onnx",
                "text_conditioner.onnx",
            ] {
                let path = model_dir.join(name);
                let fingerprint = super::local_speech::fingerprint_model(&path)
                    .await
                    .context("Pocket network is unavailable")?;
                ensure!(fingerprint.0 > 0, "empty Pocket network");
                networks.push((path, fingerprint));
            }
            for name in ["vocab.json", "token_scores.json"] {
                ensure!(
                    model_dir.join(name).is_file(),
                    "Pocket table is unavailable"
                );
            }
        } else {
            for name in [
                "voices.bin",
                "tokens.txt",
                "lexicon-us-en.txt",
                "lexicon-zh.txt",
            ] {
                ensure!(
                    model_dir.join(name).is_file(),
                    "Kokoro bundle is missing {name}"
                );
            }
            ensure!(
                model_dir.join("espeak-ng-data").is_dir(),
                "Kokoro phoneme data is unavailable"
            );
        }
        let wav = staging.0.join("narration.wav");
        let mut command = Command::new(&config.executable);
        command
            .args(["--debug=0", "--provider=cpu"])
            .arg(format!("--num-threads={}", config.threads))
            .arg(format!("--speed={}", request.speed))
            .arg(format!("--output-filename={}", wav.display()));
        if family == TtsFamily::Pocket {
            for (flag, file) in [
                ("lm-main", "lm_main.int8.onnx"),
                ("lm-flow", "lm_flow.int8.onnx"),
                ("encoder", "encoder.onnx"),
                ("decoder", "decoder.int8.onnx"),
                ("text-conditioner", "text_conditioner.onnx"),
                ("vocab-json", "vocab.json"),
                ("token-scores-json", "token_scores.json"),
            ] {
                command.arg(format!(
                    "--pocket-{flag}={}",
                    model_dir.join(file).display()
                ));
            }
            let source = if request.voice_id == 1099 {
                reference.context("missing voice reference")?.to_path_buf()
            } else {
                let sample = match request.voice_id {
                    1000 => "bria.wav",
                    1001 => "loona.wav",
                    _ => anyhow::bail!("unknown Pocket voice"),
                };
                model_dir.join("test_wavs").join(sample)
            };
            let reference = prepare_reference(&source, &staging.0).await?;
            command
                .arg(format!("--reference-audio={}", reference.display()))
                .arg(format!("--num-steps={}", request.num_steps.unwrap_or(5)))
                .arg("--pocket-voice-embedding-cache-capacity=0");
        } else {
            command
                .arg(format!("--kokoro-model={}", network.display()))
                .arg(format!(
                    "--kokoro-voices={}",
                    model_dir.join("voices.bin").display()
                ))
                .arg(format!(
                    "--kokoro-tokens={}",
                    model_dir.join("tokens.txt").display()
                ))
                .arg(format!(
                    "--kokoro-data-dir={}",
                    model_dir.join("espeak-ng-data").display()
                ))
                .arg(format!(
                    "--kokoro-lexicon={},{}",
                    model_dir.join("lexicon-us-en.txt").display(),
                    model_dir.join("lexicon-zh.txt").display()
                ))
                .arg(format!("--sid={}", request.voice_id));
            if model_dir.join("dict").is_dir() {
                command.arg(format!(
                    "--kokoro-dict-dir={}",
                    model_dir.join("dict").display()
                ));
            }
        }
        command.arg("--").arg(&request.text);
        let output = capture_output_cancellable(
            runtime,
            command,
            runtime.render_policy(vec![model_dir.clone(), staging.0.clone()], &wav),
            deadline.saturating_duration_since(tokio::time::Instant::now()),
            cancel,
        )
        .await
        .context("local TTS execution failed")?;
        // Engine diagnostics may contain narration text. Do not publish them as API errors.
        ensure!(
            output.status.success(),
            "local TTS engine rejected synthesis"
        );
        cancelled(cancel)?;
        let meta = tokio::fs::symlink_metadata(&wav)
            .await
            .context("local TTS did not write audio")?;
        ensure!(
            meta.file_type().is_file() && (44..=64 * 1024 * 1024).contains(&meta.len()),
            "invalid synthesis output file"
        );
        let bytes = tokio::fs::read(&wav).await?;
        let audio = WavSource::parse(bytes.into())?;
        ensure!(
            audio.channels() == 1
                && (8000..=48000).contains(&audio.sample_rate())
                && audio.frames() > 0,
            "invalid synthesis audio format"
        );
        let duration_us = u64::try_from(audio.frames())?
            .checked_mul(1_000_000)
            .context("synthesis duration overflow")?
            / u64::from(audio.sample_rate());
        ensure!(
            duration_us <= 600_000_000,
            "synthesized audio exceeds ten minutes"
        );
        for (network, expected) in &networks {
            ensure!(
                super::local_speech::fingerprint_model(network).await? == *expected,
                "TTS network changed during synthesis"
            );
        }
        cancelled(cancel)?;
        ensure!(
            tokio::time::Instant::now() < deadline,
            "speech synthesis deadline exceeded"
        );
        let file = tokio::fs::OpenOptions::new().write(true).open(&wav).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::hard_link(&wav, destination)
            .await
            .context("publish synthesis without replacement")?;
        Ok(SynthesizedAudio {
            engine: engine_name(family),
            voice_id: request.voice_id,
            sample_rate: audio.sample_rate(),
            frames: audio.frames(),
            duration_us,
            model_hash,
        })
    };
    // The process runner must observe cancellation itself and reap the child
    // group before staging is dropped; never timeout/drop its running future.
    operation.await
}

/// Native bounded reference preparation, shared by stock samples and future private clones.
async fn prepare_reference(source: &Path, staging: &Path) -> Result<PathBuf> {
    let metadata = tokio::fs::symlink_metadata(source).await?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= 8 * 1024 * 1024,
        "invalid voice reference file"
    );
    let bytes = tokio::fs::read(source).await?;
    let (wav, _) = normalize_reference(bytes)?;
    let path = staging.join("reference.wav");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    use tokio::io::AsyncWriteExt;
    file.write_all(&wav).await?;
    file.sync_all().await?;
    Ok(path)
}

pub(crate) fn normalize_reference(bytes: Vec<u8>) -> Result<(Vec<u8>, u64)> {
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "voice reference exceeds budget"
    );
    let audio = WavSource::parse(bytes.into())?;
    ensure!(
        (1..=2).contains(&audio.channels()) && (8000..=48000).contains(&audio.sample_rate()),
        "unsupported reference audio"
    );
    let frames =
        (audio.frames() as u64 * 24000 / u64::from(audio.sample_rate())).min(240000) as usize;
    ensure!(
        frames >= 12000,
        "voice reference is shorter than half a second"
    );
    let channels = audio.audio(0, frames, 24000)?;
    let mono: Vec<f32> = (0..frames)
        .map(|i| channels.iter().map(|c| c[i]).sum::<f32>() / channels.len() as f32)
        .collect();
    ensure!(
        mono.iter().all(|v| v.is_finite()),
        "nonfinite voice reference"
    );
    Ok((
        crate::domain::filmcraft_wav_audio::write_wav16(&mono, 1, 24000),
        frames as u64 * 1_000_000 / 24000,
    ))
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    #[tokio::test]
    async fn native_reference_mixes_resamples_caps_and_refuses_short_audio() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        std::fs::create_dir(&staging).unwrap();
        let samples: Vec<f32> = (0..48000 * 11).flat_map(|_| [0.6, 0.2]).collect();
        let source = root.path().join("stereo.wav");
        std::fs::write(
            &source,
            crate::domain::filmcraft_wav_audio::write_wav16(&samples, 2, 48000),
        )
        .unwrap();
        let output = prepare_reference(&source, &staging).await.unwrap();
        let audio = WavSource::parse(std::fs::read(output).unwrap().into()).unwrap();
        assert_eq!(audio.channels(), 1);
        assert_eq!(audio.sample_rate(), 24000);
        assert_eq!(audio.frames(), 240000);
        assert!((audio.audio(0, 1, 24000).unwrap()[0][0] - 0.4).abs() < 0.0001);
        let short = root.path().join("short.wav");
        std::fs::write(
            &short,
            crate::domain::filmcraft_wav_audio::write_wav16(&vec![0.2; 6000], 1, 24000),
        )
        .unwrap();
        assert!(prepare_reference(&short, root.path()).await.is_err());
        assert!(!root.path().join("reference.wav").exists());
    }
    #[test]
    fn voice_ids_are_scoped_to_selected_family() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lm_main.int8.onnx"), b"fixture").unwrap();
        let pocket = TtsConfig {
            model_dir: Some(root.path().into()),
            ..TtsConfig::default()
        };
        let request = SynthesisRequest {
            pauses: None,
            num_steps: None,
            text: "hello".into(),
            voice_id: 1000,
            speed: 1.,
        };
        request.validate_for_model(&pocket).unwrap();
        assert!(request.validate_for_model(&TtsConfig::default()).is_err());
        assert!(SynthesisRequest {
            voice_id: 3,
            ..request
        }
        .validate_for_model(&pocket)
        .is_err());
        std::fs::write(root.path().join("model.onnx"), b"ambiguous").unwrap();
        assert!(family(&pocket).is_err());
    }
}
