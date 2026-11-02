use video_kadr_backend::domain::concat_marquee::Marquee;
#[test]
fn original_metal_marquee_extended_light_and_stacked_passes() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-marquee-gpu.json")).unwrap();
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
        for [lights, depth] in stages {
            let rgb = Marquee { lights, depth }
                .signal([input[0], input[1], input[2]], [input; 25])
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
        Marquee::default(),
        Marquee {
            lights: 60.,
            depth: 40.
        }
    );
    for value in [-1., 101., f32::NAN, f32::INFINITY] {
        assert!(Marquee {
            lights: value,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Marquee {
            depth: value,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    assert!(serde_json::from_value::<VideoEffect>(
        serde_json::json!({"kind":"concat_marquee","parameters":{},"animation":{}})
    )
    .is_err());
}

#[test]
fn original_metal_marquee_for_mixed_and_opaque_alpha() {
    use video_kadr_backend::domain::{flat_surface, group_surface::GroupSurface};
    let mut pixels = 0;
    for (opaque, text) in [
        (false, include_str!("fixtures/concat-marquee-spatial.json")),
        (
            true,
            include_str!("fixtures/concat-marquee-opaque-spatial.json"),
        ),
    ] {
        let f: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(f["upstreamProbes"], 3);
        assert_eq!(f["adapter"]["backend"], "Metal");
        assert_eq!(f["opaqueInput"].as_bool().unwrap_or(false), opaque);
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 32);
        for (index, c) in cases.iter().enumerate() {
            let side = c["side"].as_u64().unwrap() as usize;
            let mut actual = GroupSurface {
                width: side,
                height: side,
                rgba: serde_json::from_value(c["inputRgba"].clone()).unwrap(),
            };
            if opaque {
                assert!(actual.rgba.chunks_exact(4).all(|p| p[3] == 255));
            }
            let effect = serde_json::from_value(
                serde_json::json!({"kind":"concat_marquee","parameters":c["parameters"]}),
            )
            .unwrap();
            flat_surface::apply_effect_stack(&mut actual, &[effect], 0, &[], &mut || Ok(()))
                .unwrap();
            let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
            assert_eq!(actual.rgba.len(), expected.len());
            for (a, e) in actual.rgba.chunks_exact(4).zip(expected.chunks_exact(4)) {
                pixels += 1;
                assert_eq!(a[3], e[3]);
                for i in 0..3 {
                    assert!(
                        a[i].abs_diff(e[i]) <= 1,
                        "opaque{opaque} case{index} channel{i} native{} GPU{}",
                        a[i],
                        e[i]
                    );
                }
            }
        }
    }
    assert_eq!(pixels, 5200);
}

#[test]
fn sparse_row_ring_matches_immutable_full_image_beyond_its_window() {
    use video_kadr_backend::domain::{flat_surface, group_surface::GroupSurface};
    let (w, h) = (37usize, 65usize);
    let input = GroupSurface {
        width: w,
        height: h,
        rgba: (0..w * h)
            .flat_map(|i| {
                [
                    (i * 37 % 256) as u8,
                    (i * 73 % 256) as u8,
                    (i * 11 % 256) as u8,
                    [0, 16, 128, 255][i % 4],
                ]
            })
            .collect(),
    };
    let original: Vec<[f32; 4]> = input
        .rgba
        .chunks_exact(4)
        .map(|p| {
            let rgb: [f32; 3] = std::array::from_fn(|i| {
                let v = p[i] as f64 / 255.;
                half::f16::from_f32(
                    (if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    }) as f32,
                )
                .to_f32()
            });
            [
                rgb[0],
                rgb[1],
                rgb[2],
                half::f16::from_f32(p[3] as f32 / 255.).to_f32(),
            ]
        })
        .collect();
    let p = Marquee {
        lights: 75.,
        depth: 80.,
    };
    let mut expected = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let neighbors = std::array::from_fn(|n| {
                let sx = (x as isize + (n % 5) as isize * 4 - 8).clamp(0, w as isize - 1) as usize;
                let sy = (y as isize + (n / 5) as isize * 4 - 8).clamp(0, h as isize - 1) as usize;
                original[sy * w + sx]
            });
            let center = original[y * w + x];
            let rgb = p
                .signal([center[0], center[1], center[2]], neighbors)
                .unwrap();
            for v in rgb {
                let v = v.clamp(-65504., 65504.);
                let nearest = half::f16::from_f32(v);
                let v = if nearest.to_f32().abs() > v.abs() {
                    half::f16::from_bits(nearest.to_bits() - 1).to_f32()
                } else {
                    nearest.to_f32()
                };
                let v = f64::from(v.max(0.));
                let encoded = if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1. / 2.4) - 0.055
                };
                expected.push((encoded.clamp(0., 1.) * 255.).round() as u8);
            }
            expected.push(input.rgba[(y * w + x) * 4 + 3]);
        }
    }
    let mut actual = input.clone();
    let effect = serde_json::from_value(
        serde_json::json!({"kind":"concat_marquee","parameters":{"lights":75,"depth":80}}),
    )
    .unwrap();
    flat_surface::apply_effect_stack(&mut actual, &[effect], 0, &[], &mut || Ok(())).unwrap();
    for (i, (a, e)) in actual.rgba.iter().zip(&expected).enumerate() {
        if i % 4 == 3 {
            assert_eq!(a, e);
        } else {
            assert!(a.abs_diff(*e) <= 1, "byte{i}: native{a}, full snapshot{e}");
        }
    }
}
