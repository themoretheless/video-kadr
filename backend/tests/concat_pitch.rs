#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch.rs"]
mod native;
use sha2::{Digest, Sha256};
use std::io::Read;
fn source(i: usize, ch: usize, rate: usize, frames: usize, signal: &str) -> f32 {
    if i >= frames {
        return 0.;
    }
    if signal == "steps" {
        return (if ch == 0 { 2. } else { -1.5 }) * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. };
    }
    if [0, rate / 7, frames - 1].contains(&i) {
        return [1.25, -0.75][ch];
    }
    let h = (i % 257 + ch * 1061) as u32;
    let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
}
fn fixture() -> (serde_json::Value, Vec<u8>) {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(
        f["aliasVerified"],
        serde_json::json!({"hasShortAlias":false})
    );
    assert_eq!(f["cases"].as_array().unwrap().len(), 144);
    let mut pcm = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/concat-pitch-pcm.f32le.gz").as_slice())
        .take(128 * 1024 * 1024)
        .read_to_end(&mut pcm)
        .unwrap();
    assert_eq!(pcm.len() as u64, f["pcmBytes"].as_u64().unwrap());
    (f, pcm)
}
#[test]
fn original_pitch_graph_streaming_reset_and_eof() {
    let (f, pcm) = fixture();
    let mut compared = 0;
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let signal = c["signal"].as_str().unwrap();
        let source: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| source(i, ch, rate, frames, signal))
                    .collect()
            })
            .collect();
        let input: Vec<u8> = (0..frames)
            .flat_map(|i| source.iter().flat_map(move |ch| ch[i].to_le_bytes()))
            .collect();
        assert_eq!(
            format!("{:x}", Sha256::digest(&input)),
            c["inputSha256"].as_str().unwrap()
        );
        let offset = c["offsetBytes"].as_u64().unwrap() as usize;
        let length = c["byteLength"].as_u64().unwrap() as usize;
        let expected = &pcm[offset..offset + length];
        assert_eq!(
            format!("{:x}", Sha256::digest(expected)),
            c["outputSha256"].as_str().unwrap()
        );
        let semitones = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let mut pitch = native::Pitch::new(rate as u32, channels, semitones);
        assert_eq!(
            pitch.output_rate(),
            c["outputRate"].as_u64().unwrap() as u32
        );
        let mut last = vec![];
        for pass in 0..3 {
            if pass > 0 {
                pitch.reset();
            }
            let mut actual = vec![Vec::new(); channels];
            let mut at = 0;
            let mut block = 0;
            while at < frames {
                let count = if pass == 0 {
                    65536.min(frames - at)
                } else {
                    [1, 255, 17, 1024, 4096][block % 5].min(frames - at)
                };
                let input: Vec<&[f32]> = source.iter().map(|ch| &ch[at..at + count]).collect();
                let result = pitch.push(&input, pass != 2 && at + count == frames);
                for ch in 0..channels {
                    actual[ch].extend_from_slice(&result[ch]);
                }
                at += count;
                block += 1;
            }
            if pass == 2 {
                let empty = vec![&[][..]; channels];
                let result = pitch.push(&empty, true);
                for ch in 0..channels {
                    actual[ch].extend_from_slice(&result[ch]);
                }
            }
            assert_eq!(
                actual[0].len(),
                c["outputFrames"].as_u64().unwrap() as usize,
                "case{index} pass{pass} frame count"
            );
            for (i, raw) in expected.chunks_exact(4).enumerate() {
                let reference = f32::from_le_bytes(raw.try_into().unwrap());
                let value = actual[i % channels][i / channels];
                assert!(value.is_finite() && (value-reference).abs()<=2e-7,"case{index} rate{rate} ch{channels} semitones{semitones} signal{signal} pass{pass} value{i}: {value} vs {reference}");
                if pass == 0 {
                    compared += 1;
                }
            }
            if pass > 0 {
                assert_eq!(actual, last, "case{index} reset/chunk invariance");
            }
            last = actual;
        }
    }
    assert_eq!(compared * 4, pcm.len());
    println!("qualified {compared} original float PCM values, reset/chunk/EOF replay");
}

