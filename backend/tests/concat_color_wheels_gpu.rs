use video_kadr_backend::domain::concat_color_wheels::{sample, Animation, ColorWheels};
#[test]
fn original_metal_color_wheels_and_independent_animation() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-color-wheels-gpu.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_ne!(fixture["instrumented"], true);
    assert_eq!(fixture["upstreamProbes"], 6);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1024);
    let mut failures = Vec::new();
    let mut worst = 0f32;
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<[[f32; 3]; 3]> = serde_json::from_value(c["wheels"].clone()).unwrap();
        for [lift, gamma, gain] in stages {
            let p = ColorWheels { lift, gamma, gain };
            let rgb = p.signal([input[0], input[1], input[2]]).unwrap();
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
            let allowed = tolerance[i];
            assert!(allowed > 0. && allowed <= expected[i].abs() * 0.002 + 1.01e-6);
            let e = (input[i] - expected[i]).abs();
            worst = worst.max(e / allowed);
            if e > allowed {
                failures.push(format!(
                    "case{index} channel{i} native{} GPU{} error{e} allowed{allowed}",
                    input[i], expected[i]
                ));
            }
        }
    }
    let a:Animation=serde_json::from_value(serde_json::json!({"lift.x":{"mode":"constant","value":0.25},"gamma.m":{"mode":"constant","value":-0.5},"gain.y":{"mode":"constant","value":0.75}})).unwrap();
    let p = sample(ColorWheels::default(), &a, 0).unwrap();
    assert_eq!(p.lift, [0.25, 0., 0.]);
    assert_eq!(p.gamma, [0., 0., -0.5]);
    assert_eq!(p.gain, [0., 0.75, 0.]);
    assert!(ColorWheels {
        lift: [1.01, 0., 0.],
        ..Default::default()
    }
    .validate()
    .is_err());
    assert!(serde_json::from_value::<Animation>(
        serde_json::json!({"lift.q":{"mode":"constant","value":0}})
    )
    .is_err());
    eprintln!(
        "original Metal Color Wheels:1024cases, worst fraction{worst}, failures{}\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(failures.is_empty());
}
