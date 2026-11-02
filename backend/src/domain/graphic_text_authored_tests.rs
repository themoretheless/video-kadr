use super::{
    graphic_shape_render::ShapeAppearance, graphic_text::GraphicTextParameters,
    graphic_text_layout::GraphicText,
};
#[test]
fn original_authored_text_layer_evaluation() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-text-authored.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for (i, c) in cases.iter().enumerate() {
        let mut params = serde_json::Map::new();
        for (name, p) in c["effect"]["params"].as_object().unwrap() {
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
            let (kind, value) = p["value"].as_object().unwrap().iter().next().unwrap();
            params.insert(
                key,
                if kind == "Vec2" {
                    serde_json::json!([value["x"], value["y"]])
                } else {
                    value.clone()
                },
            );
        }
        let p: GraphicTextParameters = serde_json::from_value(params.into()).unwrap();
        let (layer, matrix) = p
            .evaluate(c["enabled"].as_bool().unwrap(), (64, 48))
            .unwrap();
        let expected = &c["evaluated"];
        assert_eq!(
            layer.enabled,
            expected["layer"]["enabled"].as_bool().unwrap()
        );
        assert_eq!(
            layer.text,
            serde_json::from_value::<GraphicText>(expected["layer"]["text"].clone()).unwrap(),
            "text {i}"
        );
        assert_eq!(
            layer.appearance,
            serde_json::from_value::<ShapeAppearance>(expected["layer"]["appearance"].clone())
                .unwrap(),
            "appearance {i}"
        );
        assert_eq!(
            layer.opacity,
            expected["layer"]["opacity"].as_f64().unwrap() as f32
        );
        assert_eq!(
            matrix,
            serde_json::from_value::<[f32; 6]>(expected["matrix"].clone()).unwrap(),
            "matrix {i}"
        );
    }
}
