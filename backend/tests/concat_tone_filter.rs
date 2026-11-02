#[path = "../src/domain/concat_audio_filter.rs"]
mod concat_audio_filter;
#[path = "../src/domain/concat_tone_filter.rs"]
mod concat_tone_filter;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg double shelf reference"]
fn actual_double_shelf_arithmetic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.f64le");
    let source: Vec<f64> = (0..5017)
        .map(|i| {
            if i == 0 {
                1.25
            } else {
                let h = (i as u32).wrapping_mul(1664525).wrapping_add(1013904223);
                (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096.
            }
        })
        .collect();
    let mut file = std::fs::File::create(&path).unwrap();
    for v in &source {
        file.write_all(&v.to_le_bytes()).unwrap();
    }
    drop(file);
    for (kind, name, freq, width, gain, high) in [
        (0, "bass", 110., 0.6, 5., false),
        (0, "treble", 9000., 0.7, 4., true),
        (0, "treble", 6000., 0.8, 5., true),
        (1, "highpass", 400., 0.707, 0., false),
        (2, "lowpass", 3400., 0.707, 0., false),
        (3, "equalizer", 200., 2., 3.5, false),
        (3, "equalizer", 1600., 1.4, 4.5, false),
    ] {
        let chain = match kind {
            0 => format!("{name}=g={gain}:f={freq}:w={width}"),
            1 | 2 => format!("{name}=f={freq}"),
            _ => format!("equalizer=f={freq}:t=q:w={width}:g={gain}"),
        };
        let result = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f64le",
                "-ar",
                "44100",
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
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut filter = match kind {
            0 => concat_tone_filter::Shelf::new(44100, 1, freq, width, gain, high),
            1 => concat_tone_filter::Shelf::highpass(44100, 1, freq),
            2 => concat_tone_filter::Shelf::lowpass(44100, 1, freq),
            _ => concat_tone_filter::Shelf::peaking(44100, 1, freq, width, gain),
        }
        .unwrap();
        let mut max = 0f64;
        let mut different = 0;
        for (i, (&input, bytes)) in source.iter().zip(result.stdout.chunks_exact(8)).enumerate() {
            let actual = filter.process(0, input, true, i + 1 == source.len());
            let expected = f64::from_le_bytes(bytes.try_into().unwrap());
            if actual.to_bits() != expected.to_bits() {
                if different < 3 {
                    println!("{name} f{freq} i{i}: actual {actual:.17e} {:016x}, expected {expected:.17e} {:016x}",actual.to_bits(),expected.to_bits());
                }
                different += 1;
            }
            max = max.max((actual - expected).abs());
        }
        println!("{name} frequency{freq}: max error {max}, unequal bits {different}");
        assert!(max <= 2e-12);
    }
}
