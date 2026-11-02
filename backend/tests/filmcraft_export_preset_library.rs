use serde_json::json;
mod support;
use video_kadr_backend::{
    db::Db,
    domain::filmcraft_export_presets::{Library, Preset, DEFAULT_PRESET},
};

fn review(name: &str) -> Preset {
    Preset {
        name: name.into(),
        category: "User".into(),
        description: "client review".into(),
        settings: json!({"format":"H264","frameSize":[1280,720],"bitrateKbps":5000,"path":"/private/output.mp4","range":{"start":0,"duration":42}}),
        builtin: false,
    }
}

#[test]
fn all_original_builtin_settings_resolve_to_typed_host_requests() {
    use video_kadr_backend::{
        domain::filmcraft_export_preset_resolve::request_patch,
        domain::filmcraft_export_presets::builtins, handlers::filmcraft_video_export::VideoRequest,
    };
    for preset in builtins() {
        let mut patch =
            request_patch(&preset.settings).unwrap_or_else(|e| panic!("{}: {e}", preset.name));
        patch["document"] = json!({});
        patch["sequenceId"] = json!("6");
        patch["frames"] = json!(2);
        patch["sources"] = json!({});
        serde_json::from_value::<VideoRequest>(patch)
            .unwrap_or_else(|e| panic!("{}: {e}", preset.name));
    }
    let mut preset = video_kadr_backend::domain::filmcraft_export_presets::builtins().remove(0);
    preset.settings["hardwareEncoding"] = json!("auto");
    assert!(request_patch(&preset.settings).is_err());
}

#[test]
fn original_flat_overrides_keep_ntsc_rates_and_replace_adaptive_bitrate() {
    use video_kadr_backend::domain::{
        filmcraft_export_preset_overrides::apply, filmcraft_export_presets::builtins,
    };
    let mut settings = builtins().remove(0).settings;
    apply(&mut settings,&json!({"format":"M.P.4","width":1280.0,"height":720,"fps":29.97,"bitrateKbps":5000,"quality":255,"audio":false,"hardwareEncoding":false,"loudnessLufs":-16})).unwrap();
    assert_eq!(settings["format"], "h264");
    assert_eq!(settings["frameRate"], json!({"num":30000,"den":1001}));
    assert_eq!(settings["frameSize"], json!([1280, 720]));
    assert_eq!(settings["quality"], 100);
    assert!(settings["adaptiveBitrate"].is_null());
    assert_eq!(
        settings["effects"]["loudness"]["target_lufs"].as_f64(),
        Some(-16.)
    );
    for invalid in [
        json!({"width":1280}),
        json!({"width":1.5,"height":720}),
        json!({"bitrateKbps":4294967296u64}),
        json!({"fps":"30000/0"}),
        json!({"fps":1001}),
        json!({"scale":1e100}),
    ] {
        assert!(apply(&mut settings.clone(), &invalid).is_err(), "{invalid}");
    }
}

#[test]
fn typed_settings_preserve_original_defaults_aliases_and_import_save_boundaries() {
    use video_kadr_backend::domain::{
        filmcraft_export_preset_settings::normalize, filmcraft_export_presets::builtins,
    };
    for preset in builtins() {
        assert_eq!(
            normalize(preset.settings.clone(), true).unwrap(),
            preset.settings,
            "{}",
            preset.name
        );
    }
    let defaults = normalize(json!({}), true).unwrap();
    assert_eq!(defaults["bitrateKbps"], 20000);
    assert_eq!(defaults["audio"]["codec"], "auto");
    assert!(defaults["audio"]["sample_rate"].is_null());
    assert_eq!(
        normalize(json!({"format":"H264"}), true).unwrap()["format"],
        "h264"
    );
    for invalid in [
        json!({"quality":256}),
        json!({"audio":{"bits":65536}}),
        json!({"audio":{"channels":"stereo"}}),
        json!({"frameRate":{"num":"24","den":1}}),
        json!({"effects":{"name_overlay":{"placement":"diagonal"}}}),
    ] {
        assert!(normalize(invalid, false).is_err());
    }
    for invalid in [
        json!({"scale":0}),
        json!({"frameSize":[32769,2]}),
        json!({"frameSize":[32768,8193]}),
        json!({"audio":{"sample_rate":384001}}),
        json!({"audio":{"sample_rate":0}}),
        json!({"range":{"start":-1,"duration":1}}),
        json!({"format":"hevc","bitrateMode":"vbr2Pass"}),
        json!({"effects":{"image_overlay":{"enabled":true,"path":""}}}),
    ] {
        // Upstream preset import deserializes; saving invokes validate().
        assert!(
            normalize(invalid.clone(), false).is_ok(),
            "import: {invalid}"
        );
        assert!(normalize(invalid, true).is_err());
    }
    assert!(normalize(
        json!({"frameSize":[32768,8192],"audio":{"sample_rate":384000}}),
        true
    )
    .is_ok());
    assert!(normalize(json!({"format":"wav","fieldOrder":"upperFirst"}), true).is_ok());
    assert!(normalize(json!({"format":"h264","fieldOrder":"upperFirst"}), true).is_err());
}

