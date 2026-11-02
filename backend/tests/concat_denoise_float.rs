#[path = "../src/domain/concat_denoise.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg spectral denoiser"]
fn actual_float_white_noise_reduction_matches_owned_hops() {
    let dir = tempfile::tempdir().unwrap();
    let mut compared = 0;
    let mut failures = 0usize;
    for rate in [8000, 16000, 44100, 48000, 96000, 192000] {
        for channels in [1, 2] {
            let frames = rate + 317;
            let source: Vec<f64> = (0..frames)
                .flat_map(|i| {
                    (0..channels).map(move |ch| {
                        if (i * 8 / rate) % 8 >= 4 {
                            return 0.;
                        }
                        let h = ((i + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        let amplitude = [1e-5, 0.003, 1., 2.][(i * 8 / rate) % 4];
                        ((h >> 12) as i32 % 2049 - 1024) as f64 / 1024. * amplitude
                    })
                })
                .collect();
            let path = dir.path().join("input.f32le");
            let mut file = std::fs::File::create(&path).unwrap();
            for v in &source {
                file.write_all(&(*v as f32).to_le_bytes()).unwrap();
            }
            drop(file);
            for (noise_floor, reduction) in [(-20., 6.), (-35., 21.), (-50., 36.), (-37., 16.8)] {
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
                        &format!("afftdn=nf={noise_floor}:nr={reduction}"),
                        "-c:a",
                        "pcm_f32le",
                        "-f",
                        "f32le",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(out.stdout.len(), source.len() * 4);
                let mut effect = native::Denoiser::with_precision(
                    rate as u32,
                    channels,
                    noise_floor,
                    reduction,
                    true,
                );
                for pass in 0..2 {
                    if pass == 1 {
                        effect.reset();
                    }
                    let mut actual = Vec::new();
                    for block in source.chunks(effect.hop() * channels) {
                        let planar: Vec<Vec<f64>> = (0..channels)
                            .map(|ch| block.chunks_exact(channels).map(|f| f[ch]).collect())
                            .collect();
                        let output = effect.process_hop(&planar);
                        for i in 0..output[0].len() {
                            for ch in 0..channels {
                                actual.push(output[ch][i]);
                            }
                        }
                    }
                    let mut max = 0f64;
                    for (i, (actual, raw)) in
                        actual.iter().zip(out.stdout.chunks_exact(4)).enumerate()
                    {
                        let expected = f64::from(f32::from_le_bytes(raw.try_into().unwrap()));
                        max = max.max((actual - expected).abs());
                        if !actual.is_finite() || (actual - expected).abs() > 2e-7 {
                            failures += 1;
                            if failures < 4 {
                                println!("mismatch rate{rate} ch{channels} floor{noise_floor} reduction{reduction} pass{pass} value{i}: {actual} vs {expected}");
                            }
                        }
                    }
                    if pass == 0 {
                        compared += source.len();
                        println!(
                            "rate{rate} ch{channels} floor{noise_floor} reduction{reduction}: max{max}"
                        );
                    }
                }
            }
        }
    }
    println!("compared {compared} float PCM values; {failures} violations including reset replay");
    assert_eq!(
        failures, 0,
        "float spectral parity exceeds unchanged 2e-7 tolerance"
    );
}
