//! Offline whisper.cpp adapter; paths come from server configuration and authorized library entries.
use crate::process_control::{
    capture_output_cancellable, stream_with_progress, ProcessRuntime, ProcessStatus,
};
use anyhow::{ensure, Context, Result};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
/// Models have their own admission budget; archive/source defaults stay unchanged.
pub async fn fingerprint_model(path:&Path)->std::result::Result<(u64,String),crate::project_archive::ArchiveError>{crate::project_archive::fingerprint_file_with_limit(path,4_000_000_000).await}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechConfig {
    pub executable: PathBuf,
    pub model: Option<PathBuf>,
    pub threads: usize,
}
impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            executable: "whisper-cli".into(),
            model: None,
            threads: 2,
        }
    }
}
impl SpeechConfig {
    pub fn from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> Result<Self> {
        let model = lookup("WHISPER_MODEL_PATH")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from);
        if let Some(path) = &model {
            ensure!(path.is_absolute(), "WHISPER_MODEL_PATH must be absolute");
        }
        let executable = lookup("WHISPER_CLI")
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| "whisper-cli".into());
        let threads = lookup("WHISPER_THREADS")
            .map(|v| v.parse::<usize>())
            .transpose()?
            .unwrap_or(2);
        ensure!((1..=16).contains(&threads), "WHISPER_THREADS must be 1..16");
        Ok(Self {
            executable,
            model,
            threads,
        })
    }
}
pub use crate::domain::speech_alignment::{SpeechSegment, SpeechSegmentation, Transcript};
struct OwnedDirectory(PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn seconds(us: u64) -> String {
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}
pub fn parse_transcription(bytes: &[u8], duration_us: u64) -> Result<(String, Vec<SpeechSegment>)> {
    ensure!(
        bytes.len() <= 4 * 1024 * 1024 && duration_us > 0,
        "invalid transcript budget"
    );
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let language = value["result"]["language"]
        .as_str()
        .context("missing detected language")?
        .to_owned();
    ensure!(language.len() <= 16, "invalid detected language");
    let entries = value["transcription"]
        .as_array()
        .context("missing transcription")?;
    ensure!(
        entries.len() <= 512,
        "too many speech segments; choose a shorter source range"
    );
    let mut segments: Vec<SpeechSegment> = Vec::new();
    for entry in entries {
        let text = entry["text"]
            .as_str()
            .context("missing speech text")?
            .trim();
        if text.is_empty() {
            continue;
        }
        ensure!(text.len() <= 4096, "speech segment too long");
        let start = entry["offsets"]["from"]
            .as_u64()
            .context("invalid speech start")?
            .checked_mul(1000)
            .context("speech start overflow")?;
        let end = entry["offsets"]["to"]
            .as_u64()
            .context("invalid speech end")?
            .checked_mul(1000)
            .context("speech end overflow")?;
        ensure!(start < end, "invalid speech interval");
        if start >= duration_us {
            continue;
        }
        let end = end.min(duration_us);
        ensure!(
            segments
                .last()
                .map_or(true, |previous| previous.end_us <= start),
            "overlapping speech segments"
        );
        segments.push(SpeechSegment {
            start_us: start,
            end_us: end,
            text: text.into(),
        });
    }
    Ok((language, segments))
}
pub async fn transcribe(
    runtime: &ProcessRuntime,
    config: &SpeechConfig,
    input: &Path,
    start_us: u64,
    end_us: u64,
    staging: &Path,
    language: &str,
) -> Result<Transcript> {
    transcribe_cancellable(
        runtime,
        config,
        input,
        start_us,
        end_us,
        staging,
        language,
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
}

pub async fn transcribe_cancellable(
    runtime: &ProcessRuntime,
    config: &SpeechConfig,
    input: &Path,
    start_us: u64,
    end_us: u64,
    staging: &Path,
    language: &str,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Transcript> {
    let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
    transcribe_observed(
        runtime, config, input, start_us, end_us, staging, language, cancel, &progress,
    )
    .await
}

/// Phase-weighted progress: preparation 0..20, reported Whisper 20..95, publication checks 95..99.
pub async fn transcribe_observed(
    runtime: &ProcessRuntime,
    config: &SpeechConfig,
    input: &Path,
    start_us: u64,
    end_us: u64,
    staging: &Path,
    language: &str,
    cancel: &tokio_util::sync::CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Transcript> {
    transcribe_segmented(
        runtime,
        config,
        input,
        start_us,
        end_us,
        staging,
        language,
        cancel,
        SpeechSegmentation::Phrase,
        progress,
    )
    .await
}

pub async fn transcribe_segmented(
    runtime: &ProcessRuntime,
    config: &SpeechConfig,
    input: &Path,
    start_us: u64,
    end_us: u64,
    staging: &Path,
    language: &str,
    cancel: &tokio_util::sync::CancellationToken,
    segmentation: SpeechSegmentation,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<Transcript> {
    ensure!(!cancel.is_cancelled(), "speech recognition cancelled");
    ensure!(
        start_us < end_us && end_us - start_us <= 600_000_000,
        "speech range must be 0..10 minutes"
    );
    ensure!(
        language == "auto"
            || (language.len() >= 2
                && language.len() <= 8
                && language
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-')),
        "invalid speech language"
    );
    let _ = progress.send(5.0);
    let model = config
        .model
        .as_ref()
        .context("local Whisper model is not configured")?
        .canonicalize()?;
    let size = tokio::fs::metadata(&model).await?.len();
    ensure!(
        (1_000_000..=4_000_000_000).contains(&size),
        "invalid Whisper model size"
    );
    let (_, model_hash) = fingerprint_model(&model).await?;
    let (_, source_hash) = crate::project_archive::fingerprint_file(input).await?;
    tokio::fs::create_dir_all(staging).await?;
    let work = staging.join(format!("speech-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir(&work).await?;
    let _owned = OwnedDirectory(work.clone());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).await?;
    }
    let wav = work.join("input.wav");
    let output = work.join("transcript");
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg
        .args([
            "-nostdin",
            "-v",
            "error",
            "-threads",
            "1",
            "-protocol_whitelist",
            "file,pipe,crypto",
            "-ss",
        ])
        .arg(seconds(start_us))
        .arg("-i")
        .arg(input)
        .arg("-t")
        .arg(seconds(end_us - start_us))
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            "-f",
            "wav",
        ])
        .arg(&wav);
    let decoded = capture_output_cancellable(
        runtime,
        ffmpeg,
        runtime.render_policy(vec![input.to_path_buf()], &wav),
        Duration::from_secs(120),
        cancel,
    )
    .await?;
    ensure!(decoded.status.success(), "audio extraction failed");
    ensure!(
        tokio::fs::metadata(&wav).await?.len() <= 20 * 1024 * 1024,
        "speech audio staging budget exceeded"
    );
    let _ = progress.send(20.0);
    let mut whisper = Command::new(&config.executable);
    whisper
        .args([
            "-ng",
            "-pp",
            "-ml",
            if segmentation == SpeechSegmentation::Word {
                "1"
            } else {
                "80"
            },
            "-sow",
            "-t",
        ])
        .arg(config.threads.to_string())
        .arg("-l")
        .arg(language)
        .arg("-m")
        .arg(&model)
        .arg("-f")
        .arg(&wav)
        .args(["-oj", "-of"])
        .arg(&output);
    let (recognized, _diagnostic) = stream_with_progress(
        runtime,
        whisper,
        runtime.render_policy(vec![model.clone(), wav.clone()], &output),
        |line| parse_whisper_progress(line).map(|value| 20.0 + value * 0.75),
        progress,
        cancel,
        Duration::from_secs(300),
    )
    .await?;
    match recognized {
        ProcessStatus::Ok => {}
        ProcessStatus::Cancelled => anyhow::bail!("local Whisper recognition cancelled"),
        ProcessStatus::TimedOut => anyhow::bail!("local Whisper recognition timeout"),
        ProcessStatus::Failed => anyhow::bail!("local Whisper recognition failed"),
    }
    let json = output.with_extension("json");
    ensure!(
        tokio::fs::metadata(&json).await?.len() <= 4 * 1024 * 1024,
        "speech transcript budget exceeded"
    );
    let (language, segments) =
        parse_transcription(&tokio::fs::read(json).await?, end_us - start_us)?;
    ensure!(
        crate::project_archive::fingerprint_file(input).await?.1 == source_hash,
        "source changed during recognition"
    );
    ensure!(
        fingerprint_model(&model).await?.1 == model_hash,
        "model changed during recognition"
    );
    ensure!(!cancel.is_cancelled(), "speech recognition cancelled");
    let _ = progress.send(99.0);
    Ok(Transcript {
        segmentation,
        language,
        model_hash,
        source_start_us: start_us,
        duration_us: end_us - start_us,
        segments,
    })
}
fn parse_whisper_progress(line: &str) -> Option<f64> {
    let value = line
        .trim()
        .strip_prefix("whisper_print_progress_callback: progress =")?
        .trim()
        .strip_suffix('%')?
        .trim()
        .parse::<u32>()
        .ok()?;
    (value <= 100).then_some(f64::from(value))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whisper_progress_accepts_only_the_reported_bounded_callback() {
        assert_eq!(
            parse_whisper_progress("whisper_print_progress_callback: progress =  33%"),
            Some(33.0)
        );
        for invalid in [
            "progress = 33%",
            "whisper_print_progress_callback: progress = 101%",
            "whisper_print_progress_callback: progress = NaN%",
            "whisper_print_progress_callback: progress = -1%",
        ] {
            assert_eq!(parse_whisper_progress(invalid), None);
        }
    }
    #[tokio::test]
    #[ignore = "requires explicit local Whisper model and public audio fixture"]
    async fn configured_real_whisper_reports_intermediate_progress() {
        let model = std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL").unwrap();
        let input = std::env::var("VIDEO_KADR_TEST_WHISPER_AUDIO").unwrap();
        let directory = tempfile::tempdir().unwrap();
        let audio = directory.path().join("public-repeat.wav");
        let runtime = ProcessRuntime::local_default();
        let mut command = Command::new("ffmpeg");
        command
            .args(["-nostdin", "-v", "error", "-stream_loop", "5", "-i"])
            .arg(&input)
            .args(["-t", "60", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le"])
            .arg(&audio);
        let cancel = tokio_util::sync::CancellationToken::new();
        let output = capture_output_cancellable(
            &runtime,
            command,
            runtime.render_policy(vec![PathBuf::from(input)], &audio),
            Duration::from_secs(30),
            &cancel,
        )
        .await
        .unwrap();
        assert!(output.status.success());
        let config = SpeechConfig {
            model: Some(model.into()),
            ..Default::default()
        };
        let (progress, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let transcript = transcribe_observed(
            &runtime,
            &config,
            &audio,
            0,
            60_000_000,
            &directory.path().join("staging"),
            "en",
            &cancel,
            &progress,
        )
        .await
        .unwrap();
        drop(progress);
        let mut values = Vec::new();
        while let Some(value) = receiver.recv().await {
            values.push(value);
        }
        assert!(!transcript.segments.is_empty());
        assert!(
            values.iter().any(|value| *value > 20.0 && *value < 95.0),
            "{values:?}"
        );
        assert!(
            values.windows(2).all(|pair| pair[0] <= pair[1]),
            "{values:?}"
        );
        assert_eq!(values.last(), Some(&99.0));
        assert_eq!(
            std::fs::read_dir(directory.path().join("staging"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn parses_millisecond_offsets_and_clamps_last_segment_without_rounding_to_frames() {
        let bytes=br#"{"result":{"language":"ru"},"transcription":[{"offsets":{"from":120,"to":930},"text":" Hello "},{"offsets":{"from":930,"to":1100},"text":"world"}]}"#;
        let (language, cues) = parse_transcription(bytes, 1_000_001).unwrap();
        assert_eq!(language, "ru");
        assert_eq!(
            cues[0],
            SpeechSegment {
                start_us: 120000,
                end_us: 930000,
                text: "Hello".into()
            }
        );
        assert_eq!(cues[1].end_us, 1_000_001);
    }
    #[test]
    fn rejects_overlaps_and_bad_configuration() {
        assert!(parse_transcription(br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":900},"text":"one"},{"offsets":{"from":800,"to":1000},"text":"two"}]}"#,2_000_000).is_err());
        assert!(
            SpeechConfig::from_lookup(&mut |key| if key == "WHISPER_MODEL_PATH" {
                Some("relative.bin".into())
            } else {
                None
            })
            .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "requires explicit local Whisper model and audio fixture"]
    async fn configured_real_whisper_fixture() {
        let model = std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL").expect("Whisper test model");
        let input = std::env::var("VIDEO_KADR_TEST_WHISPER_AUDIO").expect("Whisper test audio");
        let dir = tempfile::tempdir().unwrap();
        let config = SpeechConfig {
            model: Some(model.into()),
            ..Default::default()
        };
        let transcript = transcribe(
            &ProcessRuntime::local_default(),
            &config,
            Path::new(&input),
            0,
            11_000_000,
            dir.path(),
            "en",
        )
        .await
        .unwrap();
        assert!(!transcript.segments.is_empty());
        assert!(transcript
            .segments
            .iter()
            .any(|cue| cue.text.to_lowercase().contains("country")));
        assert!(transcript
            .segments
            .iter()
            .all(|cue| cue.end_us <= 11_000_000));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let word_transcript = transcribe_segmented(
            &ProcessRuntime::local_default(),
            &config,
            Path::new(&input),
            0,
            11_000_000,
            dir.path(),
            "en",
            &tokio_util::sync::CancellationToken::new(),
            SpeechSegmentation::Word,
            &progress,
        )
        .await
        .unwrap();
        let reference = parse_transcription(
            include_bytes!("../../tests/fixtures/whisper-jfk-word-reference.json"),
            11_000_000,
        )
        .unwrap()
        .1;
        assert_eq!(word_transcript.segments, reference);
        assert_eq!(word_transcript.segmentation, SpeechSegmentation::Word);
        assert!(word_transcript.segments.iter().all(|segment| segment
            .text
            .split_whitespace()
            .count()
            == 1));
        assert!(word_transcript
            .segments
            .iter()
            .any(|segment| segment.text == "country"
                && segment.start_us == 5_650_000
                && segment.end_us == 6_410_000));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let control = tempfile::tempdir().unwrap();
            let executable = control.path().join("controlled-whisper");
            let ready = control.path().join("ready");
            let late = control.path().join("late");
            std::fs::write(&executable, b"#!/bin/sh\ntrap '' TERM\ntouch \"$0.ready\"\n(trap '' TERM; sleep 1; touch \"$0.late\") & wait\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            let ready = ready.with_file_name("controlled-whisper.ready");
            let late = late.with_file_name("controlled-whisper.late");
            let cancel = tokio_util::sync::CancellationToken::new();
            let token = cancel.clone();
            let controlled = SpeechConfig {
                executable,
                ..config.clone()
            };
            let runtime = ProcessRuntime::local_default();
            let task = transcribe_cancellable(
                &runtime,
                &controlled,
                Path::new(&input),
                0,
                11_000_000,
                dir.path(),
                "en",
                &token,
            );
            let canceller = async {
                tokio::time::timeout(Duration::from_secs(10), async {
                    while !ready.exists() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .unwrap();
                cancel.cancel();
            };
            let (result, _) = tokio::join!(task, canceller);
            assert!(result.unwrap_err().to_string().contains("cancelled"));
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                0,
                "cancellation retained extracted audio"
            );
            tokio::time::sleep(Duration::from_millis(1100)).await;
            assert!(
                !late.exists(),
                "recognition descendant survived cancellation"
            );
        }
        let missing = SpeechConfig {
            executable: dir.path().join("missing-whisper-cli"),
            ..config
        };
        assert!(transcribe(
            &ProcessRuntime::local_default(),
            &missing,
            Path::new(&input),
            0,
            11_000_000,
            dir.path(),
            "en"
        )
        .await
        .is_err());
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "failed recognition must not retain derived audio or transcript"
        );
    }
}