#[test]
fn match_source_scale_keeps_original_rounding_and_direct_render_geometry() {
    use video_kadr_backend::domain::filmcraft_export_geometry::{
        scaled_source_size, video_dimensions, Geometry, Scaling,
    };
    for (source, scale, expected) in [
        ((161, 91), 0.5, (81, 46)),
        ((1, 1), 0.001, (2, 2)),
        ((63, 35), 1.5, (95, 53)),
        ((1919, 1079), 1., (1919, 1079)),
    ] {
        assert_eq!(scaled_source_size(source, scale).unwrap(), expected);
    }
    let size = video_dimensions(81, 46);
    assert_eq!(size, (80, 46));
    let geometry = Geometry::new((161, 91), size, false, Scaling::ScaleToFit, None, false).unwrap();
    assert!(geometry.content.is_none());
    assert_eq!(geometry.offset, (0, 0));
    assert_eq!(geometry.render_scale.to_bits(), (80f32 / 161.).to_bits());
    for scale in [0., -1., f32::INFINITY, f32::NAN, 100.] {
        assert!(scaled_source_size((161, 91), scale).is_err());
    }
}

#[test]
fn caption_sidecars_match_original_range_oracles_in_all_seven_formats() {
    use filmcraft_captions::{Cue, Document, Format, WriteOptions};
    use filmcraft_time::{FrameRate, Tick};
    use serde_json::Value;
    use video_kadr_backend::domain::{
        filmcraft_export_caption_sidecar::make, graphic_template_fonts::base64_decode,
    };
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/filmcraft-caption-file-timing.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["commit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let cues=case["snapped"]["cues"].as_array().unwrap().iter().enumerate().map(|(id,cue)|json!({"id":id+1,"start":cue["start"],"duration":cue["end"].as_i64().unwrap()-cue["start"].as_i64().unwrap(),"text":cue["text"],"speaker":cue["speaker"],"cue_id":cue["id"],"settings":cue["settings"]})).collect::<Vec<_>>();
        let sequence = json!({"settings":{"frame_rate":case["rate"],"drop_frame":false},"caption_tracks":[{"enabled":false,"captions":cues,"vtt_blocks":case["snapped"]["blocks"]},{"captions":[],"vtt_blocks":[]}]});
        let optional = |value: &Value| value.as_str().map(str::to_owned);
        let original = Document {
            cues: case["exported"]["cues"]
                .as_array()
                .unwrap()
                .iter()
                .map(|cue| Cue {
                    start: Tick(cue["start"].as_i64().unwrap()),
                    end: Tick(cue["end"].as_i64().unwrap()),
                    text: cue["text"].as_str().unwrap().into(),
                    speaker: optional(&cue["speaker"]),
                    id: optional(&cue["id"]),
                    settings: cue["settings"].as_str().unwrap().into(),
                })
                .collect(),
            blocks: serde_json::from_value(case["exported"]["blocks"].clone()).unwrap(),
            warnings: Vec::new(),
        };
        for format in Format::ALL {
            let actual = make(
                &sequence,
                case["offset"].as_i64().unwrap(),
                format.extension(),
            )
            .unwrap()
            .unwrap();
            let rate: FrameRate = serde_json::from_value(case["rate"].clone()).unwrap();
            let expected = filmcraft_captions::write(
                &original,
                format,
                WriteOptions {
                    rate: Some(rate),
                    drop_frame: false,
                },
            );
            assert_eq!(actual.extension, format.extension());
            assert_eq!(
                base64_decode(&actual.bytes_base64).unwrap(),
                expected,
                "{} at {}",
                format.name(),
                case["offset"]
            );
        }
    }
    assert!(make(&json!({"caption_tracks":[]}), 0, "unknown")
        .unwrap()
        .is_none());
    assert!(make(&json!({"caption_tracks":[{"captions":[]}]}), 0, "unknown").is_err());
}

