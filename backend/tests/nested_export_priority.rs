mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use std::time::Duration;
use video_kadr_backend::{
    config::resource_classes::ResourceClass,
    domain::composition::{CanvasSpec, Composition},
    model::Job,
};

#[tokio::test]
async fn queued_export_priority_and_order_survive_server_restart() {
    let (mut state, _dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    let owner = state
        .db
        .register_auth_user("priority-exporter", "priority export password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let mut held = Vec::new();
    for _ in 0..state.resource_available(ResourceClass::Export) {
        held.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    assert!(!held.is_empty());

    let request = |path: &str, body: Value| {
        Request::post(path)
            .header("authorization", format!("Bearer {}", owner.token))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let mut ids = Vec::new();
    for red in [0.25, 0.5, 0.75, 1.0] {
        let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
        content["canvas"]["width"] = json!(16);
        content["canvas"]["height"] = json!(16);
        content["canvas"]["background"]["red"] = json!(red);
        let body = json!({
            "schemaVersion":1, "mode":"video_only", "compositionId":"root",
            "startUs":0, "endUs":100000, "renderedOutputs":{},
            "downloadName":format!("  Монтаж {}.MKV  ", ids.len() + 1),
            "project":{"schemaVersion":1,"root":"root","compositions":{
                "root":{"content":content,"instances":[],"durationUs":100000}
            }}
        });
        if ids.is_empty() {
            let mut invalid = body.clone();
            invalid["downloadName"] = json!("../чужой-файл");
            let (status, _, _) = support::send(&app, request("/api/compositions/nested-export", invalid)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        let (status, result, _) = support::send(&app, request("/api/compositions/nested-export", body)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{result}");
        ids.push(result["jobId"].as_str().unwrap().to_owned());
    }
    let cancelled = ids.pop().unwrap();
    let (status, result, _) = support::send(
        &app,
        request(&format!("/api/jobs/{cancelled}/cancel"), json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["status"], "cancelled");
    assert!(state
        .job_store
        .attempt_history(&cancelled)
        .await
        .unwrap()
        .is_empty());
    let (first, moved, promoted) = (&ids[0], &ids[1], &ids[2]);
    assert_eq!(state.job_store.deliverable_ids().await.unwrap(), ids);
    let (status, result, _) = support::send(
        &app,
        request(
            &format!("/api/compositions/nested-export/{promoted}/priority"),
            json!({"raised":true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["raised"], true);

    assert_eq!(
        state.job_store.deliverable_ids().await.unwrap(),
        vec![promoted.clone(), first.clone(), moved.clone()]
    );
    let (status, result, _) = support::send(
        &app,
        request(
            &format!("/api/compositions/nested-export/{promoted}/priority"),
            json!({"raised":false}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(state.job_store.deliverable_ids().await.unwrap(), ids);
    let (status, result, _) = support::send(
        &app,
        request(
            &format!("/api/compositions/nested-export/{moved}/move"),
            json!({"direction":"earlier"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["applied"], true);
    assert_eq!(
        result["queue"],
        json!([
            {"jobId":moved,"raised":false},{"jobId":first,"raised":false},{"jobId":promoted,"raised":false}
        ])
    );
    let (status, result, _) = support::send(
        &app,
        request(
            &format!("/api/compositions/nested-export/{promoted}/priority"),
            json!({"raised":true}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let expected = vec![promoted.clone(), moved.clone(), first.clone()];
    assert_eq!(state.job_store.deliverable_ids().await.unwrap(), expected);

    let storage = state.storage.clone();
    let tools = state.tools.as_ref().clone();
    drop(held);
    drop(app);
    drop(state);

    let db = video_kadr_backend::db::Db::open(&storage).await.unwrap();
    let library = video_kadr_backend::library::Library::load(storage.clone()).await;
    let state = video_kadr_backend::state::AppState::new(storage, 1, tools, library, db);
    state.recover_jobs().await;
    assert_eq!(
        state.get_job(&cancelled).await.unwrap().unwrap().status,
        video_kadr_backend::model::JobStatus::Cancelled
    );

    assert_eq!(state.job_store.deliverable_ids().await.unwrap(), expected);
    let app = support::router(state.clone());
    let (status, history, _) = support::send(
        &app,
        Request::get("/api/compositions/nested-export")
            .header("authorization", format!("Bearer {}", owner.token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history}");
    assert_eq!(
        history["queue"],
        json!([
            {"jobId":promoted,"raised":true},
            {"jobId":moved,"raised":false},
            {"jobId":first,"raised":false},
        ])
    );
    for (index, id) in ids.iter().chain(std::iter::once(&cancelled)).enumerate() {
        let row = history["jobs"].as_array().unwrap().iter().find(|entry| entry["jobId"] == *id).unwrap();
        assert_eq!(row["downloadName"], format!("Монтаж {}.mkv", index + 1));
    }
    let mut named_outputs = Vec::new();
    for id in &expected {
        let job = wait_for_export(&state, id).await;
        let index = ids.iter().position(|candidate| candidate == id).unwrap();
        let result = job.result.as_ref().unwrap();
        assert_eq!(result["downloadName"], format!("Монтаж {}.mkv", index + 1));
        assert_eq!(result["filename"], format!("{}.mkv", result["id"].as_str().unwrap()));
        named_outputs.push((result["id"].as_str().unwrap().to_owned(), result["downloadName"].clone()));
        verify_matroska(&state, &job);
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.library.list().await.len() < named_outputs.len() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("completed exports should reach the media library");
    let outsider = state.db.register_auth_user("export-outsider", "export outsider password").await.unwrap().unwrap();
    let cookie_request = |method: &str, url: &str, token: &str| Request::builder()
        .method(method).uri(url).header("cookie", format!("video_kadr_session={token}"))
        .body(Body::empty()).unwrap();
    let (status, listed, _) = support::send(&app, cookie_request("GET", "/api/library", &owner.token)).await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    for (output, name) in &named_outputs {
        let item = listed.as_array().unwrap().iter().find(|item| item["id"] == *output).unwrap();
        assert_eq!(item["title"], *name);
        assert_eq!(item["originalFilename"], *name);
        assert_eq!(item["filename"], format!("{output}.mkv"));
    }
    let (status, denied, _) = support::send(&app, cookie_request("GET", "/api/library", &outsider.token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(denied, json!([]));
    let output = &named_outputs[0].0;
    let download = format!("/files/outputs/{output}.mkv");
    let (status, _, _) = support::send(&app, cookie_request("GET", &download, &owner.token)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _, _) = support::send(&app, cookie_request("GET", &download, &outsider.token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = support::send(&app, cookie_request("DELETE", &format!("/api/library/{output}"), &outsider.token)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let dispatched: Vec<String> =
        sqlx::query_scalar("SELECT job_id FROM job_attempts ORDER BY rowid")
            .fetch_all(&database)
            .await
            .unwrap();
    assert_eq!(dispatched, expected);
    assert!(state.job_store.deliverable_ids().await.unwrap().is_empty());
}

async fn wait_for_export(state: &video_kadr_backend::state::AppState, id: &str) -> Job {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if job.status.is_terminal() {
                return job;
            }
            video_kadr_backend::handlers::resume_pending_jobs(state).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("export should finish")
}

fn verify_matroska(state: &video_kadr_backend::state::AppState, job: &Job) {
    let filename = job
        .result
        .as_ref()
        .and_then(|value| value.get("filename"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("export did not finish: {job:?}"));
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(state.outputs_dir().join(filename))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "ffv1");
}

#[tokio::test]
async fn timecode_export_uses_absolute_frame_boundaries_and_rejects_dropped_labels() {
    let (mut state, _dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.extend(["ffv1", "pcm_f32le", "pcm_s16le", "pcm_s24le"].map(str::to_owned));
    tools.ffmpeg_muxers.extend(["matroska", "wav"].map(str::to_owned));
    let owner = state.db.register_auth_user("timecode-exporter", "timecode export password")
        .await.unwrap().unwrap();
    let source = state.sources_dir().join("signal.wav");
    let generated = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi", "-i",
            "aevalsrc=0.25*sin(n/23)|0.2*cos(n/17):s=48000:d=2", "-c:a", "pcm_f32le"])
        .arg(&source).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result(
        "source", &json!({"id":"signal","filename":"signal.wav","url":"/files/sources/signal.wav",
            "mediaType":"audio","duration":2.0,"hasAudio":true}),
    )).await);
    let reference = decode_pcm(&source);
    let app = support::router(state.clone());
    let request_body = |rate: Value, start: &str, end: &str, first_frame: i64, mode: &str, compression: Value, channels: Option<u16>, sample_rate: Option<u32>| {
        let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
        content["canvas"]["width"] = json!(16);
        content["canvas"]["height"] = json!(16);
        let n = rate["numerator"].as_u64().unwrap();
        let d = rate["denominator"].as_u64().unwrap();
        content["canvas"]["fpsMilli"] = json!((n * 1000 + d / 2) / d);
        let clip_start = if d == 1001 { 60000000 } else { 0 };
        content["canvas"]["fpsRate"] = rate;
        content["sources"] = json!({"signal":{"id":"signal","kind":"audio","durationTicks":2000000,
            "width":0,"height":0,"hasAudio":true}});
        content["tracks"] = json!([{"kind":"audio","id":"audio","name":"Test signal","clips":[{
            "id":"clip","sourceId":"signal",
            "placement":{"timelineStartTick":clip_start,"sourceInTick":0,"sourceOutTick":2000000,"speed":1},
            "gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,
        }]}]);
        let clock = filmcraft_time::FrameRate::new(n as i64, d as i64);
        let first = clock.tick_of(first_frame).0;
        let last = clock.tick_of(first_frame + 1).0;
        let cue = |id, text, start: i64, end: i64| json!({
            "id":id,"text":text,"timelineStartTick":start.to_string(),"timelineEndTick":end.to_string(),
        });
        content["captionTracks"] = json!([{"id":"captions","name":"Captions","hidden":true,"clips":[
            cue("before","Before the cut",first - 1,first),
            cue("wanted","In the cut",first,last),
            cue("after","After the cut",last,last + 1),
        ]}]);
        json!({
                "schemaVersion":1, "mode":mode, "compositionId":"root", "audioCompression":compression,"audioChannels":channels,"audioSampleRate":sample_rate,
                "renderedOutputs":if mode == "audio_only" { json!({"root":"unavailable-video"}) } else { json!({}) },
                "startUs":0, "endUs":0, "timecodeRange":{"start":start,"end":end},
                "captionSidecar":{"trackId":"captions","format":"vtt","dropFrame":false},
                "project":{"schemaVersion":1,"root":"root","compositions":{
                    "root":{"content":content,"instances":[],"durationUs":65000000}
                }}
            })
    };
    let post = |body: Value| Request::post("/api/compositions/nested-export")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let request = |rate, start, end, first_frame, mode, compression, channels, sample_rate|
        post(request_body(rate, start, end, first_frame, mode, compression, channels, sample_rate));
    let drop = json!({"numerator":30000,"denominator":1001});
    let (status, _, _) = support::send(&app, request(drop.clone(), "00:00:59;29", "00:01:00;00", 1799, "video_audio", Value::Null, None, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for (rate, start, end, expected_start, expected_end, first_frame, first_sample, samples, signal_samples) in [
        (json!({"numerator":24,"denominator":1}), "00:00:00:01", "00:00:00:02", 41666, 83333, 1, 2000, 2000, 2000),
        (drop, "00:00:59;29", "00:01:00;02", 60026633, 60060000, 1799, 1279, 1602, 1601),
        (json!({"numerator":123,"denominator":7}), "00:00:00:17", "00:00:01:00", 967479, 1024390, 17, 46440, 2732, 2731),
    ] {
        let (status, result, _) = support::send(&app, request(rate.clone(), start, end, first_frame, "video_audio", Value::Null, None, None)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{result}");
        assert_eq!(result["startUs"], expected_start);
        assert_eq!(result["endUs"], expected_end);
        let clock = filmcraft_time::FrameRate::new(
            rate["numerator"].as_i64().unwrap(), rate["denominator"].as_i64().unwrap(),
        );
        assert_eq!(result["captionRange"], json!({
            "startTick":clock.tick_of(first_frame).0.to_string(),
            "endTick":clock.tick_of(first_frame + 1).0.to_string(),
        }));
        let job = wait_for_export(&state, result["jobId"].as_str().unwrap()).await;
        let filename = job.result.as_ref().and_then(|value| value["filename"].as_str())
            .unwrap_or_else(|| panic!("export did not finish: {job:?}"));
        let sidecar = job.result.as_ref().unwrap()["captionSidecar"]["bytesBase64"].as_str().unwrap();
        let bytes = video_kadr_backend::domain::graphic_template_fonts::base64_decode(sidecar).unwrap();
        let captions = filmcraft_captions::parse(&bytes, filmcraft_captions::Format::WebVtt).unwrap();
        assert_eq!(captions.cues.len(), 1);
        assert_eq!(captions.cues[0].text, "In the cut");
        assert_eq!(captions.cues[0].start.0, 0);
        let path = state.outputs_dir().join(filename);
        let output = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-count_frames", "-show_entries",
                "stream=nb_read_frames", "-of", "csv=p=0"])
            .arg(&path)
            .output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
        let pcm = decode_pcm(&path);
        assert_eq!(pcm.len(), samples * 8, "48 kHz stereo PCM frame count");
        let expected = &reference[first_sample * 8..(first_sample + signal_samples) * 8];
        for (index, (actual, source)) in pcm.chunks_exact(8).zip(expected.chunks_exact(8)).enumerate() {
            assert_eq!(actual, source, "PCM frame {index} at {start}");
        }
        assert!(pcm[signal_samples * 8..].iter().all(|byte| *byte == 0), "audio beyond the cut is silent");
    }
    for (codec, channels, rate) in [
        ("aac", 2, 48000), ("opus", 2, 48000), ("aac", 1, 44100),
        ("opus", 1, 48000), ("pcm_f32le", 1, 192000),
    ] {
        let compression = if codec == "pcm_f32le" { Value::Null } else {
            json!({"codec":codec, "bitrateKbps":128})
        };
        let (status, reply, _) = support::send(&app, request(
            json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
            "video_audio", compression, Some(channels), Some(rate),
        )).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{reply}");
        assert_eq!(reply["container"], "mkv");
        let job = wait_for_export(&state, reply["jobId"].as_str().unwrap()).await;
        let result = job.result.as_ref().unwrap_or_else(|| panic!("compressed video export failed: {job:?}"));
        assert_eq!(result["audioCodec"], codec);
        assert_eq!(result["audioChannels"], channels);
        assert_eq!(result["audioSampleRate"], rate);
        let output = state.outputs_dir().join(result["filename"].as_str().unwrap());
        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-count_frames", "-show_entries",
                "stream=codec_name,codec_type,nb_read_frames,sample_rate,channels", "-of", "json"])
            .arg(&output).output().unwrap();
        assert!(probe.status.success(), "{}", String::from_utf8_lossy(&probe.stderr));
        let streams: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(streams["streams"][0], json!({"codec_name":"ffv1","codec_type":"video","nb_read_frames":"1"}));
        assert_eq!(streams["streams"][1]["codec_name"], codec);
        assert_eq!(streams["streams"][1]["sample_rate"], rate.to_string());
        assert_eq!(streams["streams"][1]["channels"], channels);
        let decoded = decode_pcm(&output);
        if codec == "pcm_f32le" {
            assert_eq!(decoded.len(), 6408 * 4);
        }
        assert!(decoded.chunks_exact(4).any(|sample|
            f32::from_le_bytes(sample.try_into().unwrap()).abs() > 0.02));
    }
    for (mode, depth, codec, bits, frames) in [
        ("video_audio", "s16", "pcm_s16le", 16, 1602),
        ("video_audio", "s24", "pcm_s24le", 24, 1602),
        ("audio_only", "s16", "pcm_s16le", 16, 1601),
        ("audio_only", "s24", "pcm_s24le", 24, 1601),
    ] {
        let mut body = request_body(
            json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
            mode, Value::Null, Some(1), None,
        );
        body["pcmDepth"] = json!(depth);
        let (status, reply, _) = support::send(&app, post(body)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{reply}");
        let job = wait_for_export(&state, reply["jobId"].as_str().unwrap()).await;
        let result = job.result.as_ref().unwrap_or_else(|| panic!("integer PCM export failed: {job:?}"));
        assert_eq!(result["audioCodec"], codec);
        let output = state.outputs_dir().join(result["filename"].as_str().unwrap());
        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "a:0", "-show_entries",
                "stream=codec_name,bits_per_sample,channels", "-of", "json"])
            .arg(&output).output().unwrap();
        assert!(probe.status.success(), "{}", String::from_utf8_lossy(&probe.stderr));
        let streams: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(streams["streams"], json!([{"codec_name":codec,"bits_per_sample":bits,"channels":1}]));
        let decoded = decode_pcm(&output);
        assert_eq!(decoded.len(), frames * 4);
        assert!(decoded.chunks_exact(4).any(|sample|
            f32::from_le_bytes(sample.try_into().unwrap()).abs() > 0.02));
    }
    for (mode, compression) in [
        ("video_only", Value::Null),
        ("video_audio", json!({"codec":"aac","bitrateKbps":128})),
    ] {
        let mut body = request_body(
            json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
            mode, compression, None, None,
        );
        body["pcmDepth"] = json!("s24");
        let (status, _, _) = support::send(&app, post(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let mut audio_state = state.clone();
    let tools = std::sync::Arc::make_mut(&mut audio_state.tools);
    tools.ffmpeg_encoders.retain(|codec| codec != "ffv1");
    tools.ffmpeg_muxers.retain(|muxer| muxer != "matroska");
    let _first_render = audio_state.acquire_render_slot().await.unwrap();
    let _second_render = tokio::time::timeout(Duration::from_millis(50), audio_state.acquire_render_slot()).await;
    let audio_app = support::router(audio_state.clone());
    let (status, result, _) = support::send(
        &audio_app,
        request(json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799, "audio_only", Value::Null, None, None),
    ).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{result}");
    assert_eq!(result["container"], "wav");
    let job = wait_for_export(&audio_state, result["jobId"].as_str().unwrap()).await;
    let result = job.result.as_ref().unwrap_or_else(|| panic!("audio-only export did not finish: {job:?}"));
    assert_eq!(result["mediaType"], "audio");
    assert_eq!(result["audioCodec"], "pcm_f32le");
    assert!(result["videoCodec"].is_null());
    let filename = result["filename"].as_str().unwrap();
    assert!(filename.ends_with(".wav"));
    let path = audio_state.outputs_dir().join(filename);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(bytes.len(), 56 + 1601 * 8);
    assert_eq!(decode_pcm(&path), reference[1279 * 8..2880 * 8]);

    let submit_wav = |channels| request(
        json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
        "audio_only", Value::Null, Some(channels), None,
    );
    let (status, _, _) = support::send(&audio_app, submit_wav(6)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, mono, _) = support::send(&audio_app, submit_wav(1)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{mono}");
    let job = wait_for_export(&audio_state, mono["jobId"].as_str().unwrap()).await;
    let result = job.result.as_ref().unwrap_or_else(|| panic!("mono WAV export failed: {job:?}"));
    assert_eq!(result["audioChannels"], 1);
    let path = audio_state.outputs_dir().join(result["filename"].as_str().unwrap());
    let decoded = decode_pcm(&path);
    assert_eq!(decoded.len(), 1601 * 4);
    for (actual, stereo) in decoded.chunks_exact(4).zip(reference[1279 * 8..2880 * 8].chunks_exact(8)) {
        let left = f32::from_le_bytes(stereo[..4].try_into().unwrap());
        let right = f32::from_le_bytes(stereo[4..].try_into().unwrap());
        assert_eq!(actual, (0.5 * left + 0.5 * right).to_le_bytes().as_slice());
    }
    let mut compressed_state = audio_state.clone();
    let tools = std::sync::Arc::make_mut(&mut compressed_state.tools);
    tools.ffmpeg_muxers.extend(["mp4".into(), "ogg".into()]);
    let compressed_app = support::router(compressed_state.clone());
    let compressed_request = |codec, bitrate, channels| request(
        json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
        "audio_only", json!({"codec":codec, "bitrateKbps":bitrate}), channels, None,
    );
    let (status, _, _) = support::send(&compressed_app, compressed_request("aac", 12, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = support::send(&compressed_app, compressed_request("opus", 320, Some(1))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = support::send(&compressed_app, compressed_request("aac", 320, Some(1))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for (codec, container, channels) in [("aac", "m4a", 2), ("opus", "opus", 2), ("aac", "m4a", 1), ("opus", "opus", 1)] {
        let (status, reply, _) = support::send(&compressed_app, compressed_request(codec, 128, Some(channels))).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{reply}");
        assert_eq!(reply["container"], container);
        let job = wait_for_export(&compressed_state, reply["jobId"].as_str().unwrap()).await;
        let result = job.result.as_ref().unwrap_or_else(|| panic!("compressed export failed: {job:?}"));
        assert_eq!(result["audioCodec"], codec);
        assert_eq!(result["audioBitrateKbps"], 128);
        assert_eq!(result["audioChannels"], channels);
        let output = compressed_state.outputs_dir().join(result["filename"].as_str().unwrap());
        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,codec_type,sample_rate,channels","-of","json"])
            .arg(&output).output().unwrap();
        assert!(probe.status.success(), "{}", String::from_utf8_lossy(&probe.stderr));
        let streams: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(streams["streams"], json!([{"codec_name":codec,"codec_type":"audio","sample_rate":"48000","channels":channels}]));
        assert!(decode_pcm(&output).chunks_exact(4).any(|frame|
            f32::from_le_bytes(frame.try_into().unwrap()).abs() > 0.02));
    }

    let resampled_request = |codec, rate| request(
        json!({"numerator":30000,"denominator":1001}), "00:00:59;29", "00:01:00;02", 1799,
        "audio_only", codec, Some(1), Some(rate),
    );
    let (status, _, _) = support::send(&compressed_app, resampled_request(
        json!({"codec":"opus","bitrateKbps":128}), 44100,
    )).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for (codec, settings, rate, pcm_frames) in [
        ("pcm_f32le", Value::Null, 44100, Some(1471)),
        ("pcm_f32le", Value::Null, 192000, Some(6404)),
        ("aac", json!({"codec":"aac","bitrateKbps":128}), 44100, None),
    ] {
        let (status, reply, _) = support::send(&compressed_app, resampled_request(settings, rate)).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{reply}");
        let job = wait_for_export(&compressed_state, reply["jobId"].as_str().unwrap()).await;
        let result = job.result.as_ref().unwrap_or_else(|| panic!("resampled export failed: {job:?}"));
        assert_eq!(result["audioSampleRate"], rate);
        let path = compressed_state.outputs_dir().join(result["filename"].as_str().unwrap());
        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_name,codec_type,sample_rate,channels","-of","json"])
            .arg(&path).output().unwrap();
        assert!(probe.status.success());
        let streams: Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(streams["streams"], json!([{"codec_name":codec,"codec_type":"audio","sample_rate":rate.to_string(),"channels":1}]));
        let pcm = decode_pcm(&path);
        if let Some(frames) = pcm_frames { assert_eq!(pcm.len(), frames * 4); }
        assert!(pcm.chunks_exact(4).any(|sample| f32::from_le_bytes(sample.try_into().unwrap()).abs() > 0.02));
    }

}

fn decode_pcm(path: &std::path::Path) -> Vec<u8> {
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"]).arg(path)
        .args(["-map", "0:a:0", "-f", "f32le", "-c:a", "pcm_f32le", "pipe:1"])
        .output().unwrap();
    assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
    decoded.stdout
}

#[tokio::test]
async fn cursor_timecode_respects_the_native_frame_boundary_and_skips_drop_frame_labels() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let request = |time_us, numerator, denominator| {
        Request::post("/api/compositions/nested-export/cursor-timecode")
            .header("content-type", "application/json")
            .body(Body::from(json!({
                "timeUs":time_us,"rate":{"numerator":numerator,"denominator":denominator},"dropFrame":true,
            }).to_string())).unwrap()
    };
    for (time_us, expected) in [
        (60026632, "00:00:59;28"),
        (60026633, "00:00:59;29"),
        (60060000, "00:01:00;02"),
    ] {
        let (status, result, _) = support::send(&app, request(time_us, 30000, 1001)).await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["timecode"], expected);
    }
    let (status, result, _) = support::send(&app, request(41666, 24, 1)).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["timecode"], "00:00:00:01");
    let (status, _, _) = support::send(&app, request(0, 24, 0)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
