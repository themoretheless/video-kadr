#![cfg(feature = "concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{
    concat_qualification::spatial_transition, transition_spec::TransitionSpec,
};
fn spec(effect: &str, direction: u8) -> TransitionSpec {
    serde_json::from_value(json!({"effect":effect,"params":{"direction":{"Choice":direction}}}))
        .unwrap()
}
#[test]
fn push_moves_both_sources_while_slide_keeps_outgoing_still() {
    let a: Vec<[f32; 4]> = (0..3)
        .flat_map(|y| (0..9).map(move |x| [x as f32 / 8., y as f32 / 2., 0., 0.5]))
        .collect();
    let b: Vec<[f32; 4]> = (0..3)
        .flat_map(|y| (0..9).map(move |x| [0., y as f32 / 2., x as f32 / 8., 0.75]))
        .collect();
    let push = spatial_transition(&spec("concat.push", 0), 9, 3, &a, &b, 0.5, false).unwrap();
    let slide = spatial_transition(&spec("concat.slide", 0), 9, 3, &a, &b, 0.5, false).unwrap();
    assert_eq!(push[10], [0.6875, 0.5, 0., 0.5]);
    assert_eq!(slide[10], [0., 0.5, 0.6875, 0.75]);
    assert_eq!(push[16], [0., 0.5, 0.3125, 0.75]);
    assert_eq!(slide[16], [0.875, 0.5, 0., 0.5]);
    for effect in ["concat.push", "concat.slide"] {
        let mut mids = Vec::new();
        for direction in 0..4 {
            let s = spec(effect, direction);
            s.validate().unwrap();
            assert_eq!(spatial_transition(&s, 9, 3, &a, &b, 0., false).unwrap(), a);
            assert_eq!(spatial_transition(&s, 9, 3, &a, &b, 1., false).unwrap(), b);
            let mid = spatial_transition(&s, 9, 3, &a, &b, 0.5, false).unwrap();
            assert_eq!(
                spatial_transition(&s, 9, 3, &b, &a, 0.5, true).unwrap(),
                mid
            );
            mids.push(mid);
        }
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(mids[i], mids[j]);
            }
        }
        assert!(spec(effect, 4).validate().is_err());
    }
}

#[test]
#[ignore = "requires independently generated CONCAT_MOVEMENT_GPU_ORACLE"]
fn original_metal_source_conversion_direction_and_selection() {
    use video_kadr_backend::domain::{
        concat_effect_upgrade::{native_transition, AppliedFilter},
        concat_qualification::constant_transition,
    };
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_MOVEMENT_GPU_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
    let effect = oracle["effect"].as_str().unwrap();
    assert!(matches!(effect, "concat.push" | "concat.slide"));
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        if oracle["independentValidation"].as_bool().unwrap_or(false) {
            1728
        } else {
            2160
        }
    );
    let mut mismatches = [0usize; 4];
    let mut channels = 0;
    let mut differences = Vec::new();
    for c in cases {
        let link: AppliedFilter =
            serde_json::from_value(json!({"id":effect,"params":{"direction":c["direction"]}}))
                .unwrap();
        let native = native_transition(&link).unwrap().unwrap();
        let kernel = native.kernel.unwrap();
        let side = c["side"].as_u64().unwrap() as usize;
        let from: [f32; 4] = serde_json::from_value(c["from"].clone()).unwrap();
        let to: [f32; 4] = serde_json::from_value(c["to"].clone()).unwrap();
        let pixels = constant_transition(
            &kernel,
            side,
            from,
            to,
            c["progress"].as_f64().unwrap() as f32,
            false,
        )
        .unwrap();
        let got = pixels[side / 2 * side + side / 2];
        let expected: [f32; 4] = serde_json::from_value(c["expected"].clone()).unwrap();
        if got
            .iter()
            .zip(expected)
            .any(|(a, b)| a.to_bits() != b.to_bits())
        {
            differences.push(json!({"case":c,"nativeKernel":kernel,"actual":got}));
        }
        for k in 0..4 {
            channels += 1;
            if got[k].to_bits() != expected[k].to_bits() {
                mismatches[k] += 1;
            }
        }
    }
    if let Ok(path) = std::env::var("CONCAT_MOVEMENT_DIFF_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&differences).unwrap()).unwrap();
    }
    println!("Original {effect}: {channels} channels, mismatch RGBA {mismatches:?}");
    assert_eq!(mismatches, [0; 4]);
}
