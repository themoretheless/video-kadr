#![cfg(feature = "concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{
    concat_qualification::spatial_transition, transition_spec::TransitionSpec,
};
fn spec(effect: &str, value: f32) -> TransitionSpec {
    let key = if effect == "concat.spin" {
        "turns"
    } else {
        "twist"
    };
    serde_json::from_value(json!({"effect":effect,"params":{key:{"Float":value}}})).unwrap()
}
#[test]
fn spin_scales_rotations_and_transparency_while_swirl_clamps_source_edges() {
    let a: Vec<[f32; 4]> = (0..45)
        .map(|i| [(i % 9) as f32 / 8., (i / 9) as f32 / 4., 0., 1.])
        .collect();
    let b: Vec<[f32; 4]> = (0..45)
        .map(|i| [0., (i / 9) as f32 / 4., (i % 9) as f32 / 8., 1.])
        .collect();
    let spin = spatial_transition(&spec("concat.spin", 1.), 9, 5, &a, &b, 0.5, false).unwrap();
    assert_eq!(spin[0], [0.; 4]);
    assert_eq!(spin[22], [0.25, 0.5, 0.25, 1.]);
    let swirl = spatial_transition(&spec("concat.swirl", 70.), 9, 5, &a, &b, 0.5, false).unwrap();
    assert!(swirl.iter().all(|p| p[3] == 1.));
    let neutral = spatial_transition(&spec("concat.swirl", 0.), 9, 5, &a, &b, 0.5, false).unwrap();
    assert_ne!(swirl, neutral);
    for (effect, value) in [("concat.spin", 1.), ("concat.swirl", 70.)] {
        let s = spec(effect, value);
        assert_eq!(spatial_transition(&s, 9, 5, &a, &b, 0., false).unwrap(), a);
        assert_eq!(spatial_transition(&s, 9, 5, &a, &b, 1., false).unwrap(), b);
        let forward = spatial_transition(&s, 9, 5, &a, &b, 0.375, false).unwrap();
        assert_eq!(
            spatial_transition(&s, 9, 5, &b, &a, 0.625, true).unwrap(),
            forward
        );
    }
    for (effect, value) in [
        ("concat.spin", 0.49),
        ("concat.spin", 2.01),
        ("concat.swirl", -1.),
        ("concat.swirl", 101.),
    ] {
        assert!(spec(effect, value).validate().is_err());
    }
}

#[test]
#[ignore = "requires independently generated CONCAT_ROTATION_IMAGE_ORACLE"]
fn original_metal_full_rotation_images() {
    use video_kadr_backend::domain::concat_effect_upgrade::{native_transition, AppliedFilter};
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_ROTATION_IMAGE_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    let effect = oracle["effect"].as_str().unwrap();
    assert!(matches!(effect, "concat.spin" | "concat.swirl"));
    assert_eq!(oracle["fullImage"], true);
    assert_eq!(oracle["rendererBodyUnchanged"], true);
    assert_eq!(oracle["addedReadbackMethod"], true);
    assert_eq!(oracle["upstreamProbes"], 12);
    assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 384);
    let mut channels = 0;
    let mut mismatches = [0usize; 4];
    let mut signed_zeros = [0usize; 4];
    let mut max_abs = [0f32; 4];
    let mut selected = Vec::new();
    for c in cases {
        let width = c["width"].as_u64().unwrap() as usize;
        let height = c["height"].as_u64().unwrap() as usize;
        let parameter = c["parameter"].as_str().unwrap();
        assert_eq!(
            parameter,
            if effect == "concat.spin" {
                "turns"
            } else {
                "twist"
            }
        );
        let from: Vec<[f32; 4]> = serde_json::from_value(c["from"].clone()).unwrap();
        let to: Vec<[f32; 4]> = serde_json::from_value(c["to"].clone()).unwrap();
        let expected: Vec<[f32; 4]> = serde_json::from_value(c["expected"].clone()).unwrap();
        assert_eq!(expected.len(), width * height);
        let link: AppliedFilter =
            serde_json::from_value(json!({"id":effect,"params":{parameter:c["value"]}})).unwrap();
        let kernel = native_transition(&link).unwrap().unwrap().kernel.unwrap();
        let actual = spatial_transition(
            &kernel,
            width,
            height,
            &from,
            &to,
            c["progress"].as_f64().unwrap() as f32,
            false,
        )
        .unwrap();
        assert_eq!(actual.len(), expected.len());
        if selected.len() < 16
            && actual
                .iter()
                .zip(&expected)
                .any(|(a, b)| a.iter().zip(b).any(|(a, b)| a.to_bits() != b.to_bits()))
        {
            selected.push(c.clone());
        }
        for (a, b) in actual.iter().zip(expected) {
            for k in 0..4 {
                channels += 1;
                if a[k].to_bits() != b[k].to_bits() {
                    mismatches[k] += 1;
                    if a[k] == b[k] {
                        signed_zeros[k] += 1;
                    } else {
                        max_abs[k] = max_abs[k].max((a[k] - b[k]).abs());
                    }
                }
            }
        }
    }
    assert_eq!(
        channels,
        if oracle["independentValidation"].as_bool().unwrap_or(false) {
            132864
        } else {
            101632
        }
    );
    println!("Original {effect} full images: {channels} channels, mismatch RGBA {mismatches:?}");
    if let Ok(path) = std::env::var("CONCAT_ROTATION_DIAGNOSTIC_OUTPUT") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"effect":effect,"diagnosticSelection":true,"cases":selected}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    println!("Signed-zero mismatches RGBA {signed_zeros:?}; maximum numeric absolute error RGBA {max_abs:?}");
    assert_eq!(mismatches, [0; 4]);
}

