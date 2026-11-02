#[path = "../src/domain/concat_resample.rs"]
mod native;
use std::io::Write;
#[test]
#[ignore = "requires actual FFmpeg rational resampler"]
fn original_u8_resampler_streaming_reset_and_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.u8");
    let mut compared = 0;
    for (from, to) in [
        (8000, 48000),
        (44100, 48000),
        (48000, 44100),
        (48000, 32000),
        (192000, 48000),
        (48000, 192000),
        (71919, 48000),
        (33941, 48000),
        (48000, 48000),
        (192000, 8000),
        (8000, 192000),
    ] {
        for channels in [1, 2] {
            for frames in [1, 17, 22, 31, 32, 33, 1001, 5017] {
                let source: Vec<Vec<u8>> = (0..channels)
                    .map(|ch| {
                        (0..frames)
                            .map(|i| {
                                if i == 0 {
                                    return [u8::MAX, u8::MIN][ch];
                                }
                                let h = ((i + ch * 1061) as u32)
                                    .wrapping_mul(1664525)
                                    .wrapping_add(1013904223);
                                ((h >> 12) % 256) as u8
                            })
                            .collect()
                    })
                    .collect();
                let mut file = std::fs::File::create(&path).unwrap();
                for i in 0..frames {
                    for ch in 0..channels {
                        file.write_all(&source[ch][i].to_le_bytes()).unwrap();
                    }
                }
                drop(file);
                let mut effect = native::U8Resampler::new(from, to, channels);
                for pass in 0..3 {
                    let packet_frames = [1, 255, 4096][pass];
                    let output = std::process::Command::new("ffmpeg")
                        .args([
                            "-nostdin",
                            "-v",
                            "error",
                            "-f",
                            "u8",
                            "-ar",
                            &from.to_string(),
                            "-ac",
                            &channels.to_string(),
                            "-i",
                            path.to_str().unwrap(),
                            "-af",
                            &format!("asetnsamples=n={packet_frames}:p=0,aresample={to}"),
                            "-c:a",
                            "pcm_u8",
                            "-f",
                            "u8",
                            "pipe:1",
                        ])
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if pass > 0 {
                        effect.reset();
                    }
                    let mut actual = vec![Vec::new(); channels];
                    let mut at = 0;

                    while at < frames {
                        let count = packet_frames.min(frames - at);
                        let input: Vec<&[u8]> =
                            source.iter().map(|ch| &ch[at..at + count]).collect();
                        let out = effect.push(&input, pass != 2 && at + count == frames);
                        for ch in 0..channels {
                            actual[ch].extend_from_slice(&out[ch]);
                        }
                        at += count;
                    }
                    if pass == 2 && frames == 5017 {
                        assert!(!actual[0].is_empty(), "stream produces before EOF");
                    }
                    if pass == 2 {
                        let empty = vec![&[][..]; channels];
                        let out = effect.push(&empty, true);
                        for ch in 0..channels {
                            actual[ch].extend_from_slice(&out[ch]);
                        }
                    }
                    assert_eq!(
                        actual[0].len() * channels,
                        output.stdout.len(),
                        "length {from}->{to} ch{channels} frames{frames} pass{pass}"
                    );
                    let mut max = 0i64;
                    for (i, raw) in output.stdout.chunks_exact(1).enumerate() {
                        let expected = raw[0];
                        let value = actual[i % channels][i / channels];
                        let error = (i64::from(value) - i64::from(expected)).abs();
                        max = max.max(error);
                        assert!(error==0,"{from}->{to} ch{channels} frames{frames} pass{pass} value{i}: {value} vs {expected}, error{error}");
                    }
                    if pass == 0 {
                        compared += output.stdout.len();
                        println!("{from}->{to} ch{channels} frames{frames}: max{max}");
                    }
                }
            }
        }
    }
    println!("qualified {compared} PCM8 integer values with explicit source packet grids and empty EOF replay");
}
