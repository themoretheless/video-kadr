//! Independent Chatterbox Turbo runtime foundation based on the model author's
//! ONNX input/output contract. Not wired into product synthesis until qualified.
use anyhow::{ensure, Context, Result};
pub mod preprocessing;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Component, Path},
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub repository: String,
    pub revision: String,
    pub files: Vec<File>,
}
impl Manifest {
    pub fn bundled() -> Result<Self> {
        let value: Self = serde_json::from_str(include_str!("../model.json"))?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.repository == "ResembleAI/chatterbox-turbo-ONNX"
                && self.revision == "d21799bd0354adb85e348b8a0442a8405110a2cf",
            "unsupported model source"
        );
        ensure!(self.files.len() == 10, "incomplete inventory");
        let mut names = BTreeSet::new();
        let mut total = 0u64;
        for file in &self.files {
            ensure!(
                !file.path.is_empty()
                    && !file.path.contains(['\\', ':', '\0'])
                    && Path::new(&file.path)
                        .components()
                        .all(|p| matches!(p, Component::Normal(_)))
                    && file
                        .path
                        .split('/')
                        .all(|s| !s.is_empty() && s != "." && s != ".."),
                "invalid file path"
            );
            ensure!(
                names.insert(&file.path)
                    && file.bytes > 0
                    && file.sha256.len() == 64
                    && file
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid file identity"
            );
            total = total
                .checked_add(file.bytes)
                .context("model size overflow")?;
        }
        ensure!(total == 1_123_749_449, "unexpected model size");
        Ok(())
    }
    pub fn verify(&self, root: &Path) -> Result<()> {
        self.validate()?;
        ensure!(
            std::fs::symlink_metadata(root)?.file_type().is_dir(),
            "invalid model directory"
        );
        let expected: BTreeSet<_> = self.files.iter().map(|f| root.join(&f.path)).collect();
        let mut actual = BTreeSet::new();
        let mut pending = vec![root.to_owned()];
        let mut count = 0;
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                count += 1;
                ensure!(count <= 16, "unexpected model entries");
                let kind = entry.file_type()?;
                if kind.is_dir() {
                    pending.push(entry.path())
                } else {
                    ensure!(
                        kind.is_file() && expected.contains(&entry.path()),
                        "unexpected or symbolic model file"
                    );
                    actual.insert(entry.path());
                }
            }
        }
        ensure!(actual == expected, "missing model file");
        let mut buffer = [0u8; 65536];
        for file in &self.files {
            let mut input = std::fs::File::open(root.join(&file.path))?;
            ensure!(input.metadata()?.len() == file.bytes, "model size mismatch");
            let mut hash = Sha256::new();
            loop {
                let read = input.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
            ensure!(
                format!("{:x}", hash.finalize()) == file.sha256,
                "model checksum mismatch: {}",
                file.path
            );
        }
        Ok(())
    }
}
/// Uses the author's serialized tokenizer, including its two terminal tokens.
pub fn text_ids(root: &Path, text: &str) -> Result<Vec<i64>> {
    ensure!(
        !text.trim().is_empty() && text.len() <= 4096,
        "invalid narration text"
    );
    let tokenizer = tokenizers::Tokenizer::from_file(root.join("tokenizer.json"))
        .map_err(|e| anyhow::anyhow!("load tokenizer: {e}"))?;
    let encoded = tokenizer
        .encode(text, true)
        .map_err(|e| anyhow::anyhow!("encode text: {e}"))?;
    let ids: Vec<_> = encoded.get_ids().iter().map(|id| i64::from(*id)).collect();
    ensure!(
        ids.len() <= 1024 && ids.len() >= 3 && ids.ends_with(&[50256, 50256]),
        "invalid text boundary contract"
    );
    Ok(ids)
}
/// Qualification primitive; speech generation is not implemented by this API.
pub fn text_embedding(root: &Path, text: &str) -> Result<(Vec<i64>, Vec<f32>)> {
    let ids = text_ids(root, text)?;
    let count = ids.len();
    let mut session = ort::session::Session::builder()?
        .with_intra_threads(2)
        .map_err(|e| anyhow::anyhow!("configure ONNX session: {e}"))?
        .commit_from_file(root.join("onnx/embed_tokens_quantized.onnx"))?;
    let input = ort::value::Tensor::from_array(([1usize, count], ids.into_boxed_slice()))?;
    let output = session.run(ort::inputs!["input_ids"=>input])?;
    let (shape, values) = output[0].try_extract_tensor::<f32>()?;
    ensure!(
        shape.len() == 3
            && shape[0] == 1
            && shape[1] == count as i64
            && shape[2] == 1024
            && values.iter().all(|v| v.is_finite()),
        "invalid text embedding shape or values"
    );
    Ok((shape.to_vec(), values.to_vec()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_inventory_refuses_mutation() {
        let mut manifest = Manifest::bundled().unwrap();
        assert_eq!(manifest.files.len(), 10);
        manifest.files[0].path = "../outside".into();
        assert!(manifest.validate().is_err());
    }
    #[test]
    #[ignore = "requires explicit verified author bundle and dynamic ONNX runtime"]
    fn actual_author_tokenizer_and_embedding() {
        let root =
            std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_MODEL").unwrap());
        Manifest::bundled().unwrap().verify(&root).unwrap();
        let plain = text_ids(&root, "Welcome to our video project.").unwrap();
        let tagged = text_ids(&root, "Welcome [laugh] to our video project.").unwrap();
        assert!(plain.ends_with(&[50256, 50256]));
        assert_ne!(plain, tagged);
        let (shape, data) = text_embedding(&root, "Welcome to our video project.").unwrap();
        assert_eq!(shape[1], plain.len() as i64);
        assert_eq!(data.len(), plain.len() * 1024);
        assert!(data.iter().any(|v| *v != 0.));
    }
}

/// Owned CPU conditioning tensors. Inputs are already decoded mono24kHz PCM;
/// media decoding and source authorization remain the host application's job.
#[derive(Debug)]
pub struct FloatTensor {
    pub shape: Vec<i64>,
    pub values: Vec<f32>,
}
#[derive(Debug)]
pub struct VoiceConditioning {
    pub conditioning: FloatTensor,
    pub prompt_shape: Vec<i64>,
    pub prompt_tokens: Vec<i64>,
    pub speaker_embedding: FloatTensor,
    pub speaker_features: FloatTensor,
}
fn owned_float(value: &ort::value::DynValue, limit: usize) -> Result<FloatTensor> {
    let (shape, values) = value.try_extract_tensor::<f32>()?;
    ensure!(
        !shape.is_empty()
            && shape[0] == 1
            && !values.is_empty()
            && values.len() <= limit
            && values.iter().all(|v| v.is_finite()),
        "invalid voice tensor"
    );
    Ok(FloatTensor {
        shape: shape.to_vec(),
        values: values.to_vec(),
    })
}
fn validate_reference(samples: &[f32]) -> Result<()> {
    ensure!(
        (120000..=240000).contains(&samples.len()),
        "Chatterbox requires5..10seconds of mono24kHz PCM"
    );
    ensure!(
        samples.iter().all(|v| v.is_finite() && v.abs() <= 1.),
        "invalid reference PCM"
    );
    ensure!(
        samples.iter().any(|v| v.abs() > 1e-5),
        "reference is silent"
    );
    Ok(())
}
pub fn voice_conditioning(root: &Path, samples: &[f32]) -> Result<VoiceConditioning> {
    validate_reference(samples)?;
    let mut session = ort::session::Session::builder()?
        .with_intra_threads(2)
        .map_err(|e| anyhow::anyhow!("configure ONNX session: {e}"))?
        .commit_from_file(root.join("onnx/speech_encoder_quantized.onnx"))?;
    let input = ort::value::Tensor::from_array((
        [1usize, samples.len()],
        preprocessing::reference(samples).into_boxed_slice(),
    ))?;
    let output = session.run(ort::inputs!["audio_values"=>input])?;
    ensure!(output.len() == 4, "unexpected speech encoder contract");
    let conditioning = owned_float(&output[0], 1024 * 1024)?;
    ensure!(
        conditioning.shape.len() == 3 && conditioning.shape[2] == 1024,
        "unexpected conditioning width"
    );
    let (prompt_shape, tokens) = output[1].try_extract_tensor::<i64>()?;
    ensure!(
        prompt_shape.len() == 2
            && prompt_shape[0] == 1
            && !tokens.is_empty()
            && tokens.len() <= 1024
            && tokens.iter().all(|v| (0..6561).contains(v)),
        "invalid voice prompt tokens"
    );
    let speaker_embedding = owned_float(&output[2], 4096)?;
    let speaker_features = owned_float(&output[3], 240000)?;
    Ok(VoiceConditioning {
        conditioning,
        prompt_shape: prompt_shape.to_vec(),
        prompt_tokens: tokens.to_vec(),
        speaker_embedding,
        speaker_features,
    })
}
#[cfg(test)]
mod voice_tests {
    use super::*;
    #[test]
    fn invalid_reference_refuses_before_graph_loading() {
        let missing = Path::new("/no-chatterbox-model");
        for (samples, reason) in [
            (vec![0.1; 119999], "Chatterbox requires"),
            (vec![0.1; 240001], "Chatterbox requires"),
            (vec![0.; 120000], "reference is silent"),
            (vec![f32::NAN; 120000], "invalid reference PCM"),
            (vec![1.1; 120000], "invalid reference PCM"),
        ] {
            assert!(voice_conditioning(missing, &samples)
                .unwrap_err()
                .to_string()
                .contains(reason));
        }
    }
    #[test]
    #[ignore = "requires explicit author bundle, public voice WAV and dynamic ONNX runtime"]
    fn actual_author_voice_conditioning() {
        let root =
            std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_MODEL").unwrap());
        Manifest::bundled().unwrap().verify(&root).unwrap();
        let path = std::env::var("VIDEO_KADR_TEST_CHATTERBOX_REFERENCE").unwrap();
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 24000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .take(144000)
            .map(|s| s.unwrap() as f32 / 32768.)
            .collect();
        assert_eq!(samples.len(), 144000);
        let voice = voice_conditioning(&root, &samples).unwrap();
        eprintln!(
            "conditioning {:?}; prompt {:?}; speaker {:?}; features {:?}",
            voice.conditioning.shape,
            voice.prompt_shape,
            voice.speaker_embedding.shape,
            voice.speaker_features.shape
        );
        assert!(voice.conditioning.values.iter().any(|v| *v != 0.));
        assert!(voice.speaker_embedding.values.iter().any(|v| *v != 0.));
        assert!(voice.speaker_features.values.iter().any(|v| *v != 0.));
    }
}

pub mod generation;
