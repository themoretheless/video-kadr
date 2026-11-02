use video_kadr_backend::domain::{
    composition::VideoEffect, concat_adjust::Adjust, flat_surface, group_surface::GroupSurface,
};
#[test]
fn retained_wheel_clocks_drive_one_adjust_pass_without_changing_authored_controls() {
    let base = Adjust {
        brightness: 12.,
        fade: 8.,
        gain: [0., 0., -0.8],
        luma: vec![[0., 0.1], [1., 0.9]],
        ..Default::default()
    };
    let animation:video_kadr_backend::domain::concat_color_wheels::Animation=serde_json::from_value(serde_json::json!({"gain.m":{"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":1000,"value":1.0}]}},"lift.x":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-0.4},{"tick":1000000,"value":0.4}]}}})).unwrap();
    let effect: VideoEffect = serde_json::from_value(
        serde_json::json!({"kind":"concat_adjust","parameters":base,"animation":animation}),
    )
    .unwrap();
    let mut actual = GroupSurface {
        width: 2,
        height: 1,
        rgba: vec![40, 120, 200, 255, 80, 170, 30, 128],
    };
    let mut expected = actual.clone();
    flat_surface::apply_effect_stack(&mut actual, &[effect], 500000, &[], &mut || Ok(())).unwrap();
    let mut sampled = base.clone();
    sampled.gain[2] = 0.5;
    sampled.lift[0] = 0.;
    let effect: VideoEffect =
        serde_json::from_value(serde_json::json!({"kind":"concat_adjust","parameters":sampled}))
            .unwrap();
    flat_surface::apply_effect_stack(&mut expected, &[effect], 0, &[], &mut || Ok(())).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(base.gain[2], -0.8);
    for channel in ["exposure", "luma.0.x", "unsupported"] {
        assert!(serde_json::from_value::<VideoEffect>(serde_json::json!({"kind":"concat_adjust","parameters":{},"animation":{channel:{"mode":"constant","value":0.2}}})).is_err());
    }
}
