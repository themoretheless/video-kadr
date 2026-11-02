#![cfg(feature = "concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{
    concat_qualification::spatial_transition, transition_spec::TransitionSpec,
};
fn spec(spread: f32) -> TransitionSpec {
    serde_json::from_value(
        json!({"effect":"concat.rgb-split-wipe","params":{"spread":{"Float":spread}}}),
    )
    .unwrap()
}
#[test]
fn rgb_split_opposes_source_channels_preserves_green_alpha_and_endpoints() {
    let gradient = |alpha| -> Vec<[f32; 4]> {
        (0..27)
            .map(|i| {
                let v = (i % 9) as f32 / 8.;
                [v, v, v, alpha]
            })
            .collect()
    };
    let a = gradient(0.5);
    let b = gradient(0.75);
    let black_a = vec![[0., 0., 0., 0.5]; 27];
    let black_b = vec![[0., 0., 0., 0.75]; 27];
    let outgoing = spatial_transition(&spec(100.), 9, 3, &a, &black_b, 0.5, false).unwrap()[13];
    assert!(outgoing[0] > 0.25 && outgoing[2] < 0.25);
    assert_eq!(outgoing[1], 0.25);
    assert_eq!(outgoing[3], 0.625);
    let incoming = spatial_transition(&spec(100.), 9, 3, &black_a, &b, 0.5, false).unwrap()[13];
    assert!(incoming[0] < 0.25 && incoming[2] > 0.25);
    assert_eq!(incoming[1], 0.25);
    assert_eq!(incoming[3], 0.625);
    assert_eq!(
        spatial_transition(&spec(0.), 9, 3, &a, &black_b, 0.5, false).unwrap()[13],
        [0.25, 0.25, 0.25, 0.625]
    );
    assert_eq!(
        spatial_transition(&spec(100.), 9, 3, &a, &b, 0., false).unwrap(),
        a
    );
    assert_eq!(
        spatial_transition(&spec(100.), 9, 3, &a, &b, 1., false).unwrap(),
        b
    );
    assert!(spec(-1.).validate().is_err());
    assert!(spec(101.).validate().is_err());
}

#[test]
#[ignore = "requires independently generated CONCAT_RGB_SPLIT_IMAGE_ORACLE"]
fn original_metal_full_spatial_rgba_bits() {
    use video_kadr_backend::domain::concat_effect_upgrade::{native_transition, AppliedFilter};
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_RGB_SPLIT_IMAGE_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(oracle["effect"], "concat.rgb-split-wipe");
    assert_eq!(oracle["fullImage"], true);
    assert_eq!(oracle["rendererBodyUnchanged"], true);
    assert_eq!(oracle["addedReadbackMethod"], true);
    assert_eq!(oracle["upstreamProbes"], 12);
    assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 384);
    let mut channels = 0;
    let mut mismatches = [0usize; 4];
    for c in cases {
        let width = c["width"].as_u64().unwrap() as usize;
        let height = c["height"].as_u64().unwrap() as usize;
        let from: Vec<[f32; 4]> = serde_json::from_value(c["from"].clone()).unwrap();
        let to: Vec<[f32; 4]> = serde_json::from_value(c["to"].clone()).unwrap();
        let expected: Vec<[f32; 4]> = serde_json::from_value(c["expected"].clone()).unwrap();
        assert_eq!(expected.len(), width * height);
        let link: AppliedFilter = serde_json::from_value(
            json!({"id":"concat.rgb-split-wipe","params":{"spread":c["spread"]}}),
        )
        .unwrap();
        let native = native_transition(&link).unwrap().unwrap();
        let actual = spatial_transition(
            &native.kernel.unwrap(),
            width,
            height,
            &from,
            &to,
            c["progress"].as_f64().unwrap() as f32,
            false,
        )
        .unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            for k in 0..4 {
                channels += 1;
                if a[k].to_bits() != b[k].to_bits() {
                    mismatches[k] += 1;
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
    println!("Original RGB Split full images: {channels} channels, mismatches RGBA {mismatches:?}");
    assert_eq!(mismatches, [0; 4]);
}
