use video_kadr_backend::domain::{
    composition::VideoEffect, concat_super8::Super8, flat_surface, group_surface::GroupSurface,
};
#[test]
fn original_metal_timed_grain_and_nonuniform_grade() {
    let fixture: serde_json::Value = serde_json::from_reader(
        std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/concat-super-8-spatial.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_ne!(fixture["instrumented"], true);
    assert!(fixture["upstreamProbes"].as_u64().unwrap() > 0);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    let mut pixels = 0;
    for (index, c) in cases.iter().enumerate() {
        let side = c["side"].as_u64().unwrap() as usize;
        let mut surface = GroupSurface {
            width: side,
            height: side,
            rgba: serde_json::from_value(c["inputRgba"].clone()).unwrap(),
        };
        let working: Vec<f32> = serde_json::from_value(c["expectedWorking"].clone()).unwrap();
        for (pixel, (input, gpu)) in surface
            .rgba
            .chunks_exact(4)
            .zip(working.chunks_exact(4))
            .enumerate()
        {
            let rgb = std::array::from_fn(|i| {
                let v = input[i] as f32 / 255.;
                half::f16::from_f32(if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                })
                .to_f32()
            });
            let native = Super8 {}
                .signal(
                    rgb,
                    pixel % side,
                    pixel / side,
                    c["parameters"]["time"].as_f64().unwrap() as f32,
                )
                .unwrap();
            for channel in 0..3 {
                let v = native[channel].clamp(-65504., 65504.);
                let near = half::f16::from_f32(v);
                let actual = if near.to_f32().abs() > v.abs() {
                    half::f16::from_bits(near.to_bits() - 1).to_f32()
                } else {
                    near.to_f32()
                };
                let bits = half::f16::from_f32(gpu[channel].abs()).to_bits();
                let low = half::f16::from_bits(bits.saturating_sub(1)).to_f32();
                let high = half::f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();
                let tolerance =
                    ((gpu[channel].abs() - low).max(high - gpu[channel].abs()) * 2.).max(1e-6);
                assert!((actual-gpu[channel]).abs() <= tolerance,"working case{index} pixel{pixel} channel{channel}: native{actual} GPU{} allowed{tolerance}",gpu[channel]);
            }
            assert_eq!(half::f16::from_f32(input[3] as f32 / 255.).to_f32(), gpu[3]);
        }
        let effect =
            serde_json::from_value(serde_json::json!({"kind":"concat_super8","parameters":{}}))
                .unwrap();
        let time = (c["parameters"]["time"].as_f64().unwrap() * 1_000_000.).round() as u64;
        flat_surface::apply_effect_stack(&mut surface, &[effect], time, &[], &mut || Ok(()))
            .unwrap();
        let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
        assert_eq!(surface.rgba.len(), expected.len());
        for (a, e) in surface.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
            pixels += 1;
            assert_eq!(a[3], e[3]);
            for i in 0..3 {
                assert!(
                    a[i].abs_diff(e[i]) <= 1,
                    "case{index}, channel{i}: native{} GPU{}",
                    a[i],
                    e[i]
                );
            }
        }
    }
    assert_eq!(pixels, 2600);
}
#[test]
fn authored_contract_and_reproducible_clock() {
    let effect: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_super8","parameters":{}}))
            .unwrap();
    assert!(matches!(effect, VideoEffect::ConcatSuper8 { .. }));
    for extra in [
        serde_json::json!({"kind":"concat_super8","parameters":{"grain":50}}),
        serde_json::json!({"kind":"concat_super8","parameters":{},"animation":{}}),
    ] {
        assert!(serde_json::from_value::<VideoEffect>(extra).is_err());
    }
    let p = Super8 {};
    let at = p.signal([0.1, 0.2, 0.3], 3, 7, 0.125).unwrap();
    assert_eq!(at, p.signal([0.1, 0.2, 0.3], 3, 7, 0.125).unwrap());
    assert_ne!(at, p.signal([0.1, 0.2, 0.3], 3, 7, 1.).unwrap());
    assert_ne!(at, p.signal([0.1, 0.2, 0.3], 4, 7, 0.125).unwrap());
    for time in [f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(p.signal([0.; 3], 0, 0, time).is_err());
    }
}
