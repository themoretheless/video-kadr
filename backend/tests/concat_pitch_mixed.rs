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
    fn value(v: f64, i: usize, ch: usize) -> Self;
    fn write(self, file: &mut std::fs::File);
}
macro_rules! input {
    ($ty:ty,$format:literal,$scale:literal,$bias:literal) => {
        impl Input for $ty {
            const FORMAT: &'static str = $format;
            fn value(v: f64, _i: usize, _ch: usize) -> Self {
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
trait Output: native::Sample + concat_resample::FloatOutput {
    const FORMAT: &'static str;
    const CODEC: &'static str;
    const BYTES: usize;
    const TOLERANCE: f64;
    fn read(v: &[u8]) -> Self;
    fn number(self) -> f64;
}
impl Output for f32 {
    const FORMAT: &'static str = "f32le";
    const CODEC: &'static str = "pcm_f32le";
    const BYTES: usize = 4;
    const TOLERANCE: f64 = 2e-7;
    fn read(v: &[u8]) -> Self {
        Self::from_le_bytes(v.try_into().unwrap())
    }
    fn number(self) -> f64 {
        f64::from(self)
    }
}
impl Output for f64 {
    const FORMAT: &'static str = "f64le";
    const CODEC: &'static str = "pcm_f64le";
    const BYTES: usize = 8;
    const TOLERANCE: f64 = 2e-12;
    fn read(v: &[u8]) -> Self {
        Self::from_le_bytes(v.try_into().unwrap())
    }
    fn number(self) -> f64 {
        self
    }
}
fn qualify<I: Input, O: Output>() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pcm");
    let mut compared = 0usize;
    for (index, c) in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            [-12f32, -7., 0., 7., 12., 6.555]
                .contains(&(c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32))
        })
    {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = rate / 8 + 211;
        let semi = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let source: Vec<Vec<I>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| {
                        let v = if c["signal"] == "steps" {
                            (if ch == 0 { 2. } else { -1.5 })
                                * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. }
                        } else if [0, rate / 7, frames - 1].contains(&i) {
                            [1.25, -0.75][ch]
                        } else {
                            let h = ((i % 257 + ch * 1061) as u32)
                                .wrapping_mul(1664525)
                                .wrapping_add(1013904223);
                            (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096.
                                + (i % 7) as f64 * 1e-10
                        };
                        I::value(v, i, ch)
                    })
                    .collect()
            })
            .collect();
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..frames {
            for ch in &source {
                ch[i].write(&mut file);
            }
        }
        drop(file);
        let reference = std::process::Command::new("ffmpeg")
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
                c["chain"].as_str().unwrap(),
                "-c:a",
                O::CODEC,
                "-f",
                O::FORMAT,
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let mut pitch = native::FloatPitch::<I, O>::new(rate as u32, channels, semi);
        for pass in 0..3 {
            if pass > 0 {
                pitch.reset();
            }
            let mut actual = vec![Vec::new(); channels];
            let mut at = 0;
            let mut block = 0;
            while at < frames {
                let count = if pass == 0 {
                    65536
                } else {
                    [1, 255, 17, 1024, 4096][block % 5]
                }
                .min(frames - at);
                let refs: Vec<&[I]> = source.iter().map(|ch| &ch[at..at + count]).collect();
                let data = pitch.push(&refs, pass != 2 && at + count == frames);
                for (dst, src) in actual.iter_mut().zip(data) {
                    dst.extend(src);
                }
                at += count;
                block += 1;
            }
            if pass == 2 {
                let empty = vec![&[][..]; channels];
                for (dst, src) in actual.iter_mut().zip(pitch.push(&empty, true)) {
                    dst.extend(src);
                }
            }
            assert_eq!(
                actual[0].len() * channels * O::BYTES,
                reference.stdout.len(),
                "{}->{} case{index} pass{pass} frame count",
                I::FORMAT,
                O::FORMAT
            );
            for (i, raw) in reference.stdout.chunks_exact(O::BYTES).enumerate() {
                let expected = O::read(raw).number();
                let value = actual[i % channels][i / channels].number();
                let error = (value - expected).abs();
                assert!(value.is_finite()&&error<=O::TOLERANCE,"{}->{} case{index} rate{rate} ch{channels} semi{semi} pass{pass} value{i}: {value} vs {expected} error{error}",I::FORMAT,O::FORMAT);
            }
        }
        compared += reference.stdout.len() / O::BYTES;
        println!(
            "{}->{} case{index} rate{rate} ch{channels} semi{semi}: passed",
            I::FORMAT,
            O::FORMAT
        );
    }
    println!(
        "qualified {compared} {}->{} values plus chunk/reset/EOF replay",
        I::FORMAT,
        O::FORMAT
    );
}
#[test]
#[ignore = "requires actual FFmpeg mixed-format Pitch graphs"]
fn original_mixed_format_pitch_graphs() {
    qualify::<u8, f32>();
    qualify::<i16, f32>();
    qualify::<i32, f32>();
    qualify::<f32, f32>();
    qualify::<f64, f32>();
    qualify::<u8, f64>();
    qualify::<i16, f64>();
    qualify::<i32, f64>();
    qualify::<f32, f64>();
    qualify::<f64, f64>();
}
