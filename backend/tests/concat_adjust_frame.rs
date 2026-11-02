use video_kadr_backend::domain::{
    concat_adjust::Adjust, group_surface::GroupSurface, linear_color_frame::LinearColorFrame,
};
fn apply(p: &Adjust, input: GroupSurface) -> GroupSurface {
    let mut frame = LinearColorFrame::from_surface(&input, &mut || Ok(())).unwrap();
    p.apply_linear(&mut frame, &mut || Ok(())).unwrap();
    frame.to_surface(&mut || Ok(())).unwrap()
}
#[test]
fn spatial_pass_preserves_alpha_and_reflection_symmetry() {
    let width = 7;
    let height = 5;
    let mut rgba = vec![];
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[
                ((x * 37 + y * 11) % 256) as u8,
                ((x * 7 + y * 53) % 256) as u8,
                ((x * 79 + y * 17) % 256) as u8,
                [255, 128, 16][(x + y) % 3],
            ]);
        }
    }
    let input = GroupSurface {
        width,
        height,
        rgba,
    };
    let p = Adjust {
        sharpen: 80.,
        vignette: 70.,
        exposure: 0.7,
        brightness: -8.,
        fade: 13.,
        ..Default::default()
    };
    let output = apply(&p, input.clone());
    assert!(input
        .rgba
        .chunks_exact(4)
        .zip(output.rgba.chunks_exact(4))
        .all(|(a, b)| a[3] == b[3]));
    assert_ne!(output, input);
    let reflect = |s: &GroupSurface| {
        let mut t = s.clone();
        for y in 0..height {
            for x in 0..width {
                let a = (y * width + x) * 4;
                let b = (y * width + width - 1 - x) * 4;
                t.rgba[a..a + 4].copy_from_slice(&s.rgba[b..b + 4]);
            }
        }
        t
    };
    let reflected = reflect(&apply(&p, reflect(&input)));
    for (a, b) in output.rgba.iter().zip(&reflected.rgba) {
        assert!(
            a.abs_diff(*b) <= 1,
            "frozen-input neighborhood/UV symmetry broken: {a}/{b}"
        );
    }
}
#[test]
fn singleton_edges_cancel_and_invalid_parameters() {
    let input = GroupSurface {
        width: 1,
        height: 1,
        rgba: vec![80, 120, 200, 128],
    };
    let no_sharp = apply(
        &Adjust {
            fade: 12.,
            ..Default::default()
        },
        input.clone(),
    );
    let sharp = apply(
        &Adjust {
            fade: 12.,
            sharpen: 100.,
            ..Default::default()
        },
        input.clone(),
    );
    assert_eq!(no_sharp, sharp);
    let mut frame = LinearColorFrame::from_surface(&input, &mut || Ok(())).unwrap();
    assert!(Adjust::default()
        .apply_linear(&mut frame, &mut || anyhow::bail!("cancelled"))
        .is_err());
    for (name, value) in [
        ("exposure", 3.1),
        ("temperature", 1999.),
        ("fade", -1.),
        ("sharpen", 101.),
        ("brightness", 101.),
    ] {
        let mut p = serde_json::to_value(Adjust::default()).unwrap();
        p[name] = serde_json::json!(value);
        let p: Adjust = serde_json::from_value(p).unwrap();
        assert!(p.validate().is_err(), "{name}");
    }
    assert!(serde_json::from_value::<Adjust>(serde_json::json!({"unsupported":1})).is_err());
}

#[test]
fn original_metal_nonuniform_sharpen_vignette_frames() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/concat-adjust-spatial.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 7);
    assert_eq!(fixture["syntheticInputAndSelector"], true);
    assert_ne!(fixture["instrumented"], true);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 40);
    let mut pixels = 0;
    let mut failures = vec![];
    for (index, c) in cases.iter().enumerate() {
        let side = c["side"].as_u64().unwrap() as usize;
        let input: Vec<u8> = serde_json::from_value(c["inputRgba"].clone()).unwrap();
        let expected: Vec<u8> = serde_json::from_value(c["expectedRgba"].clone()).unwrap();
        assert_eq!(input.len(), side * side * 4);
        assert_eq!(expected.len(), input.len());
        let parameters: Adjust = serde_json::from_value(c["parameters"].clone()).unwrap();
        let effect: video_kadr_backend::domain::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"concat_adjust","parameters":parameters}),
        )
        .unwrap();
        let template: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/concat-color-edit.json")).unwrap();
        let mut document =
            template["cases"][0]["original"]["compositions"]["root"]["content"].clone();
        document["tracks"][0]["clips"][0]["effects"] = serde_json::json!([effect]);
        let composition: video_kadr_backend::domain::composition::Composition =
            serde_json::from_value(document).unwrap();
        composition.validate().unwrap();
        assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
        let mut output = GroupSurface {
            width: side,
            height: side,
            rgba: input,
        };
        video_kadr_backend::domain::flat_surface::apply_effect_stack(
            &mut output,
            &[effect],
            0,
            &[],
            &mut || Ok(()),
        )
        .unwrap();
        for (pixel, (actual, expected)) in output
            .rgba
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
        {
            pixels += 1;
            assert_eq!(actual[3], expected[3], "alpha case {index} pixel {pixel}");
            for channel in 0..3 {
                if actual[channel].abs_diff(expected[channel]) > 1 {
                    failures.push((index, pixel, channel, actual[channel], expected[channel]));
                }
            }
        }
    }
    assert_eq!(pixels, 3250);
    assert!(
        failures.is_empty(),
        "{} mismatches: {:?}",
        failures.len(),
        &failures[..failures.len().min(30)]
    );
}

#[test]
#[ignore = "explicit 4K original-neighborhood preservation qualification"]
fn four_k_original_neighbor_control() {
    use sha2::{Digest, Sha256};
    let (width, height) = (3840usize, 2160usize);
    let rgba = (0..width * height)
        .flat_map(|i| {
            let x = i % width;
            let y = i / width;
            [
                ((x * 37 + y * 11) % 256) as u8,
                ((x * 7 + y * 53) % 256) as u8,
                ((x * 79 + y * 17) % 256) as u8,
                [0, 16, 128, 255][(x + y) % 4],
            ]
        })
        .collect::<Vec<_>>();
    let input_hash = format!("{:x}", Sha256::digest(&rgba));
    let p = Adjust {
        sharpen: 80.,
        vignette: 70.,
        brightness: -8.,
        contrast: 30.,
        temperature: 5000.,
        tint: 10.,
        saturation: 25.,
        fade: 13.,
        luma: vec![[0., 0.], [0.25, 0.18], [0.75, 0.84], [1., 1.]],
        blue: vec![[0., 0.2], [1., 0.8]],
        ..Default::default()
    };
    let output = apply(
        &p,
        GroupSurface {
            width,
            height,
            rgba,
        },
    );
    assert_eq!(output.rgba.len(), width * height * 4);
    for (i, p) in output.rgba.chunks_exact(4).enumerate() {
        assert_eq!(p[3], [0, 16, 128, 255][(i % width + i / width) % 4]);
    }
    let hash = format!("{:x}", Sha256::digest(&output.rgba));
    println!("4K input_sha256={input_hash} output_sha256={hash}");
    if let Ok(expected) = std::env::var("VIDEO_KADR_ADJUST_4K_EXPECTED") {
        assert_eq!(hash, expected);
    }
}
