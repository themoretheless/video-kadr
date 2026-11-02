use super::*;
fn authored_parameters(case: &Value) -> Value {
    let mut parameters = serde_json::Map::new();
    for (name, param) in case["effect"]["params"].as_object().unwrap() {
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
fn animated_effect(target: &Value) -> Value {
    let mut base = target.clone();
    let mut animation = serde_json::Map::new();
    let defaults =
        serde_json::to_value(video_kadr_backend::domain::graphic_shape::GraphicShape::default())
            .unwrap();
    for (group, axes, property) in [
        ("size", "XY", "size"),
        ("position", "XY", "position"),
        ("anchor", "XY", "anchor"),
        ("fill", "RGBA", "fillColor"),
        ("stroke", "RGBA", "strokeColor"),
        ("stroke2", "RGBA", "stroke2Color"),
        ("background", "RGBA", "backgroundColor"),
        ("shadow", "RGBA", "shadowColor"),
    ] {
        for (index, axis) in axes.chars().enumerate() {
            let value = target[property][index].as_f64().unwrap();
            let delta = if axes == "RGBA" {
                value.min(1. - value).min(0.01).max(0.)
            } else {
                0.25
            };
            animation.insert(format!("{group}{axis}"),json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-delta},{"tick":750,"value":value+delta}]}}));
        }
        base[property] = defaults[property].clone();
    }
    for (name, lo, hi) in [
        ("sides", 3., 64.),
        ("cornerRadius", 0., 10000.),
        ("strokeWidth", 0., 1000.),
        ("stroke2Width", 0., 1000.),
        ("backgroundOpacity", 0., 100.),
        ("backgroundSize", 0., 1000.),
        ("backgroundRadius", 0., 1000.),
        ("shadowOpacity", 0., 100.),
        ("shadowAngle", -1e20, 1e20),
        ("shadowDistance", 0., 1000.),
        ("shadowSize", 0., 1000.),
        ("shadowBlur", 0., 1000.),
        ("scale", 0., 100000.),
        ("scaleWidth", 0., 100000.),
        ("rotation", -1e20, 1e20),
        ("opacity", 0., 100.),
    ] {
        let value = target[name].as_f64().unwrap();
        let delta = (value - lo).min(hi - value).min(0.25).max(0.);
        animation.insert(name.into(),json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-delta},{"tick":750,"value":value+delta}]}}));
        base[name] = defaults[name].clone();
    }
    assert_eq!(animation.len(), 42);
    json!({"kind":"graphic_shape","parameters":base,"animation":animation})
}
#[tokio::test]
async fn image_graphic_shape_lossless_original_frames() {
    use video_kadr_backend::domain::composition::{
        CanvasSpec, Composition, SourceId, TransformSpec,
    };
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/filmcraft-graphic-shape.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 72);
    let mut count = 0;
    for (index, case) in cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c["enabled"] == true)
    {
        let (state, _directory) = make_state(true, true).await;
        image::RgbaImage::from_raw(64, 48, vec![0; 64 * 48 * 4])
            .unwrap()
            .save(state.sources_dir().join("main.png"))
            .unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"main","filename":"main.png","url":"/files/sources/main.png","mediaType":"image"}))).await);
        let mut canvas = CanvasSpec::default();
        canvas.width = 64;
        canvas.height = 48;
        canvas.fps_milli = 24000;
        let mut composition = Composition::new(canvas);
        composition.sources=serde_json::from_value(json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":64,"height":48,"hasAudio":false}})).unwrap();
        let parameters = authored_parameters(case);
        let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
        for animated in [false, true] {
            let effect = if animated {
                animated_effect(&parameters)
            } else {
                json!({"kind":"graphic_shape","parameters":parameters})
            };
            composition.tracks=serde_json::from_value(json!([{"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[effect]}]}])).unwrap();
            let inputs = composition
                .sources
                .keys()
                .map(|id| (id.clone(), state.sources_dir().join("main.png")))
                .collect();
            let mut decode = |_: &SourceId| {
                Ok(video_kadr_backend::domain::group_surface::GroupSurface {
                    width: 64,
                    height: 48,
                    rgba: vec![0; 64 * 48 * 4],
                })
            };
            let native = video_kadr_backend::domain::flat_surface::render_generated_images(
                &composition,
                1500000,
                &mut || Ok(()),
                Some(&mut decode),
            )
            .unwrap()
            .unwrap();
            let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
            let movie = video_kadr_backend::tools::composition_native_video::prepare_native_video(
                &state.process_runtime,
                &state.outputs_dir(),
                &composition,
                &inputs,
                1500000..1510000,
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
                64 * 48 * 4,
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
    assert_eq!(count, 72);
}
