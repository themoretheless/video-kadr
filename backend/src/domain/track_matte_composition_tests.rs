//! Original Track Matte qualification with a rendered, effected hidden track.
use super::*;
#[test]
fn original_effected_track_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-track-matte-composition.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    for (index, c) in cases.iter().enumerate() {
        let mut canvas = super::super::composition::CanvasSpec::default();
        canvas.width = 18;
        canvas.height = 10;
        let mut composition = Composition::new(canvas);
        composition.sources = serde_json::from_value(serde_json::json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":18,"height":10,"hasAudio":false},"control":{"id":"control","kind":"image","durationTicks":0,"width":6,"height":4,"hasAudio":false}})).unwrap();
        let t = &c["transform"];
        let constant = |v: &serde_json::Value| serde_json::json!({"mode":"constant","value":v});
        let transform = serde_json::json!({"x":constant(&t["x"]),"y":constant(&t["y"]),"scaleX":constant(&t["scaleX"]),"scaleY":constant(&t["scaleY"]),"rotationDegrees":constant(&t["rotation"]),"anchorX":0.5,"anchorY":0.5});
        composition.tracks = serde_json::from_value(serde_json::json!([
          {"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":1000000,"durationTicks":1000000,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"track_matte","parameters":c["parameters"]}]}]},
          {"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"control","timelineStartTick":1000000,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":0.7},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{"max":c["controlBlur"]}}]}]}
        ])).unwrap();
        let main: Vec<u8> = serde_json::from_value(c["rgba"].clone()).unwrap();
        let control: Vec<u8> = serde_json::from_value(c["controlRgba"].clone()).unwrap();
        let mut decode = |id: &super::super::composition::SourceId| {
            Ok(if id.as_str() == "main" {
                GroupSurface {
                    width: 18,
                    height: 10,
                    rgba: main.clone(),
                }
            } else {
                GroupSurface {
                    width: 6,
                    height: 4,
                    rgba: control.clone(),
                }
            })
        };
        let mut context = CompositionEffectContext {
            exact_sample: None,
            temporal_clip: None,
            clip_duration_us: 1000000,
            composition: &composition,
            time_us: 1500000,
            depth: 0,
            library: &[],
            matrix: GroupAffine::IDENTITY,
            image: Some(&mut decode),
            video: None,
        };
        let mut child = GroupSurface {
            width: 18,
            height: 10,
            rgba: main.clone(),
        };
        let CompositionTrack::Image { clips, .. } = &composition.tracks[0] else {
            panic!("main")
        };
        let working = apply_effect_stack_retained(
            &mut child,
            &clips[0].effects,
            500000,
            &[],
            Some(&mut context),
            &mut || Ok(()),
        )
        .unwrap();
        let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
        assert_eq!(
            working
                .pixels()
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            expected,
            "original effected matte track {index}"
        );
    }
}
