use video_kadr_backend::domain::{
    composition::VideoEffect, concat_falling_dust::FallingDust, flat_surface,
    group_surface::GroupSurface,
};
#[test]
fn unchanged_original_metal_motes_and_background() {
    let f: serde_json::Value = serde_json::from_reader(
        std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/concat-falling-dust-gpu.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert_ne!(f["instrumented"], true);
    assert!(f["upstreamProbes"].as_u64().unwrap() > 0);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3360);
    let mut changed_motes = 0;
    for (index, c) in cases.iter().enumerate() {
        let p: FallingDust = serde_json::from_value(c["parameters"].clone()).unwrap();
        let input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let side = c["side"].as_u64().unwrap() as usize;
        let x = c["x"].as_u64().unwrap() as usize;
        let y = c["y"].as_u64().unwrap() as usize;
        let time = c["time"].as_f64().unwrap() as f32;
        let actual = p
            .signal([input[0], input[1], input[2]], x, y, side, side, time)
            .unwrap();
        let clean = FallingDust {
            dust: 0.,
            leak: p.leak,
        }
        .signal([input[0], input[1], input[2]], x, y, side, side, time)
        .unwrap();
        if (actual[0] - clean[0]).abs() > 0.001 {
            changed_motes += 1;
        }
        for channel in 0..3 {
            let v = actual[channel].clamp(-65504., 65504.);
            let nearest = half::f16::from_f32(v);
            let got = if nearest.to_f32().abs() > v.abs() {
                half::f16::from_bits(nearest.to_bits() - 1).to_f32()
            } else {
                nearest.to_f32()
            };
            let expected = c["gpu"][channel].as_f64().unwrap() as f32;
            let tolerance = c["tolerance"][channel].as_f64().unwrap() as f32;
            assert!((got-expected).abs()<=tolerance,"case {index} pixel {x},{y}/{side} channel {channel}: native {got} GPU {expected} tolerance {tolerance}");
        }
        assert_eq!(input[3], c["gpu"][3].as_f64().unwrap() as f32);
    }
    assert!(
        changed_motes > 500,
        "must exercise visible dust, not only the background: {changed_motes}"
    );
}
#[test]
fn wire_contract_and_absolute_clock_refusal() {
    let effect: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_falling_dust","parameters":{}}))
            .unwrap();
    assert!(
        matches!(effect,VideoEffect::ConcatFallingDust{parameters} if parameters==FallingDust::default())
    );
    for parameters in [
        serde_json::json!({"dust":101}),
        serde_json::json!({"leak":-1}),
        serde_json::json!({"unknown":0}),
    ] {
        let decoded = serde_json::from_value::<VideoEffect>(
            serde_json::json!({"kind":"concat_falling_dust","parameters":parameters}),
        );
        assert!(
            decoded.is_err()
                || matches!(decoded.unwrap(), VideoEffect::ConcatFallingDust { parameters } if parameters.validate().is_err())
        );
    }
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_falling_dust","parameters":{},"animation":{}})
    )
    .is_err());
    for seconds in [f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(FallingDust::default()
            .signal([0.; 3], 0, 0, 32, 32, seconds)
            .is_err());
    }
    assert!(FallingDust::default()
        .signal([0.; 3], 32, 0, 32, 32, 0.)
        .is_err());
    assert!(FallingDust::default()
        .signal([0.; 3], 0, 0, 0, 32, 0.)
        .is_err());
}
#[test]
fn native_stack_dimensions_hidden_alpha_clock_and_cancellation() {
    let original = GroupSurface {
        width: 37,
        height: 65,
        rgba: (0..37 * 65)
            .flat_map(|i| [(i % 256) as u8, 91, 188, [0, 16, 128, 255][i % 4]])
            .collect(),
    };
    let effect: VideoEffect = serde_json::from_value(
        serde_json::json!({"kind":"concat_falling_dust","parameters":{"dust":100,"leak":75}}),
    )
    .unwrap();
    let render = |time| {
        let mut image = original.clone();
        flat_surface::apply_effect_stack(&mut image, &[effect.clone()], time, &[], &mut || Ok(()))
            .unwrap();
        image
    };
    let a = render(125000);
    let b = render(125000);
    let c = render(1250000);
    assert_eq!(a.rgba, b.rgba);
    assert_ne!(a.rgba, c.rgba);
    assert_eq!((a.width, a.height), (37, 65));
    for (p, q) in a.rgba.chunks_exact(4).zip(original.rgba.chunks_exact(4)) {
        assert_eq!(p[3], q[3]);
    }
    let mut stopped = original.clone();
    let mut calls = 0;
    assert!(
        flat_surface::apply_effect_stack(&mut stopped, &[effect], 125000, &[], &mut || {
            calls += 1;
            anyhow::bail!("authored cancellation")
        })
        .is_err()
    );
    assert!(calls > 0);
}

#[test]
fn original_metal_nonuniform_and_alpha() {
    let mut pixels = 0;
    for opaque in [false, true] {
        let f: serde_json::Value = serde_json::from_str(if opaque {
            include_str!("fixtures/concat-falling-dust-opaque-spatial.json")
        } else {
            include_str!("fixtures/concat-falling-dust-spatial.json")
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
                serde_json::json!({"kind":"concat_falling_dust","parameters":parameters}),
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
