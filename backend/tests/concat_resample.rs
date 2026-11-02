#[path = "../src/domain/concat_resample.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg rational resampler"]
fn original_kaiser_pcm_streaming_reset_and_short_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let mut compared = 0;
    for (from, to) in [
        (8000, 48000),
        (44100, 48000),
        (48000, 44100),
        (48000, 32000),
        (192000, 48000),
        (48000, 192000),
        (71919, 48000),
        (33941, 48000),
        (48000, 48000),
        (192000, 8000),
        (8000, 192000),
    ] {
        for channels in [1, 2] {
            for frames in [1, 17, 22, 31, 32, 33, 1001, 5017] {
                let source: Vec<Vec<f32>> = (0..channels)
                    .map(|ch| {
                        (0..frames)
                            .map(|i| {
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
                        &from.to_string(),
                        "-ac",
                        &channels.to_string(),
                        "-i",
                        path.to_str().unwrap(),
                        "-af",
                        &format!("aresample={to}"),
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
                let mut effect = native::Resampler::new(from, to, channels);
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
                            [1, 7, 31, 255, 17, 1024][block % 6].min(frames - at)
                        };
                        let input: Vec<&[f32]> =
                            source.iter().map(|ch| &ch[at..at + count]).collect();
                        let out = effect.push(&input, pass != 2 && at + count == frames);
                        for ch in 0..channels {
                            actual[ch].extend_from_slice(&out[ch]);
                        }
                        at += count;
                        block += 1;
                        let (retained, taps) = effect.retained_frames();
                        assert!(retained <= taps + 1, "bounded rational input window");
                    }
                    if pass == 2 && frames == 5017 {
                        assert!(!actual[0].is_empty(), "stream produces before EOF");
                    }
                    if pass == 2 {
                        let empty = vec![&[][..]; channels];
                        let out = effect.push(&empty, true);
                        for ch in 0..channels {
                            actual[ch].extend_from_slice(&out[ch]);
                        }
                    }
                    assert_eq!(
                        actual[0].len() * channels * 4,
                        output.stdout.len(),
                        "length {from}->{to} ch{channels} frames{frames} pass{pass}"
                    );
                    let mut max = 0f32;
                    for (i, raw) in output.stdout.chunks_exact(4).enumerate() {
                        let expected = f32::from_le_bytes(raw.try_into().unwrap());
                        let value = actual[i % channels][i / channels];
                        let error = (value - expected).abs();
                        max = max.max(error);
                        assert!(value.is_finite() && error<=2e-7,"{from}->{to} ch{channels} frames{frames} pass{pass} value{i}: {value} vs {expected}, error{error}");
                    }
                    if pass == 0 {
                        compared += output.stdout.len() / 4;
                        println!("{from}->{to} ch{channels} frames{frames}: max{max}");
                    }
                }
            }
        }
    }
    println!("qualified {compared} float PCM values plus chunk/reset/empty EOF replay");
}
