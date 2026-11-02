#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch.rs"]
mod native;
use native as concat_pitch;
#[path = "../src/domain/concat_pitch_timeline.rs"]
mod concat_pitch_timeline;
use std::io::Write;
trait Integer: native::Sample + PartialEq + std::fmt::Display {
    const FORMAT: &'static str;
    const CODEC: &'static str;
    const BYTES: usize;
    fn quantize(value: f64, index: usize, channel: usize, frames: usize) -> Self;
    fn write(self, file: &mut std::fs::File);
    fn read(bytes: &[u8]) -> Self;
    fn integer(self) -> i64;
}
macro_rules! integer {
    ($ty:ty, $format:literal, $codec:literal, $bytes:literal, $scale:literal) => {
        impl Integer for $ty {
            const FORMAT: &'static str = $format;
            const CODEC: &'static str = $codec;
            const BYTES: usize = $bytes;
            fn quantize(value: f64, index: usize, channel: usize, frames: usize) -> Self {
                if index == 0 {
                    return if channel == 0 { Self::MAX } else { Self::MIN };
                }
                if index + 1 == frames {
                    return if channel == 0 { Self::MIN } else { Self::MAX };
                }
                // Low bits in PCM32 must survive untouched WSOLA bypass/tail samples.
                (value * $scale) as Self | ((index % 7) as Self)
            }
            fn write(self, file: &mut std::fs::File) {
                file.write_all(&self.to_le_bytes()).unwrap();
            }
            fn read(bytes: &[u8]) -> Self {
                Self::from_le_bytes(bytes.try_into().unwrap())
            }
            fn integer(self) -> i64 {
                i64::from(self)
            }
        }
    };
}
integer!(i16, "s16le", "pcm_s16le", 2, 12000.);
integer!(i32, "s32le", "pcm_s32le", 4, 1000000000.);
impl Integer for u8 {
    const FORMAT: &'static str = "u8";
    const CODEC: &'static str = "pcm_u8";
    const BYTES: usize = 1;
    fn quantize(value: f64, index: usize, channel: usize, frames: usize) -> Self {
        if index == 0 {
            return if channel == 0 { 255 } else { 0 };
        }
        if index + 1 == frames {
            return if channel == 0 { 0 } else { 255 };
        }
        (128. + value * 60.) as u8
    }
    fn write(self, file: &mut std::fs::File) {
        file.write_all(&[self]).unwrap();
    }
    fn read(bytes: &[u8]) -> Self {
        bytes[0]
    }
    fn integer(self) -> i64 {
        i64::from(self)
    }
}
#[test]
#[ignore = "requires actual FFmpeg U8 Pitch graph"]
fn original_u8_pitch_graph_streaming_reset_and_eof() {
    qualify::<u8>();
}
#[test]
#[ignore = "requires actual FFmpeg integer Pitch graph"]
fn original_pcm16_pitch_graph_streaming_reset_and_eof() {
    qualify::<i16>();
}
#[test]
#[ignore = "requires actual FFmpeg integer Pitch graph"]
fn original_pcm32_pitch_graph_streaming_reset_and_eof() {
    qualify::<i32>();
}
fn qualify<T: Integer>() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("input.{}", T::FORMAT));
    let mut compared = 0;
    let mut cases = f["cases"].as_array().unwrap().clone();
    // Short source EOF before/around the WSOLA window, in both channel layouts.
    for c in f["cases"].as_array().unwrap().iter().filter(|c| {
        c["rate"] == 48000
            && c["signal"] == "noise"
            && [-12., -7., 7., 12.]
                .contains(&c["settings"][0]["params"]["semitones"].as_f64().unwrap())
    }) {
        for frames in [1, 17, 511, 1024, 2049] {
            let mut short = c.clone();
            short["inputFrames"] = serde_json::json!(frames);
            cases.push(short);
        }
    }
    for c in f["cases"].as_array().unwrap().iter().filter(|c| {
        c["signal"] == "noise"
            && [8000, 48000].contains(&c["rate"].as_u64().unwrap())
            && [-7., 7.].contains(&c["settings"][0]["params"]["semitones"].as_f64().unwrap())
    }) {
        let rate = c["rate"].as_u64().unwrap();
        let packets: &[usize] = if rate == 8000 {
            &[65536]
        } else {
            &[1, 255, 1024]
        };
        for &packet in packets {
            let mut framed = c.clone();
            framed["inputFrames"] = serde_json::json!(if rate == 8000 { 65553 } else { 8193 });
            framed["explicitPacketFrames"] = serde_json::json!(packet);
            cases.push(framed);
        }
    }
    for (index, c) in cases.iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let signal = c["signal"].as_str().unwrap();
        let source: Vec<Vec<T>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| {
                        if signal == "steps" {
                            return (if ch == 0 { 2. } else { -1.5 })
                                * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. };
                        }
                        if [0, rate / 7, frames - 1].contains(&i) {
                            return [1.25, -0.75][ch];
                        }
                        let h = ((i % 257 + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        (((h >> 12) % 2049) as i32 - 1024) as f64 / 4096. + (i % 7) as f64 * 1e-10
                    })
                    .enumerate()
                    .map(|(i, v)| T::quantize(v, i, ch, frames))
                    .collect()
            })
            .collect();
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..frames {
            for ch in 0..channels {
                source[ch][i].write(&mut file);
            }
        }
        drop(file);
        let packet_frames = c["explicitPacketFrames"]
            .as_u64()
            .unwrap_or(1u64 << (rate as u64 / 10).ilog2()) as usize;
        let chain = if c["explicitPacketFrames"].is_number() {
            format!(
                "asetnsamples=n={packet_frames}:p=0,{}",
                c["chain"].as_str().unwrap()
            )
        } else {
            c["chain"].as_str().unwrap().to_owned()
        };
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                T::FORMAT,
                "-ar",
                &rate.to_string(),
                "-ac",
                &channels.to_string(),
                "-i",
                path.to_str().unwrap(),
                "-af",
                &chain,
                "-c:a",
                T::CODEC,
                "-f",
                T::FORMAT,
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let semitones = c["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let mut pitch =
            native::Pitch::with_packet_size(rate as u32, channels, semitones, packet_frames);
        let mut last = vec![];
        let mut maximum = 0i64;
        for pass in 0..4 {
            if pass > 0 {
                pitch.reset();
            }
            let mut actual = vec![Vec::new(); channels];
            let mut at = 0;
            let mut block = 0;
            while at < frames {
                let count = if pass == 3 {
                    packet_frames.min(frames - at)
                } else if pass == 0 {
                    65536.min(frames - at)
                } else {
                    [1, 255, 17, 1024, 4096][block % 5].min(frames - at)
                };
                let input: Vec<&[T]> = source.iter().map(|ch| &ch[at..at + count]).collect();
                let result = if pass == 3 {
                    pitch.push_packet(&input, false)
                } else {
                    pitch.push(&input, pass != 2 && at + count == frames)
                };
                for ch in 0..channels {
                    actual[ch].extend_from_slice(&result[ch]);
                }
                at += count;
                block += 1;
            }
            if pass >= 2 {
                let empty = vec![&[][..]; channels];
                let result = if pass == 3 {
                    pitch.push_packet(&empty, true)
                } else {
                    pitch.push(&empty, true)
                };
                for ch in 0..channels {
                    actual[ch].extend_from_slice(&result[ch]);
                }
            }
            assert_eq!(
                actual[0].len() * channels * T::BYTES,
                output.stdout.len(),
                "case{index} pass{pass} frame count"
            );
            for (i, raw) in output.stdout.chunks_exact(T::BYTES).enumerate() {
                let reference = T::read(raw);
                let value = actual[i % channels][i / channels];
                let error = (value.integer() - reference.integer()).abs();
                maximum = maximum.max(error);
                assert!(error == 0,"case{index} rate{rate} ch{channels} semitones{semitones} signal{signal} pass{pass} value{i}: {value} vs {reference}, error{error}");
                if pass == 0 {
                    compared += 1;
                }
            }
            if pass > 0 {
                assert!(actual == last, "case{index} reset/chunk invariance");
            }
            last = actual;
        }
        let mut timeline = concat_pitch_timeline::Timeline::<T>::new(
            rate as u32,
            channels,
            semitones,
            0..frames as i64,
            0,
        )
        .unwrap();
        let mut read = |at: i64, max: usize| -> anyhow::Result<Vec<Vec<T>>> {
            let n = max.min(packet_frames - at as usize % packet_frames);
            Ok(source
                .iter()
                .map(|ch| ch[at as usize..at as usize + n].to_vec())
                .collect())
        };
        let natural_frames = output.stdout.len() / T::BYTES / channels;
        for start in [natural_frames / 2, natural_frames.saturating_sub(31), 0] {
            let out = timeline
                .render(start as i64, 127, &mut read, &mut || Ok(()))
                .unwrap();
            for ch in 0..channels {
                for (i, &value) in out[ch].iter().enumerate() {
                    let expected = if start + i < natural_frames {
                        let offset = ((start + i) * channels + ch) * T::BYTES;
                        T::read(&output.stdout[offset..offset + T::BYTES])
                    } else {
                        T::silence()
                    };
                    assert!(
                        value == expected,
                        "case{index} timeline {start} ch{ch} i{i}: {value} vs {expected}"
                    );
                }
            }
        }
        println!("case{index} rate{rate} ch{channels} semitones{semitones} {signal}: max{maximum}");
    }
    println!(
        "qualified {compared} original {} PCM values, reset/chunk/EOF replay",
        T::FORMAT
    );
}
