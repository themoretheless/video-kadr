//! Actual FFmpeg differential gate for the shared native Concat limiter primitive.
#[path = "../src/domain/concat_limiter.rs"]
mod native;
use native::LookaheadLimiter;
use std::io::Write;
fn input(frames: usize, channels: usize, signal: usize) -> Vec<f64> {
    (0..frames)
        .flat_map(|i| {
            (0..channels).map(move |ch| match signal {
                0 => {
                    if [0, 67, 303, frames - 1].contains(&i) {
                        if ch == 0 {
                            2.5
                        } else {
                            -1.25
                        }
                    } else {
                        0.
                    }
                }
                1 => {
                    (if (i / 117) % 3 == 0 { 0.03 } else { 3.5 }) * if ch == 0 { 1. } else { -0.7 }
                }
                _ => {
                    let h = (i as u32)
                        .wrapping_add(ch as u32 * 1061)
                        .wrapping_mul(1664525)
                        .wrapping_add(1013904223);
                    (((h >> 8) % 8193) as i32 - 4096) as f64 / 1024.
                }
            })
        })
        .collect()
}
#[test]
#[ignore = "requires actual FFmpeg"]
fn original_default_limiter_pcm_and_native_event_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f64le");
    let mut compared = 0;
    for rate in [8000, 13001, 44100, 48000, 192000] {
        for channels in [1, 2] {
            for signal in 0..3 {
                for (limit, level, release_ms) in [
                    (0.85, false, 50.),
                    (0.9, true, 50.),
                    (0.92, true, 50.),
                    (0.95, true, 50.),
                    (0.94, true, 60.),
                    (0.94, false, 1.),
                    (0.94, true, 500.),
                ] {
                    let source = input(rate as usize / 5 + 17, channels, signal);
                    let mut file = std::fs::File::create(&path).unwrap();
                    for v in &source {
                        file.write_all(&v.to_le_bytes()).unwrap();
                    }
                    drop(file);
                    let filter = format!(
                        "alimiter=limit={limit}:release={release_ms}:level={}",
                        if level { "true" } else { "false" }
                    );
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
                            &filter,
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
                    let mut effect = if release_ms == 50. {
                        LookaheadLimiter::new(rate, channels, limit, level)
                    } else {
                        LookaheadLimiter::with_release(rate, channels, limit, level, release_ms)
                    };
                    assert_eq!(
                        effect.audible_delay(),
                        (f64::from(rate) * 0.005) as usize - 1
                    );
                    let mut actual = source.clone();
                    for frame in actual.chunks_exact_mut(channels) {
                        effect.process_frame(frame);
                    }
                    for (index, (&actual, bytes)) in
                        actual.iter().zip(out.stdout.chunks_exact(8)).enumerate()
                    {
                        let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                        assert!((actual-expected).abs() <= 2e-12,"rate {rate} ch {channels} signal {signal} limit {limit} release {release_ms}, value {index}: {actual} vs {expected}, error {}",(actual-expected).abs());
                        compared += 1;
                    }
                    effect.reset();
                    let mut reset = source.clone();
                    for frame in reset.chunks_exact_mut(channels) {
                        effect.process_frame(frame);
                    }
                    assert_eq!(reset, actual);
                }
            }
        }
    }
    println!("compared {compared} double PCM values, unchanged 2e-12 absolute gate");
}
