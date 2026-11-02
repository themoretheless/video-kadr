#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch.rs"]
mod native;
use std::{collections::BTreeMap, io::Write};
fn frame_sizes(log: &[u8]) -> BTreeMap<usize, Vec<usize>> {
    let mut result: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for line in String::from_utf8_lossy(log)
        .lines()
        .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
    {
        let id = line
            .split("Parsed_ashowinfo_")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let count = line
            .split("nb_samples:")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        result.entry(id).or_default().push(count);
    }
    result
}
trait Input: native::Sample + concat_resample::FloatInput {
    const RAW: &'static str;
    fn sample(v: f64) -> Self;
    fn write(self, f: &mut std::fs::File);
}
macro_rules! input {
    ($ty:ty,$raw:literal,$scale:literal,$bias:literal) => {
        impl Input for $ty {
            const RAW: &'static str = $raw;
            fn sample(v: f64) -> Self {
                (v * $scale + $bias) as Self
            }
            fn write(self, f: &mut std::fs::File) {
                f.write_all(&self.to_le_bytes()).unwrap();
            }
        }
    };
}
input!(u8, "u8", 60., 128.);
input!(i16, "s16le", 12000., 0.);
input!(i32, "s32le", 1000000000., 0.);
input!(f32, "f32le", 1., 0.);
input!(f64, "f64le", 1., 0.);
fn qualify<T: Input>() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pcm");
    let mut compared = 0;
    for (index, c) in cases.iter().enumerate().filter(|(_, c)| {
        c["signal"] == "noise"
            && [-12f32, -7., 0., 7., 12., 6.555]
                .contains(&(c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32))
    }) {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let semi = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let second = cases
            .iter()
            .find(|c| {
                c["rate"] == 48000
                    && c["channels"] == channels
                    && c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32 == -semi
            })
            .unwrap();
        let frames = 8193usize;
        let source: Vec<Vec<T>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| {
                        let h = ((i % 257 + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        T::sample(
                            (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096.
                                + (i % 7) as f64 * 1e-10,
                        )
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
        let chain = format!(
            "{},ashowinfo,{},ashowinfo",
            c["chain"].as_str().unwrap(),
            second["chain"].as_str().unwrap()
        );
        let reference = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "info",
                "-f",
                T::RAW,
                "-ar",
                &rate.to_string(),
                "-ac",
                &channels.to_string(),
                "-i",
            ])
            .arg(&path)
            .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let sizes = frame_sizes(&reference.stderr);
        assert!(sizes.keys().all(|id| [4, 9].contains(id)));
        let mut first = native::Pitch::<T>::new(rate as u32, channels, semi);
        let mut next = native::FloatPitch::<T, f32>::new(48000, channels, -semi);
        for pass in 0..3 {
            if pass > 0 {
                first.reset();
                next.reset();
            }
            let mut actual = vec![Vec::new(); channels];
            let mut first_sizes = Vec::new();
            let mut next_sizes = Vec::new();
            let mut forward = |packets: Vec<Vec<Vec<T>>>, next: &mut native::FloatPitch<T, f32>| {
                for packet in packets {
                    assert!(!packet[0].is_empty());
                    first_sizes.push(packet[0].len());
                    let refs: Vec<&[T]> = packet.iter().map(Vec::as_slice).collect();
                    for out in next.push_packet_frames(&refs, false) {
                        assert!(!out[0].is_empty());
                        next_sizes.push(out[0].len());
                        for (dst, src) in actual.iter_mut().zip(out) {
                            dst.extend(src);
                        }
                    }
                }
            };
            let mut at = 0;
            let mut block = 0;
            while at < frames {
                let count = if pass == 0 {
                    65536
                } else {
                    [1, 255, 17, 1024, 4096][block % 5]
                }
                .min(frames - at);
                let refs: Vec<&[T]> = source.iter().map(|ch| &ch[at..at + count]).collect();
                forward(
                    first.push_frames(&refs, pass != 2 && at + count == frames),
                    &mut next,
                );
                at += count;
                block += 1;
            }
            if pass == 2 {
                forward(first.push_frames(&vec![&[][..]; channels], true), &mut next);
            }
            drop(forward);
            for out in next.push_packet_frames(&vec![&[][..]; channels], true) {
                assert!(!out[0].is_empty());
                next_sizes.push(out[0].len());
                for (dst, src) in actual.iter_mut().zip(out) {
                    dst.extend(src);
                }
            }
            assert_eq!(
                &first_sizes,
                sizes.get(&4).map(Vec::as_slice).unwrap_or(&[]),
                "{} case{index} pass{pass} first frames",
                T::RAW
            );
            assert_eq!(
                &next_sizes,
                sizes.get(&9).map(Vec::as_slice).unwrap_or(&[]),
                "{} case{index} pass{pass} second frames",
                T::RAW
            );
            assert_eq!(
                actual[0].len() * channels * 4,
                reference.stdout.len(),
                "{} case{index} pass{pass} output length",
                T::RAW
            );
            for (i, raw) in reference.stdout.chunks_exact(4).enumerate() {
                let expected = f32::from_le_bytes(raw.try_into().unwrap());
                let value = actual[i % channels][i / channels];
                assert!(value.is_finite()&&(value-expected).abs()<=2e-7,"{} case{index} rate{rate} ch{channels} semi{semi} pass{pass} value{i}: {value} vs {expected}",T::RAW);
            }
        }
        compared += reference.stdout.len() / 4;
    }
    println!(
        "qualified {compared} {} repeated-Pitch values and both exact frame-size sequences",
        T::RAW
    );
}
#[test]
#[ignore = "requires actual FFmpeg framed chained-Pitch transport"]
fn original_repeated_pitch_retains_real_output_frames() {
    qualify::<u8>();
    qualify::<i16>();
    qualify::<i32>();
    qualify::<f32>();
    qualify::<f64>();
}
