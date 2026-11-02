//! Regression for all graphic-shape animation channels and clip-local signed clocks.
use super::{
    graphic_shape::GraphicShape,
    graphic_shape_animation::{sample, validate_animation, GraphicShapeAnimation},
};
#[test]
fn all_channels_signed_clock_preserve_static_contract() {
    let values = serde_json::from_str::<serde_json::Value>(r#"{"sizeX":24.25,"sizeY":12.5,"positionX":10.125,"positionY":-7.75,"anchorX":1.25,"anchorY":-0.75,"fillR":0.2,"fillG":0.3,"fillB":0.4,"fillA":0.5,"strokeR":0.3,"strokeG":0.4,"strokeB":0.5,"strokeA":0.6,"stroke2R":0.4,"stroke2G":0.5,"stroke2B":0.6,"stroke2A":0.7,"backgroundR":0.5,"backgroundG":0.6,"backgroundB":0.7,"backgroundA":0.8,"shadowR":0.6,"shadowG":0.7,"shadowB":0.8,"shadowA":0.9,"sides":7.5,"cornerRadius":2.5,"strokeWidth":1.25,"stroke2Width":3.5,"backgroundOpacity":40.0,"backgroundSize":3.25,"backgroundRadius":2.75,"shadowOpacity":60.0,"shadowAngle":123.25,"shadowDistance":4.5,"shadowSize":1.75,"shadowBlur":5.5,"scale":75.0,"scaleWidth":125.0,"rotation":-17.25,"opacity":42.5}"#).unwrap();
    let animation:GraphicShapeAnimation=serde_json::from_value(serde_json::Value::Object(values.as_object().unwrap().iter().map(|(key,value)|{
  let value=value.as_f64().unwrap();let delta=if value>0.&&value<1.{0.01}else{0.25};(key.clone(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":-250,"value":value-delta},{"tick":250,"value":value+delta}]}}))
 }).collect())).unwrap();
    assert_eq!(animation.len(), 42);
    validate_animation(&animation).unwrap();
    assert_eq!(
        animation,
        serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap()
    );
    let base = GraphicShape {
        shape: 3,
        points: vec![[-2., -1.], [2., -1.], [0., 1.]],
        uniform_scale: false,
        stroke: true,
        stroke2: true,
        background: true,
        shadow: true,
        ..Default::default()
    };
    let expected = GraphicShape {
        size: [24.25, 12.5],
        position: Some([10.125, -7.75]),
        anchor: [1.25, -0.75],
        fill_color: [0.2, 0.3, 0.4, 0.5],
        stroke_color: [0.3, 0.4, 0.5, 0.6],
        stroke2_color: [0.4, 0.5, 0.6, 0.7],
        background_color: [0.5, 0.6, 0.7, 0.8],
        shadow_color: [0.6, 0.7, 0.8, 0.9],
        sides: 7.5,
        corner_radius: 2.5,
        stroke_width: 1.25,
        stroke2_width: 3.5,
        background_opacity: 40.,
        background_size: 3.25,
        background_radius: 2.75,
        shadow_opacity: 60.,
        shadow_angle: 123.25,
        shadow_distance: 4.5,
        shadow_size: 1.75,
        shadow_blur: 5.5,
        scale: 75.,
        scale_width: 125.,
        rotation: -17.25,
        opacity: 42.5,
        ..base.clone()
    };
    assert_eq!(
        sample(base.clone(), &animation, 0, (96, 48)).unwrap(),
        expected
    );
    let precise = 1.23456789012345;
    let animation: GraphicShapeAnimation = serde_json::from_value(
        serde_json::json!({"positionX":{"mode":"constant","value":precise}}),
    )
    .unwrap();
    assert_eq!(
        sample(base, &animation, 0, (96, 48)).unwrap().position,
        Some([precise, 24.])
    );
    for (channel, value) in [
        ("sides", 2.),
        ("fillA", 1.1),
        ("scale", 100001.),
        ("shadowBlur", 1001.),
        ("opacity", -0.1),
    ] {
        let invalid: GraphicShapeAnimation =
            serde_json::from_value(serde_json::json!({channel:{"mode":"constant","value":value}}))
                .unwrap();
        assert!(validate_animation(&invalid).is_err());
    }
}