#[test]
#[ignore = "instrumented diagnostic only; requires CONCAT_ROTATION_SAMPLES_DIAGNOSTIC"]
fn diagnose_weighted_rotation_samples_before_mix() {
    use video_kadr_backend::domain::concat_effect_upgrade::{native_transition, AppliedFilter};
    let data: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_ROTATION_SAMPLES_DIAGNOSTIC").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(data["instrumented"], true);
    assert_eq!(data["rendererBodyUnchanged"], true);
    let effect = data["effect"].as_str().unwrap();
    assert!(matches!(effect, "concat.spin" | "concat.swirl"));
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let half_target = |value: f32| {
        let h = half::f16::from_f32(value);
        if h.to_f32().abs() > value.abs() {
            half::f16::from_bits(h.to_bits() - 1).to_f32()
        } else {
            h.to_f32()
        }
    };
    let mut counts = [[0usize; 4]; 2];
    let mut checked = 0usize;
    for entry in cases {
        let c = &entry["originalCase"];
        let width = c["width"].as_u64().unwrap() as usize;
        let height = c["height"].as_u64().unwrap() as usize;
        let parameter = c["parameter"].as_str().unwrap();
        let link: AppliedFilter =
            serde_json::from_value(json!({"id":effect,"params":{parameter:c["value"]}})).unwrap();
        let kernel = native_transition(&link).unwrap().unwrap().kernel.unwrap();
        let from: Vec<[f32; 4]> = serde_json::from_value(c["from"].clone()).unwrap();
        let to: Vec<[f32; 4]> = serde_json::from_value(c["to"].clone()).unwrap();
        let zeros = vec![[0.; 4]; width * height];
        let progress = c["progress"].as_f64().unwrap() as f32;
        let t = progress * progress * (3. - 2. * progress);
        for (side, label) in [(0, "a"), (1, "b")] {
            let (actual_from, actual_to) = if side == 0 {
                (&from, &zeros)
            } else {
                (&zeros, &to)
            };
            let actual = spatial_transition(
                &kernel,
                width,
                height,
                actual_from,
                actual_to,
                progress,
                false,
            )
            .unwrap();
            for k in 0..4 {
                let encoded: Vec<[f32; 4]> =
                    serde_json::from_value(entry["sampleBits"][format!("{label}{k}")].clone())
                        .unwrap();
                assert_eq!(encoded.len(), width * height);
                for (index, pixel) in encoded.iter().enumerate() {
                    assert!(pixel
                        .iter()
                        .all(|v| *v >= 0. && *v <= 255. && v.fract() == 0.));
                    let sample = f32::from_le_bytes(pixel.map(|v| v as u8));
                    assert!(sample.is_finite());
                    let expected = if side == 0 {
                        half_target(sample * (1. - t) + 0. * t)
                    } else {
                        half_target(0. * (1. - t) + sample * t)
                    };
                    checked += 1;
                    if actual[index][k].to_bits() != expected.to_bits() {
                        counts[side][k] += 1;
                    }
                }
            }
        }
    }
    println!("INSTRUMENTED weighted sample comparison: {checked} channels; outgoing/incoming mismatch RGBA {counts:?}");
    assert!(checked > 0);
    assert!(
        counts.iter().flatten().any(|v| *v > 0),
        "selected diagnostic should locate the known pre-mix discrepancy"
    );
}

#[test]
#[ignore = "instrumented diagnostic only; requires CONCAT_ROTATION_UV_DIAGNOSTIC"]
fn diagnose_filtering_at_observed_gpu_coordinates() {
    use video_kadr_backend::domain::concat_qualification::texture_samples;
    let data: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_ROTATION_UV_DIAGNOSTIC").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(data["instrumented"], true);
    assert_eq!(data["rendererBodyUnchanged"], true);
    let effect = data["effect"].as_str().unwrap();
    assert!(matches!(effect, "concat.spin" | "concat.swirl"));
    let cases = data["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let mut mismatches = [[0usize; 4]; 2];
    let mut checked = 0;
    for entry in cases {
        let c = &entry["originalCase"];
        let width = c["width"].as_u64().unwrap() as usize;
        let height = c["height"].as_u64().unwrap() as usize;
        let column = |name: &str| -> Vec<f32> {
            let values: Vec<[f32; 4]> =
                serde_json::from_value(entry["sampleBits"][name].clone()).unwrap();
            assert_eq!(values.len(), width * height);
            values
                .iter()
                .map(|v| {
                    assert!(v.iter().all(|v| *v >= 0. && *v <= 255. && v.fract() == 0.));
                    f32::from_le_bytes(v.map(|v| v as u8))
                })
                .collect()
        };
        for (side, label, input) in [(0, "a", "from"), (1, "b", "to")] {
            let pixels: Vec<[f32; 4]> = serde_json::from_value(c[input].clone()).unwrap();
            let ux = column(&format!("uv{label}0"));
            let uy = column(&format!("uv{label}1"));
            let uv: Vec<[f32; 2]> = ux.iter().zip(uy).map(|(x, y)| [*x, y]).collect();
            let actual = texture_samples(width, height, &pixels, &uv).unwrap();
            for k in 0..4 {
                let expected = column(&format!("{label}{k}"));
                for index in 0..actual.len() {
                    let masked =
                        effect == "concat.spin" && uv[index].iter().any(|v| *v < 0. || *v > 1.);
                    let value = if masked { 0. } else { actual[index][k] };
                    checked += 1;
                    if value.to_bits() != expected[index].to_bits() {
                        mismatches[side][k] += 1;
                    }
                }
            }
        }
    }
    println!("INSTRUMENTED unchanged sampler at GPU UV: {checked} channels; outgoing/incoming mismatch RGBA {mismatches:?}");
    assert!(checked > 0);
}
