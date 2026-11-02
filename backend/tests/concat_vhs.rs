use video_kadr_backend::domain::{
    composition::VideoEffect, concat_vhs::Vhs, flat_surface, group_surface::GroupSurface,
};
#[test]
fn original_metal_extended_and_timed_stacks() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-vhs-gpu.json")).unwrap();
    assert_eq!(
        f["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(f["adapter"]["backend"], "Metal");
    assert!(f["upstreamProbes"].as_u64().unwrap() > 0);
    assert_ne!(f["instrumented"], true);
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2640);
    for (index, c) in cases.iter().enumerate() {
        let input: [f32; 4] = serde_json::from_value(c["effectiveInput"].clone()).unwrap();
        let gpu: [f32; 4] = serde_json::from_value(c["gpu"].clone()).unwrap();
        let tolerance: [f32; 4] = serde_json::from_value(c["tolerance"].clone()).unwrap();
        assert_eq!(c["samplePixel"], serde_json::json!([4, 4]));
        // A held pass with grain is spatial even when its upload was uniform.
        // Preserve the complete preceding plane; only the final sampled pixel may be pruned.
        let settings = c["settings"].as_array().unwrap();
        let mut image = [input; 64];
        for (stage, setting) in settings.iter().enumerate() {
            let p: Vhs = serde_json::from_value(setting.clone()).unwrap();
            let original = image;
            for pixel in 0..64 {
                if stage + 1 == settings.len() && pixel != 36 {
                    continue;
                }
                let (x, y) = (pixel % 8, pixel / 8);
                let neighbors = std::array::from_fn(|n| {
                    let sx = (x as isize + (n % 7) as isize - 3).clamp(0, 7) as usize;
                    let sy = (y as isize + (n / 7) as isize - 3).clamp(0, 7) as usize;
                    original[sy * 8 + sx]
                });
                let rgb = p
                    .signal(neighbors, x, y, c["time"].as_f64().unwrap() as f32)
                    .unwrap();
                for i in 0..3 {
                    let v = rgb[i].clamp(-65504., 65504.);
                    let h = half::f16::from_f32(v);
                    image[pixel][i] = if h.to_f32().abs() > v.abs() {
                        half::f16::from_bits(h.to_bits() - 1).to_f32()
                    } else {
                        h.to_f32()
                    };
                }
            }
        }
        let input = image[36];
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
fn original_metal_nonuniform_and_alpha() {
    let mut pixels = 0;
    for opaque in [false, true] {
        let f: serde_json::Value = serde_json::from_str(if opaque {
            include_str!("fixtures/concat-vhs-opaque-spatial.json")
        } else {
            include_str!("fixtures/concat-vhs-spatial.json")
        })
        .unwrap();
        assert_eq!(
            f["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(f["adapter"]["backend"], "Metal");
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 160);
        for (index, c) in cases.iter().enumerate() {
            let side = c["side"].as_u64().unwrap() as usize;
            let mut actual = GroupSurface {
                width: side,
                height: side,
                rgba: serde_json::from_value(c["inputRgba"].clone()).unwrap(),
            };
            let mut parameters = c["parameters"].clone();
            let time = parameters
                .as_object_mut()
                .unwrap()
                .remove("time")
                .unwrap()
                .as_f64()
                .unwrap();
            let effect = serde_json::from_value(
                serde_json::json!({"kind":"concat_vhs","parameters":parameters}),
            )
            .unwrap();
            flat_surface::apply_effect_stack(
                &mut actual,
                &[effect],
                (time * 1000000.).round() as u64,
                &[],
                &mut || Ok(()),
            )
            .unwrap();
            let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
            assert_eq!(actual.rgba.len(), expected.len());
            for (a, e) in actual.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
                pixels += 1;
                assert_eq!(a[3], e[3]);
                for i in 0..3 {
                    assert!(
                        a[i].abs_diff(e[i]) <= 1,
                        "opaque{opaque} case{index} channel{i}: native{} GPU{}",
                        a[i],
                        e[i]
                    );
                }
            }
        }
    }
    assert_eq!(pixels, 31616);
}

#[test]
fn strict_controls_and_clock() {
    let effect: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_vhs","parameters":{}})).unwrap();
    assert!(matches!(effect,VideoEffect::ConcatVhs{parameters} if parameters==Vhs::default()));
    for parameters in [
        serde_json::json!({"bleed":101}),
        serde_json::json!({"lines":-1}),
        serde_json::json!({"unknown":0}),
    ] {
        let decoded = serde_json::from_value::<VideoEffect>(
            serde_json::json!({"kind":"concat_vhs","parameters":parameters}),
        );
        assert!(
            decoded.is_err()
                || matches!(decoded.unwrap(),VideoEffect::ConcatVhs{parameters}if parameters.validate().is_err())
        );
    }
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_vhs","parameters":{},"animation":{}})
    )
    .is_err());
    for t in [f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(Vhs::default().signal([[0.; 4]; 49], 0, 0, t).is_err());
    }
}

