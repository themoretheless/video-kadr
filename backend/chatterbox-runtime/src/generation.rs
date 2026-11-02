//! Author ONNX contracts with owned KV-cache tensors and bounded token sampling.
use crate::{text_ids, voice_conditioning, Manifest};
use anyhow::{ensure, Result};
use ort::{
    session::{Session, SessionInputValue},
    value::{DynValue, Tensor},
};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Clone, Copy)]
pub struct Options {
    pub max_tokens: usize,
    pub seed: u64,
    pub sample: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            max_tokens: 1024,
            seed: 42,
            sample: true,
        }
    }
}
fn session(root: &Path, file: &str) -> Result<Session> {
    Ok(Session::builder()?
        .with_intra_threads(2)
        .map_err(|e| anyhow::anyhow!("configure ONNX session: {e}"))?
        .commit_from_file(root.join("onnx").join(file))?)
}
fn floats(shape: Vec<usize>, values: Vec<f32>) -> Result<DynValue> {
    Ok(Tensor::from_array((shape, values.into_boxed_slice()))?.into_dyn())
}
fn ints(shape: Vec<usize>, values: Vec<i64>) -> Result<DynValue> {
    Ok(Tensor::from_array((shape, values.into_boxed_slice()))?.into_dyn())
}
fn embed(session: &mut Session, ids: Vec<i64>) -> Result<Vec<f32>> {
    let count = ids.len();
    let output = session.run(ort::inputs!["input_ids"=>ints(vec![1,count],ids)?])?;
    let (shape, data) = output[0].try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == [1, count as i64, 1024] && data.iter().all(|v| v.is_finite()),
        "invalid generated-token embedding"
    );
    Ok(data.to_vec())
}
fn draw(logits: &[f32], written: &[i64], rng: &mut u64, sample: bool) -> Result<i64> {
    ensure!(
        logits.len() == 6563 && logits.iter().all(|v| v.is_finite()),
        "invalid speech logits"
    );
    let mut seen = vec![false; logits.len()];
    for token in written {
        if let Ok(id) = usize::try_from(*token) {
            if id < seen.len() {
                seen[id] = true;
            }
        }
    }
    let mut scores: Vec<(usize, f64)> = logits
        .iter()
        .enumerate()
        .map(|(id, value)| {
            let value = if seen[id] {
                if *value < 0. {
                    *value * 1.2
                } else {
                    *value / 1.2
                }
            } else {
                *value
            };
            (id, f64::from(value) / 0.8)
        })
        .collect();
    scores.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    if !sample {
        return Ok(scores[0].0 as i64);
    }
    scores.truncate(1000);
    let peak = scores[0].1;
    let weights: Vec<f64> = scores.iter().map(|s| (s.1 - peak).exp()).collect();
    let total: f64 = weights.iter().sum();
    let mut kept = 0.;
    let mut end = 0;
    for weight in &weights {
        kept += weight;
        end += 1;
        if kept / total >= 0.95 {
            break;
        }
    }
    *rng = rng.wrapping_add(0x9e3779b97f4a7c15);
    let mut bits = *rng;
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d049bb133111eb);
    bits ^= bits >> 31;
    let mut target = ((bits >> 11) as f64 / (1u64 << 53) as f64) * kept;
    for i in 0..end {
        target -= weights[i];
        if target <= 0. {
            return Ok(scores[i].0 as i64);
        }
    }
    Ok(scores[end - 1].0 as i64)
}
/// Cached CPU networks and a prepared reference, reused across narration chunks.
pub struct Engine {
    root: std::path::PathBuf,
    voice: crate::VoiceConditioning,
    embedding: Session,
    language: Session,
    decoder: Session,
}
fn validate_text(text: &str, options: Options) -> Result<()> {
    ensure!(
        (1..=1024).contains(&options.max_tokens),
        "invalid generation bounds"
    );
    ensure!(
        !text.trim().is_empty() && text.len() <= 4096,
        "invalid narration text"
    );
    Ok(())
}
impl Engine {
    pub fn load(root: &Path, reference: &[f32], cancel: &AtomicBool) -> Result<Self> {
        crate::validate_reference(reference)?;
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        Manifest::bundled()?.verify(root)?;
        let voice = voice_conditioning(root, reference)?;
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        Ok(Self {
            root: root.to_owned(),
            voice,
            embedding: session(root, "embed_tokens_quantized.onnx")?,
            language: session(root, "language_model_quantized.onnx")?,
            decoder: session(root, "conditional_decoder_quantized.onnx")?,
        })
    }
    /// Token/cache state is new for every chunk; learned networks/reference are reused.
    pub fn chunk(
        &mut self,
        text: &str,
        options: Options,
        cancel: &AtomicBool,
        mut progress: impl FnMut(usize),
    ) -> Result<Vec<f32>> {
        validate_text(text, options)?;
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        let Self {
            root,
            voice,
            embedding,
            language,
            decoder,
        } = self;
        let text = embed(
            embedding,
            text_ids(root, &crate::preprocessing::text(text))?,
        )?;
        let mut current = voice.conditioning.values.clone();
        current.extend(text);
        let mut sequence = current.len() / 1024;
        let mut total = sequence;
        let names: Vec<String> = language
            .inputs()
            .iter()
            .filter(|i| i.name().starts_with("past_key_values"))
            .map(|i| i.name().to_owned())
            .collect();
        ensure!(names.len() == 48, "unexpected language cache contract");
        let mut cache = Vec::new();
        for _ in &names {
            cache.push(floats(vec![1, 16, 0, 64], vec![])?);
        }
        let mut written = vec![6561i64];
        let mut rng = options.seed;
        let mut stopped = false;
        for step in 0..options.max_tokens {
            ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
            let mut inputs: Vec<(String, SessionInputValue<'static>)> = vec![
                (
                    "inputs_embeds".into(),
                    floats(vec![1, sequence, 1024], current)?.into(),
                ),
                (
                    "attention_mask".into(),
                    ints(vec![1, total], vec![1; total])?.into(),
                ),
                (
                    "position_ids".into(),
                    ints(
                        vec![1, sequence],
                        ((total - sequence)..total).map(|n| n as i64).collect(),
                    )?
                    .into(),
                ),
            ];
            for (name, value) in names.iter().cloned().zip(cache) {
                inputs.push((name, value.into()));
            }
            let output = language.run(inputs)?;
            ensure!(output.len() == 49, "unexpected language outputs");
            let token = {
                let (shape, logits) = output[0].try_extract_tensor::<f32>()?;
                ensure!(
                    shape.len() == 3 && shape[0] == 1 && shape[2] == 6563 && logits.len() >= 6563,
                    "unexpected language logits"
                );
                draw(
                    &logits[logits.len() - 6563..],
                    &written,
                    &mut rng,
                    options.sample,
                )?
            };
            progress(step + 1);
            ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
            if token == 6562 {
                stopped = true;
                break;
            }
            ensure!((0..6561).contains(&token), "invalid speech token");
            written.push(token);
            cache = output.into_iter().skip(1).map(|(_, v)| v).collect();
            current = embed(embedding, vec![token])?;
            sequence = 1;
            total += 1;
        }
        ensure!(
            stopped && written.len() > 1,
            "speech generation did not finish within token limit"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        let mut tokens = voice.prompt_tokens.clone();
        tokens.extend_from_slice(&written[1..]);
        tokens.extend([4299, 4299, 4299]);
        let output=decoder.run(ort::inputs!["speech_tokens"=>ints(vec![1,tokens.len()],tokens)?,"speaker_embeddings"=>floats(voice.speaker_embedding.shape.iter().map(|n|*n as usize).collect(),voice.speaker_embedding.values.clone())?,"speaker_features"=>floats(voice.speaker_features.shape.iter().map(|n|*n as usize).collect(),voice.speaker_features.values.clone())?])?;
        let (shape, wave) = output[0].try_extract_tensor::<f32>()?;
        ensure!(
            shape.len() == 2
                && shape[0] == 1
                && !wave.is_empty()
                && wave.len() <= 24000 * 60
                && wave.iter().all(|v| v.is_finite()),
            "invalid generated waveform"
        );
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        Ok(wave.to_vec())
    }
}
/// Single-chunk compatibility entry point; cancellation is checked between graphs.
pub fn synthesize(
    root: &Path,
    text: &str,
    reference: &[f32],
    options: Options,
    cancel: &AtomicBool,
    progress: impl FnMut(usize),
) -> Result<Vec<f32>> {
    validate_text(text, options)?;
    crate::validate_reference(reference)?;
    ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
    Engine::load(root, reference, cancel)?.chunk(text, options, cancel, progress)
}
/// Sentence boundaries follow the source contract: newline, or .!? followed by
/// whitespace/end.250Unicode characters is a packing target, not truncation.
pub fn chunks(text: &str) -> Vec<String> {
    let mut boundaries = vec![0];
    for (index, ch) in text.char_indices() {
        let next = index + ch.len_utf8();
        if ch == '\n'
            || (".!?".contains(ch) && text[next..].chars().next().is_none_or(char::is_whitespace))
        {
            boundaries.push(next);
        }
    }
    if boundaries.last().copied() != Some(text.len()) {
        boundaries.push(text.len());
    }
    let mut output: Vec<String> = Vec::new();
    for span in boundaries.windows(2) {
        let sentence = text[span[0]..span[1]]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if sentence.is_empty() {
            continue;
        }
        if let Some(last) = output.last_mut() {
            if last.chars().count() + 1 + sentence.chars().count() <= 250 {
                last.push(' ');
                last.push_str(&sentence);
                continue;
            }
        }
        output.push(sentence);
    }
    output
}
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub chunk_index: usize,
    pub chunk_count: usize,
    pub tokens: usize,
}
/// Whole narration is returned only on success. At most600seconds are assembled;
/// failed/cancelled chunks never yield a partial successful utterance.
pub fn synthesize_long(
    root: &Path,
    text: &str,
    reference: &[f32],
    options: Options,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<Vec<f32>> {
    validate_text(text, options)?;
    let normalized = crate::preprocessing::text(text);
    let plan = chunks(&normalized);
    ensure!(
        !plan.is_empty() && plan.len() <= 32,
        "invalid narration chunk count"
    );
    let mut engine = Engine::load(root, reference, cancel)?;
    let mut output = Vec::new();
    for (index, text) in plan.iter().enumerate() {
        ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
        let wave = engine.chunk(
            text,
            Options {
                seed: options.seed.wrapping_add(index as u64),
                ..options
            },
            cancel,
            |tokens| {
                progress(Progress {
                    chunk_index: index,
                    chunk_count: plan.len(),
                    tokens,
                })
            },
        )?;
        let gap = if index == 0 { 0 } else { 6000 };
        ensure!(
            output.len() + gap + wave.len() <= 24000 * 600,
            "narration exceeds duration limit"
        );
        output.resize(output.len() + gap, 0.);
        output.extend(wave);
    }
    ensure!(!cancel.load(Ordering::Relaxed), "generation cancelled");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sentence_chunks_preserve_unicode_decimal_tags_and_long_sentences() {
        let text = "  Версия 2.5 работает!\n [laugh]  Next chapter?  Yes. ";
        assert_eq!(
            chunks(text),
            vec!["Версия 2.5 работает! [laugh] Next chapter? Yes."]
        );
        let first = format!("{}.", "я".repeat(248));
        let text = format!("{first} Next.");
        assert_eq!(chunks(&text), vec![first, "Next.".to_owned()]);
        let sentence = format!("{}.", "a".repeat(300));
        assert_eq!(chunks(&sentence), vec![sentence]);
        let text = "Alpha.\n\nBeta!  Gamma?";
        assert_eq!(chunks(text).join(" "), "Alpha. Beta! Gamma?");
        assert!(chunks(" \n\t ").is_empty());
    }
    #[test]
    fn sampler_penalty_changes_greedy_choice_and_seed_repeats() {
        let mut logits = vec![-100.; 6563];
        logits[10] = 2.;
        logits[11] = 1.9;
        assert_eq!(draw(&logits, &[], &mut 1, false).unwrap(), 10);
        assert_eq!(draw(&logits, &[10], &mut 1, false).unwrap(), 11);
        let mut a = 7;
        let mut b = 7;
        for _ in 0..20 {
            assert_eq!(
                draw(&logits, &[], &mut a, true).unwrap(),
                draw(&logits, &[], &mut b, true).unwrap()
            );
        }
        logits[0] = f32::NAN;
        assert!(draw(&logits, &[], &mut a, true).is_err());
    }
    #[test]
    fn invalid_or_cancelled_generation_refuses_before_model_loading() {
        let root = Path::new("/missing-chatterbox-generation-model");
        let reference = vec![0.1; 144000];
        let flag = AtomicBool::new(false);
        let failure =
            synthesize(root, "", &reference, Options::default(), &flag, |_| {}).unwrap_err();
        assert!(failure.to_string().contains("invalid narration text"));
        let failure = synthesize(
            root,
            "Hello.",
            &reference,
            Options {
                max_tokens: 0,
                ..Default::default()
            },
            &flag,
            |_| {},
        )
        .unwrap_err();
        assert!(failure.to_string().contains("invalid generation bounds"));
        let failure = synthesize(
            root,
            "Hello.",
            &reference[..100],
            Options::default(),
            &flag,
            |_| {},
        )
        .unwrap_err();
        assert!(failure.to_string().contains("Chatterbox requires"));
        flag.store(true, Ordering::Relaxed);
        let failure = synthesize(
            root,
            "Hello.",
            &reference,
            Options::default(),
            &flag,
            |_| {},
        )
        .unwrap_err();
        assert!(failure.to_string().contains("generation cancelled"));
    }
    #[test]
    #[ignore = "requires verified author models, public reference and ONNX Runtime"]
    fn actual_narration() {
        let root =
            std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_MODEL").unwrap());
        let path = std::env::var("VIDEO_KADR_TEST_CHATTERBOX_REFERENCE").unwrap();
        let mut reader = hound::WavReader::open(path).unwrap();
        assert_eq!(reader.spec().sample_rate, 24000);
        assert_eq!(reader.spec().channels, 1);
        let reference: Vec<f32> = reader
            .samples::<i16>()
            .take(144000)
            .map(|v| v.unwrap() as f32 / 32768.)
            .collect();
        let long = std::env::var_os("VIDEO_KADR_TEST_CHATTERBOX_LONG").is_some();
        let wave = if long {
            let text="Welcome to our video project, where we edit a short movie, arrange the pictures on a timeline, and add clear narration for every scene. The second chapter explains audio editing, music tracks, and subtitles, then describes how to save the finished project for our team.";
            assert_eq!(chunks(text).len(), 2);
            let mut seen = Vec::new();
            let wave = synthesize_long(
                &root,
                text,
                &reference,
                Options {
                    max_tokens: 512,
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |progress| {
                    assert_eq!(progress.chunk_count, 2);
                    seen.push(progress.chunk_index);
                    if progress.tokens % 50 == 0 {
                        eprintln!("chunk {} tokens {}", progress.chunk_index, progress.tokens);
                    }
                },
            )
            .unwrap();
            assert!(seen.contains(&0) && seen.contains(&1));
            assert!(seen.windows(2).all(|p| p[0] <= p[1]));
            assert!(wave.len() > 24000 * 10);
            wave
        } else {
            synthesize(
                &root,
                "Welcome to our video project.",
                &reference,
                Options {
                    max_tokens: 256,
                    ..Default::default()
                },
                &AtomicBool::new(false),
                |n| {
                    if n % 25 == 0 {
                        eprintln!("speech tokens: {n}")
                    }
                },
            )
            .unwrap()
        };
        let path = std::env::var("VIDEO_KADR_TEST_CHATTERBOX_OUTPUT").unwrap();
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        assert!(wave.len() >= 24000);
        for sample in wave {
            writer
                .write_sample((sample.clamp(-1., 1.) * 32767.).round() as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    #[ignore = "requires verified author models, public reference and ONNX Runtime"]
    fn actual_cancelled_engine_can_be_reused() {
        let root =
            std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_MODEL").unwrap());
        let reference_path = std::env::var("VIDEO_KADR_TEST_CHATTERBOX_REFERENCE").unwrap();
        let mut reader = hound::WavReader::open(reference_path).unwrap();
        assert_eq!(reader.spec().sample_rate, 24000);
        assert_eq!(reader.spec().channels, 1);
        let reference: Vec<f32> = reader
            .samples::<i16>()
            .take(144000)
            .map(|v| v.unwrap() as f32 / 32768.)
            .collect();
        let flag = AtomicBool::new(false);
        let mut engine = Engine::load(&root, &reference, &flag).unwrap();
        let mut progress = Vec::new();
        let failed = engine.chunk(
            "Welcome to our video project.",
            Options {
                max_tokens: 256,
                ..Default::default()
            },
            &flag,
            |tokens| {
                progress.push(tokens);
                flag.store(true, Ordering::Relaxed)
            },
        );
        assert!(failed
            .unwrap_err()
            .to_string()
            .contains("generation cancelled"));
        assert_eq!(progress, vec![1]);
        assert!(flag.load(Ordering::Relaxed));
        flag.store(false, Ordering::Relaxed);
        let mut seen = Vec::new();
        let wave = engine
            .chunk(
                "Welcome to our video project.",
                Options {
                    max_tokens: 256,
                    ..Default::default()
                },
                &flag,
                |tokens| seen.push(tokens),
            )
            .unwrap();
        assert_eq!(seen.first(), Some(&1));
        assert!(seen.len() > 1);
        assert!(wave.len() >= 24000 && wave.iter().all(|v| v.is_finite()));
        let output = std::env::var("VIDEO_KADR_TEST_CHATTERBOX_OUTPUT").unwrap();
        let mut writer = hound::WavWriter::create(
            output,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in wave {
            writer
                .write_sample((sample.clamp(-1., 1.) * 32767.).round() as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
    }
}
