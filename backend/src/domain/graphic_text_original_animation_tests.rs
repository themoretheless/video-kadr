//! Original graphic-layer parameter interpolation on a translated signed clock.
use super::{
    graphic_shape_render::ShapeAppearance,
    graphic_source_text_animation::{SourceTextAnimation, SourceTextKey},
    graphic_text::GraphicTextParameters,
    graphic_text_animation::{sample, GraphicTextAnimation},
    graphic_text_layout::GraphicText,
};
#[test]
fn original_animated_text_layer_and_matrix() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-text-animated.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 162);
    for (index, c) in cases.iter().enumerate() {
        let mut parameters = serde_json::Map::new();
        let mut animation = serde_json::Map::new();
        let mut source_text = None;
        for (name, param) in c["effect"]["params"].as_object().unwrap() {
            let mut upper = false;
            let key: String = name
                .chars()
                .filter_map(|ch| {
                    if ch == '_' {
                        upper = true;
                        None
                    } else {
                        let c = if upper { ch.to_ascii_uppercase() } else { ch };
                        upper = false;
                        Some(c)
                    }
                })
                .collect();
            let (kind, value) = param["value"].as_object().unwrap().iter().next().unwrap();
            parameters.insert(
                key.clone(),
                if kind == "Vec2" {
                    serde_json::json!([value["x"], value["y"]])
                } else {
                    value.clone()
                },
            );
            if let Some(keys) = param["keyframes"].as_array() {
                let endpoints: Vec<&serde_json::Value> =
                    keys.iter().map(|k| &k["value"][kind]).collect();
                assert_eq!(endpoints.len(), 2);
                let mut insert = |channel: String, a: serde_json::Value, b: serde_json::Value| {
                    animation.insert(channel,serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":1500,"value":b}]}}));
                };
                match kind.as_str() {
                    "Float" => insert(key, endpoints[0].clone(), endpoints[1].clone()),
                    "Vec2" => {
                        for (axis, property) in [("X", "x"), ("Y", "y")] {
                            insert(
                                format!("{key}{axis}"),
                                endpoints[0][property].clone(),
                                endpoints[1][property].clone(),
                            );
                        }
                    }
                    "Color" => {
                        let group = key.strip_suffix("Color").unwrap();
                        for (i, axis) in "RGBA".chars().enumerate() {
                            insert(
                                format!("{group}{axis}"),
                                endpoints[0][i].clone(),
                                endpoints[1][i].clone(),
                            );
                        }
                    }
                    "Text" => {
                        source_text = Some(SourceTextAnimation {
                            time_base: 1000,
                            keyframes: vec![
                                SourceTextKey {
                                    tick: 0,
                                    value: endpoints[0].as_str().unwrap().into(),
                                },
                                SourceTextKey {
                                    tick: 1500,
                                    value: endpoints[1].as_str().unwrap().into(),
                                },
                            ],
                        });
                    }
                    _ => panic!("unexpected original animation type"),
                }
            }
        }
        let base: GraphicTextParameters = serde_json::from_value(parameters.into()).unwrap();
        let animation: GraphicTextAnimation = serde_json::from_value(animation.into()).unwrap();
        assert_eq!(animation.len(), 44);
        // Translate both authored keys and sample by250ms; host clip-local ticks stay nonnegative.
        let local = ((c["seconds"].as_f64().unwrap() + 0.25) * 1e6).round() as u64;
        let mut parameters = sample(base, &animation, local, (64, 48)).unwrap();
        parameters.text = source_text.unwrap().sample(local).unwrap().into();
        let (layer, matrix) = parameters
            .evaluate(c["enabled"].as_bool().unwrap(), (64, 48))
            .unwrap();
        let expected = &c["evaluated"]["layer"];
        assert_eq!(
            layer.text,
            serde_json::from_value::<GraphicText>(expected["text"].clone()).unwrap(),
            "text {index}"
        );
        assert_eq!(
            layer.appearance,
            serde_json::from_value::<ShapeAppearance>(expected["appearance"].clone()).unwrap(),
            "appearance {index}"
        );
        assert_eq!(layer.opacity, expected["opacity"].as_f64().unwrap() as f32);
        assert_eq!(layer.enabled, expected["enabled"].as_bool().unwrap());
        let expected_matrix: filmcraft_text::Xform =
            serde_json::from_value(c["evaluated"]["matrix"].clone()).unwrap();
        assert_eq!(matrix, expected_matrix, "original animated matrix{index}");
    }
}
