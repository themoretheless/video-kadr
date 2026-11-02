//! Imported original .fcgt layers retain their authored tick clock and exact renderer output.
use super::graphic_templates::{builtin_templates, from_original};
use serde_json::{json, Value};
#[test]
fn imported_animated_shapes_and_text_match_original_pixels() {
    let catalog: Value =
        serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
    let fixtures = [
        include_str!("../../tests/fixtures/filmcraft-graphic-shape-animated.json"),
        include_str!("../../tests/fixtures/filmcraft-graphic-text-animated.json"),
    ];
    let mut count = 0;
    for fixture in fixtures {
        let fixture: Value = serde_json::from_str(fixture).unwrap();
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let mut raw = catalog["cases"][0].clone();
            let mut effect = case["effect"].clone();
            effect["layer"] = json!({"uid":1});
            raw["layers"] = json!([effect]);
            raw["controls"] = json!([]);
            raw["canvas"] = json!([64, 48]);
            raw["graphic"] = json!({});
            let template = from_original(&raw).unwrap();
            assert!(template.graphic.layers[0].original_parameters.is_some());
            let local = (case["seconds"].as_f64().unwrap() * 1000000.0).round() as i128;
            let frame = template
                .graphic
                .render_signed(
                    (64, 48),
                    (64, 48),
                    local,
                    [1., 0., 0., 1., 0., 0.],
                    &mut || Ok(()),
                )
                .unwrap();
            let exported = super::graphic_template_export::export_native(
                &serde_json::to_value(&template).unwrap(),
                0,
            )
            .unwrap();
            super::graphic_template_file::GraphicTemplateFile::read(
                &serde_json::to_vec(&exported).unwrap(),
            )
            .unwrap();
            let restored = from_original(&exported).unwrap();
            let restored_frame = restored
                .graphic
                .render_signed(
                    (64, 48),
                    (64, 48),
                    local,
                    [1., 0., 0., 1., 0., 0.],
                    &mut || Ok(()),
                )
                .unwrap();
            assert_eq!(
                restored_frame.pixels(),
                frame.pixels(),
                "exported restored case{index}"
            );
            let expected = case["bits"].as_array().unwrap();
            assert_eq!(expected.len(), frame.pixels().len() * 4);
            for (component, (actual, expected)) in
                frame.pixels().iter().flatten().zip(expected).enumerate()
            {
                assert_eq!(
                    actual.to_bits(),
                    expected.as_u64().unwrap() as u32,
                    "case{index}component{component}"
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 378);
    assert_eq!(builtin_templates().unwrap().len(), 8);
}

#[test]
fn imported_template_control_edit_inserts_key_at_signed_local_clock() {
    let catalog: Value =
        serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
    let mut raw = catalog["cases"][0].clone();
    raw["controls"] = json!([{"id":"opacity","name":"Opacity","kind":"slider","layer":1,"param":"opacity","min":0,"max":100}]);
    raw["layers"][0]["params"]["opacity"]["keyframes"] = json!([{"time":-254016000000i64,"value":{"Float":0.0},"interp":"Linear"},{"time":254016000000i64,"value":{"Float":100.0},"interp":"Hold"}]);
    let template = from_original(&raw).unwrap();
    assert_eq!(
        template.control_values_at(0).unwrap()["opacity"],
        json!(50.0)
    );
    let edited = template
        .set_control_at("OPACITY", json!(42.0), -500000)
        .unwrap();
    assert_eq!(
        edited.control_values_at(-500000).unwrap()["opacity"],
        json!(42.0)
    );
    let keys = &edited.graphic.layers[0]
        .original_parameters
        .as_ref()
        .unwrap()["opacity"]["keyframes"];
    assert_eq!(keys.as_array().unwrap().len(), 3);
    assert_eq!(keys[1]["time"], json!(-127008000000i64));
    assert_eq!(keys[0]["value"], json!({"Float":0.0}));
    assert_eq!(keys[2]["value"], json!({"Float":100.0}));
    assert_eq!(
        template.control_values_at(0).unwrap()["opacity"],
        json!(50.0)
    );
}

#[test]
fn imported_embedded_font_resources_survive_native_project_metadata_roundtrip() {
    let catalog: Value =
        serde_json::from_str(include_str!("data/filmcraft-graphic-templates.json")).unwrap();
    let mut raw = catalog["cases"][0].clone();
    raw["resources"] = json!({"fonts":[{"family":"Test","style":"Regular","license":"OFL-1.1","file":"test.ttf","data":"TWFu"}]});
    let template = from_original(&raw).unwrap();
    assert_eq!(
        serde_json::to_value(&template.resources).unwrap(),
        raw["resources"]
    );
    let metadata = super::graphic_templates::GraphicTemplateMetadata::from(&template);
    metadata.validate().unwrap();
    let restored: super::graphic_templates::GraphicTemplateMetadata =
        serde_json::from_slice(&serde_json::to_vec(&metadata).unwrap()).unwrap();
    assert_eq!(restored.resources, template.resources);
}

#[test]
fn all_builtin_native_graphics_export_and_restore_without_content_loss() {
    for template in builtin_templates().unwrap() {
        let document = super::graphic_template_export::export_native(
            &serde_json::to_value(&template).unwrap(),
            0,
        )
        .unwrap();
        super::graphic_template_file::GraphicTemplateFile::read(
            &serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        let restored = from_original(&document).unwrap();
        assert_eq!(restored.graphic, template.graphic);
        assert_eq!(restored.roll, template.roll);
        assert_eq!(restored.responsive_time, template.responsive_time);
    }
}
