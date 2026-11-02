use std::collections::BTreeMap;
use video_kadr_backend::domain::concat_adjust::Adjust;
fn parameters(values: BTreeMap<String, f64>) -> Adjust {
    let mut p = serde_json::to_value(Adjust::default()).unwrap();
    for (k, v) in &values {
        if !k.contains('.') {
            p[k] = serde_json::json!(v);
        }
    }
    for name in ["lift", "gamma", "gain"] {
        for (i, axis) in ["x", "y", "m"].iter().enumerate() {
            if let Some(v) = values.get(&format!("{name}.{axis}")) {
                p[name][i] = serde_json::json!(v);
            }
        }
    }
    for name in ["luma", "red", "green", "blue"] {
        let mut points = vec![];
        for i in 0..8 {
            if let (Some(x), Some(y)) = (
                values.get(&format!("{name}.{i}.x")),
                values.get(&format!("{name}.{i}.y")),
            ) {
                points.push([*x, *y]);
            }
        }
        p[name] = serde_json::json!(points);
    }
    serde_json::from_value(p).unwrap()
}
#[test]
fn original_metal_single_pass_adjust() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-adjust-gpu.json")).unwrap();
    assert_eq!(f["upstreamProbes"], 7);
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_ne!(f["instrumented"], true);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1984);
    let mut failures = vec![];
    let mut worst = 0f32;
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<BTreeMap<String, f64>> =
            serde_json::from_value(c["stages"].clone()).unwrap();
        for stage in stages {
            let p = parameters(stage);
            let rgb = [input[0], input[1], input[2]];
            let output = p.signal(rgb, [rgb; 9], [4.5 / 8., 4.5 / 8.]).unwrap();
            for i in 0..3 {
                let v = output[i].clamp(-65504., 65504.);
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
            let ratio = (input[i] - expected[i]).abs() / tolerance[i];
            worst = worst.max(ratio);
            if ratio > 1. {
                failures.push((index, c["name"].clone(), i, input[i], expected[i], ratio));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing samples, worst {worst}: {:?}",
        failures.len(),
        &failures[..failures.len().min(24)]
    );
}
