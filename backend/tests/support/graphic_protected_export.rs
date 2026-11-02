use super::*;
#[tokio::test]
async fn protected_graphic_lossless_frames_follow_original_clock() {
    use video_kadr_backend::domain::composition::{CanvasSpec, Composition, TransformSpec};
    let fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/filmcraft-graphic-responsive-time.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 12672);
    let (state, _directory) = make_state(true, true).await;
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap().iter().filter(|c| {
        c["scale"] == 100000
            && c["meta"]["intro"] == 300000
            && c["meta"]["outro"] == 400000
            && c["meta"]["designDuration"] == 4000000
            && [1000000, 5000000].contains(&c["d"].as_i64().unwrap())
            && [0, 300000, 400000, 900000].contains(&c["u"].as_i64().unwrap())
    }) {
        let relative = case["u"].as_u64().unwrap();
        let duration = case["d"].as_u64().unwrap();
        let expected_clock = case["expected"].as_i64().unwrap();
        let mut canvas = CanvasSpec::default();
        canvas.width = 64;
        canvas.height = 48;
        canvas.fps_milli = 24000;
        let mut composition = Composition::new(canvas);
        composition.tracks=serde_json::from_value(json!([{"kind":"graphic","id":"graphics","name":"Protected","clips":[{"id":"graphic","timelineStartTick":1000000,"timelineEndTick":1000000+duration,"width":64,"height":48,"animationOffsetUs":250000,"responsiveTime":case["meta"],"graphic":{"layers":[{"uid":1,"content":{"kind":"shape","parameters":{"size":[24,16]},"animation":{"opacity":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":-1000000,"value":0},{"tick":4000000,"value":100}]}}}}}]},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0}}]}])).unwrap();
        composition.validate().unwrap();
        assert!(composition.sources.is_empty());
        let time = 1000000 + relative;
        let preview = video_kadr_backend::domain::flat_surface::render_generated_images(
            &composition,
            time,
            &mut || Ok(()),
            None,
        )
        .unwrap()
        .unwrap();
        let mut reference = serde_json::to_value(&composition).unwrap();
        let clip = &mut reference["tracks"][0]["clips"][0];
        clip.as_object_mut().unwrap().remove("responsiveTime");
        let content = &mut clip["graphic"]["layers"][0]["content"];
        content.as_object_mut().unwrap().remove("animation");
        content["parameters"]["opacity"] =
            json!(((expected_clock as f64 + 1000000.) / 5000000. * 100.).clamp(0., 100.));
        let reference: Composition = serde_json::from_value(reference).unwrap();
        let expected = video_kadr_backend::domain::flat_surface::render_generated_images(
            &reference,
            time,
            &mut || Ok(()),
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            preview.rgba, expected.rgba,
            "original clock {expected_clock} case{count}"
        );
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let movie = video_kadr_backend::tools::composition_native_video::prepare_native_video(
            &state.process_runtime,
            &state.outputs_dir(),
            &composition,
            &std::collections::BTreeMap::new(),
            time..time + 10000,
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
            decoded.stdout, preview.rgba,
            "decoded protected case{count}"
        );
        count += 1;
    }
    assert_eq!(count, 24);
}
