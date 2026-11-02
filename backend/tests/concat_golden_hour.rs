use video_kadr_backend::domain::concat_light_looks::GoldenHour;
#[test]
fn original_metal_golden_hour_extended_light_and_stacked_passes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-golden-hour-gpu.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 3);
    assert_ne!(fixture["instrumented"], true);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1144);
    let mut failures = vec![];
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<[f32; 2]> = serde_json::from_value(c["settings"].clone()).unwrap();
        for [glow, warmth] in stages {
            let rgb = GoldenHour { glow, warmth }
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
    assert_eq!(
        GoldenHour::default(),
        GoldenHour {
            glow: 50.,
            warmth: 4600.
        }
    );
    for value in [-1., 101., f32::NAN, f32::INFINITY] {
        assert!(GoldenHour {
            glow: value,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(GoldenHour {
            warmth: value,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_golden_hour","parameters":{},"animation":{}})
    )
    .is_err());
}

#[test]
fn kelvin_range_is_not_percent() {
    for warmth in [3499., 6501., f32::NAN, f32::INFINITY] {
        assert!(GoldenHour {
            warmth,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    for warmth in [3500., 4600., 6500.] {
        GoldenHour {
            warmth,
            ..Default::default()
        }
        .validate()
        .unwrap();
    }
}
