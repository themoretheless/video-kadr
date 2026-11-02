use sha2::{Digest, Sha256};
use std::io::Read;
use video_kadr_backend::domain::{
    filmcraft_audio_mapping::native_export_preroll_at,
    filmcraft_dsp::{validate_specs, Chain, EffectSpec, TimelineChain},
};
fn source(i: usize, ch: usize, rate: usize, frames: usize, signal: &str) -> f32 {
    if i >= frames {
        return 0.;
    }
    if signal == "steps" {
        return (if ch == 0 { 2. } else { -1.5 }) * if (i * 8 / rate) % 4 < 2 { 1. } else { -1. };
    }
    if [0, rate / 7, frames - 1].contains(&i) {
        return [1.25, -0.75][ch];
    }
    let h = (i % 257 + ch * 1061) as u32;
    let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
}
fn fixture() -> (serde_json::Value, Vec<u8>) {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-tones-pcm.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(
        f["aliasVerified"],
        serde_json::json!(["bass", "treble", "bright"])
    );
    assert_eq!(f["cases"].as_array().unwrap().len(), 316);
    let mut pcm = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/concat-tones-pcm.f32le.gz").as_slice())
        .take(128 * 1024 * 1024)
        .read_to_end(&mut pcm)
        .unwrap();
    assert_eq!(pcm.len() as u64, f["pcmBytes"].as_u64().unwrap());
    (f, pcm)
}
#[test]
fn original_tones_pcm_chunk_reset_and_eof() {
    let (f, pcm) = fixture();
    let mut compared = 0;
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let signal = c["signal"].as_str().unwrap();
        let output_frames = c["outputFrames"].as_u64().unwrap() as usize;
        let input: Vec<u8> = (0..frames)
            .flat_map(|i| {
                (0..channels).flat_map(move |ch| source(i, ch, rate, frames, signal).to_le_bytes())
            })
            .collect();
        assert_eq!(
            format!("{:x}", Sha256::digest(&input)),
            c["inputSha256"].as_str().unwrap()
        );
        let offset = c["offsetBytes"].as_u64().unwrap() as usize;
        let length = c["byteLength"].as_u64().unwrap() as usize;
        let expected = &pcm[offset..offset + length];
        assert_eq!(
            format!("{:x}", Sha256::digest(expected)),
            c["outputSha256"].as_str().unwrap()
        );
        let specs: Vec<EffectSpec> = serde_json::from_value(c["settings"].clone()).unwrap();
        let mut chain = Chain::new(rate as u32, channels, &specs).unwrap();
        chain.set_input_frames(frames).unwrap();
        assert_eq!(chain.latency_samples(), 0);
        let mut last = vec![];
        for pass in 0..2 {
            if pass > 0 {
                chain.reset();
            }
            let mut data: Vec<Vec<f32>> = (0..channels)
                .map(|ch| {
                    (0..output_frames + 31)
                        .map(|i| source(i, ch, rate, frames, signal))
                        .collect()
                })
                .collect();
            let mut at = 0;
            let mut chunk = 0;
            while at < output_frames + 31 {
                let n = [1, 255, 17, 1024, 4096][chunk % 5].min(output_frames + 31 - at);
                let mut block: Vec<&mut [f32]> =
                    data.iter_mut().map(|ch| &mut ch[at..at + n]).collect();
                chain.process(&mut block, &mut || Ok(())).unwrap();
                at += n;
                chunk += 1;
            }
            for (i, raw) in expected.chunks_exact(4).enumerate() {
                let expected = f32::from_le_bytes(raw.try_into().unwrap());
                let actual = data[i % channels][i / channels];
                assert!(
                    (actual - expected).abs() <= 2e-7,
                    "case{index} pass{pass} sample{i}: {actual} vs {expected}, error {}",
                    (actual - expected).abs()
                );
                compared += usize::from(pass == 0);
            }
            assert!(data
                .iter()
                .all(|ch| ch[output_frames..].iter().all(|v| *v == 0.)));
            if pass > 0 {
                assert_eq!(data, last);
            }
            last = data;
        }
    }
    assert_eq!(compared, 22301934);
}
#[test]
fn cold_tones_seeks_use_source_age_and_preserve_absolute_clip_clock() {
    let (f, pcm) = fixture();
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let signal = c["signal"].as_str().unwrap();
        let offset = c["offsetBytes"].as_u64().unwrap() as usize;
        let specs: Vec<EffectSpec> = serde_json::from_value(c["settings"].clone()).unwrap();
        let origin = 1777;
        let mut timeline = TimelineChain::new(rate as u32, channels, &specs, 0)
            .unwrap()
            .with_input_bounds(origin, origin + frames as i64)
            .unwrap();
        for start in [rate * 3 / 4, 17, frames - 113] {
            let count = 113;
            assert!(
                native_export_preroll_at(&specs, rate as u32, start as f64 / rate as f64).unwrap()
                    >= start
            );
            let data = timeline
                .render(
                    origin + start as i64,
                    count,
                    &mut |first, n| {
                        Ok((0..channels)
                            .map(|ch| {
                                (0..n)
                                    .map(|i| {
                                        let p = first + i as i64 - origin;
                                        if p < 0 {
                                            0.
                                        } else {
                                            source(p as usize, ch, rate, frames, signal)
                                        }
                                    })
                                    .collect()
                            })
                            .collect())
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            for ch in 0..channels {
                for i in 0..count {
                    let p = offset + ((start + i) * channels + ch) * 4;
                    let expected = f32::from_le_bytes(pcm[p..p + 4].try_into().unwrap());
                    assert!(
                        (data[ch][i] - expected).abs() <= 2e-7,
                        "seek case{index} start{start} ch{ch} frame{i}: {} vs {expected}",
                        data[ch][i]
                    );
                }
            }
        }
    }
}
#[test]
fn strict_static_tone_admission_and_bypass() {
    for id in ["concat.bass", "concat.treble", "concat.bright"] {
        for bad in [
            serde_json::json!({"id":id,"params":{"gain":12.5}}),
            serde_json::json!({"id":id,"params":{"gain":-1}}),
            serde_json::json!({"id":id,"params":{"drive":6}}),
            serde_json::json!({"id":id,"animation":{"gain":{"mode":"constant","value":5}}}),
        ] {
            assert!(validate_specs(&[serde_json::from_value(bad).unwrap()]).is_err());
        }
        let spec: EffectSpec =
            serde_json::from_value(serde_json::json!({"id":id,"bypass":true})).unwrap();
        let mut chain = Chain::new(48000, 1, &[spec]).unwrap();
        let mut pcm = vec![0.75; 1025];
        chain.process(&mut [&mut pcm], &mut || Ok(())).unwrap();
        assert_eq!(pcm, vec![0.75; 1025]);
    }
}

#[test]
fn raw_host_multichannel_precision_run_remains_admissible() {
    for channels in [4, 16] {
        let specs:Vec<EffectSpec>=serde_json::from_value(serde_json::json!([{"id":"concat.distorted","params":{"drive":1}},{"id":"concat.bass"},{"id":"concat.echo"},{"id":"concat.treble"}])).unwrap();
        let mut chain = Chain::new(48000, channels, &specs).unwrap();
        chain.set_input_frames(1025).unwrap();
        let mut pcm: Vec<Vec<f32>> = (0..channels)
            .map(|ch| vec![0.01 * (ch + 1) as f32; 1025])
            .collect();
        let mut refs: Vec<&mut [f32]> = pcm.iter_mut().map(Vec::as_mut_slice).collect();
        chain.process(&mut refs, &mut || Ok(())).unwrap();
        assert!(pcm.iter().flatten().all(|v| v.is_finite() && v.abs() <= 1.));
    }
}
