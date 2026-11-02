//! Exact derived-shift constructor against metadata from the pinned Sweet compiler.
#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch.rs"]
mod native;
use std::io::Write;
trait Input: native::Sample + concat_resample::FloatInput {
    const FORMAT: &'static str;
    fn value(v: f64) -> Self;
    fn write(self, file: &mut std::fs::File);
}
macro_rules! input {
    ($ty:ty,$format:literal,$scale:literal,$bias:literal) => {
        impl Input for $ty {
            const FORMAT: &'static str = $format;
            fn value(v: f64) -> Self {
                (v * $scale + $bias) as Self
            }
            fn write(self, file: &mut std::fs::File) {
                file.write_all(&self.to_le_bytes()).unwrap();
            }
        }
    };
}
input!(u8, "u8", 60., 128.);
input!(i16, "s16le", 12000., 0.);
input!(i32, "s32le", 1000000000., 0.);
input!(f32, "f32le", 1., 0.);
input!(f64, "f64le", 1., 0.);
fn qualify<I: Input>() {
    let plans: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-sweet-plans.json")).unwrap();
    assert_eq!(plans["commit"], "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pcm");
    let mut compared = 0;
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            let frames = 8193;
            let source = (0..channels)
                .map(|ch| {
                    (0..frames)
                        .map(|i| {
                            let h = ((i % 257 + ch * 1061) as u32)
                                .wrapping_mul(1664525)
                                .wrapping_add(1013904223);
                            I::value(
                                (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096.
                                    + (i % 7) as f64 * 1e-10,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let mut file = std::fs::File::create(&path).unwrap();
            for i in 0..frames {
                for ch in &source {
                    ch[i].write(&mut file);
                }
            }
            drop(file);
            for (case, plan) in plans["cases"].as_array().unwrap().iter().enumerate() {
                let original = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-f",
                        I::FORMAT,
                        "-ar",
                        &rate.to_string(),
                        "-ac",
                        &channels.to_string(),
                        "-i",
                    ])
                    .arg(&path)
                    .args([
                        "-af",
                        plan["prefix"].as_str().unwrap(),
                        "-c:a",
                        "pcm_f64le",
                        "-f",
                        "f64le",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    original.status.success(),
                    "{}",
                    String::from_utf8_lossy(&original.stderr)
                );
                let mut graph = native::FloatPitch::<I, f64>::new_with_derived_source_planar(
                    rate,
                    channels,
                    plan["derivedShift"].as_f64().unwrap(),
                    false,
                );
                for grid in [257, 65536] {
                    graph.reset();
                    let mut actual = vec![Vec::new(); channels];
                    for at in (0..frames).step_by(grid) {
                        let end = (at + grid).min(frames);
                        let refs = source.iter().map(|ch| &ch[at..end]).collect::<Vec<_>>();
                        for (dst, ch) in actual.iter_mut().zip(graph.push(&refs, end == frames)) {
                            dst.extend(ch);
                        }
                    }
                    assert_eq!(
                        actual[0].len() * channels * 8,
                        original.stdout.len(),
                        "{} case{case} rate{rate} ch{channels} length",
                        I::FORMAT
                    );
                    for (i, bytes) in original.stdout.chunks_exact(8).enumerate() {
                        let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                        let value = actual[i % channels][i / channels];
                        assert!(value.is_finite() && (value-expected).abs()<=2e-12,"{} case{case} rate{rate} ch{channels} grid{grid} value{i}: {value} vs {expected}",I::FORMAT);
                        compared += 1;
                    }
                }
            }
        }
    }
    println!("derived {}: {compared} double values", I::FORMAT);
}
#[test]
#[ignore = "requires original Sweet plans and actual FFmpeg"]
fn original_sweet_derived_pitch_matches_all_source_scalars() {
    qualify::<u8>();
    qualify::<i16>();
    qualify::<i32>();
    qualify::<f32>();
    qualify::<f64>();
}
