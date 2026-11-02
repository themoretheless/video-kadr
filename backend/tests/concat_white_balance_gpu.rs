use video_kadr_backend::domain::concat_white_balance::{sample, Animation, WhiteBalance};
#[test]
fn original_metal_white_balance_and_independent_animation() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-white-balance-gpu.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 6);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2560);
    assert_ne!(
        fixture["instrumented"], true,
        "instrumented diagnostics cannot qualify package parity"
    );
    let mut failures = Vec::new();
    let mut worst = 0f32;
    for (index, c) in cases.iter().enumerate() {
        let input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let gpu: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        let alpha = input[3];
        let mut got = input;
        let mut pass_trace = Vec::new();
        let mut prefix = Vec::new();
        for t in c["amounts"].as_array().unwrap() {
            let p = WhiteBalance {
                temperature: t.as_f64().unwrap() as f32,
                tint: c["intensity"].as_f64().unwrap() as f32,
            };
            let light = p.signal([got[0], got[1], got[2]]).unwrap();
            for i in 0..3 {
                let v = light[i].clamp(-65504., 65504.);
                let nearest = half::f16::from_f32(v);
                got[i] = if nearest.to_f32().abs() > v.abs() {
                    half::f16::from_bits(nearest.to_bits() - 1).to_f32()
                } else {
                    nearest.to_f32()
                };
            }
            prefix.push(t.clone());
            let original_prefix = cases.iter().find(|candidate| {
                candidate["effectiveInput"] == c["effectiveInput"]
                    && candidate["intensity"] == c["intensity"]
                    && candidate["amounts"].as_array() == Some(&prefix)
            });
            pass_trace.push(format!(
                "temperature={} raw={light:?} native_held={got:?} original_prefix={}",
                p.temperature,
                original_prefix.map(|case| case["gpu"].to_string()).unwrap_or_else(|| "unavailable".into())
            ));
        }
        assert_eq!(alpha, got[3]);
        assert_eq!(got[3], gpu[3]);
        for i in 0..3 {
            let allowed = tolerance[i];
            assert!(allowed > 0. && allowed <= gpu[i].abs() * 0.002 + 1.01e-6);
            let actual = got[i];
            let error = (actual - gpu[i]).abs();
            worst = worst.max(error / allowed);
            if error > allowed {
                failures.push(format!("case{index} channel{i}: native{actual} GPU{} error{error} allowed{allowed}; temperature{} tint{} input{:?}",gpu[i],c["amounts"],c["intensity"],input));
                failures.last_mut().unwrap().push_str(&format!("\npass trace: {}", pass_trace.join("; ")));
            }
        }
    }
    let a:Animation=serde_json::from_value(serde_json::json!({"temperature":{"mode":"constant","value":4500},"tint":{"mode":"constant","value":25}})).unwrap();
    assert_eq!(
        sample(WhiteBalance::default(), &a, 0).unwrap(),
        WhiteBalance {
            temperature: 4500.,
            tint: 25.
        }
    );
    for p in [
        WhiteBalance {
            temperature: 1999.,
            tint: 0.,
        },
        WhiteBalance {
            temperature: 6500.,
            tint: 101.,
        },
        WhiteBalance {
            temperature: f32::NAN,
            tint: 0.,
        },
    ] {
        assert!(p.validate().is_err());
    }
    eprintln!("original Metal White Balance:2560 cases, max tolerance fraction={worst}; discrepancies={}\n{}",failures.len(),failures.join("\n"));
    assert!(
        failures.is_empty(),
        "original Metal White Balance numerical equivalence remains unqualified"
    );
}
