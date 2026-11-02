use super::{composition::VideoEffect, filmcraft_video_project};
#[test]
fn all_210_original_graphic_color_rasters_match_after_curve_import() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-color-import.json"
    ))
    .unwrap();
    assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 210);
    for (i, c) in cases.iter().enumerate() {
        let e = &c["effect"];
        let mut parameters = serde_json::Map::new();
        for (name, p) in e["params"].as_object().unwrap() {
            let mut upper = false;
            let name: String = name
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
            parameters.insert(
                name,
                if kind == "Vec2" {
                    serde_json::json!([value["x"], value["y"]])
                } else {
                    value.clone()
                },
            );
        }
        let native: VideoEffect =
            serde_json::from_value(serde_json::json!({"kind":e["effect"],"parameters":parameters}))
                .unwrap();
        let control = c["control"].as_str().unwrap();
        let native = filmcraft_video_project::with_source_color_animation(
            native,
            control,
            &e["params"][control],
        )
        .unwrap();
        let saved = serde_json::from_value(serde_json::to_value(&native).unwrap()).unwrap();
        assert_eq!(native, saved);
        let partition = filmcraft_video_project::partition_adapted_effects(vec![saved]).unwrap();
        assert!(partition.media_effects.is_empty());
        let at = c["at"].as_str().unwrap().parse().unwrap();
        let frozen = partition
            .graphic
            .snapshot_original_ticks((80, 64), at)
            .unwrap();
        let frame = frozen
            .render((80, 64), (80, 64), 0, [1., 0., 0., 1., 0., 0.], &mut || {
                Ok(())
            })
            .unwrap();
        let actual: Vec<u32> = frame
            .pixels()
            .iter()
            .flatten()
            .map(|v| v.to_bits())
            .collect();
        let expected: Vec<u32> = serde_json::from_value(c["bits"].clone()).unwrap();
        assert_eq!(
            actual, expected,
            "graphic color scene{i}:{}:{} at{at}",
            e["effect"], control
        );
    }
}
