use video_kadr_backend::domain::{
    composition::VideoEffect, concat_retro_cartoon::RetroCartoon, flat_surface,
    group_surface::GroupSurface,
};
#[test]
fn original_metal_extended_and_fractional_levels() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-retro-cartoon-gpu.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert!(f["upstreamProbes"].as_u64().unwrap() > 0);
    assert_ne!(f["instrumented"], true);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1408);
    for (index, c) in cases.iter().enumerate() {
        let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let gpu: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        for values in c["settings"].as_array().unwrap() {
            let p = RetroCartoon {
                levels: values[0].as_f64().unwrap() as f32,
                ink: values[1].as_f64().unwrap() as f32,
            };
            let rgb = p.signal([[input[0], input[1], input[2]]; 9]).unwrap();
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
        assert_eq!(input[3], gpu[3]);
        for i in 0..3 {
            assert!(tolerance[i] > 0. && tolerance[i] <= gpu[i].abs() * 0.002 + 1.01e-6);
            assert!(
                (input[i] - gpu[i]).abs() <= tolerance[i],
                "case{index} channel{i} native{} GPU{} allowed{}",
                input[i],
                gpu[i],
                tolerance[i]
            );
        }
    }
}
#[test]
fn original_metal_nonuniform_sobel() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-retro-cartoon-spatial.json")).unwrap();
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
            serde_json::json!({"kind":"concat_retro_cartoon","parameters":c["parameters"]}),
        )
        .unwrap();
        flat_surface::apply_effect_stack(&mut actual, &[effect], 0, &[], &mut || Ok(())).unwrap();
        let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
        assert_eq!(actual.rgba.len(), expected.len());
        for (a, e) in actual.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
            pixels += 1;
            assert_eq!(a[3], e[3]);
            for i in 0..3 {
                assert!(
                    a[i].abs_diff(e[i]) <= 1,
                    "case{index} channel{i}: native{} GPU{}",
                    a[i],
                    e[i]
                );
            }
        }
    }
    assert_eq!(pixels, 2600);
}
#[test]
fn static_parameter_contract() {
    assert_eq!(
        RetroCartoon::default(),
        RetroCartoon {
            levels: 5.,
            ink: 60.
        }
    );
    for p in [
        RetroCartoon {
            levels: 1.9,
            ink: 60.,
        },
        RetroCartoon {
            levels: 8.1,
            ink: 60.,
        },
        RetroCartoon {
            levels: f32::NAN,
            ink: 60.,
        },
        RetroCartoon {
            levels: 5.,
            ink: -1.,
        },
        RetroCartoon {
            levels: 5.,
            ink: 101.,
        },
    ] {
        assert!(p.validate().is_err());
    }
    for extra in [
        serde_json::json!({"kind":"concat_retro_cartoon","parameters":{},"animation":{}}),
        serde_json::json!({"kind":"concat_retro_cartoon","parameters":{"noise":1}}),
    ] {
        assert!(serde_json::from_value::<VideoEffect>(extra).is_err());
    }
}
