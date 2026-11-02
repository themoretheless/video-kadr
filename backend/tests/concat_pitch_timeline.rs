#[path = "../src/domain/concat_denoise.rs"]
mod concat_denoise;
#[path = "../src/domain/concat_pitch.rs"]
mod concat_pitch;
#[path = "../src/domain/concat_resample.rs"]
mod concat_resample;
#[path = "../src/domain/concat_tempo.rs"]
mod concat_tempo;
#[path = "../src/domain/concat_pitch_timeline.rs"]
mod native;
use std::io::Write;

#[test]
#[ignore = "requires actual FFmpeg clipped-source Pitch graph"]
fn clipped_original_pitch_timeline_seek_chunk_and_eof() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-pitch-pcm.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.f32le");
    let mut compared = 0;
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let rate = case["rate"].as_u64().unwrap() as usize;
        let channels = case["channels"].as_u64().unwrap() as usize;
        let frames = case["inputFrames"].as_u64().unwrap() as usize;
        let packet = case["inputPacketFrames"].as_u64().unwrap() as usize;
        let semitones = case["settings"][0]["params"]["semitones"].as_f64().unwrap() as f32;
        let source: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                (0..frames)
                    .map(|i| {
                        if case["signal"] == "steps" {
                            return (if ch == 0 { 2. } else { -1.5 })
                                * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. };
                        }
                        if [0, rate / 7, frames - 1].contains(&i) {
                            return [1.25, -0.75][ch];
                        }
                        let h = ((i % 257 + ch * 1061) as u32)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
                    })
                    .collect()
            })
            .collect();
        let mut file = std::fs::File::create(&path).unwrap();
        for i in 0..frames {
            for ch in &source {
                file.write_all(&ch[i].to_le_bytes()).unwrap();
            }
        }
        drop(file);
        let bounds = (37 + index % 43) as i64..(frames - 113) as i64;
        let chain = format!(
            "atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{}",
            bounds.start,
            bounds.end,
            case["chain"].as_str().unwrap()
        );
        let reference = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "f32le",
                "-ar",
                &rate.to_string(),
                "-ac",
                &channels.to_string(),
                "-i",
                path.to_str().unwrap(),
                "-af",
                &chain,
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let values: Vec<f32> = reference
            .stdout
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let count = values.len() / channels;
        let origin = 73;
        let mut timeline =
            native::Timeline::new(rate as u32, channels, semitones, bounds.clone(), origin)
                .unwrap();
        let mut read = |at: i64, max: usize| -> anyhow::Result<Vec<Vec<f32>>> {
            assert!(at >= bounds.start && at < bounds.end);
            assert!(max <= (bounds.end - at) as usize && max <= 65536);
            let n = max.min(packet - at as usize % packet);
            Ok(source
                .iter()
                .map(|ch| ch[at as usize..at as usize + n].to_vec())
                .collect())
        };
        let expected_at = |at: i64, ch: usize| {
            let i = at - origin;
            if i < 0 || i >= count as i64 {
                0.
            } else {
                values[i as usize * channels + ch]
            }
        };
        for pass in 0..2 {
            let mut at = origin - 23;
            let end = origin + count as i64 + 17;
            let mut block = 0;
            while at < end {
                let n = if pass == 0 {
                    65536
                } else {
                    [1, 7, 511, 1024, 17][block % 5]
                }
                .min((end - at) as usize);
                let out = timeline.render(at, n, &mut read, &mut || Ok(())).unwrap();
                assert!(timeline.queued_frames() <= 1048576);
                for ch in 0..channels {
                    for (i, &value) in out[ch].iter().enumerate() {
                        let error = (value - expected_at(at + i as i64, ch)).abs();
                        assert!(
                            value.is_finite() && error <= 2e-7,
                            "case{index} pass{pass} at{at} ch{ch} i{i}: error{error}"
                        );
                    }
                }
                at += n as i64;
                block += 1;
            }
        }
        // Cold forward/backward reads and EOF crossing match the same original stream.
        for start in [
            origin + count as i64 / 3,
            origin + count as i64 - 31,
            origin + 19,
            origin - 17,
        ] {
            let out = timeline
                .render(start, 127, &mut read, &mut || Ok(()))
                .unwrap();
            for ch in 0..channels {
                for (i, &value) in out[ch].iter().enumerate() {
                    assert!(
                        (value - expected_at(start + i as i64, ch)).abs() <= 2e-7,
                        "case{index} seek{start} ch{ch} i{i}"
                    );
                }
            }
        }
        let mut checkpoints = 0;
        assert!(timeline
            .render(origin + 5, 127, &mut read, &mut || {
                checkpoints += 1;
                anyhow::ensure!(checkpoints < 3, "cancelled");
                Ok(())
            })
            .is_err());
        let recovered = timeline
            .render(origin + 5, 127, &mut read, &mut || Ok(()))
            .unwrap();
        for ch in 0..channels {
            for (i, &value) in recovered[ch].iter().enumerate() {
                assert!(
                    (value - expected_at(origin + 5 + i as i64, ch)).abs() <= 2e-7,
                    "case{index} cancellation recovery"
                );
            }
        }
        compared += values.len();
        println!("case{index} rate{rate} ch{channels} semitones{semitones}: clipped timeline/seek/cancel passed");
    }
    println!("qualified {compared} original clipped Pitch values plus timeline replays");
}

