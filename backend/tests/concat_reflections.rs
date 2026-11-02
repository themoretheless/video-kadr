use sha2::{Digest, Sha256};
use std::io::Read;
use video_kadr_backend::domain::{
    filmcraft_audio_mapping::{native_export_preroll, native_export_preroll_at},
    filmcraft_dsp::{validate_specs, Chain, EffectSpec, TimelineChain},
};
fn source(frame: usize, channel: usize, rate: usize, frames: usize) -> f32 {
    if frame >= frames {
        return 0.;
    }
    if [0, rate / 7, frames - 1].contains(&frame) {
        return [1.25, -0.75][channel];
    }
    let h = (frame % 257 + channel * 1061) as u32;
    let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
}
fn fixture() -> (serde_json::Value, Vec<u8>) {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-reflections-pcm.json")).unwrap();
    assert_eq!(
        manifest["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    for name in ["room", "hall", "plate", "cathedral"] {
        assert_eq!(manifest["aliasVerified"][name], format!("concat.{name}"));
    }
    let mut pcm = vec![];
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/concat-reflections-pcm.f32le.gz").as_slice(),
    )
    .take(64 * 1024 * 1024)
    .read_to_end(&mut pcm)
    .unwrap();
    assert_eq!(pcm.len() as u64, manifest["pcmBytes"].as_u64().unwrap());
    assert_eq!(manifest["cases"].as_array().unwrap().len(), 156);
    (manifest, pcm)
}
#[test]
fn original_concat_chain_and_actual_ffmpeg_float_pcm() {
    let (f, pcm) = fixture();
    let mut compared = 0;
    let mut clipped = 0;
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as usize;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let output_frames = c["outputFrames"].as_u64().unwrap() as usize;
        let input: Vec<u8> = (0..frames)
            .flat_map(|i| {
                (0..channels).flat_map(move |ch| source(i, ch, rate, frames).to_le_bytes())
            })
            .collect();
        assert_eq!(
            format!("{:x}", Sha256::digest(&input)),
            c["inputSha256"].as_str().unwrap()
        );
        let offset = c["offsetBytes"].as_u64().unwrap() as usize;
        let bytes = c["byteLength"].as_u64().unwrap() as usize;
        let expected = &pcm[offset..offset + bytes];
        assert_eq!(
            format!("{:x}", Sha256::digest(expected)),
            c["outputSha256"].as_str().unwrap()
        );
        let settings: Vec<EffectSpec> = serde_json::from_value(c["settings"].clone()).unwrap();
        let mut chain = Chain::new(rate as u32, channels, &settings).unwrap();
        assert_eq!(chain.latency_samples(), 0);
        let mut planar: Vec<Vec<f32>> = (0..channels)
            .map(|ch| {
                (0..output_frames)
                    .map(|i| source(i, ch, rate, frames))
                    .collect()
            })
            .collect();
        let mut at = 0;
        let mut chunk = 0;
        while at < output_frames {
            let n = [1, 255, 17, 1024, 4096][chunk % 5].min(output_frames - at);
            let mut block: Vec<&mut [f32]> =
                planar.iter_mut().map(|ch| &mut ch[at..at + n]).collect();
            chain.process(&mut block, &mut || Ok(())).unwrap();
            at += n;
            chunk += 1;
        }
        for (i, raw) in expected.chunks_exact(4).enumerate() {
            let expected = f32::from_le_bytes(raw.try_into().unwrap());
            let actual = planar[i % channels][i / channels];
            assert!((-1.0..=1.0).contains(&actual));
            if expected.abs() == 1. {
                clipped += 1;
            }
            assert!(
                (actual - expected).abs() <= 2e-7,
                "case{index} sample{i}: native{actual} original{expected}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 12042921);
    assert!(
        clipped > 10,
        "must exercise per-stage clipping, got {clipped}"
    );
}
#[test]
fn exact_finite_history_for_seek_and_stacked_reflections() {
    let (f, pcm) = fixture();
    for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let rate = c["rate"].as_u64().unwrap() as u32;
        let channels = c["channels"].as_u64().unwrap() as usize;
        let frames = c["inputFrames"].as_u64().unwrap() as usize;
        let output_frames = c["outputFrames"].as_u64().unwrap() as usize;
        let offset = c["offsetBytes"].as_u64().unwrap() as usize;
        let settings: Vec<EffectSpec> = serde_json::from_value(c["settings"].clone()).unwrap();
        let pre = native_export_preroll(&settings, rate).unwrap();
        assert_eq!(
            pre,
            native_export_preroll_at(&settings, rate, 0.75).unwrap()
        );
        let mut timeline = TimelineChain::new(rate, channels, &settings, pre).unwrap();
        for start in [rate as usize * 3 / 4, 17, frames - 1, pre + 1] {
            let count = 113.min(output_frames - start);
            let actual = timeline
                .render(
                    start as i64,
                    count,
                    &mut |first, n| {
                        Ok((0..channels)
                            .map(|ch| {
                                (0..n)
                                    .map(|i| {
                                        let p = first + i as i64;
                                        if p < 0 {
                                            0.
                                        } else {
                                            source(p as usize, ch, rate as usize, frames)
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
                    let j = offset + ((start + i) * channels + ch) * 4;
                    let expected = f32::from_le_bytes(pcm[j..j + 4].try_into().unwrap());
                    assert!((actual[ch][i]-expected).abs()<=2e-7,"seek case{index} start{start} channel{ch} sample{i}: native{} original{expected}",actual[ch][i]);
                }
            }
        }
    }
}
#[test]
fn strict_static_admission_bypass_and_existing_dsp() {
    for bad in [
        serde_json::json!({"id":"concat.echo","params":{"delay":0.01}}),
        serde_json::json!({"id":"concat.echo","params":{"decay":1.}}),
        serde_json::json!({"id":"concat.echo","params":{"unknown":1.}}),
        serde_json::json!({"id":"concat.unknown"}),
        serde_json::json!({"id":"concat.echo","animation":{"delay":{"mode":"constant","value":0.5}}}),
    ] {
        let spec: EffectSpec = serde_json::from_value(bad).unwrap();
        assert!(validate_specs(&[spec]).is_err());
    }
    let spec: EffectSpec =
        serde_json::from_value(serde_json::json!({"id":"concat.echo","bypass":true})).unwrap();
    assert_eq!(native_export_preroll(&[spec.clone()], 48000).unwrap(), 0);
    let mut chain = Chain::new(48000, 1, &[spec]).unwrap();
    let mut input = vec![0.25; 1025];
    chain.process(&mut [&mut input], &mut || Ok(())).unwrap();
    assert_eq!(input, vec![0.25; 1025]);
    let old: EffectSpec =
        serde_json::from_value(serde_json::json!({"id":"amplify","params":{"gain":-6.}})).unwrap();
    assert_eq!(native_export_preroll(&[old.clone()], 48000).unwrap(), 0);
    assert!(Chain::new(48000, 1, &[old]).is_ok());
}
#[test]
fn all_reflection_contracts_and_long_stacked_history() {
    for id in [
        "concat.room",
        "concat.hall",
        "concat.plate",
        "concat.cathedral",
    ] {
        for params in [
            serde_json::json!({"size":-1}),
            serde_json::json!({"size":101}),
            serde_json::json!({"delay":0.25}),
        ] {
            let spec: EffectSpec =
                serde_json::from_value(serde_json::json!({"id":id,"params":params})).unwrap();
            assert!(validate_specs(&[spec]).is_err());
        }
        let spec: EffectSpec = serde_json::from_value(
            serde_json::json!({"id":id,"animation":{"size":{"mode":"constant","value":25}}}),
        )
        .unwrap();
        assert!(validate_specs(&[spec]).is_err());
        let spec: EffectSpec =
            serde_json::from_value(serde_json::json!({"id":id,"params":{"size":37}})).unwrap();
        let mut chain = Chain::new(8000, 1, &[spec]).unwrap();
        let mut a: Vec<f32> = (0..12000).map(|i| if i == 0 { 1. } else { 0. }).collect();
        chain.process(&mut [&mut a], &mut || Ok(())).unwrap();
        chain.reset();
        let mut b: Vec<f32> = (0..12000).map(|i| if i == 0 { 1. } else { 0. }).collect();
        chain.process(&mut [&mut b], &mut || Ok(())).unwrap();
        assert_eq!(a, b, "reset {id}");
    }
    let spec: EffectSpec =
        serde_json::from_value(serde_json::json!({"id":"concat.cathedral","params":{"size":100}}))
            .unwrap();
    let specs = vec![spec; 16];
    let pre = native_export_preroll(&specs, 8000).unwrap();
    assert_eq!(pre, 16 * 9600);
    assert!(TimelineChain::new(8000, 1, &specs, pre).is_ok());
}
