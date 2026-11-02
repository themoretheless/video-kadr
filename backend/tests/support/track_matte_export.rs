use super::*;
#[tokio::test]
async fn image_track_matte_external_lossless_original_frames() {
    use video_kadr_backend::domain::composition::{
        CanvasSpec, Composition, SourceId, TransformSpec,
    };
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/filmcraft-track-matte-composition.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    let mut count = 0;
    for (index, case) in cases.iter().enumerate() {
        let (state, _directory) = make_state(true, true).await;
        let main: Vec<u8> = serde_json::from_value(case["rgba"].clone()).unwrap();
        let control: Vec<u8> = serde_json::from_value(case["controlRgba"].clone()).unwrap();
        for (id, width, height, pixels) in [
            ("main", 18, 10, main.clone()),
            ("control", 6, 4, control.clone()),
        ] {
            let filename = format!("{id}.png");
            image::RgbaImage::from_raw(width, height, pixels)
                .unwrap()
                .save(state.sources_dir().join(&filename))
                .unwrap();
            assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"image"}))).await);
        }
        let mut canvas = CanvasSpec::default();
        canvas.width = 18;
        canvas.height = 10;
        canvas.fps_milli = 10000;
        let mut composition = Composition::new(canvas);
        composition.sources=serde_json::from_value(json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":18,"height":10,"hasAudio":false},"control":{"id":"control","kind":"image","durationTicks":0,"width":6,"height":4,"hasAudio":false}})).unwrap();
        let t = &case["transform"];
        let constant = |v: &Value| json!({"mode":"constant","value":v});
        let transform = json!({"x":constant(&t["x"]),"y":constant(&t["y"]),"scaleX":constant(&t["scaleX"]),"scaleY":constant(&t["scaleY"]),"rotationDegrees":constant(&t["rotation"]),"anchorX":0.5,"anchorY":0.5});
        composition.tracks=serde_json::from_value(json!([
          {"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"track_matte","parameters":case["parameters"]}]}]},
          {"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"control","timelineStartTick":1000000,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":0.7},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{"max":case["controlBlur"]}}]}]}
        ])).unwrap();
        let inputs = composition
            .sources
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    state.sources_dir().join(format!("{}.png", id.as_str())),
                )
            })
            .collect();
        let expected: Vec<u32> = serde_json::from_value(case["bits"].clone()).unwrap();
        for local in [0, 500000, 900000] {
            let start = 1000000 + local;
            let mut decode = |id: &SourceId| {
                Ok(if id.as_str() == "main" {
                    video_kadr_backend::domain::group_surface::GroupSurface {
                        width: 18,
                        height: 10,
                        rgba: main.clone(),
                    }
                } else {
                    video_kadr_backend::domain::group_surface::GroupSurface {
                        width: 6,
                        height: 4,
                        rgba: control.clone(),
                    }
                })
            };
            let native = video_kadr_backend::domain::flat_surface::render_generated_images(
                &composition,
                start,
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
                start..start + 10000,
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
                18 * 10 * 4,
                "frame count case{index} local{local}"
            );
            assert_eq!(
                decoded.stdout, native.rgba,
                "native/lossless case{index} local{local}"
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
                        "original case{index} local{local} channel{c}"
                    );
                }
                assert_eq!(pixel[3], 255);
            }
            count += 1;
        }
    }
    assert_eq!(count, 72);
}
