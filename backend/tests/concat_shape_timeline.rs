use serde_json::json;
use video_kadr_backend::domain::{concat_shape::ShapeKind, graphic_timeline::GraphicTimelineClip};
fn clip(kind: &str) -> GraphicTimelineClip {
    serde_json::from_value(json!({"id":"shape","timelineStartTick":1000000,"timelineEndTick":3000000,"width":160,"height":120,"graphic":{"layers":[]},"concatShape":{"kind":kind,"size":0.5,"fill":"#ff804080","stroke":"#102030ff","strokeWidth":0.025},"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"enabled":true})).unwrap()
}
fn pixels(clip: &GraphicTimelineClip, time: u64) -> Option<Vec<u8>> {
    clip.render_timeline((160, 120), time, &mut |_, _, _, _| Ok(()), &mut || Ok(()))
        .unwrap()
        .map(|frame| frame.to_surface(&mut || Ok(())).unwrap().rgba)
}
#[test]
fn retained_shapes_render_in_graphic_timeline_with_exact_visibility_and_roundtrip() {
    for name in [
        "square",
        "circle",
        "triangle",
        "parallelogram",
        "trapezoid",
        "line",
        "arrow",
    ] {
        let clip = clip(name);
        clip.validate().unwrap();
        let raw = clip
            .concat_shape
            .as_ref()
            .unwrap()
            .render((160, 120), &mut || Ok(()))
            .unwrap();
        let actual = pixels(&clip, 1500000).unwrap();
        assert!(actual
            .iter()
            .zip(&raw.rgba)
            .all(|(a, b)| a.abs_diff(*b) <= 1));
        assert!(pixels(&clip, 999999).is_none());
        assert!(pixels(&clip, 3000000).is_none());
        let restored: GraphicTimelineClip =
            serde_json::from_value(serde_json::to_value(&clip).unwrap()).unwrap();
        assert_eq!(restored, clip);
        assert_eq!(pixels(&restored, 1500000), Some(actual));
    }
}
#[test]
fn shape_pose_opacity_and_effect_clock_reach_the_timeline_renderer() {
    let mut clip = clip("arrow");
    let original = pixels(&clip, 1500000).unwrap();
    clip.opacity = serde_json::from_value(json!({"mode":"constant","value":0.4})).unwrap();
    clip.transform.rotation_degrees =
        serde_json::from_value(json!({"mode":"constant","value":30})).unwrap();
    assert_ne!(pixels(&clip, 1500000).unwrap(), original);
    clip.animation_offset_us = 250000;
    let mut called = false;
    clip.render_timeline(
        (160, 120),
        1500000,
        &mut |frame, effects, local, _| {
            assert_eq!(local, 750000);
            assert!(effects.is_empty());
            assert_eq!(frame.dimensions(), (160, 120));
            called = true;
            Ok(())
        },
        &mut || Ok(()),
    )
    .unwrap();
    assert!(called);
}
#[test]
fn retained_shape_never_silently_replaces_an_existing_graphic_stack() {
    use video_kadr_backend::domain::{
        graphic_clip::{GraphicContent, GraphicLayer},
        graphic_shape::GraphicShape,
    };
    let mut clip = clip("circle");
    clip.graphic.layers.push(GraphicLayer {
        uid: 1,
        enabled: true,
        original_parameters: None,
        pin: None,
        content: GraphicContent::Shape {
            parameters: GraphicShape::default(),
            animation: None,
        },
    });
    assert!(clip.validate().is_err());
    assert_eq!(clip.concat_shape.as_ref().unwrap().kind, ShapeKind::Circle);
    clip.concat_shape = None;
    clip.validate().unwrap();
    assert!(serde_json::to_value(&clip)
        .unwrap()
        .get("concatShape")
        .is_none());
}

#[test]
fn source_free_graphic_composition_persists_and_renders_the_retained_shape() {
    use video_kadr_backend::domain::{composition::Composition, flat_surface};
    let document = json!({"schemaVersion":1,"timeBase":1000000,"sources":{},"canvas":{"width":160,"height":120,"fpsMilli":30000,"background":{"red":0,"green":0,"blue":0,"alpha":1}},"tracks":[{"kind":"graphic","id":"shapes","name":"Shapes","hidden":false,"locked":false,"clips":[clip("triangle")]}]});
    let composition: Composition = serde_json::from_value(document).unwrap();
    let frame = flat_surface::render_generated(&composition, 1500000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    let before = flat_surface::render_generated(&composition, 500000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    let after = flat_surface::render_generated(&composition, 3000000, &mut || Ok(()))
        .unwrap()
        .unwrap();
    assert_ne!(frame.rgba, before.rgba);
    assert_eq!(before.rgba, after.rgba);
    let restored: Composition =
        serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();
    assert_eq!(
        flat_surface::render_generated(&restored, 1500000, &mut || Ok(()))
            .unwrap()
            .unwrap()
            .rgba,
        frame.rgba
    );
}
