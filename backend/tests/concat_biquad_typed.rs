#[path = "../src/domain/concat_audio_filter.rs"]
mod concat_audio_filter;
#[path = "../src/domain/concat_tone_filter.rs"]
mod concat_tone_filter;
#[path = "../src/domain/concat_biquad_stream.rs"]
mod native;
use std::io::Write;
trait Pcm: native::Sample + PartialEq {
    const RAW: &'static str;
    const CODEC: &'static str;
    const BYTES: usize;
    const TOLERANCE: f64;
    fn signal(i: usize, ch: usize) -> Self;
    fn write(self, f: &mut std::fs::File);
    fn read(b: &[u8]) -> Self;
}
macro_rules! pcm {
    ($ty:ty,$raw:literal,$codec:literal,$bytes:literal,$scale:literal,$tol:literal) => {
        impl Pcm for $ty {
            const RAW: &'static str = $raw;
            const CODEC: &'static str = $codec;
            const BYTES: usize = $bytes;
            const TOLERANCE: f64 = $tol;
            fn signal(i: usize, ch: usize) -> Self {
                let v = if i % 257 == 0 {
                    1.5
                } else if i % 257 == 128 {
                    -1.5
                } else {
                    (((i * 37 + ch * 1061) % 2049) as f64 - 1024.) / 1024.
                };
                (v * $scale) as Self
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
    use native::Design::*;
    let profiles = [
        (Lowpass, 1200., 0.707, 0., "lowpass=f=1200:t=q:w=0.707"),
        (Highpass, 120., 0.707, 0., "highpass=f=120:t=q:w=0.707"),
        (Highpass, 100., 1.1, 0., "highpass=f=100:t=q:w=1.1"),
        (Peak, 140., 1., 3., "equalizer=f=140:t=q:w=1:g=3"),
        (LowShelf, 110., 0.6, 12., "bass=f=110:t=q:w=0.6:g=12"),
        (HighShelf, 6000., 0.8, 5., "treble=f=6000:t=q:w=0.8:g=5"),
    ];
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.pcm");
    let mut compared = 0;
    let mut cases = Vec::new();
    for rate in [8000, 44100, 48000, 192000] {
        for channels in [1, 2] {
            for frames in [17, 8193] {
                cases.push((rate, channels, frames));
            }
        }
    }
    cases.push((48000, 2, 131073));
    for (rate, channels, frames) in cases {
        let source: Vec<Vec<T>> = (0..channels)
            .map(|ch| (0..frames).map(|i| T::signal(i, ch)).collect())
            .collect();
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..frames {
            for ch in &source {
                ch[i].write(&mut f);
            }
        }
        drop(f);
        for (profile, &(kind, freq, width, gain, chain)) in profiles.iter().enumerate() {
            for &packet in if frames > 65536 {
                &[131073usize][..]
            } else {
                &[1, 255, 1024][..]
            } {
                let graph = format!("asetnsamples=n={packet}:p=0,{chain}");
                let reference = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-f",
                        T::RAW,
                        "-ar",
                        &rate.to_string(),
                        "-ac",
                        &channels.to_string(),
                        "-i",
                    ])
                    .arg(&path)
                    .args(["-af", &graph, "-c:a", T::CODEC, "-f", T::RAW, "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                let mut filter =
                    native::Filter::<T>::new(kind, rate, channels, freq, width, gain).unwrap();
                for pass in 0..2 {
                    if pass > 0 {
                        filter.reset();
                    }
                    let mut actual = vec![Vec::new(); channels];
                    let mut at = 0;
                    while at < frames {
                        let n = packet.min(frames - at);
                        let refs: Vec<&[T]> = source.iter().map(|ch| &ch[at..at + n]).collect();
                        for (dst, src) in actual.iter_mut().zip(filter.push_packet(&refs).unwrap())
                        {
                            dst.extend(src);
                        }
                        at += n;
                    }
                    assert_eq!(
                        actual[0].len() * channels * T::BYTES,
                        reference.stdout.len()
                    );
                    for (i, raw) in reference.stdout.chunks_exact(T::BYTES).enumerate() {
                        let expected = T::read(raw).value();
                        let value = actual[i % channels][i / channels].value();
                        assert!(value.is_finite()&&(value-expected).abs()<=T::TOLERANCE,"{} rate{rate} ch{channels} frames{frames} profile{profile} packet{packet} pass{pass} value{i}: {value} vs {expected}",T::RAW);
                    }
                }
                compared += reference.stdout.len() / T::BYTES;
            }
        }
    }
    println!(
        "qualified {compared} {} typed biquad values plus reset and odd/even packets",
        T::RAW
    );
}
#[test]
#[ignore = "requires pinned FFmpeg typed biquad arithmetic"]
fn original_typed_biquads() {
    qualify::<i16>();
    qualify::<i32>();
    qualify::<f32>();
    qualify::<f64>();
}

#[test]
fn bypass_and_invalid_packets_preserve_state() {
    for (frequency, width) in [(0., 0.707), (30000., 0.707), (120., 0.)] {
        let mut bypass =
            native::Filter::<i16>::new(native::Design::Highpass, 48000, 1, frequency, width, 0.)
                .unwrap();
        assert_eq!(
            bypass.push_packet(&[&[i16::MIN, 0, i16::MAX]]).unwrap(),
            vec![vec![i16::MIN, 0, i16::MAX]]
        );
    }
    let make =
        || native::Filter::<f32>::new(native::Design::Highpass, 48000, 1, 120., 0.707, 0.).unwrap();
    let mut filter = make();
    assert!(filter.push_packet(&[&[f32::NAN]]).is_err());
    assert!(filter.push_packet(&[]).is_err());
    assert_eq!(
        filter.push_packet(&[&[1., 0., -1.]]).unwrap(),
        make().push_packet(&[&[1., 0., -1.]]).unwrap()
    );
}
