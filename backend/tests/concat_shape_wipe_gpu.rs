//! Independent original Metal output qualification; generated data remain ignored.
#![cfg(feature = "concat-qualification")]
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    concat_qualification::constant_transition, transition_spec::TransitionSpec,
};
#[test]
#[ignore = "requires independently generated CONCAT_SHAPE_WIPE_GPU_ORACLE"]
fn original_metal_all_shapes_and_softness_bits() {
    let oracle: Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_SHAPE_WIPE_GPU_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(oracle["effect"], "concat.shape-wipe");
    assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2592);
    let mut mismatches = [[0usize; 4]; 3];
    let mut counts = [0usize; 3];
    for case in cases {
        let side = case["side"].as_u64().unwrap() as usize;
        let shape = case["shape"].as_u64().unwrap() as usize;
        assert!(shape < 3);
        let spec:TransitionSpec=serde_json::from_value(json!({"effect":"concat.shape-wipe","params":{"shape":{"Choice":shape},"softness":{"Float":case["softness"]}}})).unwrap();
        spec.validate().unwrap();
        let from: [f32; 4] = serde_json::from_value(case["from"].clone()).unwrap();
        let to: [f32; 4] = serde_json::from_value(case["to"].clone()).unwrap();
        let result = constant_transition(
            &spec,
            side,
            from,
            to,
            case["progress"].as_f64().unwrap() as f32,
            false,
        )
        .unwrap();
        let got = result[side / 2 * side + side / 2];
        let expected: [f32; 4] = serde_json::from_value(case["expected"].clone()).unwrap();
        for k in 0..4 {
            counts[shape] += 1;
            if got[k].to_bits() != expected[k].to_bits() {
                mismatches[shape][k] += 1;
            }
        }
    }
    assert_eq!(counts, [3456; 3]);
    println!("Original Shape Wipe: channels by shape {counts:?}; RGBA mismatches circle/heart/star {mismatches:?}");
    assert_eq!(mismatches, [[0; 4]; 3]);
}
