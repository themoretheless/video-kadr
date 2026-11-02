#[path = "../src/domain/concat_log_crusher.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires real FFmpeg logarithmic quantizer"]
fn actual_double_log_quantizer_matches_native() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f64le");
    let mut source = vec![
        0., -0., 1., -1., 0.25, -0.75, 1e-100, -1e-50, 1e-12, -1e-8, 2., -4.,
    ];
    source.extend((0..8193).map(|i| {
        let h = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
        (((h >> 8) % 8193) as i32 - 4096) as f64 / 1024.
    }));
    let mut file = std::fs::File::create(&path).unwrap();
    for v in &source {
        file.write_all(&v.to_le_bytes()).unwrap();
    }
    drop(file);
    let mut values = 0;
    for bits in [4., 8., 12., 16.] {
        for mix in [0.6, 0.8] {
            let chain = format!("acrusher=bits={bits}:mode=log:mix={mix}");
            let out = std::process::Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "f64le",
                    "-ar",
                    "48000",
                    "-ac",
                    "1",
                    "-i",
                    path.to_str().unwrap(),
                    "-af",
                    &chain,
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
            let effect = native::LogCrusher::new(bits, mix);
            let mut max = 0f64;
            let mut different = 0;
            for (i, (&input, raw)) in source.iter().zip(out.stdout.chunks_exact(8)).enumerate() {
                let actual = effect.process(input);
                let expected = f64::from_le_bytes(raw.try_into().unwrap());
                max = max.max((actual - expected).abs());
                if actual.to_bits() != expected.to_bits() {
                    if different < 3 {
                        println!("bits{bits} mix{mix} value{i}: {actual:.17e} vs {expected:.17e}");
                    }
                    different += 1;
                }
                assert!(
                    actual.is_finite() && (actual - expected).abs() <= 2e-12,
                    "bits{bits} mix{mix} value{i}: {actual} vs {expected}"
                );
                values += 1;
            }
            println!("bits{bits} mix{mix}: max {max}, differing bits {different}");
        }
    }
    println!("qualified {values} double PCM values");
}
