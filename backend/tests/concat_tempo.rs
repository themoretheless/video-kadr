#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_tempo.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg WSOLA tempo"]
fn original_tempo_pcm_streaming_reset_and_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let mut compared = 0;
    for signal in ["noise", "tone", "silence", "steps"] {
        for tempo in [0.5, 0.66741993, 1., 1.18920712, 1.5, 2.] {
            for channels in [1, 2] {
                for frames in [1, 511, 1024, 8193, 48017] {
                    let source: Vec<Vec<f32>> = (0..channels)
                        .map(|ch| {
                            (0..frames)
                                .map(|i| {
                                    if signal == "silence" {
                                        return 0.;
                                    }
                                    if signal == "tone" {
                                        return ((i as f64 * (0.071 + ch as f64 * 0.017)).sin()
                                            * 0.4)
                                            as f32;
                                    }
                                    if signal == "steps" {
                                        return (if ch == 0 { 2. } else { -3. })
                                            * if (i / 127) % 2 == 0 { 1. } else { -1. };
                                    }
                                    if i == 0 {
                                        return [1.25, -0.75][ch];
                                    }
                                    let h = ((i + ch * 1061) as u32)
                                        .wrapping_mul(1664525)
                                        .wrapping_add(1013904223);
                                    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
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
                            "f32le",
                            "-ar",
                            "48000",
                            "-ac",
                            &channels.to_string(),
                            "-i",
                            path.to_str().unwrap(),
                            "-af",
                            &format!("atempo={tempo}"),
                            "-c:a",
                            "pcm_f32le",
                            "-f",
                            "f32le",
                            "pipe:1",
                        ])
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let mut effect = native::Tempo::new(48000, channels, tempo);
                    for pass in 0..3 {
                        if pass > 0 {
                            effect.reset();
                        }
                        let mut actual = vec![Vec::new(); channels];
                        let mut at = 0;
                        let mut block = 0;
                        while at < frames {
                            let count = if pass == 0 {
                                frames - at
                            } else {
                                [1, 255, 17, 1024, 4096][block % 5].min(frames - at)
                            };
                            let input: Vec<&[f32]> =
                                source.iter().map(|ch| &ch[at..at + count]).collect();
                            let out = effect.push(&input, pass != 2 && at + count == frames);
                            for ch in 0..channels {
                                actual[ch].extend_from_slice(&out[ch]);
                            }
                            at += count;
                            block += 1;
                            let (retained, bound) = effect.retained_frames();
                            assert!(retained <= bound, "bounded WSOLA history");
                        }
                        if pass == 2 && frames == 48017 {
                            assert!(!actual[0].is_empty(), "tempo produces before EOF");
                        }
                        if pass == 2 {
                            let empty = vec![&[][..]; channels];
                            let out = effect.push(&empty, true);
                            for ch in 0..channels {
                                actual[ch].extend_from_slice(&out[ch]);
                            }
                        }
                        assert_eq!(actual[0].len()*channels*4,output.stdout.len(),"length signal{signal} tempo{tempo} ch{channels} frames{frames} pass{pass}");
                        let mut max = 0f32;
                        for (i, raw) in output.stdout.chunks_exact(4).enumerate() {
                            let expected = f32::from_le_bytes(raw.try_into().unwrap());
                            let value = actual[i % channels][i / channels];
                            let error = (value - expected).abs();
                            max = max.max(error);
                            assert!(value.is_finite() && error<=2e-7,"signal{signal} tempo{tempo} ch{channels} frames{frames} pass{pass} value{i}: {value} vs {expected}, error{error}");
                        }
                        if pass == 0 {
                            compared += output.stdout.len() / 4;
                            println!(
                                "signal{signal} tempo{tempo} ch{channels} frames{frames}: max{max}"
                            );
                        }
                    }
                }
            }
        }
    }
    println!("qualified {compared} float PCM values plus chunk/reset/empty EOF replay");
}
