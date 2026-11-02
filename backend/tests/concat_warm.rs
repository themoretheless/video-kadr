use video_kadr_backend::domain::concat_color_presets::Warm;
#[test]
fn original_metal_warm_extended_light_and_stacked_passes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-warm-gpu.json")).unwrap();
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
            let rgb = Warm {
                temperature: values[0],
            }
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
fn source_static_parameter_contract() {
    use video_kadr_backend::domain::composition::VideoEffect;
    assert_eq!(Warm::default(), Warm { temperature: 4600. });
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_warm","parameters":{},"animation":{}})
    )
    .is_err());
    assert!(serde_json::from_value::<Warm>(serde_json::json!({"unknown":0})).is_err());
    for value in [2999., 6001., f32::NAN, f32::INFINITY] {
        assert!(Warm {
            temperature: value,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    for value in [3000., 6000.] {
        Warm {
            temperature: value,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
}