#[test]
#[ignore = "requires actual FFmpeg prefix/tempo isolation"]
fn isolate_pitch_prefix_and_tempo_packetization() {
    use std::io::Write;
    let (f, pcm) = fixture();
    let c = &f["cases"][24];
    let rate = c["rate"].as_u64().unwrap() as usize;
    let channels = c["channels"].as_u64().unwrap() as usize;
    let frames = c["inputFrames"].as_u64().unwrap() as usize;
    let signal = c["signal"].as_str().unwrap();
    let source: Vec<Vec<f32>> = (0..channels)
        .map(|ch| {
            (0..frames)
                .map(|i| source(i, ch, rate, frames, signal))
                .collect()
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let mut file = std::fs::File::create(&path).unwrap();
    for i in 0..frames {
        for ch in 0..channels {
            file.write_all(&source[ch][i].to_le_bytes()).unwrap();
        }
    }
    drop(file);
    let chain = c["chain"].as_str().unwrap();
    let (prefix, tempo) = chain.rsplit_once(",atempo=").unwrap();
    let tempo = tempo.parse::<f64>().unwrap();
    let run = |path: &std::path::Path, rate: usize, chain: &str| {
        let out = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f32le",
                "-ar",
                &rate.to_string(),
                "-ac",
                &channels.to_string(),
                "-i",
                path.to_str().unwrap(),
                "-af",
                chain,
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
            .chunks_exact(4)
            .map(|raw| f32::from_le_bytes(raw.try_into().unwrap()))
            .collect::<Vec<_>>()
    };
    let reference = run(&path, rate, prefix);
    let mut a = concat_resample::Resampler::new(rate as u32, 48000, channels);
    let mut b = concat_resample::Resampler::new(96000, 48000, channels);
    let refs: Vec<&[f32]> = source.iter().map(Vec::as_slice).collect();
    let normalized = a.push(&refs, true);
    let refs: Vec<&[f32]> = normalized.iter().map(Vec::as_slice).collect();
    let shifted = b.push(&refs, true);
    assert_eq!(reference.len(), shifted[0].len() * channels);
    let mut different = 0;
    let mut max = 0f32;
    for (i, &expected) in reference.iter().enumerate() {
        let value = shifted[i % channels][i / channels];
        max = max.max((value - expected).abs());
        if value.to_bits() != expected.to_bits() {
            different += 1;
        }
    }
    println!(
        "prefix frames{}, differing bits {different}, max {max}",
        shifted[0].len()
    );
    let prefix_path = dir.path().join("prefix.f32le");
    let mut file = std::fs::File::create(&prefix_path).unwrap();
    for &v in &reference {
        file.write_all(&v.to_le_bytes()).unwrap();
    }
    drop(file);
    let reframed = run(&prefix_path, 48000, &format!("atempo={tempo}"));
    let offset = c["offsetBytes"].as_u64().unwrap() as usize;
    let length = c["byteLength"].as_u64().unwrap() as usize;
    let golden: Vec<f32> = pcm[offset..offset + length]
        .chunks_exact(4)
        .map(|raw| f32::from_le_bytes(raw.try_into().unwrap()))
        .collect();
    let mut processor = concat_tempo::Tempo::new(48000, channels, tempo);
    let refs: Vec<&[f32]> = shifted.iter().map(Vec::as_slice).collect();
    let native = processor.push(&refs, true);
    println!(
        "original frames{}, reframed frames{}, native frames{}",
        golden.len() / channels,
        reframed.len() / channels,
        native[0].len()
    );
    for (name, data) in [("reframed", reframed), ("original", golden)] {
        let first = data
            .iter()
            .enumerate()
            .find(|(i, v)| (*v - native[*i % channels][*i / channels]).abs() > 2e-7)
            .map(|(i, _)| i);
        println!("native vs {name}: first mismatch {first:?}");
    }
    assert_eq!(
        different, 0,
        "prefix must preserve original PCM before tempo"
    );
}

#[test]
#[ignore = "requires actual FFmpeg source-frame qualification"]
fn explicit_source_packets_match_original_pitch() {
    use std::io::Write;
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let frames = 8193;
    let mut compared = 0;
    for channels in [1, 2] {
        let source: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| source(i, ch, 48000, frames, "noise"))
                    .collect()
            })
            .collect();
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..frames {
            for ch in 0..channels {
                file.write_all(&source[ch][i].to_le_bytes()).unwrap();
            }
        }
        drop(file);
        for semitones in [-7., 0., 7., 12.] {
            let case = f["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["settings"][0]["params"]["semitones"].as_f64() == Some(semitones))
                .unwrap();
            for packet in [1, 255, 1024, 4096] {
                let chain = format!(
                    "asetnsamples=n={packet}:p=0,{}",
                    case["chain"].as_str().unwrap()
                );
                let reference = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-f",
                        "f32le",
                        "-ar",
                        "48000",
                        "-ac",
                        &channels.to_string(),
                        "-i",
                        path.to_str().unwrap(),
                        "-af",
                        &chain,
                        "-c:a",
                        "pcm_f32le",
                        "-f",
                        "f32le",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(reference.status.success());
                let mut pitch =
                    native::Pitch::with_packet_size(48000, channels, semitones as f32, packet);
                for mode in 0..2 {
                    if mode == 1 {
                        pitch.reset();
                    }
                    let mut actual = vec![vec![]; channels];
                    let mut at = 0;
                    let mut block = 0;
                    while at < frames {
                        let count = if mode == 1 {
                            packet.min(frames - at)
                        } else {
                            [1, 255, 17, 1024, 4096][block % 5].min(frames - at)
                        };
                        let input: Vec<&[f32]> =
                            source.iter().map(|ch| &ch[at..at + count]).collect();
                        let out = if mode == 1 {
                            pitch.push_packet(&input, at + count == frames)
                        } else {
                            pitch.push(&input, at + count == frames)
                        };
                        for ch in 0..channels {
                            actual[ch].extend_from_slice(&out[ch]);
                        }
                        at += count;
                        block += 1;
                    }
                    assert_eq!(
                        actual[0].len() * channels * 4,
                        reference.stdout.len(),
                        "ch{channels} semitones{semitones} packet{packet} mode{mode}"
                    );
                    for (i, raw) in reference.stdout.chunks_exact(4).enumerate() {
                        let expected = f32::from_le_bytes(raw.try_into().unwrap());
                        let value = actual[i % channels][i / channels];
                        assert!((value-expected).abs()<=2e-7,"ch{channels} semitones{semitones} packet{packet} mode{mode} value{i}: {value} vs {expected}");
                        if mode == 0 {
                            compared += 1;
                        }
                    }
                }
            }
        }
    }
    println!("qualified {compared} float PCM values across 32 explicit frame profiles and both source APIs");
}
