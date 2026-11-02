use serde_json::Value;
use video_kadr_backend::domain::{
    concat_spatial_filters::{Sharpen, Vignette},
    group_surface::GroupSurface,
};
#[test]
fn original_metal_uniform_spatial_filters() {
    for (kind, raw) in [
        ("sharpen", include_str!("fixtures/concat-sharpen-gpu.json")),
        (
            "vignette",
            include_str!("fixtures/concat-vignette-gpu.json"),
        ),
    ] {
        let f: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(f["upstreamProbes"], 6);
        assert_eq!(f["adapter"]["backend"], "Metal");
        assert_eq!(
            f["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 256);
        for (index, c) in cases.iter().enumerate() {
            let mut input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
            let expected: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
            let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
            for parameters in c["stages"].as_array().unwrap() {
                let rgb = [input[0], input[1], input[2]];
                let out = if kind == "sharpen" {
                    serde_json::from_value::<Sharpen>(parameters.clone())
                        .unwrap()
                        .signal([rgb; 25])
                        .unwrap()
                } else {
                    serde_json::from_value::<Vignette>(parameters.clone())
                        .unwrap()
                        .signal(rgb, [4.5 / 8., 4.5 / 8.])
                        .unwrap()
                };
                for i in 0..3 {
                    let v = out[i].clamp(-65504., 65504.);
                    let half = half::f16::from_f32(v);
                    input[i] = if half.to_f32().abs() > v.abs() {
                        half::f16::from_bits(half.to_bits() - 1).to_f32()
                    } else {
                        half.to_f32()
                    };
                }
            }
            assert_eq!(input[3], expected[3]);
            for i in 0..3 {
                assert!(tolerance[i] > 0. && tolerance[i] <= expected[i].abs() * 0.002 + 1.01e-6);
                assert!(
                    (input[i] - expected[i]).abs() <= tolerance[i],
                    "{kind} case{index} channel{i}: {}/{}, tolerance{}",
                    input[i],
                    expected[i],
                    tolerance[i]
                );
            }
        }
    }
}
#[test]
fn original_metal_nonuniform_spatial_filters() {
    for (kind, raw) in [
        (
            "sharpen",
            include_str!("fixtures/concat-sharpen-spatial.json"),
        ),
        (
            "vignette",
            include_str!("fixtures/concat-vignette-spatial.json"),
        ),
    ] {
        let f: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(f["upstreamProbes"], 6);
        assert_eq!(f["syntheticInputAndSelector"], true);
        assert_eq!(f["adapter"]["backend"], "Metal");
        assert_eq!(
            f["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 24);
        let mut pixels = 0;
        for (index, c) in cases.iter().enumerate() {
            let side = c["side"].as_u64().unwrap() as usize;
            let rgba: Vec<u8> = serde_json::from_value(c["inputRgba"].clone()).unwrap();
            let input = GroupSurface {
                width: side,
                height: side,
                rgba,
            };

            let effect:video_kadr_backend::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":format!("concat_{kind}"),"parameters":c["parameters"]})).unwrap();
            let mut surface = input.clone();
            video_kadr_backend::domain::flat_surface::apply_effect_stack(
                &mut surface,
                &[effect],
                0,
                &[],
                &mut || Ok(()),
            )
            .unwrap();
            let out = surface;
            let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
            assert_eq!(out.rgba.len(), expected.len());
            for (i, (a, e)) in out.rgba.iter().zip(expected).enumerate() {
                if i % 4 == 3 {
                    assert_eq!(*a, e, "{kind} case{index} alpha{i}");
                    pixels += 1;
                } else {
                    assert!(a.abs_diff(e) <= 1, "{kind} case{index} byte{i}: {a}/{e}");
                }
            }
        }
        assert_eq!(pixels, 1950);
    }
}

#[test]
fn source_ranges_and_static_wire_contract() {
    for amount in [0., 0.19, 3.01, f32::NAN, f32::INFINITY] {
        assert!(Sharpen { amount }.validate().is_err());
    }
    for strength in [0., 9.9, 100.1, f32::NAN, f32::INFINITY] {
        assert!(Vignette { strength }.validate().is_err());
    }
    for kind in ["concat_sharpen", "concat_vignette"] {
        assert!(serde_json::from_value::<video_kadr_backend::domain::composition::VideoEffect>(serde_json::json!({"kind":kind,"parameters":{},"animation":{"mode":"constant","value":1}})).is_err());
    }
    assert!(serde_json::from_value::<Sharpen>(serde_json::json!({"radius":2})).is_err());
    assert!(serde_json::from_value::<Vignette>(serde_json::json!({"amount":50})).is_err());
}
