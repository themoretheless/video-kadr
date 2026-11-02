#[path = "../src/domain/concat_deesser.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg deesser"]
fn actual_deesser_matches_owned_stage_and_reset() {
    let dir = tempfile::tempdir().unwrap();
    let mut values = 0;
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            let frames = 8193;
            let source: Vec<f64> = (0..frames * channels)
                .map(|i| {
                    if i % 991 < channels {
                        return 0.;
                    }
                    let h = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
                    ((h >> 12) as i32 % 2049 - 1024) as f64 / 1024.
                })
                .collect();
            let path = dir.path().join("input.f64le");
            let mut file = std::fs::File::create(&path).unwrap();
            for v in &source {
                file.write_all(&v.to_le_bytes()).unwrap();
            }
            drop(file);
            for (intensity, maximum, frequency) in [
                (0., 0.5, 0.5),
                (0.3, 0.5, 0.5),
                (0.5, 0.5, 0.5),
                (0.18, 0.45, 0.55),
                (0.388, 0.45, 0.55),
                (0.5, 0.45, 0.55),
            ] {
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
                        &format!("deesser=i={intensity}:m={maximum}:f={frequency}"),
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
                let mut effect =
                    native::Deesser::new(rate as u32, channels, intensity, maximum, frequency);
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
                        assert!(actual.is_finite() && (actual-expected).abs()<=2e-12,"rate{rate} ch{channels} intensity{intensity} pass{pass} value{i}: {actual} vs {expected}");
                    }
                    if pass == 0 {
                        values += source.len();
                        println!("rate{rate} ch{channels} i{intensity} m{maximum} f{frequency}: max{max}");
                    }
                }
            }
        }
    }
    println!("qualified {values} double PCM values");
}
