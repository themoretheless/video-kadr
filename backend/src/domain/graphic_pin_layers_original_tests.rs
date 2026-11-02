use super::graphic_clip::{GraphicClip, GraphicContent, GraphicLayer};
#[test]
fn original_pinned_layer_stack_bounds_matrices_and_pixels() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-pin-layers.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 256);
    for (i, c) in cases.iter().enumerate() {
        let mut layers = vec![];
        for e in c["effects"].as_array().unwrap() {
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
                pin: e["layer"]
                    .get("pin")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()).unwrap()),
                uid: e["layer"]["uid"].as_u64().unwrap(),
                enabled: e["enabled"].as_bool().unwrap(),
                content,
            });
        }
        let graphic = GraphicClip { layers };
        let evaluated = graphic
            .evaluated_layers((64, 48), 0, &mut || Ok(()))
            .unwrap();
        for (index, layer) in evaluated.iter().enumerate() {
            let expected_bounds: [u64; 4] =
                serde_json::from_value(c["boundsBits"][index].clone()).unwrap();
            let expected_matrix: [u64; 6] =
                serde_json::from_value(c["matrixBits"][index].clone()).unwrap();
            assert_eq!(
                layer.bounds().unwrap().map(f64::to_bits),
                expected_bounds,
                "case{i} layer{index} bounds"
            );
            assert_eq!(
                layer.matrix.map(f64::to_bits),
                expected_matrix,
                "case{i} layer{index} matrix"
            );
        }
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
