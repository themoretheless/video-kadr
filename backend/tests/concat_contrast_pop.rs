use video_kadr_backend::domain::concat_contrast_pop::ContrastPop;
#[test]
fn original_metal_contrast_pop_extended_light_and_stacked_passes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-contrast-pop-gpu.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 3);
    assert_ne!(fixture["instrumented"], true);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 320);
    let mut failures = vec![];
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<f32> = serde_json::from_value(c["contrasts"].clone()).unwrap();
        for contrast in stages {
            let rgb = ContrastPop { contrast }
                .signal([input[0], input[1], input[2]])
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
fn original_static_parameter_admission() {
    assert_eq!(ContrastPop::default().contrast, 1.25);
    for contrast in [0.99, 2.01, f32::NAN, f32::INFINITY] {
        assert!(ContrastPop { contrast }.validate().is_err());
    }
    assert!(serde_json::from_value::<ContrastPop>(serde_json::json!({"amount":0.5})).is_err());
    assert!(ContrastPop::default().signal([f32::NAN, 0., 0.]).is_err());
}

#[test]
fn registered_stack_preserves_alpha_and_matches_direct_native_passes() {
    use video_kadr_backend::domain::{
        composition::VideoEffect, flat_surface, group_surface::GroupSurface,
        linear_color_frame::LinearColorFrame,
    };
    let input = GroupSurface {
        width: 4,
        height: 1,
        rgba: vec![
            220, 30, 110, 0, 12, 180, 55, 16, 140, 90, 230, 128, 240, 210, 90, 255,
        ],
    };
    let effects: Vec<VideoEffect> = serde_json::from_value(serde_json::json!([
        {"kind":"concat_contrast_pop","parameters":{"contrast":1.25}},
        {"kind":"concat_contrast_pop","parameters":{"contrast":2}}
    ]))
    .unwrap();
    let mut expected = LinearColorFrame::from_surface(&input, &mut || Ok(())).unwrap();
    for contrast in [1.25, 2.] {
        ContrastPop { contrast }
            .apply_linear(&mut expected, &mut || Ok(()))
            .unwrap();
    }
    let expected = expected.to_surface(&mut || Ok(())).unwrap();
    let mut actual = input.clone();
    flat_surface::apply_effect_stack(&mut actual, &effects, 0, &[], &mut || Ok(())).unwrap();
    assert_eq!(actual, expected);
    assert_ne!(actual.rgba, input.rgba);
    for (a, b) in actual.rgba.chunks_exact(4).zip(input.rgba.chunks_exact(4)) {
        assert_eq!(a[3], b[3]);
    }
    assert!(serde_json::from_value::<VideoEffect>(serde_json::json!({"kind":"concat_contrast_pop","parameters":{},"animation":{"mode":"constant","value":1.5}})).is_err());
}
