use super::graphic_clip::{GraphicClip, GraphicContent, GraphicLayer};
#[test]
fn original_source_free_layer_stack_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-clip.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    for (i, c) in cases.iter().enumerate() {
        let mut layers = vec![];
        for (index, e) in c["effects"].as_array().unwrap().iter().enumerate() {
            let mut params = serde_json::Map::new();
            for (name, p) in e["params"].as_object().unwrap() {
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
            let content = if e["effect"] == "graphic_text" {
                GraphicContent::Text {
                    parameters: serde_json::from_value(params.into()).unwrap(),
                    animation: None,
                    source_text_animation: None,
                }
            } else {
                GraphicContent::Shape {
                    parameters: serde_json::from_value(params.into()).unwrap(),
                    animation: None,
                }
            };
            layers.push(GraphicLayer {
                original_parameters: None,
                pin: None,
                uid: index as u64 + 1,
                enabled: e["enabled"].as_bool().unwrap(),
                content,
            });
        }
        let graphic = GraphicClip { layers };
        let prefix = serde_json::from_value::<[f64; 6]>(c["prefix"].clone()).unwrap();
        let frame = graphic
            .render((64, 48), (80, 64), 0, prefix, &mut || Ok(()))
            .unwrap();
        let expected = c["bits"].as_array().unwrap();
        assert_eq!(expected.len(), frame.pixels().len() * 4);
        for (index, (actual, expected)) in frame.pixels().iter().flatten().zip(expected).enumerate()
        {
            assert_eq!(
                actual.to_bits(),
                expected.as_u64().unwrap() as u32,
                "case{i}component{index}"
            );
        }
    }
}
