#[path = "../src/domain/concat_compressor.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg compressor"]
fn actual_enhance_voice_compressor_matches_owned_stage() {
    let dir = tempfile::tempdir().unwrap();
    let mut compared = 0;
    for rate in [8000, 44100, 48000, 192000] {
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
                        let amplitude = [0.1, 0.35, 1., 2.][(i * 8 / rate) % 4];
                        ((h >> 12) as i32 % 2049 - 1024) as f64 / 1024. * amplitude
                    })
                })
                .collect();
            let path = dir.path().join("input.f64le");
            let mut file = std::fs::File::create(&path).unwrap();
            for v in &source {
                file.write_all(&v.to_le_bytes()).unwrap();
            }
            drop(file);
            for (threshold, ratio) in [(0.5, 2.), (0.32, 3.2), (0.2, 4.), (0.38, 2.8)] {
                let out = std::process::Command::new("ffmpeg")
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
                        &format!(
                            "acompressor=threshold={threshold}:ratio={ratio}:attack=15:release=120"
                        ),
                        "-c:a",
                        "pcm_f64le",
                        "-f",
                        "f64le",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(out.stdout.len(), source.len() * 8);
                let mut effect = native::Compressor::new(rate as u32, threshold, ratio);
                for pass in 0..2 {
                    if pass == 1 {
                        effect.reset();
                    }
                    let mut actual = source.clone();
                    for block in actual.chunks_mut(257 * channels) {
                        for frame in block.chunks_exact_mut(channels) {
                            effect.process_frame(frame);
                        }
                    }
                    let mut max = 0f64;
                    for (i, (actual, raw)) in
                        actual.iter().zip(out.stdout.chunks_exact(8)).enumerate()
                    {
                        let expected = f64::from_le_bytes(raw.try_into().unwrap());
                        max = max.max((actual - expected).abs());
                        assert!(actual.is_finite() && (actual-expected).abs()<=2e-12,"rate{rate} ch{channels} threshold{threshold} ratio{ratio} pass{pass} value{i}: {actual} vs {expected}");
                    }
                    if pass == 0 {
                        compared += source.len();
                        println!(
                            "rate{rate} ch{channels} threshold{threshold} ratio{ratio}: max{max}"
                        );
                    }
                }
            }
        }
    }
    println!("qualified {compared} double PCM values");
}
