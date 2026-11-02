use video_kadr_backend::domain::{
    composition::VideoEffect, concat_timed_looks::Mono, flat_surface, group_surface::GroupSurface,
};
#[test]
fn original_metal_extended_and_timed_stacks() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-mono-gpu.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert!(f["upstreamProbes"].as_u64().unwrap() > 0);
    assert_ne!(f["instrumented"], true);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5016);
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let gpu: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        assert_eq!(c["samplePixel"], serde_json::json!([4, 4]));
        for setting in c["settings"].as_array().unwrap() {
            let p: Mono = serde_json::from_value(setting.clone()).unwrap();
            let rgb = p
                .signal(
                    [input[0], input[1], input[2]],
                    4,
                    4,
                    c["time"].as_f64().unwrap() as f32,
                )
                .unwrap();
            for i in 0..3 {
                let v = rgb[i].clamp(-65504., 65504.);
                let h = half::f16::from_f32(v);
                input[i] = if h.to_f32().abs() > v.abs() {
                    half::f16::from_bits(h.to_bits() - 1).to_f32()
                } else {
                    h.to_f32()
                };
            }
        }
        assert_eq!(input[3], gpu[3]);
        for i in 0..3 {
            assert!(tolerance[i] > 0. && tolerance[i] <= gpu[i].abs() * 0.002 + 1.01e-6);
            assert!(
                (input[i] - gpu[i]).abs() <= tolerance[i],
                "case{index} channel{i} native{} GPU{} allowed{}",
                input[i],
                gpu[i],
                tolerance[i]
            );
        }
    }
}
#[test]
fn original_metal_nonuniform_and_alpha() {
    let mut pixels = 0;
    for opaque in [false, true] {
        let f: serde_json::Value = serde_json::from_str(if opaque {
            include_str!("fixtures/concat-mono-opaque-spatial.json")
        } else {
            include_str!("fixtures/concat-mono-spatial.json")
        })
        .unwrap();
        assert_eq!(
            f["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(f["adapter"]["backend"], "Metal");
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 128);
        for (index, c) in cases.iter().enumerate() {
            let side = c["side"].as_u64().unwrap() as usize;
            let mut actual = GroupSurface {
                width: side,
                height: side,
                rgba: serde_json::from_value(c["inputRgba"].clone()).unwrap(),
            };
            let mut parameters = c["parameters"].clone();
            let time = parameters
                .as_object_mut()
                .unwrap()
                .remove("time")
                .unwrap()
                .as_f64()
                .unwrap();
            let effect = serde_json::from_value(
                serde_json::json!({"kind":"concat_mono","parameters":parameters}),
            )
            .unwrap();
            flat_surface::apply_effect_stack(
                &mut actual,
                &[effect],
                (time * 1000000.).round() as u64,
                &[],
                &mut || Ok(()),
            )
            .unwrap();
            let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
            assert_eq!(actual.rgba.len(), expected.len());
            for (a, e) in actual.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
                pixels += 1;
                assert_eq!(a[3], e[3]);
                for i in 0..3 {
                    assert!(
                        a[i].abs_diff(e[i]) <= 1,
                        "opaque{opaque} case{index} channel{i}: native{} GPU{}",
                        a[i],
                        e[i]
                    );
                }
            }
        }
    }
    assert_eq!(pixels, 20800);
}
#[test]
fn source_contract_and_clock() {
    let _: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_mono","parameters":{}})).unwrap();
    for extra in [
        serde_json::json!({"kind":"concat_mono","parameters":{},"animation":{}}),
        serde_json::json!({"kind":"concat_mono","parameters":{"unknown":1}}),
    ] {
        assert!(serde_json::from_value::<VideoEffect>(extra).is_err());
    }
    for value in [-101., 101., f32::NAN] {
        let mut p = Mono::default();
        p.colour = value;
        assert!(p.validate().is_err());
    }
    for value in [-1., 101., f32::NAN] {
        let mut p = Mono::default();
        p.contrast = value;
        assert!(p.validate().is_err());
    }
    let mut p = Mono::default();
    p.grain = 75.;
    let a = p.signal([0.1, 0.2, 0.3], 3, 7, 0.125).unwrap();
    assert_eq!(a, p.signal([0.1, 0.2, 0.3], 3, 7, 0.125).unwrap());
    assert_ne!(a, p.signal([0.1, 0.2, 0.3], 3, 7, 1.25).unwrap());
    assert_ne!(a, p.signal([0.1, 0.2, 0.3], 4, 7, 0.125).unwrap());
    for time in [f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(p.signal([0.; 3], 0, 0, time).is_err());
    }
    for grain in [-1., 101., f32::NAN] {
        p.grain = grain;
        assert!(p.validate().is_err());
    }
}
