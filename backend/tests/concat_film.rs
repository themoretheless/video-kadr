use video_kadr_backend::domain::{
    composition::VideoEffect, concat_film_camera_looks::Film, flat_surface,
    group_surface::GroupSurface,
};
#[test]
fn original_metal_extended_and_timed_stacks() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-film-gpu.json")).unwrap();
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
            let p: Film = serde_json::from_value(setting.clone()).unwrap();
            let original = image;
            for pixel in 0..64 {
                if stage + 1 == settings.len() && pixel != 36 {
                    continue;
                }
                let (x, y) = (pixel % 8, pixel / 8);
                let center = original[pixel];
                let neighbors = std::array::from_fn(|n| {
                    let sx = (x as isize + (n % 5) as isize * 4 - 8).clamp(0, 7) as usize;
                    let sy = (y as isize + (n / 5) as isize * 4 - 8).clamp(0, 7) as usize;
                    original[sy * 8 + sx]
                });
                let rgb = p
                    .signal(
                        [center[0], center[1], center[2]],
                        neighbors,
                        x,
                        y,
                        c["time"].as_f64().unwrap() as f32,
                    )
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
            include_str!("fixtures/concat-film-opaque-spatial.json")
        } else {
            include_str!("fixtures/concat-film-spatial.json")
        })
        .unwrap();
        assert_eq!(
            f["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(f["adapter"]["backend"], "Metal");
        let cases = f["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 128);
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
                serde_json::json!({"kind":"concat_film","parameters":parameters}),
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
    assert_eq!(pixels, 20800);
}
#[test]
fn source_contract_and_clock() {
    assert_eq!(
        Film::default(),
        Film {
            grain: 40.,
            warmth: 50.
        }
    );
    let _: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_film","parameters":{}})).unwrap();
    for extra in [
        serde_json::json!({"kind":"concat_film","parameters":{},"animation":{}}),
        serde_json::json!({"kind":"concat_film","parameters":{"unknown":1}}),
    ] {
        assert!(serde_json::from_value::<VideoEffect>(extra).is_err());
    }
    for value in [-1., 101., f32::NAN] {
        let mut p = Film::default();
        p.grain = value;
        assert!(p.validate().is_err());
    }
    let mut p = Film::default();
    p.grain = 75.;
    let a = p
        .signal([0.1, 0.2, 0.3], [[0.1, 0.2, 0.3, 1.]; 25], 3, 7, 0.125)
        .unwrap();
    assert_eq!(
        a,
        p.signal([0.1, 0.2, 0.3], [[0.1, 0.2, 0.3, 1.]; 25], 3, 7, 0.125)
            .unwrap()
    );
    assert_ne!(
        a,
        p.signal([0.1, 0.2, 0.3], [[0.1, 0.2, 0.3, 1.]; 25], 3, 7, 1.25)
            .unwrap()
    );
    assert_ne!(
        a,
        p.signal([0.1, 0.2, 0.3], [[0.1, 0.2, 0.3, 1.]; 25], 4, 7, 0.125)
            .unwrap()
    );
    for time in [f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(p.signal([0.; 3], [[0.; 4]; 25], 0, 0, time).is_err());
    }
    for warmth in [-1., 101., f32::NAN] {
        p.warmth = warmth;
        assert!(p.validate().is_err());
    }
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
    let p = Film {
        grain: 75.,
        warmth: 80.,
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
                .signal([center[0], center[1], center[2]], neighbors, x, y, 1.25)
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
        serde_json::json!({"kind":"concat_film","parameters":{"grain":75,"warmth":80}}),
    )
    .unwrap();
    flat_surface::apply_effect_stack(&mut actual, &[effect], 1250000, &[], &mut || Ok(())).unwrap();
    for (i, (a, e)) in actual.rgba.iter().zip(&expected).enumerate() {
        if i % 4 == 3 {
            assert_eq!(a, e);
        } else {
            assert!(a.abs_diff(*e) <= 1, "byte{i}: native{a}, full snapshot{e}");
        }
    }
}
