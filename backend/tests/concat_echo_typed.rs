#[path = "../src/domain/concat_echo_kernel.rs"]
mod native;
use std::io::Write;
trait Pcm: native::Sample + PartialEq {
    const FORMAT: &'static str;
    const CODEC: &'static str;
    const BYTES: usize;
    const TOLERANCE: f64;
    fn signal(i: usize, ch: usize) -> Self;
    fn write(self, f: &mut std::fs::File);
    fn read(b: &[u8]) -> Self;
}
macro_rules! pcm {
    ($ty:ty,$fmt:literal,$codec:literal,$bytes:literal,$scale:literal,$tolerance:literal) => {
        impl Pcm for $ty {
            const FORMAT: &'static str = $fmt;
            const CODEC: &'static str = $codec;
            const BYTES: usize = $bytes;
            const TOLERANCE: f64 = $tolerance;
            fn signal(i: usize, ch: usize) -> Self {
                let value = if i % 257 == 0 {
                    1.25
                } else if i % 257 == 128 {
                    -1.5
                } else {
                    (((i * 37 + ch * 1061) % 2049) as f64 - 1024.) / 1024.
                };
                (value * $scale) as Self
            }
            fn write(self, f: &mut std::fs::File) {
                f.write_all(&self.to_le_bytes()).unwrap();
            }
            fn read(b: &[u8]) -> Self {
                Self::from_le_bytes(b.try_into().unwrap())
            }
        }
    };
}
pcm!(i16, "s16le", "pcm_s16le", 2, 32768., 0.);
pcm!(i32, "s32le", "pcm_s32le", 4, 2147483648., 0.);
pcm!(f32, "f32le", "pcm_f32le", 4, 1., 2e-7);
pcm!(f64, "f64le", "pcm_f64le", 8, 1., 2e-12);
fn qualify<T: Pcm>() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pcm");
    let mut compared = 0;
    let profiles: [(f32, f32, &[f32], &[f32]); 4] = [
        (0.8, 0.85, &[250.], &[0.4]),
        (0.8, 0.88, &[120., 240.], &[0.45, 0.28]),
        (0.8, 0.85, &[0.1, 1.17, 17.51], &[0.7, 0.4, 0.125]),
        (1., 1., &[1., 2., 3.], &[1., 1., 1.]),
    ];
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            for frames in [0, 1, 511, 5007] {
                let source: Vec<Vec<T>> = (0..channels)
                    .map(|ch| (0..frames).map(|i| T::signal(i, ch)).collect())
                    .collect();
                let mut file = std::fs::File::create(&path).unwrap();
                for i in 0..frames {
                    for ch in &source {
                        ch[i].write(&mut file);
                    }
                }
                drop(file);
                for (profile, (ingain, outgain, delays, decays)) in profiles.iter().enumerate() {
                    let numbers = |values: &[f32]| {
                        values
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("|")
                    };
                    let graph = format!(
                        "aecho={ingain}:{outgain}:{}:{}",
                        numbers(delays),
                        numbers(decays)
                    );
                    let reference = std::process::Command::new("ffmpeg")
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
                        ])
                        .arg(&path)
                        .args(["-af", &graph, "-c:a", T::CODEC, "-f", T::FORMAT, "pipe:1"])
                        .output()
                        .unwrap();
                    assert!(
                        reference.status.success(),
                        "{}",
                        String::from_utf8_lossy(&reference.stderr)
                    );
                    let mut echo =
                        native::Echo::<T>::new(rate, channels, *ingain, *outgain, delays, decays)
                            .unwrap();
                    let retained = echo.retained_samples();
                    assert!(retained <= 8_388_608);
                    for pass in 0..3 {
                        if pass > 0 {
                            echo.reset();
                        }
                        let mut actual = vec![Vec::new(); channels];
                        let mut at = 0;
                        let mut block = 0;
                        while at < frames {
                            let count = if pass == 0 {
                                frames - at
                            } else {
                                [1, 17, 255, 7, 1024][block % 5].min(frames - at)
                            };
                            let refs: Vec<&[T]> =
                                source.iter().map(|ch| &ch[at..at + count]).collect();
                            let out = echo.push_packet(&refs).unwrap();
                            assert_eq!(out[0].len(), count);
                            for (dst, src) in actual.iter_mut().zip(out) {
                                dst.extend(src);
                            }
                            at += count;
                            block += 1;
                        }
                        let tail = retained / channels;
                        let mut remaining = tail;
                        while let Some(packet) = echo.finish_packet() {
                            assert_eq!(packet[0].len(), remaining.min(2048));
                            remaining -= packet[0].len();
                            for (dst, src) in actual.iter_mut().zip(packet) {
                                dst.extend(src);
                            }
                        }
                        assert_eq!(remaining, 0);
                        assert_eq!(echo.retained_samples(), retained);
                        assert!(echo.push_packet(&vec![&[][..]; channels]).is_err());
                        assert_eq!(
                            actual[0].len() * channels * T::BYTES,
                            reference.stdout.len(),
                            "{} rate{rate} ch{channels} frames{frames} profile{profile} length",
                            T::FORMAT
                        );
                        for (i, b) in reference.stdout.chunks_exact(T::BYTES).enumerate() {
                            let value = actual[i % channels][i / channels].double();
                            let expected = T::read(b).double();
                            assert!((value-expected).abs()<=T::TOLERANCE,"{} rate{rate} ch{channels} frames{frames} profile{profile} pass{pass} value{i}: {value} vs {expected}",T::FORMAT);
                        }
                        if pass == 0 {
                            compared += reference.stdout.len() / T::BYTES;
                        }
                    }
                }
            }
        }
    }
    println!(
        "qualified {compared} {} Echo values plus packet/reset/EOF replay",
        T::FORMAT
    );
}
#[test]
#[ignore = "requires actual FFmpeg typed Echo arithmetic"]
fn original_typed_echo() {
    qualify::<i16>();
    qualify::<i32>();
    qualify::<f32>();
    qualify::<f64>();
}
#[test]
fn invalid_configuration_and_input_are_refused_before_state_changes() {
    assert!(native::Echo::<f32>::new(0, 1, 0.8, 0.85, &[10.], &[0.4]).is_err());
    assert!(native::Echo::<f32>::new(48000, 1, 0.8, 0.85, &[], &[]).is_err());
    assert!(native::Echo::<f32>::new(48000, 1, 0.8, 0.85, &[f32::NAN], &[0.4]).is_err());
    let mut echo = native::Echo::<f32>::new(48000, 1, 0.8, 0.85, &[10.], &[0.4]).unwrap();
    assert!(echo.push_packet(&[&[f32::NAN]]).is_err());
    assert!(echo.push_packet(&[]).is_err());
    assert_eq!(echo.push_packet(&[&[0.]]).unwrap(), vec![vec![0.]]);
}
