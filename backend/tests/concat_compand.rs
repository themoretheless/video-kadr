//! Actual original Sweet compand profile differential gate; no stored PCM fixtures.
#[path = "../src/domain/concat_compand.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg"]
fn original_sweet_compand_profile_samples_and_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f64le");
    let mut compared = 0;
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            for signal in 0..4 {
                let frames = rate as usize / 3 + 17;
                let source = (0..frames)
                    .flat_map(|i| {
                        (0..channels).map(move |ch| match signal {
                            0 => {
                                if i % 101 == 0 {
                                    if ch == 0 {
                                        4.
                                    } else {
                                        -2.
                                    }
                                } else {
                                    0.
                                }
                            }
                            1 => (((i / 317) % 5) as f64 / 4.) * if ch == 0 { 0.99 } else { -0.03 },
                            2 => {
                                if i % 2 == 0 {
                                    1.5
                                } else {
                                    -0.75
                                }
                            }
                            _ => {
                                let h = (i as u32)
                                    .wrapping_add(ch as u32 * 1061)
                                    .wrapping_mul(1664525)
                                    .wrapping_add(1013904223);
                                (((h >> 8) % 8193) as i32 - 4096) as f64 / 4096.
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                let mut file = std::fs::File::create(&path).unwrap();
                for value in &source {
                    file.write_all(&value.to_le_bytes()).unwrap();
                }
                drop(file);
                let original=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","f64le","-ar",&rate.to_string(),"-ac",&channels.to_string(),"-i"]).arg(&path).args(["-af","compand=attacks=0.010:decays=0.180:points=-80/-80|-30/-24|-18/-13|-8/-5|0/-1.5:soft-knee=5:gain=1","-c:a","pcm_f64le","-f","f64le","pipe:1"]).output().unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                assert_eq!(original.stdout.len(), source.len() * 8);
                let mut effect = native::Compand::sweet(rate, channels);
                for pass in 0..2 {
                    if pass > 0 {
                        effect.reset();
                    }
                    let mut actual = source.clone();
                    for frame in actual.chunks_exact_mut(channels) {
                        effect.process_frame(frame);
                    }
                    for (i, (&actual, bytes)) in actual
                        .iter()
                        .zip(original.stdout.chunks_exact(8))
                        .enumerate()
                    {
                        let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                        assert!(actual.is_finite() && expected.is_finite() && (actual-expected).abs()<=2e-12,"rate{rate} channels{channels} signal{signal} pass{pass} value{i}: {actual} vs {expected}");
                        compared += 1;
                    }
                }
            }
        }
    }
    println!("Sweet compand: {compared} double values including reset replay");
}