#[test]
fn native_stack_dimensions_hidden_alpha_clock_and_cancellation() {
    let original = GroupSurface {
        width: 37,
        height: 65,
        rgba: (0..37 * 65)
            .flat_map(|i| [(i % 256) as u8, 91, 188, [0, 16, 128, 255][i % 4]])
            .collect(),
    };
    let effect: VideoEffect = serde_json::from_value(
        serde_json::json!({"kind":"concat_vhs","parameters":{"bleed":37,"lines":75}}),
    )
    .unwrap();
    let render = |time| {
        let mut image = original.clone();
        flat_surface::apply_effect_stack(&mut image, &[effect.clone()], time, &[], &mut || Ok(()))
            .unwrap();
        image
    };
    let a = render(125000);
    let b = render(125000);
    let c = render(1250000);
    assert_eq!(a.rgba, b.rgba);
    assert_ne!(a.rgba, c.rgba);
    assert_eq!((a.width, a.height), (37, 65));
    for (p, q) in a.rgba.chunks_exact(4).zip(original.rgba.chunks_exact(4)) {
        assert_eq!(p[3], q[3]);
    }
    let mut stopped = original.clone();
    let mut calls = 0;
    assert!(
        flat_surface::apply_effect_stack(&mut stopped, &[effect], 125000, &[], &mut || {
            calls += 1;
            anyhow::bail!("authored cancellation")
        })
        .is_err()
    );
    assert!(calls > 0);
}
#[test]
fn original_nonuniform_float_fractional_bleed() {
    let mut count = 0;
    for text in [
        include_str!("fixtures/concat-vhs-spatial.json"),
        include_str!("fixtures/concat-vhs-opaque-spatial.json"),
        include_str!("fixtures/concat-vhs-fractional-spatial.json"),
    ] {
        let f: serde_json::Value = serde_json::from_str(text).unwrap();
        for (index, c) in f["cases"].as_array().unwrap().iter().enumerate() {
            let side = c["side"].as_u64().unwrap() as usize;
            let input: Vec<u8> = serde_json::from_value(c["inputRgba"].clone()).unwrap();
            let source: Vec<[f32; 4]> = input
                .chunks_exact(4)
                .map(|p| {
                    let mut out = [0.; 4];
                    for i in 0..3 {
                        let v = p[i] as f32 / 255.;
                        out[i] = half::f16::from_f32(if v <= 0.04045 {
                            v / 12.92
                        } else {
                            ((v + 0.055) / 1.055).powf(2.4)
                        })
                        .to_f32();
                    }
                    out[3] = half::f16::from_f32(p[3] as f32 / 255.).to_f32();
                    out
                })
                .collect();
            let mut parameters = c["parameters"].clone();
            let time = parameters
                .as_object_mut()
                .unwrap()
                .remove("time")
                .unwrap()
                .as_f64()
                .unwrap() as f32;
            let p: Vhs = serde_json::from_value(parameters).unwrap();
            let expected: Vec<f32> = serde_json::from_value(c["expectedWorking"].clone()).unwrap();
            for (pixel, gpu) in expected.chunks_exact(4).enumerate() {
                let (x, y) = (pixel % side, pixel / side);
                let n = std::array::from_fn(|n| {
                    let sx =
                        (x as isize + (n % 7) as isize - 3).clamp(0, side as isize - 1) as usize;
                    let sy =
                        (y as isize + (n / 7) as isize - 3).clamp(0, side as isize - 1) as usize;
                    source[sy * side + sx]
                });
                let rgb = p.signal(n, x, y, time).unwrap();
                for i in 0..3 {
                    let v = rgb[i].clamp(-65504., 65504.);
                    let near = half::f16::from_f32(v);
                    let actual = if near.to_f32().abs() > v.abs() {
                        half::f16::from_bits(near.to_bits() - 1).to_f32()
                    } else {
                        near.to_f32()
                    };
                    let bits = half::f16::from_f32(gpu[i].abs()).to_bits();
                    let low = half::f16::from_bits(bits.saturating_sub(1)).to_f32();
                    let high = half::f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();
                    let tolerance = ((gpu[i].abs() - low).max(high - gpu[i].abs()) * 2.).max(1e-6);
                    assert!((actual-gpu[i]).abs()<=tolerance,"spatial case{index} pixel{x},{y}/{side} ch{i}: native{actual} original{} tolerance{tolerance}",gpu[i]);
                }
                assert_eq!(source[pixel][3], gpu[3]);
                count += 1;
            }
        }
    }
    assert_eq!(count, 31954);
}
