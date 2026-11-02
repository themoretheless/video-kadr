use super::*;
fn authored_parameters(case: &Value) -> Value {
    let mut parameters = serde_json::Map::new();
    for (name, param) in case["params"].as_object().unwrap() {
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
            key,
            if kind == "Vec2" {
                json!([value["x"], value["y"]])
            } else {
                value.clone()
            },
        );
    }
    parameters.into()
}
#[tokio::test]
async fn rolled_graphic_track_lossless_original_frames() {
    use video_kadr_backend::domain::composition::{CanvasSpec, Composition, TransformSpec};
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/filmcraft-graphic-roll-layers.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 144);
    let mut count = 0;
    for (index, case) in cases.iter().enumerate() {
        let (state, _directory) = make_state(true, true).await;
        let mut canvas = CanvasSpec::default();
        canvas.width = 80;
        canvas.height = 64;
        canvas.fps_milli = 24000;
        let mut composition = Composition::new(canvas);
        let layers: Vec<Value> = case["effects"].as_array().unwrap().iter().map(|effect| json!({
            "uid": effect["layer"]["uid"], "pin": effect["layer"].get("pin").cloned().unwrap_or(Value::Null),
            "enabled": effect["enabled"], "content": {"kind": if effect["effect"] == "graphic_text" {"text"} else {"shape"}, "parameters": authored_parameters(effect)}
        })).collect();
        let mut roll = case["roll"].clone();
        for key in ["preroll", "easeIn", "easeOut", "postroll"] {
            roll[key] = json!(roll[key].as_i64().unwrap() * 100000);
        }
        let time = 1000000 + (case["u"].as_u64().unwrap() * 100000).min(999999);
        let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
        {
            let animated = false;
            composition.tracks=serde_json::from_value(json!([{"kind":"graphic","id":"graphics","name":"Graphics","clips":[{"id":"graphic","timelineStartTick":1000000,"timelineEndTick":2000000,"width":64,"height":48,"roll":roll,"graphic":{"layers":layers},"transform":TransformSpec{x:video_kadr_backend::domain::composition::AnimatableValue::constant(-8.),y:video_kadr_backend::domain::composition::AnimatableValue::constant(-8.),..Default::default()},"opacity":{"mode":"constant","value":1.0}}]}])).unwrap();
            composition.validate().unwrap();
            assert!(composition.sources.is_empty());
            let inputs = std::collections::BTreeMap::new();
            let native = video_kadr_backend::domain::flat_surface::render_generated_images(
                &composition,
                time,
                &mut || Ok(()),
                None,
            )
            .unwrap()
            .unwrap();
            let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
            let movie = video_kadr_backend::tools::composition_native_video::prepare_native_video(
                &state.process_runtime,
                &state.outputs_dir(),
                &composition,
                &inputs,
                time..(time + 10000).min(2000000),
                std::time::Duration::from_secs(30),
                &tokio_util::sync::CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
            let decoded = std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(movie.path())
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                decoded.status.success(),
                "{}",
                String::from_utf8_lossy(&decoded.stderr)
            );
            assert_eq!(
                decoded.stdout.len(),
                80 * 64 * 4,
                "frame count case{index} animated{animated}"
            );
            assert_eq!(
                decoded.stdout, native.rgba,
                "native/lossless case{index} animated{animated}"
            );
            for (pixel, bits) in decoded.stdout.chunks_exact(4).zip(expected.chunks_exact(4)) {
                let a = f32::from_bits(bits[3]);
                let alpha = (a * 255. + 1e-4).round().clamp(0., 255.);
                for c in 0..3 {
                    let expected = if a <= 1e-6 {
                        0
                    } else {
                        let v = (f32::from_bits(bits[c]) / a).max(0.);
                        let d = if v <= 0.0031308 {
                            v * 12.92
                        } else {
                            1.055 * v.powf(1. / 2.4) - 0.055
                        };
                        ((d * 255. + 1e-4).round().clamp(0., 255.) * alpha / 255.).round() as u8
                    };
                    assert!(
                        pixel[c].abs_diff(expected) <= 1,
                        "original case{index} animated{animated} channel{c}"
                    );
                }
                assert_eq!(pixel[3], 255);
            }
            count += 1;
        }
    }
    assert_eq!(count, 144);
}
