use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_poster_frame::{edit, stored_tick};

#[test]
fn poster_target_source_clock_selection_and_clear_preserve_snapshot() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-options-oracle.json")).unwrap();
    let original =
        json!({"format":"filmcraft.project","schema_version":12,"project":fixture["projects"][0]});
    let mut document = original.clone();
    let media = &mut document["project"]["items"]["5"]["kind"]["Media"];
    media["interpret"]["frame_rate"] = json!({"num":30000,"den":1001});
    let context = json!({"sourceItem":"5","sourcePlayhead":"101677220000","projectSelection":["5"],"activeSequence":"21"});
    let set = edit(
        "clip.setPosterFrame",
        json!({"document":document,"context":context,"time":"2"}),
    )
    .unwrap();
    assert_eq!(
        set["result"],
        json!({"item":5,"posterFrame":93_232_339_200_i64})
    );
    assert_eq!(
        stored_tick(&set["document"]["project"]["items"]["5"]).as_deref(),
        Some("93232339200")
    );
    let clear = edit(
        "clip.clearPosterFrame",
        json!({"document":set["documentText"],"context":context}),
    )
    .unwrap();
    assert_eq!(clear["result"], json!({"item":5,"posterFrame":null}));
    assert_eq!(clear["document"], document);

    let selected = edit("clip.setPosterFrame", json!({"document":document,"time":"-1","context":{"sourceItem":"5","sourcePlayhead":"0","projectSelection":["49"]}})).unwrap();
    assert_eq!(
        selected["result"],
        json!({"item":49,"posterFrame":-10_594_584_000_i64})
    );
    assert!(edit("clip.setPosterFrame", json!({"document":document})).is_err());
    assert!(edit(
        "clip.setPosterFrame",
        json!({"document":document,"item":"999"})
    )
    .is_err());
}

mod support;