#[test]
fn invalid_packets_empty_sources_and_silence_are_explicit() {
    assert!(native::Timeline::<f32>::new(0, 1, 0., 0..1, 0).is_err());
    assert!(native::Timeline::<f32>::new(48000, 0, 0., 0..1, 0).is_err());
    assert!(native::Timeline::<f32>::new(48000, 1, f32::NAN, 0..1, 0).is_err());
    assert!(native::Timeline::<f32>::new(48000, 1, 0., 2..1, 0).is_err());
    let mut empty = native::Timeline::<u8>::new(48000, 1, 7., 19..19, 100).unwrap();
    assert_eq!(
        empty
            .render(0, 255, &mut |_, _| unreachable!(), &mut || Ok(()))
            .unwrap(),
        vec![vec![128; 255]]
    );
    let mut timeline = native::Timeline::<f32>::new(48000, 1, 7., 10..100, 0).unwrap();
    for kind in 0..5 {
        assert!(timeline
            .render(
                0,
                1,
                &mut |_, n| Ok(match kind {
                    0 => vec![],
                    1 => vec![vec![]],
                    2 => vec![vec![0.; n + 1]],
                    3 => vec![vec![0.; n], vec![0.; n]],
                    _ => vec![vec![f32::NAN; n]],
                }),
                &mut || Ok(())
            )
            .is_err());
    }
    assert!(timeline
        .render(i64::MAX, 1, &mut |_, _| unreachable!(), &mut || Ok(()))
        .is_err());
    assert!(timeline
        .render(0, 65537, &mut |_, _| unreachable!(), &mut || Ok(()))
        .is_err());
    let out = timeline
        .render(0, 511, &mut |_, n| Ok(vec![vec![0.; n]]), &mut || Ok(()))
        .unwrap();
    assert_eq!(out, vec![vec![0.; 511]]);
}

#[test]
fn long_source_is_streamed_and_failed_reads_reconstruct() {
    let mut timeline = native::Timeline::<f32>::new(48000, 2, 7., 37..48_000 * 3600, 0).unwrap();
    let mut read_frames = 0;
    let mut read = |at: i64, max: usize| -> anyhow::Result<Vec<Vec<f32>>> {
        let n = max.min(4096 - at as usize % 4096);
        read_frames += n;
        Ok(vec![vec![0.25; n], vec![-0.125; n]])
    };
    let out = timeline.render(0, 127, &mut read, &mut || Ok(())).unwrap();
    assert_eq!(out[0].len(), 127);
    assert!(
        read_frames < 65536,
        "first output must not decode the entire hour"
    );
    assert!(timeline.queued_frames() < 65536);
    assert!(timeline
        .render(
            511,
            127,
            &mut |_, _| anyhow::bail!("decoder failed"),
            &mut || Ok(())
        )
        .is_err());
    let mut read = |at: i64, max: usize| -> anyhow::Result<Vec<Vec<f32>>> {
        let n = max.min(4096 - at as usize % 4096);
        Ok(vec![vec![0.25; n], vec![-0.125; n]])
    };
    let recovered = timeline.render(0, 127, &mut read, &mut || Ok(())).unwrap();
    assert_eq!(out, recovered);
}
