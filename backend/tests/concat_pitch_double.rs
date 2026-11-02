#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg double Pitch graph"]
fn original_double_pitch_graph_streaming_reset_and_eof() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f64le");
    let mut compared = 0;
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let signal = c["signal"].as_str().unwrap();
        let source: Vec<Vec<f64>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| {
                        if signal == "steps" {
                            return (if ch == 0 { 2. } else { -1.5 })
                                * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. };
                        }
                        if [0, rate / 7, frames - 1].contains(&i) {
                            return [1.25, -0.75][ch];
                        }
                        let h = ((i % 257 + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096. + (i % 7) as f64 * 1e-10
                    })
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
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f64le",
                "-ar",
                &rate.to_string(),
                "-ac",
                &channels.to_string(),
                "-i",
                path.to_str().unwrap(),
                "-af",
                c["chain"].as_str().unwrap(),
                "-c:a",
                "pcm_f64le",
                "-f",
                "f64le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let semitones = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let mut pitch = native::Pitch::new(rate as u32, channels, semitones);
        let mut last = vec![];
        let mut maximum = 0f64;
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
                let input: Vec<&[f64]> = source.iter().map(|ch| &ch[at..at + count]).collect();
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
                actual[0].len() * channels * 8,
                output.stdout.len(),
                "case{index} pass{pass} frame count"
            );
            for (i, raw) in output.stdout.chunks_exact(8).enumerate() {
                let reference = f64::from_le_bytes(raw.try_into().unwrap());
                let value = actual[i % channels][i / channels];
                let error = (value - reference).abs();
                maximum = maximum.max(error);
                assert!(value.is_finite() && error<=2e-12,"case{index} rate{rate} ch{channels} semitones{semitones} signal{signal} pass{pass} value{i}: {value} vs {reference}, error{error}");
                if pass == 0 {
                    compared += 1;
                }
            }
            if pass > 0 {
                assert!(actual == last, "case{index} reset/chunk invariance");
            }
            last = actual;
        }
        println!("case{index} rate{rate} ch{channels} semitones{semitones} {signal}: max{maximum}");
    }
    println!("qualified {compared} original double PCM values, reset/chunk/EOF replay");
}
