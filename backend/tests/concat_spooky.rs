use video_kadr_backend::domain::concat_low_light_looks::Spooky;
#[test]
fn original_metal_spooky_extended_light_and_stacked_passes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-spooky-gpu.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 3);
    assert_ne!(fixture["instrumented"], true);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 616);
    let mut failures = vec![];
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<Vec<f32>> = serde_json::from_value(c["settings"].clone()).unwrap();
        for values in stages {
            let rgb = Spooky { dread: values[0] }
                .signal([input[0], input[1], input[2]], [0.5, 0.5])
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
        assert_eq!(input[3], expected[3]);
        for i in 0..3 {
            assert!(tolerance[i] > 0. && tolerance[i] <= expected[i].abs() * 0.002 + 1.01e-6);
            if (input[i] - expected[i]).abs() > tolerance[i] {
                failures.push((index, i, input[i], expected[i], tolerance[i]));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures: {:?}",
        failures.len(),
        &failures[..failures.len().min(20)]
    );
}
#[test]
fn source_static_parameter_contract() {
    use video_kadr_backend::domain::composition::VideoEffect;
    assert_eq!(Spooky::default(), Spooky { dread: 60. });
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_spooky","parameters":{},"animation":{}})
    )
    .is_err());
    for value in [-1., 101., f32::NAN, f32::INFINITY] {
        assert!(Spooky {
            dread: value,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

#[test]
fn original_metal_nonuniform_spooky() {
    use video_kadr_backend::domain::{flat_surface, group_surface::GroupSurface};
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-spooky-spatial.json")).unwrap();
    assert_eq!(f["upstreamProbes"], 3);
    assert_eq!(f["adapter"]["backend"], "Metal");
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    let mut pixels = 0;
    for (index, c) in cases.iter().enumerate() {
        let side = c["side"].as_u64().unwrap() as usize;
        let mut actual = GroupSurface {
            width: side,
            height: side,
            rgba: serde_json::from_value(c["inputRgba"].clone()).unwrap(),
        };
        let effect = serde_json::from_value(
            serde_json::json!({"kind":"concat_spooky","parameters":c["parameters"]}),
        )
        .unwrap();
        flat_surface::apply_effect_stack(&mut actual, &[effect], 0, &[], &mut || Ok(())).unwrap();
        let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
        for (a, e) in actual.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
            pixels += 1;
            assert_eq!(a[3], e[3]);
            for i in 0..3 {
                assert!(
                    a[i].abs_diff(e[i]) <= 1,
                    "case{index} channel{i} native{} GPU{}",
                    a[i],
                    e[i]
                );
            }
        }
    }
    assert_eq!(pixels, 2600);
}