#[tokio::test]
async fn saved_poster_and_clear_select_different_real_video_pixels() {
    use axum::http::StatusCode;
    use video_kadr_backend::library::MediaEntry;
    let (state, dir) = support::make_state(true, false).await;
    let clip = dir.path().join("sources/poster.mkv");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=32x24:r=24:d=1",
            "-f",
            "lavfi",
            "-i",
            "color=lime:s=32x24:r=24:d=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-c:v",
            "ffv1",
            "-threads",
            "1",
        ])
        .arg(&clip)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        state
            .library
            .add(MediaEntry::from_result(
                "source",
                &json!({
                    "id":"poster-video","filename":"poster.mkv","mediaType":"video"
                })
            ))
            .await
    );
    let owner = state
        .db
        .register_auth_user("poster-owner", "public test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("poster-outsider", "public test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Cover frames")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Poster source",
            &json!({"schemaVersion":1,"sources":{"poster-video":{"id":"poster-video"}}}),
            &["poster-video".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();

    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["items"]["1"]["kind"]["Media"]["info"]["duration"] =
        json!(609_638_400_000_i64);
    let set = edit(
        "clip.setPosterFrame",
        json!({
            "document":document,"item":"1","seconds":1.4
        }),
    )
    .unwrap();
    let clear = edit(
        "clip.clearPosterFrame",
        json!({"document":set["documentText"],"item":"1"}),
    )
    .unwrap();
    let request = |input: &Value, token: &str, view: &str, fraction: Option<f64>| {
        axum::http::Request::post("/api/compositions/filmcraft/poster-preview")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::from(
                json!({"document":input,"item":"1","view":view,"hoverFraction":fraction,"sources":{"1":"poster-video"}})
                    .to_string(),
            ))
            .unwrap()
    };
    let (status, _, _) = support::send(
        &app,
        request(&set["documentText"], &outsider.token, "card", None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    for (snapshot, view, fraction, ticks, green) in [
        (&set["documentText"], "selected", None, "317520000000", true),
        (&set["documentText"], "card", None, "317520000000", true),
        (
            &clear["documentText"],
            "selected",
            None,
            "190512000000",
            false,
        ),
        (&clear["documentText"], "card", None, "127008000000", false),
        (&set["documentText"], "card", Some(0.1), "0", false),
        (
            &clear["documentText"],
            "card",
            Some(0.65),
            "381024000000",
            true,
        ),
    ] {
        let (status, body, _) =
            support::send(&app, request(snapshot, &owner.token, view, fraction)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["sampleTicks"], ticks);
        let pixel = body["rgbaHex"].as_str().unwrap();
        let red = u8::from_str_radix(&pixel[0..2], 16).unwrap();
        let lime = u8::from_str_radix(&pixel[2..4], 16).unwrap();
        assert!(
            if green {
                lime > 220 && red < 30
            } else {
                red > 220 && lime < 30
            },
            "{pixel:.8}"
        );
    }
    for (view, fraction) in [("card", -0.1), ("selected", 0.5)] {
        let (status, _, _) =
            support::send(&app, request(&document, &owner.token, view, Some(fraction))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[test]
fn hover_marks_use_source_frame_times_before_thumbnail_quantization() {
    use video_kadr_backend::domain::filmcraft_marks::edit;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["items"]["1"]["kind"]["Media"]["info"]["duration"] =
        json!(609_638_400_000_i64);
    let input = edit(
        "project.setMarks",
        json!({"document":document,"item":"1","hoverMark":{"side":"in","fraction":0.65}}),
    )
    .unwrap();
    let media = "/project/items/1/kind/Media";
    let marked = input["document"].pointer(media).unwrap();
    assert_eq!(marked["mark_in"], 391_608_000_000_i64);
    assert!(marked["mark_out"].is_null());
    let output = edit("project.setMarks", json!({"document":input["documentText"],"item":"1","hoverMark":{"side":"out","fraction":1.0}})).unwrap();
    let marked = output["document"].pointer(media).unwrap();
    assert_eq!(marked["mark_in"], 391_608_000_000_i64);
    assert_eq!(marked["mark_out"], 599_054_400_000_i64);
    assert!(edit(
        "project.setMarks",
        json!({"document":document,"item":"1","in":"0","hoverMark":{"side":"in","fraction":0.65}})
    )
    .is_err());
}

#[tokio::test]
async fn card_mark_range_includes_out_frame_and_uses_open_end_defaults() {
    use video_kadr_backend::{error::ApiJson, handlers::filmcraft_project::import};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let media = "/project/items/1/kind/Media";
    let frame = 10_584_000_000_i64;
    document.pointer_mut(media).unwrap()["info"]["duration"] = json!(10 * frame);
    for (input, output, range) in [
        (None, None, Value::Null),
        (Some(2 * frame), None, json!({"start":0.2,"end":1.0})),
        (None, Some(5 * frame), json!({"start":0.0,"end":0.6})),
        (
            Some(2 * frame),
            Some(5 * frame),
            json!({"start":0.2,"end":0.6}),
        ),
        (
            Some(-frame),
            Some(20 * frame),
            json!({"start":0.0,"end":1.0}),
        ),
    ] {
        let source = document.pointer_mut(media).unwrap();
        source["mark_in"] = json!(input);
        source["mark_out"] = json!(output);
        let request = serde_json::from_value(json!({"document":document})).unwrap();
        let manifest = import(ApiJson(request)).await.unwrap().0;
        let item = manifest["projectItems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "1")
            .unwrap();
        assert_eq!(item["cardMarkRange"], range);
    }
}

#[tokio::test]
async fn icon_sort_keys_use_source_timecode_and_subclip_offset() {
    use video_kadr_backend::{error::ApiJson, handlers::filmcraft_project::import};
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let media = &mut document["project"]["items"]["1"]["kind"]["Media"];
    media["info"]["start_timecode"] = json!(30);
    media["info"]["file_size"] = json!(2048);
    media["info"]["audio"] = json!({"codec":"pcm_s16le","sample_rate":48000,"channels":2});
    let frame = filmcraft_time::TICKS_PER_SECOND / 24;
    media["mark_in"] = json!(4 * frame);
    media["mark_out"] = json!(12 * frame);
    document["project"]["items"]["1"]["metadata"] =
        json!({"Scene":"Opening","Review owner":"Alex","Poster Frame":"0","Freeform Stack":"1"});
    let request = serde_json::from_value(json!({"document":document})).unwrap();
    let imported = import(ApiJson(request)).await.unwrap().0;
    let keys = |id: &str| {
        &imported["projectItems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap()["iconSort"]
    };
    assert_eq!(keys("1")["mediaStart"], json!(317_520_000_000.0));
    assert_eq!(keys("2")["mediaStart"], json!(444_528_000_000.0));
    assert_eq!(keys("2")["mediaEnd"], json!(814_968_000_000.0));
    assert_eq!(keys("2")["mediaType"], "Subclip");
    assert_eq!(keys("2")["videoInfo"], "4 x 4 (1.0000)");
    assert_eq!(keys("1")["videoDuration"], json!((9 * frame) as f64));
    assert_eq!(keys("1")["timecodes"]["mediaStart"], "00:00:01:06");
    assert_eq!(keys("1")["timecodes"]["videoIn"], "00:00:01:10");
    assert_eq!(keys("1")["timecodes"]["audioOut"], "00:00:01:18");
    assert_eq!(keys("1")["timecodes"]["audioDuration"], "00:00:00:09");
    assert_eq!(keys("2")["timecodes"]["subclipStart"], "00:00:01:18");
    assert_eq!(keys("2")["timecodes"]["subclipEnd"], "00:00:03:05");
    assert!(keys("2")["timecodes"].get("audioOut").is_none());
    assert_eq!(keys("2")["audioCodec"], "pcm_s16le");
    assert_eq!(keys("2")["videoCodec"], "Synthetic");
    assert_eq!(keys("1")["status"], "Online");
    assert_eq!(keys("1")["proxy"], "None");
    assert_eq!(keys("1")["colorSpace"], "sRGB");
    assert_eq!(keys("1")["videoUsage"], 4);
    assert_eq!(keys("1")["audioUsage"], 4);
    assert_eq!(keys("2")["videoUsage"], 0);
    assert_eq!(keys("2")["audioUsage"], 2);
    assert_eq!(keys("2")["filePath"], "/original/clock.mkv");
    assert_eq!(keys("2")["fileSize"], 2048);
    assert_eq!(keys("1")["metadata"], json!({"Scene":"Opening","Review owner":"Alex"}));
}

#[tokio::test]
async fn sequence_thumbnails_can_bypass_effects_without_changing_their_clock() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["effect"] == "invert"
                && case["profile"] == 1
                && case["mask_mode"] == "Add"
                && case["scale"] == 1.0
        })
        .unwrap();
    let document = &case["document"];
    let post = |path, body: Value| {
        axum::http::Request::post(path)
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body.to_string()))
            .unwrap()
    };
    for (view, hover) in [("selected", None), ("card", None), ("card", Some(0.1))] {
        let input = json!({"document":document,"item":"6","view":view,"hoverFraction":hover});
        let endpoint = "/api/compositions/filmcraft/poster-preview";
        let (status, _, enabled) = support::send(&app, post(endpoint, input.clone())).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{enabled:?}");
        let mut bypass = input;
        bypass["effects"] = json!(false);
        let (status, _, disabled) = support::send(&app, post(endpoint, bypass)).await;
        assert_eq!(status, axum::http::StatusCode::OK, "{disabled:?}");
        let enabled: Value = serde_json::from_str(&enabled).unwrap();
        let disabled: Value = serde_json::from_str(&disabled).unwrap();
        assert_eq!(enabled["effects"], true);
        assert_eq!(disabled["effects"], false);
        assert_eq!(enabled["ptsTicks"], disabled["ptsTicks"]);
        assert_ne!(enabled["rgbaHex"], disabled["rgbaHex"], "{view} {hover:?}");
        let (status, _, program) = support::send(
            &app,
            post(
                "/api/compositions/filmcraft/preview-frame",
                json!({"document":document,"sequenceId":"6","timeTicks":enabled["ptsTicks"]}),
            ),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let program: Value = serde_json::from_str(&program).unwrap();
        assert_eq!(enabled["rgbaHex"], program["rgbaHex"]);
    }
}
