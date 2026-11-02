use video_kadr_backend::domain::concat_curves::{Curves, PreparedCurve};
#[test]
fn original_metal_curves_and_bounded_normalization() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-curves-gpu.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert_eq!(f["upstreamProbes"], 6);
    assert_ne!(f["instrumented"], true);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1280);
    let mut failures = Vec::new();
    let mut worst = 0f32;
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let stages: Vec<[Vec<[f64; 2]>; 4]> = serde_json::from_value(c["stages"].clone()).unwrap();
        for [luma, red, green, blue] in stages {
            let rgb = Curves {
                luma,
                red,
                green,
                blue,
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
            let allowed = tolerance[i];
            assert!(allowed > 0. && allowed <= expected[i].abs() * 0.002 + 1.01e-6);
            let e = (input[i] - expected[i]).abs();
            worst = worst.max(e / allowed);
            if e > allowed {
                failures.push(format!(
                    "case{index} channel{i}:native{} GPU{} error{e} allowed{allowed}",
                    input[i], expected[i]
                ));
            }
        }
    }
    assert!(PreparedCurve::new(&vec![[0., 0.]; 9]).is_err());
    assert!(PreparedCurve::new(&[[f64::NAN, 0.]]).is_err());
    assert!(PreparedCurve::new(&[[0., 1.1]]).is_err());
    let empty = PreparedCurve::new(&[[0.3, 0.8]]).unwrap();
    assert_eq!(empty.sample(2.), 2.);
    let partial = PreparedCurve::new(&[[0.2, 0.3], [0.8, 0.7]]).unwrap();
    assert!((partial.sample(-0.1) - 0.2).abs() < 1e-6);
    assert!((partial.sample(1.2) - 0.9).abs() < 1e-6);
    eprintln!(
        "original Metal Curves:1280cases,worst fraction{worst},failures{}\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(failures.is_empty());
}
