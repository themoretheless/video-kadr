use video_kadr_backend::domain::concat_shape::{ShapeKind, ShapeStyle};
#[test]
fn all_original_figures_retain_style_and_produce_distinct_source_free_pixels() {
    let mut frames = Vec::new();
    for kind in [
        ShapeKind::Square,
        ShapeKind::Circle,
        ShapeKind::Triangle,
        ShapeKind::Parallelogram,
        ShapeKind::Trapezoid,
        ShapeKind::Line,
        ShapeKind::Arrow,
    ] {
        let style = ShapeStyle {
            kind,
            size: 0.5,
            fill: "#ff804080".into(),
            stroke: "#102030ff".into(),
            stroke_width: 0.025,
        };
        let wire = serde_json::to_value(&style).unwrap();
        assert_eq!(serde_json::from_value::<ShapeStyle>(wire).unwrap(), style);
        let frame = style.render((160, 120), &mut || Ok(())).unwrap();
        assert_eq!(frame.rgba.len(), 160 * 120 * 4);
        assert!(frame.rgba.chunks_exact(4).any(|p| p[3] > 0));
        assert!(frames.iter().all(|previous| previous != &frame.rgba));
        frames.push(frame.rgba);
    }
}
#[test]
fn centered_square_geometry_translucency_and_outline_are_preserved() {
    let style = ShapeStyle {
        size: 0.5,
        fill: "#ff000080".into(),
        stroke: String::new(),
        stroke_width: 0.,
        ..Default::default()
    };
    let frame = style.render((160, 120), &mut || Ok(())).unwrap();
    let pixel = |x: usize, y: usize| &frame.rgba[(y * 160 + x) * 4..(y * 160 + x + 1) * 4];
    assert_eq!(pixel(80, 60), &[255, 0, 0, 128]);
    assert_eq!(pixel(49, 60)[3], 0);
    assert_eq!(pixel(110, 60)[3], 0);
    assert_eq!(pixel(80, 29)[3], 0);
    assert_eq!(pixel(80, 90)[3], 0);
    let outline = ShapeStyle {
        fill: String::new(),
        stroke: "#00ff00ff".into(),
        stroke_width: 0.02,
        ..style
    };
    let outlined = outline.render((160, 120), &mut || Ok(())).unwrap();
    assert_eq!(outlined.rgba[(60 * 160 + 80) * 4 + 3], 0);
    assert!(outlined.rgba.chunks_exact(4).any(|p| p[1] > 0 && p[3] > 0));
}
#[test]
fn invalid_shape_input_canvas_and_cancelled_render_refuse() {
    for wire in [
        serde_json::json!({"size":0}),
        serde_json::json!({"size":2.1}),
        serde_json::json!({"strokeWidth":-0.1}),
        serde_json::json!({"fill":"red"}),
        serde_json::json!({"stroke":"#zzzzzz"}),
    ] {
        let style: ShapeStyle = serde_json::from_value(wire).unwrap();
        assert!(style.render((160, 120), &mut || Ok(())).is_err());
    }
    assert!(serde_json::from_value::<ShapeStyle>(serde_json::json!({"kind":"unknown"})).is_err());
    assert!(serde_json::from_value::<ShapeStyle>(serde_json::json!({"extra":true})).is_err());
    let style = ShapeStyle::default();
    assert!(style.render((0, 120), &mut || Ok(())).is_err());
    assert!(style.render((3841, 120), &mut || Ok(())).is_err());
    assert!(style
        .render((160, 120), &mut || anyhow::bail!("cancelled"))
        .is_err());
}
