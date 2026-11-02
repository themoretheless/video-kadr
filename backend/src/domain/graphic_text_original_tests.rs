//! Bit-exact comparison with pinned original rich text layer renderer.
use super::{
    graphic_shape_render::ShapeAppearance,
    graphic_text_layout::GraphicText,
    graphic_text_render::{raster_layer, TextLayer},
};
#[test]
fn original_rich_text_appearance_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/filmcraft-graphic-text.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 72);
    for (i, c) in cases.iter().enumerate() {
        let layer = TextLayer {
            enabled: true,
            text: serde_json::from_value::<GraphicText>(c["text"].clone()).unwrap(),
            appearance: serde_json::from_value::<ShapeAppearance>(c["appearance"].clone()).unwrap(),
            opacity: c["opacity"].as_f64().unwrap() as f32,
        };
        layer.text.validate().unwrap();
        let matrix = serde_json::from_value::<[f32; 6]>(c["matrix"].clone()).unwrap();
        let actual = raster_layer(&layer, &matrix, 128, 96);
        if c["raster"].is_null() {
            assert!(actual.is_none(), "case {i}");
            continue;
        }
        let r = actual.unwrap();
        let expected = &c["raster"];
        assert_eq!(
            (r.x, r.y, r.img.w, r.img.h),
            (
                expected["x"].as_i64().unwrap() as i32,
                expected["y"].as_i64().unwrap() as i32,
                expected["width"].as_u64().unwrap() as usize,
                expected["height"].as_u64().unwrap() as usize
            ),
            "bounds {i}"
        );
        let bits = expected["bits"].as_array().unwrap();
        assert_eq!(r.img.px.len(), bits.len());
        for (p, (a, b)) in r.img.px.iter().zip(bits).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.as_u64().unwrap() as u32,
                "case {i} component {p}"
            );
        }
    }
}
