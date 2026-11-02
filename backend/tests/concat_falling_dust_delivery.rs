mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition, TransformSpec},
    library::MediaEntry,
};
fn authored_parameters(c: &Value) -> Value {
    let mut v = c["parameters"].clone();
    v.as_object_mut().unwrap().remove("time");
    v
}
fn command(args: &[&str]) -> Vec<u8> {
    let out = std::process::Command::new("ffmpeg")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
async fn terminal(app: &axum::Router, id: &str) -> Value {
    for _ in 0..3000 {
        let (status, body, _) = support::send(app, support::get(&format!("/api/jobs/{id}"))).await;
        assert_eq!(status, StatusCode::OK);
        if matches!(
            body["status"].as_str(),
            Some("done" | "error" | "cancelled")
        ) {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job did not finish")
}
#[tokio::test]
#[ignore = "requires actual FFmpeg/FFprobe/libx264 and ordinary background export worker"]
async fn ordinary_falling_dust_delivery_uses_native_effect_and_selected_clip_clock() {
    let (mut state, _directory) = support::make_state(true, false).await;
    assert!(String::from_utf8(command(&["-hide_banner", "-encoders"]))
        .unwrap()
        .contains("pcm_s16le"));
    assert!(String::from_utf8(command(&["-hide_banner", "-muxers"]))
        .unwrap()
        .contains("wav"));
    let capabilities = std::sync::Arc::make_mut(&mut state.tools);
    capabilities.ffmpeg_encoders.push("pcm_s16le".into());
    capabilities.ffmpeg_muxers.push("wav".into());
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/concat-falling-dust-opaque-spatial.json"
    ))
    .unwrap();
    assert_eq!(
        oracle["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(oracle["opaqueInput"], true);
    assert_eq!(oracle["upstreamProbes"], 3);
    let case = |seed| {
        oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["side"] == 16
                    && c["seed"] == seed
                    && c["parameters"]["time"] == 1.25
                    && c["parameters"]["dust"] == 75
                    && c["parameters"]["leak"] == 80
            })
            .unwrap()
    };
    let mut input = vec![];
    for seed in 0..2 {
        let mut frame: Vec<u8> = serde_json::from_value(case(seed)["inputRgba"].clone()).unwrap();
        for p in frame.chunks_exact_mut(4) {
            p[3] = 255;
        }
        input.extend(frame);
    }
    let raw = state.sources_dir().join("falling_dust-delivery.rgba");
    std::fs::write(&raw, &input).unwrap();
    let movie = state.sources_dir().join("falling_dust-delivery.mov");
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
        "16x16",
        "-r",
        "4",
        "-i",
        raw.to_str().unwrap(),
        "-f",
        "lavfi",
        "-i",
        "aevalsrc=0.2*sin(2*PI*(440+440*gte(t\\,0.25))*t):s=48000:d=0.5",
        "-c:a",
        "pcm_s16le",
        "-c:v",
        "qtrle",
        "-pix_fmt",
        "argb",
        "-threads",
        "1",
        movie.to_str().unwrap(),
    ]);
    state.library.add(MediaEntry::from_result("source",&json!({"id":"falling_dust-source","filename":"falling_dust-delivery.mov","url":"/files/sources/falling_dust-delivery.mov","mediaType":"video","width":16,"height":16,"duration":0.5,"fps":4,"vcodec":"qtrle","hasAudio":true,"audioChannels":1}))).await;
    let expected: Vec<u8> = serde_json::from_value(case(1)["expectedRgba"].clone()).unwrap();
    let target: Vec<u8> = expected
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter().copied())
        .collect();
    assert_ne!(case(0)["expectedRgba"], case(1)["expectedRgba"]);
    // Film's original RGB signal is independent of alpha; use opaque delivery to isolate H264 conversion.
    let reference_raw = state.outputs_dir().join("falling_dust-original.rgb");
    std::fs::write(&reference_raw, &target).unwrap();
    let reference_movie = state.outputs_dir().join("falling_dust-original.mp4");
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "-s",
        "16x16",
        "-r",
        "4",
        "-i",
        reference_raw.to_str().unwrap(),
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "23",
        "-pix_fmt",
        "yuv420p",
        reference_movie.to_str().unwrap(),
    ]);
    let decode = |path: &Path| {
        command(&[
            "-nostdin",
            "-v",
            "error",
            "-i",
            path.to_str().unwrap(),
            "-map",
            "0:v:0",
            "-an",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
    };
    let reference = decode(&reference_movie);
    assert_eq!(reference.len(), 768);
    let metrics = |bytes: &[u8]| {
        assert_eq!(bytes.len(), target.len());
        let mut peak = 0u8;
        let mut squared = 0.;
        for (a, b) in bytes.iter().zip(&target) {
            let d = a.abs_diff(*b);
            peak = peak.max(d);
            squared += f64::from(d).powi(2);
        }
        (peak, squared / bytes.len() as f64)
    };
    let (reference_peak, reference_mse) = metrics(&reference);
    let app = support::router(state.clone());
    let mut results = vec![];
    let mut jobs = vec![];
    for internal in [false, true] {
        let parameters=authored_parameters(case(1));
        let conversion=Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.falling-dust","params":parameters}]}).to_string())).unwrap();
        let(status,converted,_)=support::send(&app,conversion).await;assert_eq!(status,StatusCode::OK,"{converted}");
        let effect=converted["nativeEffects"][0].clone();assert_eq!(effect["kind"],"concat_falling_dust");assert_eq!(effect["parameters"].as_object().unwrap().len(),2);for key in ["dust","leak"]{assert_eq!(effect["parameters"][key].as_f64(),parameters[key].as_f64());}
        let mut composition = Composition::new(CanvasSpec {
            width: 16,
            height: 16,
            fps_milli: 4000,
            ..Default::default()
        });
        composition.sources=serde_json::from_value(json!({"falling_dust-source":{"id":"falling_dust-source","kind":"video","durationTicks":500000,"width":16,"height":16,"hasAudio":true,"audioChannels":1}})).unwrap();
        let selected = json!({"id":"clip","sourceId":"falling_dust-source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":500000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":true,"effects":[effect]});
        let mut clips = vec![];
        if internal {
            let mut prior = selected.clone();
            prior["id"] = json!("prior");
            prior["placement"]["timelineStartTick"] = json!(0);
            clips.push(prior);
        }
        clips.push(selected);
        composition.tracks =
            serde_json::from_value(json!([{"id":"v","kind":"video","name":"Video","clips":clips}]))
                .unwrap();
        composition.validate().unwrap();
        assert!(video_kadr_backend::domain::flat_surface::requires_native_video(&composition));
        let body = json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"mp4","codec":"h264"},"range":{"startTicks":1250000,"endTicks":1500000}}});
        let request = Request::builder()
            .method("POST")
            .uri("/api/compositions/render")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let (status, accepted, _) = support::send(&app, request).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
        let id = accepted["jobId"].as_str().unwrap();
        jobs.push(id.to_owned());
        let done = terminal(&app, id).await;
        assert_eq!(done["status"], "done", "internal gap{internal}: {done}");
        let output = state
            .outputs_dir()
            .join(done["result"]["filename"].as_str().unwrap());
        assert!(output.is_file());
        let bytes = decode(&output);
        let (peak, mse) = metrics(&bytes);
        assert!(
            peak <= reference_peak.saturating_add(2) && mse <= reference_mse * 10f64.powf(0.1) + 1.,
            "internal gap{internal}: delivery peak{peak}/{reference_peak} mse{mse}/{reference_mse}"
        );
        let audio = command(&[
            "-nostdin",
            "-v",
            "error",
            "-i",
            output.to_str().unwrap(),
            "-map",
            "0:a:0",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-c:a",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ]);
        let samples = audio
            .chunks_exact(8)
            .map(|frame| f32::from_le_bytes(frame[..4].try_into().unwrap()) as f64)
            .collect::<Vec<_>>();
        assert!(samples.len() >= 11000);
        let tone = |frequency: f64| {
            let (mut re, mut im) = (0., 0.);
            for (i, sample) in samples.iter().take(11000).skip(1000).enumerate() {
                let phase = std::f64::consts::TAU * frequency * i as f64 / 48000.;
                re += sample * phase.cos();
                im += sample * phase.sin();
            }
            re.hypot(im) / 10000.
        };
        assert!(
            tone(880.) > 0.02 && tone(880.) > tone(440.) * 10.,
            "selected source audio clock lost: 880={} 440={}",
            tone(880.),
            tone(440.)
        );
        if internal {
            let mut gap = body.clone();
            gap["output"]["range"] = json!({"startTicks":750000,"endTicks":1000000});
            let request = Request::builder()
                .method("POST")
                .uri("/api/compositions/render")
                .header("content-type", "application/json")
                .body(Body::from(gap.to_string()))
                .unwrap();
            let (status, accepted, _) = support::send(&app, request).await;
            assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
            let gap_id = accepted["jobId"].as_str().unwrap();
            assert_ne!(id, gap_id);
            let done = terminal(&app, gap_id).await;
            assert_eq!(done["status"], "done", "gap-only delivery: {done}");
            let gap_output = state
                .outputs_dir()
                .join(done["result"]["filename"].as_str().unwrap());
            let black = decode(&gap_output);
            assert_eq!(black.len(), 768);
            assert!(
                black.iter().all(|v| *v <= 4),
                "native empty span must deliver black"
            );
            let silence = command(&[
                "-nostdin",
                "-v",
                "error",
                "-i",
                gap_output.to_str().unwrap(),
                "-map",
                "0:a:0",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ]);
            assert!(silence.len() >= 11000 * 8);
            assert!(
                silence
                    .chunks_exact(4)
                    .all(|p| f32::from_le_bytes(p.try_into().unwrap()).abs() < 1e-5),
                "empty span delivered source audio"
            );
            for empty in [false, true] {
                let mut audio_only = if empty { gap.clone() } else { body.clone() };
                audio_only["output"]["profile"] = json!({"container":"audio","codec":"wav"});
                let request = Request::builder()
                    .method("POST")
                    .uri("/api/compositions/render")
                    .header("content-type", "application/json")
                    .body(Body::from(audio_only.to_string()))
                    .unwrap();
                let (status, accepted, _) = support::send(&app, request).await;
                assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
                let done = terminal(&app, accepted["jobId"].as_str().unwrap()).await;
                assert_eq!(
                    done["status"], "done",
                    "sparse audio-only empty{empty}: {done}"
                );
                let output = state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap());
                assert_eq!(output.extension().and_then(|p| p.to_str()), Some("wav"));
                let pcm = command(&[
                    "-nostdin",
                    "-v",
                    "error",
                    "-i",
                    output.to_str().unwrap(),
                    "-ac",
                    "2",
                    "-ar",
                    "48000",
                    "-c:a",
                    "pcm_f32le",
                    "-f",
                    "f32le",
                    "pipe:1",
                ]);
                assert_eq!(pcm.len(), 12000 * 8);
                let samples = pcm
                    .chunks_exact(8)
                    .map(|p| f32::from_le_bytes(p[..4].try_into().unwrap()) as f64)
                    .collect::<Vec<_>>();
                if empty {
                    assert!(samples.iter().all(|v| v.abs() < 1e-5));
                } else {
                    let amplitude = |frequency: f64| {
                        let (mut re, mut im) = (0., 0.);
                        for (i, sample) in samples.iter().take(11000).skip(1000).enumerate() {
                            let phase = std::f64::consts::TAU * frequency * i as f64 / 48000.;
                            re += sample * phase.cos();
                            im += sample * phase.sin();
                        }
                        re.hypot(im) / 10000.
                    };
                    assert!(amplitude(880.) > 0.02 && amplitude(880.) > amplitude(440.) * 10.);
                }
            }
        }
        results.push(bytes);
    }
    assert_eq!(
        jobs.iter().collect::<std::collections::BTreeSet<_>>().len(),
        2
    );
    for result in &results[1..] {
        assert_eq!(
            &results[0], result,
            "selected-range gap clock differs from original Film grade"
        );
    }
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
