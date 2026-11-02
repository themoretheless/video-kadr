#[path = "../src/domain/concat_robot.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg modulation filters"]
fn modulation_matches_original_filter_contract() {
    let dir = tempfile::tempdir().unwrap();
    for rate in [8000, 44100, 48000] {
        for channels in [1, 2] {
            for speed in [0.56, 1.2, 2.] {
                let source: Vec<f64> = (0..(rate + 317) * channels)
                    .map(|i| {
                        let h = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
                        ((h >> 12) as i32 % 2049 - 1024) as f64 / 4096.
                    })
                    .collect();
                let path = dir.path().join("input.f64le");
                let mut file = std::fs::File::create(&path).unwrap();
                for value in &source {
                    file.write_all(&value.to_le_bytes()).unwrap();
                }
                drop(file);
                let out = std::process::Command::new("ffmpeg").args([
                    "-nostdin", "-v", "error", "-f", "f64le", "-ar", &rate.to_string(),
                    "-ac", &channels.to_string(), "-i", path.to_str().unwrap(), "-af",
                    &format!("aphaser=type=t:speed={speed}:decay=0.6,flanger=delay=2:depth=4:speed=0.8"),
                    "-c:a", "pcm_f64le", "-f", "f64le", "pipe:1"
                ]).output().unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(out.stdout.len(), source.len() * 8);
                let mut effect = native::Robot::new(rate as u32, channels, speed);
                let mut actual = source.clone();
                for frame in actual.chunks_exact_mut(channels) {
                    effect.process_frame(frame, true);
                }
                let mut max = 0f64;
                for (i, (actual, raw)) in actual.iter().zip(out.stdout.chunks_exact(8)).enumerate()
                {
                    let expected = f64::from_le_bytes(raw.try_into().unwrap());
                    max = max.max((actual - expected).abs());
                    assert!((actual-expected).abs() <= 2e-12, "rate{rate} ch{channels} speed{speed} value{i}: {actual} vs {expected}, max{max}");
                }
                effect.reset();
                println!("rate{rate} ch{channels} speed{speed}: max{max}");
            }
        }
    }
}
