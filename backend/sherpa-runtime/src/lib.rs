//! Native official sherpa API, including exact zero sentence-gap generation.
//! Host publication, identity and authorization remain the caller's responsibility.
use anyhow::{ensure, Context, Result};
use sherpa_onnx::{GenerationConfig, OfflineTts, OfflineTtsConfig};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub struct Kokoro(OfflineTts);
#[derive(Debug)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}
impl Kokoro {
    pub fn load(root: &Path, threads: i32) -> Result<Self> {
        ensure!(
            root.is_absolute() && (1..=16).contains(&threads),
            "invalid runtime configuration"
        );
        let path = |name: &str| -> Result<String> {
            let p = root.join(name);
            let metadata = std::fs::symlink_metadata(&p)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "model entry is a symlink"
            );
            p.to_str()
                .map(str::to_owned)
                .context("model path is not UTF-8")
        };
        let mut config = OfflineTtsConfig::default();
        config.model.num_threads = threads;
        config.model.provider = Some("cpu".into());
        config.max_num_sentences = 1;
        config.silence_scale = 0.2;
        config.model.kokoro.model = Some(path(if root.join("model.int8.onnx").is_file() {
            "model.int8.onnx"
        } else {
            "model.onnx"
        })?);
        config.model.kokoro.voices = Some(path("voices.bin")?);
        config.model.kokoro.tokens = Some(path("tokens.txt")?);
        config.model.kokoro.data_dir = Some(path("espeak-ng-data")?);
        config.model.kokoro.lexicon = Some(format!(
            "{},{}",
            path("lexicon-us-en.txt")?,
            path("lexicon-zh.txt")?
        ));
        Ok(Self(
            OfflineTts::create(&config).context("create Kokoro engine")?,
        ))
    }
    pub fn speak(
        &self,
        text: &str,
        voice: i32,
        speed: f32,
        pauses: f32,
        cancel: Arc<AtomicBool>,
        mut progress: impl FnMut(f32) + 'static,
    ) -> Result<Audio> {
        ensure!(
            !text.trim().is_empty() && text.len() <= 4096 && !text.contains('\0'),
            "invalid narration text"
        );
        ensure!(
            (0..=27).contains(&voice) || (45..=52).contains(&voice),
            "unsupported speaker"
        );
        ensure!(
            speed.is_finite()
                && (0.5..=2.).contains(&speed)
                && pauses.is_finite()
                && (0.0..=1.).contains(&pauses),
            "invalid generation controls"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        let observed = cancel.clone();
        let config = GenerationConfig {
            sid: voice,
            speed,
            silence_scale: pauses,
            ..Default::default()
        };
        let output = self
            .0
            .generate_with_config(
                text,
                &config,
                Some(move |_: &[f32], p: f32| {
                    if observed.load(Ordering::Relaxed) {
                        return false;
                    }
                    progress(p);
                    !observed.load(Ordering::Relaxed)
                }),
            )
            .context("native generation failed")?;
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        let samples = output.samples();
        let rate = output.sample_rate();
        ensure!(
            rate > 0
                && !samples.is_empty()
                && samples.len() <= rate as usize * 600
                && samples.iter().all(|s| s.is_finite()),
            "invalid generated audio"
        );
        Ok(Audio {
            samples: samples.to_vec(),
            sample_rate: rate as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires official Kokoro model and shared sherpa library"]
    fn actual_zero_and_full_sentence_pauses() {
        let root = std::env::var("VIDEO_KADR_TEST_TTS_MODEL").unwrap();
        let engine = Kokoro::load(Path::new(&root), 2).unwrap();
        let text="Welcome to our video project. We edit pictures and add clear narration. Thank you for listening.";
        let zero = engine
            .speak(text, 3, 1., 0., Arc::new(AtomicBool::new(false)), |_| {})
            .unwrap();
        let full = engine
            .speak(text, 3, 1., 1., Arc::new(AtomicBool::new(false)), |_| {})
            .unwrap();
        assert_eq!(zero.sample_rate, full.sample_rate);
        assert!(
            full.samples.len() > zero.samples.len(),
            "full pauses must add actual samples"
        );
        let cancel = Arc::new(AtomicBool::new(false));
        let requested = cancel.clone();
        assert!(engine
            .speak(text, 3, 1., 0.2, cancel, move |_| {
                requested.store(true, Ordering::Relaxed)
            })
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert!(engine
            .speak(
                "Welcome back.",
                3,
                1.,
                0.2,
                Arc::new(AtomicBool::new(false)),
                |_| {}
            )
            .is_ok());
    }
}