#[test]
fn exact_source_range_retains_original_video_grid_and_sample_floors() {
    use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
    use video_kadr_backend::domain::{
        composition::FrameRateRatio, filmcraft_export_range::SourceRange,
    };
    for start in [
        0,
        1,
        TICKS_PER_SECOND * 3 / 2 + TICKS_PER_SECOND / 1000,
        9_007_199_254_740_993,
    ] {
        let end = start + TICKS_PER_SECOND / 5;
        let range = SourceRange {
            start_ticks: start.to_string(),
            end_ticks: end.to_string(),
        };
        for (n, d) in [(24, 1), (25, 1), (24000, 1001), (7, 3), (60000, 1001)] {
            let rate = FrameRate::new(n, d);
            let first = rate.frame_at(Tick(start));
            let last = rate.frame_at(Tick(end - 1)) + 1;
            assert_eq!(
                range
                    .video_span(FrameRateRatio {
                        numerator: n as u32,
                        denominator: d as u32
                    })
                    .unwrap(),
                ((last - first) as u64, rate.tick_of(first).0)
            );
        }
        for rate in [8000, 44117, 48000, 192000] {
            let (first, count) = range.audio_span(rate).unwrap();
            let a = Tick(start).to_units_floor(i64::from(rate));
            let b = Tick(end).to_units_floor(i64::from(rate));
            assert_eq!(Tick(first).to_units_floor(i64::from(rate)), a);
            assert_eq!(count, (b - a) as u64);
        }
    }
    for (start, end) in [(-1, 1), (0, 0), (2, 1), (0, Tick::MAX.0 + 1)] {
        assert!(SourceRange {
            start_ticks: start.to_string(),
            end_ticks: end.to_string()
        }
        .bounds()
        .is_err());
    }
    assert!(serde_json::from_value::<SourceRange>(json!({"startTicks":0,"endTicks":"1"})).is_err());
}

