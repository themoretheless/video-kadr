use super::{graphic_shape::GraphicShape, graphic_text::GraphicTextParameters};
#[test]
fn negative_layer_time_samples_numeric_and_held_text_keys() {
    let envelope = serde_json::json!({"opacity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":-1000000,"value":20},{"tick":1000000,"value":80}]}}});
    let shape = super::graphic_shape_animation::sample_signed(
        GraphicShape::default(),
        &serde_json::from_value(envelope.clone()).unwrap(),
        -500000,
        (64, 48),
    )
    .unwrap();
    let text = super::graphic_text_animation::sample_signed(
        GraphicTextParameters::default(),
        &serde_json::from_value(envelope).unwrap(),
        -500000,
        (64, 48),
    )
    .unwrap();
    assert_eq!(shape.opacity, 35.);
    assert_eq!(text.opacity, 35.);
    let held:super::graphic_source_text_animation::SourceTextAnimation=serde_json::from_value(serde_json::json!({"timeBase":3,"keyframes":[{"tick":-3,"value":"Привет"},{"tick":-1,"value":"مرحبا"},{"tick":1,"value":"Final"}]})).unwrap();
    assert_eq!(held.sample_signed(-333334).unwrap(), "Привет");
    assert_eq!(held.sample_signed(-333333).unwrap(), "مرحبا");
    assert!(held.sample_signed(i128::MAX).is_err());
}
#[test]
fn timeline_protection_maps_layers_but_preserves_ordinary_effect_clock() {
    use super::composition::{AnimatableValue, TransformSpec};
    use super::graphic_timeline::GraphicTimelineClip;
    let clip:GraphicTimelineClip=serde_json::from_value(serde_json::json!({
        "id":"protected", "timelineStartTick":1000000,"timelineEndTick":5000000,
        "animationOffsetUs":250000,"width":64,"height":48,
        "responsiveTime":{"intro":1000000,"outro":500000,"designIn":-500000,"designDuration":2000000},
        "graphic":{"layers":[{"uid":1,"content":{"kind":"shape","parameters":{"size":[24,16]},"animation":{"opacity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":-1000000,"value":20},{"tick":1000000,"value":80}]}}}}}]},
        "transform":TransformSpec::default(),"opacity":AnimatableValue::constant(1.),
        "effects":[{"kind":"graphic_shape","parameters":{"size":[4,4]}}]
    })).unwrap();
    assert_eq!(clip.layer_time(1000000).unwrap(), Some(-500000));
    assert_eq!(clip.layer_time(2000000).unwrap(), Some(500000));
    assert_eq!(clip.layer_time(4500000).unwrap(), Some(1000000));
    let mut effect_time = None;
    let frame = clip
        .render_timeline(
            (64, 48),
            1000000,
            &mut |_, _, local, _| {
                effect_time = Some(local);
                Ok(())
            },
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
    assert_eq!(effect_time, Some(250000));
    let expected = clip
        .graphic
        .render_signed(
            (64, 48),
            (64, 48),
            -500000,
            [1., 0., 0., 1., 0., 0.],
            &mut || Ok(()),
        )
        .unwrap();
    assert_eq!(frame.pixels(), expected.pixels());
}
