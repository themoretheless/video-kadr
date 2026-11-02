mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::{io::Read, path::Path, time::Duration};
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition},
    library::MediaEntry,
};
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
fn request(body: &Value, token: Option<&str>) -> Request<Body> {
    let mut r = Request::builder()
        .method("POST")
        .uri("/api/compositions/render")
        .header("content-type", "application/json");
    if let Some(token) = token {
        r = r.header("authorization", format!("Bearer {token}"));
    }
    r.body(Body::from(body.to_string())).unwrap()
}
async fn terminal(app: &axum::Router, id: &str, token: &str) -> Value {
    for _ in 0..3000 {
        let r = Request::builder()
            .uri(format!("/api/jobs/{id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (status, done, _) = support::send(app, r).await;
        assert_eq!(status, StatusCode::OK, "{done}");
        if matches!(
            done["status"].as_str(),
            Some("done" | "error" | "cancelled")
        ) {
            return done;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Reflections export did not finish")
}
fn source(frame: usize, ch: usize, frames: usize, signal: &str) -> f32 {
    if signal == "steps" {
        return (if ch == 0 { 2. } else { -1.5 })
            * if (frame * 8 / 48000) % 4 < 2 { 1. } else { -1. };
    }
    if [0, 48000 / 7, frames - 1].contains(&frame) {
        return [1.25, -0.75][ch];
    }
    let h = (frame % 257 + ch * 1061) as u32;
    let h = h.wrapping_mul(1664525).wrapping_add(1013904223);
    (((h >> 12) % 2049) as i32 - 1024) as f32 / 4096.
}
fn decode(path: &Path) -> Vec<f32> {
    command(&[
        "-nostdin",
        "-v",
        "error",
        "-i",
        path.to_str().unwrap(),
        "-ar",
        "48000",
        "-ac",
        "2",
        "-c:a",
        "pcm_f32le",
        "-f",
        "f32le",
        "pipe:1",
    ])
    .chunks_exact(4)
    .map(|p| f32::from_le_bytes(p.try_into().unwrap()))
    .collect()
}
#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe and native PCM export worker"]
async fn private_source_native_stadium_wav_export_and_selected_seek() {
    let (mut state, _dir) = support::make_state(true, false).await;
    let caps = std::sync::Arc::make_mut(&mut state.tools);
    caps.ffmpeg_encoders.push("pcm_s16le".into());
    caps.ffmpeg_muxers.push("wav".into());
    let owner = state
        .db
        .register_auth_user("stadium-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("stadium-outsider", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/concat-stadium-pcm.json")).unwrap();
    let mut pcm = vec![];
    flate2::read::GzDecoder::new(include_bytes!("fixtures/concat-stadium-pcm.f32le.gz").as_slice())
        .read_to_end(&mut pcm)
        .unwrap();
    let cases: Vec<&Value> = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["rate"] == 48000 && c["channels"] == 2)
        .collect();
    assert_eq!(cases.len(), 10);
    let frames = cases[0]["inputFrames"].as_u64().unwrap() as usize;
    for signal in ["noise", "steps"] {
        let input: Vec<u8> = (0..frames)
            .flat_map(|i| (0..2).flat_map(move |ch| source(i, ch, frames, signal).to_le_bytes()))
            .collect();
        let raw = state.sources_dir().join(format!("stadium-{signal}.f32le"));
        std::fs::write(&raw, &input).unwrap();
        let filename = format!("stadium-{signal}.wav");
        let wav = state.sources_dir().join(&filename);
        command(&[
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "2",
            "-i",
            raw.to_str().unwrap(),
            "-c:a",
            "pcm_f32le",
            wav.to_str().unwrap(),
        ]);
        assert_eq!(
            decode(&wav)
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
            input
        );
        state.library.add(MediaEntry::from_result("source",&json!({"id":signal,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"audio","duration":frames as f64/48000.,"hasAudio":true,"audioChannels":2,"acodec":"pcm_f32le"}))).await;
    }
    let space = state
        .db
        .create_space(&owner.user.username, "Stadium source")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Stadium source claim",
            &json!({"schemaVersion":1,"sources":{"noise":{"id":"noise"},"steps":{"id":"steps"}}}),
            &["noise".into(), "steps".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let (status, catalog, _) =
        support::send(&app, support::get("/api/compositions/audio-dsp-catalog")).await;
    assert_eq!(status, StatusCode::OK);
    let info = catalog["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "concat.stadium")
        .unwrap();
    assert_eq!(info["nativeExport"], true);
    let params = info["params"].as_array().unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0]["id"], "size");
    assert_eq!(params[0]["default"].as_f64(), Some(6.));
    assert_eq!(params[0]["min"].as_f64(), Some(1.));
    assert_eq!(params[0]["max"].as_f64(), Some(10.));
    assert_eq!(params[0]["animatable"], false);
    assert_eq!(params[0]["step"].as_f64(), Some(0.5));
    let mut jobs = 0;
    let mut first_export = None;
    for (pi, case) in cases
        .iter()
        .enumerate()
        .filter(|(i, _)| [0, 3, 5, 7, 8, 9].contains(i))
    {
        let signal = case["signal"].as_str().unwrap();
        let mut composition = Composition::new(CanvasSpec::default());
        composition.sources=serde_json::from_value(json!({(signal):{"id":signal,"kind":"audio","durationTicks":1125354,"width":0,"height":0,"hasAudio":true,"audioChannels":2}})).unwrap();
        composition.tracks=serde_json::from_value(json!([{"kind":"audio","id":"audio","name":"Stadium","clips":[{"id":"clip","sourceId":signal,"placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":case["settings"]}]}])).unwrap();
        composition.validate().unwrap();
        assert!(
            video_kadr_backend::tools::composition_native_audio::needs_native_audio(&composition)
        );
        let body = json!({"schemaVersion":1,"composition":composition,"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":1000000,"endTicks":2000000}}});
        if pi == 0 {
            let (s, _, _) = support::send(&app, request(&body, None)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED);
            let (s, _, _) = support::send(&app, request(&body, Some(&outsider.token))).await;
            assert_eq!(s, StatusCode::FORBIDDEN);
            let mut bad = body.clone();
            bad["composition"]["tracks"][0]["clips"][0]["dspEffects"][0]["params"] =
                json!({"size":101});
            let (s, _, _) = support::send(&app, request(&bad, Some(&owner.token))).await;
            assert_eq!(s, StatusCode::BAD_REQUEST);
        }
        for (start, end, first, count) in [
            (1000000, 2000000, 0, 48000),
            (1500000, 1750000, 24000, 12000),
            (500000, 750000, 0, 12000),
        ] {
            let mut selected = body.clone();
            selected["output"]["range"] = json!({"startTicks":start,"endTicks":end});
            let (s, accepted, _) =
                support::send(&app, request(&selected, Some(&owner.token))).await;
            assert_eq!(s, StatusCode::ACCEPTED, "{accepted}");
            let done = terminal(&app, accepted["jobId"].as_str().unwrap(), &owner.token).await;
            assert_eq!(done["status"], "done", "{done}");
            let actual = decode(
                &state
                    .outputs_dir()
                    .join(done["result"]["filename"].as_str().unwrap()),
            );
            assert_eq!(actual.len(), count * 2);
            if pi == 0 && start == 1000000 {
                first_export = Some((
                    selected.clone(),
                    accepted["jobId"].as_str().unwrap().to_owned(),
                    actual.clone(),
                ));
            }
            let offset = case["offsetBytes"].as_u64().unwrap() as usize;
            for (i, v) in actual.iter().enumerate() {
                let expected = if start < 1000000 {
                    0.
                } else {
                    let j = offset + (first * 2 + i) * 4;
                    f32::from_le_bytes(pcm[j..j + 4].try_into().unwrap())
                        .clamp(-1., 1. - 1. / 32768.)
                };
                assert!(
                    (v - expected).abs() <= 1. / 32768. + 2e-7,
                    "plan{pi} selected{start} sample{i}: native{v} original{expected}"
                );
            }
            jobs += 1;
        }
    }
    assert_eq!(jobs, 18);
    state
        .db
        .set_space_member(
            &space.id,
            &owner.user.username,
            &outsider.user.username,
            video_kadr_backend::db::SpaceRole::Viewer,
        )
        .await
        .unwrap()
        .unwrap();
    let (same_request, owner_job, owner_pcm) = first_export.unwrap();
    let (status, accepted, _) =
        support::send(&app, request(&same_request, Some(&outsider.token))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let member_job = accepted["jobId"].as_str().unwrap();
    assert_ne!(
        member_job, owner_job,
        "render job deduplication must retain actor ownership"
    );
    let done = terminal(&app, member_job, &outsider.token).await;
    assert_eq!(done["status"], "done", "{done}");
    assert_eq!(
        decode(
            &state
                .outputs_dir()
                .join(done["result"]["filename"].as_str().unwrap())
        ),
        owner_pcm
    );

    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(15)).await);
}