#[test]
fn quick_export_range_keeps_original_inclusive_out_work_area_and_minimum() {
    use serde_json::Value;
    use video_kadr_backend::domain::{
        filmcraft_export_range::SourceRange, filmcraft_project_file::SourceProjectDocument,
        filmcraft_quick_export_range::resolve,
    };
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/filmcraft-caption-sequence.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut raw = fixture["cases"][0]["document"].clone();
    let sequence = &mut raw["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["video_tracks"] = json!([]);
    sequence["audio_tracks"] = json!([]);
    sequence["caption_tracks"] = json!([]);
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    sequence["mark_in"] = Value::Null;
    sequence["mark_out"] = Value::Null;
    sequence["work_area"] = Value::Null;
    let fd = 10584000000i64;
    let read =
        |value: &Value| SourceProjectDocument::read(&serde_json::to_vec(value).unwrap()).unwrap();
    assert_eq!(
        resolve(&read(&raw), 6, None, None)
            .unwrap()
            .bounds()
            .unwrap(),
        (0, fd)
    );
    raw["project"]["items"]["6"]["kind"]["Sequence"]["mark_in"] = json!(fd * 2);
    raw["project"]["items"]["6"]["kind"]["Sequence"]["mark_out"] = json!(fd * 4);
    let document = read(&raw);
    assert_eq!(
        resolve(&document, 6, None, None).unwrap().bounds().unwrap(),
        (fd * 2, fd * 5)
    );
    assert_eq!(
        resolve(&document, 6, Some("entire sequence"), None)
            .unwrap()
            .bounds()
            .unwrap(),
        (0, fd)
    );
    assert!(resolve(&document, 6, Some("workArea"), None).is_err());
    raw["project"]["items"]["6"]["kind"]["Sequence"]["work_area"] =
        json!({"start":fd*3,"duration":fd*2});
    let document = read(&raw);
    assert_eq!(
        resolve(&document, 6, Some("work_area"), None)
            .unwrap()
            .bounds()
            .unwrap(),
        (fd * 3, fd * 5)
    );
    let custom = SourceRange {
        start_ticks: "1".into(),
        end_ticks: "2".into(),
    };
    assert_eq!(
        resolve(&document, 6, None, Some(&custom))
            .unwrap()
            .bounds()
            .unwrap(),
        (1, 2)
    );
    assert!(resolve(&document, 6, Some("custom"), None).is_err());
}

#[test]
fn original_library_collisions_filters_and_portable_round_trip() {
    let mut library = Library::default();
    assert_eq!(library.all().len(), 30);
    assert!(
        library
            .find("match source adaptive HIGH bitrate")
            .unwrap()
            .builtin
    );
    assert!(library.save(review(DEFAULT_PRESET), true).is_err());
    library.save(review(" Review – 720p "), true).unwrap();
    assert!(library.save(review("review720P"), false).is_err());
    let preset = library.find("REVIEW 720p").unwrap();
    assert_eq!(preset.settings["path"], "");
    assert!(preset.settings["range"].is_null());
    assert!(library.favorite("review720P", None).unwrap());
    assert!(library.favorite("Apple ProRes 422 HQ", Some(true)).unwrap());
    assert_eq!(
        library
            .list("client", Some("user"), Some("h264"), true)
            .len(),
        1
    );
    assert_eq!(library.list("prores", None, None, false).len(), 5);
    let file = library
        .export(Some(&["Review 720p".into(), "Apple ProRes 422 HQ".into()]))
        .unwrap();
    assert_eq!(file.favorites.len(), 2);
    library.delete("review720p").unwrap();
    assert!(!library.is_favorite("review720p"));
    let imported = library.import(&serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(
        imported,
        vec!["Review – 720p", "Apple ProRes 422 HQ (imported)"]
    );
    assert!(!library.find(&imported[1]).unwrap().builtin);
    assert!(!library.is_favorite("review720p"));
    assert_eq!(
        library.find("Review720p").unwrap().settings["frameSize"],
        json!([1280, 720])
    );
    let before = serde_json::to_value(&library).unwrap();
    assert!(library
        .import(br#"{"format":"filmcraft.export-presets","version":2,"presets":[]}"#)
        .is_err());
    assert_eq!(serde_json::to_value(&library).unwrap(), before);
}

#[tokio::test]
async fn persistence_owner_isolation_rollback_and_concurrent_updates() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(directory.path()).await.unwrap();
    db.edit_filmcraft_export_presets("alice", |l| l.save(review("Review 720p"), true))
        .await
        .unwrap();
    assert!(db
        .filmcraft_export_presets("bob")
        .await
        .unwrap()
        .presets
        .is_empty());
    let reopened = Db::open(directory.path()).await.unwrap();
    assert_eq!(
        reopened
            .filmcraft_export_presets("alice")
            .await
            .unwrap()
            .presets
            .len(),
        1
    );
    let failed: anyhow::Result<()> = reopened
        .edit_filmcraft_export_presets("alice", |l| {
            l.delete("Review720p")?;
            anyhow::bail!("rollback")
        })
        .await;
    assert!(failed.is_err());
    assert!(db
        .filmcraft_export_presets("alice")
        .await
        .unwrap()
        .find("Review720p")
        .is_some());
    let (a, b) = tokio::join!(
        db.edit_filmcraft_export_presets("alice", |l| l.favorite("Review720p", None)),
        reopened.edit_filmcraft_export_presets("alice", |l| l.favorite("Review720p", None))
    );
    a.unwrap();
    b.unwrap();
    assert!(!db
        .filmcraft_export_presets("alice")
        .await
        .unwrap()
        .is_favorite("Review720p"));
}

#[tokio::test]
async fn authenticated_http_library_preserves_source_settings_and_owner_boundaries() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use serde_json::Value;
    use tower::ServiceExt;
    let (state, _directory) = support::make_state(false, false).await;
    let alice = state
        .db
        .register_auth_user("preset-alice", "test password")
        .await
        .unwrap()
        .unwrap();
    let bob = state
        .db
        .register_auth_user("preset-bob", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    async fn call(app: axum::Router, token: Option<&str>, body: Value) -> (u16, Value) {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/compositions/filmcraft/export-presets")
            .header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = app
            .oneshot(request.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        let status = response.status().as_u16();
        let value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        (status, value)
    }
    assert_eq!(
        call(
            app.clone(),
            None,
            json!({"action":"get","name":DEFAULT_PRESET})
        )
        .await
        .0,
        401
    );
    let saved=call(app.clone(),Some(&alice.token),json!({"action":"save","name":"Review 720p","from":"YouTube 1080p Full HD","description":"client review","settings":{"frameSize":[1280,720],"bitrateKbps":5000,"audio":{"channels":6},"path":"/private/file.mp4","range":{"start":0,"duration":42}}})).await;
    assert_eq!(saved.0, 200, "{saved:?}");
    let got = call(
        app.clone(),
        Some(&alice.token),
        json!({"action":"get","name":"REVIEW-720P"}),
    )
    .await;
    assert_eq!(got.0, 200);
    assert_eq!(got.1["builtin"], false);
    assert_eq!(got.1["settings"]["frameSize"], json!([1280, 720]));
    assert_eq!(got.1["settings"]["audio"]["sample_rate"], 48000);
    assert_eq!(got.1["settings"]["audio"]["channels"], 6);
    assert_eq!(got.1["settings"]["path"], "");
    assert!(got.1["settings"]["range"].is_null());
    assert_eq!(
        call(
            app.clone(),
            Some(&bob.token),
            json!({"action":"get","name":"Review720p"})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        call(
            app.clone(),
            Some(&alice.token),
            json!({"action":"save","name":"review720p","overwrite":false})
        )
        .await
        .0,
        400
    );
    assert_eq!(
        call(
            app.clone(),
            Some(&alice.token),
            json!({"action":"favorite","name":"review720p"})
        )
        .await
        .1["favorite"],
        true
    );
    let response=app.clone().oneshot(Request::builder().uri("/api/compositions/filmcraft/export-presets?query=client&favorites=true&category=user&format=h264").header("authorization",format!("Bearer {}",alice.token)).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), 200);
    let list: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(list["presets"].as_array().unwrap().len(), 1);
    let file = call(
        app.clone(),
        Some(&alice.token),
        json!({"action":"export","names":["review720p"]}),
    )
    .await;
    assert_eq!(file.0, 200);
    assert_eq!(file.1["format"], "filmcraft.export-presets");
    assert_eq!(
        call(
            app.clone(),
            Some(&bob.token),
            json!({"action":"import","file":file.1})
        )
        .await
        .0,
        200
    );
    let imported = call(
        app.clone(),
        Some(&bob.token),
        json!({"action":"get","name":"review720p"}),
    )
    .await;
    assert_eq!(imported.0, 200);
    assert_eq!(imported.1["favorite"], false);
    assert_eq!(imported.1["settings"], got.1["settings"]);
    let flat=call(app.clone(),Some(&alice.token),json!({"action":"save","name":"Flat review","from":DEFAULT_PRESET,"settings":{"frameSize":[320,240],"bitrateKbps":9999},"width":1280,"height":720,"fps":"30000/1001","bitrateKbps":5000})).await;
    assert_eq!(flat.0, 200, "{flat:?}");
    let flat = call(
        app.clone(),
        Some(&alice.token),
        json!({"action":"get","name":"Flat review"}),
    )
    .await;
    assert_eq!(flat.1["settings"]["frameSize"], json!([1280, 720]));
    assert_eq!(
        flat.1["settings"]["frameRate"],
        json!({"num":30000,"den":1001})
    );
    assert_eq!(flat.1["settings"]["bitrateKbps"], 5000);
    assert!(flat.1["settings"]["adaptiveBitrate"].is_null());
    assert_eq!(
        call(
            app.clone(),
            Some(&alice.token),
            json!({"action":"save","name":"Bad flat","width":1280})
        )
        .await
        .0,
        400
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
