mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_image_sequence::{self, Numbered},
    filmcraft_project_file::SourceProjectDocument,
    group_surface::GroupSurface,
    warp_stabilizer::{Affine, Vec2},
};
const FD: i64 = 10_584_000_000;
#[test]
fn original_numbering_padding_suffix_gaps_and_candidate_order() {
    let names = [
        "shot_0007.png",
        "shot_0008.png",
        "shot_0010.png",
        "shot_8.png",
        "shot_0009.PNG",
        "shot_000010.png",
        "xshot_0009.png",
    ]
    .map(str::to_owned);
    let frames = filmcraft_image_sequence::frames("/shots/shot_0008.png", &names).unwrap();
    assert_eq!(
        frames,
        vec![
            Some("/shots/shot_0008.png".into()),
            None,
            Some("/shots/shot_0010.png".into())
        ]
    );
    let n = Numbered::parse("C:\\素材\\s99.Left.PNG").unwrap();
    assert_eq!(n.dir, "C:\\素材\\");
    assert_eq!(n.number_of("s100.Left.PNG"), Some(100));
    assert_eq!(n.number_of("s0100.Left.PNG"), None);
    for name in [
        "shot.png",
        "file1.wav",
        "shot1.exr",
        "shot1234567890123456789.png",
    ] {
        assert!(Numbered::parse(name).is_none())
    }
    let rate = filmcraft_time::FrameRate { num: 24, den: 1 };
    assert_eq!(
        filmcraft_image_sequence::candidates(&frames, rate, FD * 2 - 1),
        vec![0, 2]
    );
    assert_eq!(
        filmcraft_image_sequence::candidates(&frames, rate, FD * 2),
        vec![2, 0]
    );
    assert_eq!(
        filmcraft_image_sequence::candidates(&frames, rate, -1),
        vec![0, 2]
    );
    assert_eq!(
        filmcraft_image_sequence::candidates(&frames, rate, i64::MAX),
        vec![2, 0]
    );
    let many = (0..10).map(|n| Some(n.to_string())).collect::<Vec<_>>();
    assert_eq!(
        filmcraft_image_sequence::candidates(&many, rate, FD * 8),
        vec![8, 7, 6, 5, 4, 3, 2, 1]
    );
    let capped = filmcraft_image_sequence::frames(
        "x0.png",
        &["x0.png".into(), "x999999999999999999.png".into()],
    )
    .unwrap();
    assert_eq!(capped.len(), 1_000_001);
    assert!(capped.last().unwrap().is_none());
}
fn source() -> SourceProjectDocument {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut d = fixture["cases"][0]["document"].clone();
    let m = &mut d["project"]["items"]["1"]["kind"]["Media"];
    m["media"] = json!({"File":{"path":"/untrusted/not-a-sequence.mov"}});
    m["info"]["kind"] = json!("ImageSequence");
    m["info"]["video"]["width"] = json!(240);
    m["info"]["video"]["height"] = json!(180);
    m["info"]["video"]["frame_rate"] = json!({"num":24,"den":1});
    let q = &mut d["project"]["items"]["6"]["kind"]["Sequence"];
    q["settings"]["width"] = json!(240);
    q["settings"]["height"] = json!(180);
    let c = &mut q["video_tracks"][0]["items"][0];
    c["start"] = json!(0);
    c["source_in"] = json!(0);
    c["duration"] = json!(FD * 12);
    c["speed"] = json!(1.);
    c["effects"] = json!([{"effect":"opacity","enabled":true,"params":{"opacity":{"value":{"Float":100.}},"blend":{"value":{"Choice":0}}},"masks":[{"name":"Track","mode":"Add","track_method":"PositionScaleRotation","path":{"value":{"Path":{"closed":true,"vertices":[{"p":{"x":40.,"y":30.}},{"p":{"x":200.,"y":30.}},{"p":{"x":200.,"y":150.}},{"p":{"x":40.,"y":150.}}]}}},"feather":{"value":{"Float":0.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}}}]}]);
    SourceProjectDocument::read(&serde_json::to_vec(&d).unwrap()).unwrap()
}
fn frame(dx: f64, dy: f64) -> GroupSurface {
    let inv = Affine::motion(
        Vec2::new(120. + dx, 90. + dy),
        Vec2::new(1., 1.),
        0.,
        Vec2::new(120., 90.),
    )
    .inverse()
    .unwrap();
    let mut rgba = vec![];
    for y in 0..180 {
        for x in 0..240 {
            let p = inv.apply(Vec2::new(x as f64, y as f64));
            let v = (0.5
                + 0.2 * (p.x * 0.21).sin() * (p.y * 0.17).cos()
                + 0.1 * ((p.x + p.y) * 0.43).sin()
                + 0.08 * (p.x * 0.9 - p.y * 0.6).sin())
            .clamp(0., 1.);
            let n = (v * 255.).round() as u8;
            rgba.extend([n, n, n, 255]);
        }
    }
    GroupSurface {
        width: 240,
        height: 180,
        rgba,
    }
}
async fn add(
    state: &video_kadr_backend::state::AppState,
    dir: &std::path::Path,
    number: u32,
    dx: f64,
    dy: f64,
) {
    let filename = format!("shot_{number:04}.png");
    let f = frame(dx, dy);
    image::save_buffer(
        dir.join("sources").join(&filename),
        &f.rgba,
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    assert!(
        state
            .library
            .add(video_kadr_backend::library::MediaEntry::from_result(
                "source",
                &json!({"id":format!("seq{number}"),"filename":filename,"mediaType":"image"})
            ))
            .await
    );
}
fn input(text: &str, time: i64) -> Value {
    json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":"seq8"},"direction":"forward","timeTicks":time.to_string()})
}
async fn post(app: &axum::Router, input: Value) -> (axum::http::StatusCode, Value) {
    post_token(app, input, None).await
}
async fn post_token(
    app: &axum::Router,
    input: Value,
    token: Option<&str>,
) -> (axum::http::StatusCode, Value) {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/compositions/filmcraft/track-mask")
        .header("content-type", "application/json")
        .body(Body::from(input.to_string()))
        .unwrap();
    if let Some(token) = token {
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}
#[tokio::test]
async fn authorized_sequence_tracks_motion_holes_and_corrupt_frame_fallback() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    add(&state, dir.path(), 11, 9., -6.).await;
    let app = support::router(state.clone());
    let doc = source();
    let mut text = String::from_utf8(doc.encode_preserved().unwrap()).unwrap();
    for (step, expected_x, expected_y) in [(0, 43., 28.), (1, 43., 28.), (2, 49., 24.)] {
        if step == 2 {
            std::fs::write(dir.path().join("sources/shot_0010.png"), b"corrupt").unwrap();
            assert!(
                state
                    .library
                    .add(video_kadr_backend::library::MediaEntry::from_result(
                        "source",
                        &json!({"id":"seq10","filename":"shot_0010.png","mediaType":"image"})
                    ))
                    .await
            );
        }
        let (status, result) = post(&app, input(&text, FD * step)).await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["tracking"]["status"], "tracked");
        assert_eq!(
            result["tracking"]["sourceTicks"],
            (FD * (step + 1)).to_string()
        );
        text = result["documentText"].as_str().unwrap().into();
        let edited: Value = serde_json::from_str(&text).unwrap();
        let keys=edited.pointer("/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks/0/path/keyframes").unwrap().as_array().unwrap();
        let p = &keys.last().unwrap()["value"]["Path"]["vertices"][0]["p"];
        assert!((p["x"].as_f64().unwrap() - expected_x).abs() < 0.8);
        assert!((p["y"].as_f64().unwrap() - expected_y).abs() < 0.8);
    }
}
#[tokio::test]
async fn sequence_constructor_and_eight_candidate_limit_refuse_without_publication() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    for number in 9..=16 {
        let filename = format!("shot_{number:04}.png");
        std::fs::write(dir.path().join("sources").join(&filename), b"corrupt").unwrap();
        state
            .library
            .add(video_kadr_backend::library::MediaEntry::from_result(
                "source",
                &json!({"id":format!("seq{number}"),"filename":filename,"mediaType":"image"}),
            ))
            .await;
    }
    let app = support::router(state);
    let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    let (status, result) = post(&app, input(&text, FD * 7)).await;
    assert_eq!(status, 400);
    assert!(result.get("documentText").is_none());
    std::fs::write(dir.path().join("sources/shot_0008.png"), b"first corrupt").unwrap();
    let (status, result) = post(&app, input(&text, 0)).await;
    assert_eq!(status, 400);
    assert!(result.get("documentText").is_none());
}

fn last_point(result: &Value) -> (f64, f64) {
    let doc: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    let keys=doc.pointer("/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks/0/path/keyframes").unwrap().as_array().unwrap();
    let p = &keys.last().unwrap()["value"]["Path"]["vertices"][0]["p"];
    (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap())
}
#[tokio::test]
async fn uploaded_uuid_files_use_preserved_original_names_or_legacy_upload_titles() {
    for legacy in [false, true] {
        let (state, dir) = support::make_state(false, false).await;
        let ids = [
            "11111111-1111-4111-8111-111111111111",
            "22222222-2222-4222-8222-222222222222",
        ];
        for (i, id) in ids.iter().enumerate() {
            let filename = format!("{id}.png");
            let f = frame(i as f64 * 3., i as f64 * -2.);
            image::save_buffer(
                dir.path().join("sources").join(&filename),
                &f.rgba,
                240,
                180,
                image::ColorType::Rgba8,
            )
            .unwrap();
            let mut entry = json!({"id":id,"filename":filename,"mediaType":"image","title":if legacy {format!("shot_{:04}",8+i)} else {"Renamed display title".into()}});
            if !legacy {
                entry["originalFilename"] = json!(format!("shot_{:04}.png", 8 + i));
            }
            state
                .library
                .add(video_kadr_backend::library::MediaEntry::from_result(
                    "source", &entry,
                ))
                .await;
        }
        let app = support::router(state);
        let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
        let mut request = input(&text, 0);
        request["sources"]["1"] = json!(ids[0]);
        let (status, result) = post(&app, request).await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(result["tracking"]["status"], "tracked");
        let (x, y) = last_point(&result);
        assert!((x - 43.).abs() < 0.55);
        assert!((y - 28.).abs() < 0.55);
    }
}

#[tokio::test]
async fn private_sequence_frames_are_absent_from_unprivileged_discovery() {
    let (state, dir) = support::make_state(false, false).await;
    for (number, x, y) in [(8, 0., 0.), (9, 3., -2.), (10, 6., -4.), (11, 9., -6.)] {
        add(&state, dir.path(), number, x, y).await;
    }
    let owner = state
        .db
        .register_auth_user("sequence-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("sequence-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Private sequence frame")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Frame claim",
            &json!({"schemaVersion":1,"sources":{"seq10":{"id":"seq10"}}}),
            &["seq10".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state);
    let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    for token in [None, Some(outsider.token.as_str())] {
        let (status, result) = post_token(&app, input(&text, FD), token).await;
        assert_eq!(status, 200, "{result}");
        let (x, y) = last_point(&result);
        assert!((x - 40.).abs() < 1e-6);
        assert!((y - 30.).abs() < 1e-6);
    }
    let (status, result) = post_token(&app, input(&text, FD), Some(&owner.token)).await;
    assert_eq!(status, 200, "{result}");
    let (x, y) = last_point(&result);
    assert!((x - 40.).abs() < 1e-6);
    assert!((y - 30.).abs() < 1e-6);
    let mut first_private = input(&text, 0);
    first_private["sources"]["1"] = json!("seq10");
    assert_eq!(post(&app, first_private.clone()).await.0, 401);
    assert_eq!(
        post_token(&app, first_private.clone(), Some(&outsider.token))
            .await
            .0,
        403
    );
    assert_eq!(
        post_token(&app, first_private, Some(&owner.token)).await.0,
        200
    );
}

#[tokio::test]
async fn sequence_discovery_keeps_storage_scopes_separate_and_refuses_ambiguous_names() {
    use video_kadr_backend::library::MediaEntry;
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let foreign = dir.path().join("sources/spaces/foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    let f = frame(6., -4.);
    image::save_buffer(
        foreign.join("shot_0010.png"),
        &f.rgba,
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    state.library.add(MediaEntry::from_result("source",&json!({"id":"foreign","filename":"shot_0010.png","storageKey":"spaces/foreign/shot_0010.png","mediaType":"image"}))).await;
    let app = support::router(state.clone());
    let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    let (status, result) = post(&app, input(&text, FD)).await;
    assert_eq!(status, 200, "{result}");
    let (x, y) = last_point(&result);
    assert!((x - 40.).abs() < 1e-6);
    assert!((y - 30.).abs() < 1e-6);
    let f = frame(50., 0.);
    image::save_buffer(
        dir.path().join("sources/duplicate.png"),
        &f.rgba,
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    state.library.add(MediaEntry::from_result("source",&json!({"id":"duplicate","filename":"duplicate.png","originalFilename":"shot_0009.png","mediaType":"image"}))).await;
    let (status, result) = post(&app, input(&text, 0)).await;
    assert_eq!(status, 400);
    assert!(result.get("documentText").is_none());
}

#[tokio::test]
async fn private_sequence_tracks_after_authorized_storage_migration() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let owner = state
        .db
        .register_auth_user("private-sequence-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("private-sequence-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Sequence")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Sequence claim",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"},"seq9":{"id":"seq9"}}}),
            &["seq8".into(), "seq9".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    assert_eq!(post(&app, input(&text, 0)).await.0, 401);
    assert_eq!(
        post_token(&app, input(&text, 0), Some(&outsider.token))
            .await
            .0,
        403
    );
    let (status, result) = post_token(&app, input(&text, 0), Some(&owner.token)).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["tracking"]["status"], "tracked");
    let (x, y) = last_point(&result);
    assert!((x - 43.).abs() < 0.55);
    assert!((y - 28.).abs() < 0.55);
    for id in ["seq8", "seq9"] {
        assert!(state
            .library
            .get(id)
            .await
            .unwrap()
            .storage_key
            .unwrap()
            .starts_with(&format!("spaces/{}/", space.id)));
    }
}

#[tokio::test]
async fn raw_sequence_frames_keep_original_full_resolution_tracker_analysis() {
    use video_kadr_backend::domain::motion_tracker::{self, Gray, Prepared, TrackMethod};
    let (state, dir) = support::make_state(false, false).await;
    let mut pictures = vec![];
    for (index, dx, dy) in [(0, 0., 0.), (1, 3., -2.)] {
        let mut rgba = vec![];
        for y in 0..721 {
            for x in 0..1101 {
                let x = x as f64 - dx;
                let y = y as f64 - dy;
                let v = (0.5
                    + 0.2 * (x * 0.21).sin() * (y * 0.17).cos()
                    + 0.1 * ((x + y) * 0.43).sin()
                    + 0.08 * (x * 0.9 - y * 0.6).sin())
                .clamp(0., 1.);
                let n = (v * 255.).round() as u8;
                rgba.extend([n, n, n, 255]);
            }
        }
        let filename = format!("shot_{:04}.png", 8 + index);
        image::save_buffer(
            dir.path().join("sources").join(&filename),
            &rgba,
            1101,
            721,
            image::ColorType::Rgba8,
        )
        .unwrap();
        state
            .library
            .add(video_kadr_backend::library::MediaEntry::from_result(
                "source",
                &json!({"id":format!("seq{}",8+index),"filename":filename,"mediaType":"image"}),
            ))
            .await;
        pictures.push(rgba);
    }
    let region = vec![
        Vec2::new(100., 100.),
        Vec2::new(1000., 100.),
        Vec2::new(1000., 650.),
        Vec2::new(100., 650.),
    ];
    let a = Prepared::new(Gray::from_rgba8(1101, 721, &pictures[0]));
    let b = Prepared::new(Gray::from_rgba8(1101, 721, &pictures[1]));
    let expected = motion_tracker::track_step(&a, &b, &region, TrackMethod::PositionScaleRotation)
        .expect("detailed raw sequence must track");
    let mut doc = source();
    let video = &mut doc.envelope["project"]["items"]["1"]["kind"]["Media"]["info"]["video"];
    video["width"] = json!(1101);
    video["height"] = json!(721);
    doc.envelope
        .pointer_mut(
            "/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks/0/path/value",
        )
        .unwrap()["Path"]["vertices"] = json!(region
        .iter()
        .map(|p| json!({"p":{"x":p.x,"y":p.y}}))
        .collect::<Vec<_>>());
    let text = String::from_utf8(doc.encode_preserved().unwrap()).unwrap();
    let app = support::router(state);
    let (status, result) = post(&app, input(&text, 0)).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["tracking"]["inliers"], expected.inliers);
    let actual: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    let vertices=&actual.pointer("/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks/0/path/keyframes").unwrap()[1]["value"]["Path"]["vertices"];
    for (i, p) in region.iter().enumerate() {
        let expected = expected.transform.apply(*p);
        assert!((vertices[i]["p"]["x"].as_f64().unwrap() - expected.x).abs() < 1e-10);
        assert!((vertices[i]["p"]["y"].as_f64().unwrap() - expected.y).abs() < 1e-10);
    }
}

#[tokio::test]
async fn sequence_frame_dimension_changes_lose_tracking_without_publishing_keys() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    image::save_buffer(
        dir.path().join("sources/shot_0009.png"),
        &vec![128; 120 * 180 * 4],
        120,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    state
        .library
        .add(video_kadr_backend::library::MediaEntry::from_result(
            "source",
            &json!({"id":"seq9","filename":"shot_0009.png","mediaType":"image"}),
        ))
        .await;
    let app = support::router(state);
    let text = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    let (status, result) = post(&app, input(&text, 0)).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["tracking"]["status"], "lost");
    assert_eq!(result["documentText"], text);
}

async fn preview(app: &axum::Router, document: &Value, time: i64, binding: &str) -> Value {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let request=Request::builder().method("POST").uri("/api/compositions/filmcraft/preview-frame")
        .header("content-type","application/json")
        .body(Body::from(json!({"document":document.to_string(),"sequenceId":"6","timeTicks":time.to_string(),"sources":{"1":binding}}).to_string())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body: Value = serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn sequence_preview_matches_still_frames_with_gaps_effects_and_source_retiming() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    add(&state, dir.path(), 11, 9., -6.).await;
    let app = support::router(state.clone());
    let original: Value = serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut pixels = Vec::new();
    for (speed, source_in, time, selected) in [
        (1., 0, 0, "seq8"),
        (1., 0, FD, "seq9"),
        (1., 0, FD * 2, "seq9"),
        (1., 0, FD * 3, "seq11"),
        (0., FD, FD * 4, "seq9"),
        (-1., 0, FD, "seq9"),
    ] {
        let mut sequence = original.clone();
        let clip = &mut sequence["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]
            ["items"][0];
        clip["speed"] = json!(speed);
        clip["source_in"] = json!(source_in);
        let mut still = sequence.clone();
        still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
        let actual = preview(&app, &sequence, time, "seq8").await;
        let expected = preview(&app, &still, time, selected).await;
        assert!(
            actual["rgbaHex"] == expected["rgbaHex"],
            "speed {speed}, source_in {source_in}, time {time}"
        );
        pixels.push(actual["rgbaHex"].clone());
    }
    let mut reverse = original.clone();
    let clip =
        &mut reverse["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0];
    clip["reverse"] = json!(true);
    clip["duration"] = json!(FD * 4);
    let mut still = reverse.clone();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    assert!(
        preview(&app, &reverse, FD * 2, "seq8").await["rgbaHex"]
            == preview(&app, &still, FD * 2, "seq9").await["rgbaHex"],
        "reverse samples the last tick before the frame boundary"
    );
    assert_ne!(pixels[0], pixels[1]);
    assert_ne!(pixels[1], pixels[3]);
    // A later source frame must not be frozen by a previous preview request.
    let before = preview(&app, &original, FD, "seq8").await;
    let changed = frame(12., 5.);
    image::save_buffer(
        dir.path().join("sources/shot_0009.png"),
        &changed.rgba,
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let after = preview(&app, &original, FD, "seq8").await;
    assert_ne!(before["rgbaHex"], after["rgbaHex"]);
    let mut still = original.clone();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    assert_eq!(
        after["rgbaHex"],
        preview(&app, &still, FD, "seq9").await["rgbaHex"]
    );
}

#[tokio::test]
async fn sequence_stabilization_reuses_paths_and_invalidates_nonfirst_frame_revision() {
    let (state, dir) = support::make_state(false, false).await;
    for n in 8..20 {
        add(&state, dir.path(), n, (n - 8) as f64, 0.).await;
    }
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    // The oracle also contains 198 MB of pixel vectors, unused by this cache test.
    #[derive(serde::Deserialize)]
    struct OracleCase { document: Value }
    #[derive(serde::Deserialize)]
    struct Oracle { cases: Vec<OracleCase> }
    let fixture_started=std::time::Instant::now();
    let oracle: Oracle = serde_json::from_reader(std::io::BufReader::new(flate2::read::GzDecoder::new(
        std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/filmcraft-source-stabilizer.json.gz"
        ))
        .unwrap(),
    )))
    .unwrap();
    eprintln!("stabilizer fixture configuration loaded in {:?}",fixture_started.elapsed());
    document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]
        ["effects"] = oracle.cases.first().unwrap().document["project"]["items"]["6"]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0]["effects"]
        .clone();
    let app = support::router(state.clone());
    eprintln!("starting first stabilization preview");
    let preview_started=std::time::Instant::now();
    let first = preview(&app, &document, FD * 4, "seq8").await;
    eprintln!("first stabilization preview {:?}",preview_started.elapsed());
    let hits = state
        .filmcraft_video_previews
        .stabilization_path_cache_stats()
        .unwrap()
        .2;
    eprintln!("starting cached stabilization preview");
    assert_eq!(first, preview(&app, &document, FD * 4, "seq8").await);
    assert_eq!(
        state
            .filmcraft_video_previews
            .stabilization_path_cache_stats()
            .unwrap()
            .2,
        hits + 1
    );
    let changed = frame(-7., 8.);
    image::save_buffer(
        dir.path().join("sources/shot_0009.png"),
        &changed.rgba,
        240,
        180,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let hits = state
        .filmcraft_video_previews
        .stabilization_path_cache_stats()
        .unwrap()
        .2;
    eprintln!("starting revised-frame stabilization preview");
    let revised = preview(&app, &document, FD * 4, "seq8").await;
    assert_eq!(
        state
            .filmcraft_video_previews
            .stabilization_path_cache_stats()
            .unwrap()
            .2,
        hits,
        "nonfirst frame mutation must invalidate analysis"
    );
    let mut cold = state.clone();
    cold.filmcraft_video_previews = std::sync::Arc::new(
        video_kadr_backend::services::filmcraft_video_preview::VideoPreviewCache::with_storage(
            dir.path().join("independent-preview-cache"),
        ),
    );
    assert_eq!(
        revised,
        preview(&support::router(cold), &document, FD * 4, "seq8").await
    );
}

#[tokio::test]
async fn private_sequence_background_analysis_uses_owner_and_rechecks_revocation() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{config::resource_classes::ResourceClass, model::JobStatus};
    async fn terminal(
        state: &video_kadr_backend::state::AppState,
        id: &str,
    ) -> video_kadr_backend::model::Job {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let job = state.get_job(id).await.unwrap().unwrap();
                if matches!(
                    job.status,
                    JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                ) {
                    return job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    let (state, dir) = support::make_state(false, false).await;
    for n in 8..14 {
        add(&state, dir.path(), n, (n - 8) as f64, 0.).await;
    }
    let owner = state
        .db
        .register_auth_user("sequence-analysis-owner", "analysis test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("sequence-analysis-outsider", "analysis test password")
        .await
        .unwrap()
        .unwrap();
    let admin = state
        .db
        .register_auth_user("sequence-space-admin", "analysis test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&admin.user.username, "Private image sequence")
        .await
        .unwrap();
    state
        .db
        .set_space_member(
            &space.id,
            &admin.user.username,
            &owner.user.username,
            video_kadr_backend::db::SpaceRole::Editor,
        )
        .await
        .unwrap()
        .unwrap();
    let mut ids: Vec<String> = (8..14).map(|n| format!("seq{n}")).collect();
    ids.sort();
    state
        .db
        .create_space_composition_project(
            "Private sequence",
            &json!({"schemaVersion":1,"sources":ids.iter().map(|id|(id.clone(),json!({"id":id}))).collect::<serde_json::Map<String,Value>>()}),
            &ids,
            &admin.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    document["project"]["items"]["1"]["kind"]["Media"]["info"]["duration"] = json!(FD * 6);
    document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]
        ["duration"] = json!(FD * 6);
    let oracle: Value = serde_json::from_reader(flate2::read::GzDecoder::new(
        std::fs::File::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/filmcraft-source-stabilizer.json.gz"
        ))
        .unwrap(),
    ))
    .unwrap();
    document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]
        ["effects"] = oracle["cases"][0]["document"]["project"]["items"]["6"]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0]["effects"]
        .clone();
    let input = json!({"document":document.to_string(),"sequenceId":"6","timeTicks":FD.to_string(),"sources":{"1":"seq8"}});
    let app = support::router(state.clone());
    let make = |token: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/compositions/filmcraft/analysis-jobs")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(input.to_string()))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(make(&outsider.token))
            .await
            .unwrap()
            .status(),
        403
    );
    let response = app.clone().oneshot(make(&owner.token)).await.unwrap();
    assert_eq!(response.status(), 202);
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let job = terminal(&state, id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    assert_eq!(job.result.unwrap()["ready"], true);
    for id in &ids {
        assert!(state
            .library
            .get(id)
            .await
            .unwrap()
            .storage_key
            .unwrap()
            .starts_with(&format!("spaces/{}/", space.id)));
    }
    // Every numbered member, not only the explicit root binding, belongs to the queued snapshot.
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        permits.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let response = app.clone().oneshot(make(&owner.token)).await.unwrap();
    assert_eq!(response.status(), 202);
    let submitted: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let changed_id = submitted["jobId"].as_str().unwrap();
    let requests = state
        .job_store
        .active_requests(video_kadr_backend::jobs::JobKind::Stabilization, 256)
        .await
        .unwrap();
    let payload = &requests
        .iter()
        .find(|r| r.job.id == changed_id)
        .unwrap()
        .payload;
    assert!(payload["sequence_snapshots"]["seq8"].is_array());
    assert!(!payload.to_string().contains(&owner.token));
    let entry = state.library.get("seq9").await.unwrap();
    let path = state.library.resolve_media_path(&entry).await.unwrap();
    let changed = frame(-9., 7.);
    image::save_buffer(path, &changed.rgba, 240, 180, image::ColorType::Rgba8).unwrap();
    drop(permits);
    let failed = terminal(&state, changed_id).await;
    assert_eq!(failed.status, JobStatus::Error, "{failed:?}");
    assert_eq!(failed.error_kind.as_deref(), Some("validation"));
    assert!(failed.result.is_none());
    // Queue a second job, revoke the persisted owner's Space membership, then let it execute.
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Analysis) {
        permits.push(
            state
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let response = app.clone().oneshot(make(&owner.token)).await.unwrap();
    assert_eq!(response.status(), 202);
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = result["jobId"].as_str().unwrap();
    assert_eq!(
        state
            .db
            .remove_space_member(&space.id, &admin.user.username, &owner.user.username)
            .await
            .unwrap(),
        Some(true)
    );
    drop(permits);
    let failed = terminal(&state, id).await;
    assert_eq!(failed.status, JobStatus::Error, "{failed:?}");
    assert!(failed.result.is_none());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

async fn export_png(
    app: &axum::Router,
    document: &Value,
    time: i64,
    binding: &str,
    token: Option<&str>,
) -> (axum::http::StatusCode, Vec<u8>) {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/compositions/filmcraft/export-frame")
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response=app.clone().oneshot(request.body(Body::from(json!({"document":document.to_string(),"sequenceId":"6","timeTicks":time.to_string(),"sources":{"1":binding}}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    if status.is_success() {
        assert_eq!(response.headers()["content-type"], "image/png");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("frame.png"));
    }
    (
        status,
        to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}
#[tokio::test]
async fn full_size_png_export_samples_sequence_frames_and_enforces_private_access() {
    let (state, dir) = support::make_state(false, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let app = support::router(state.clone());
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let settings = &mut document["project"]["items"]["6"]["kind"]["Sequence"]["settings"];
    settings["width"] = json!(960);
    settings["height"] = json!(720);
    let mut still = document.clone();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    let (status, bytes) = export_png(&app, &document, FD, "seq8", None).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&bytes));
    let actual = image::load_from_memory(&bytes).unwrap().into_rgba8();
    assert_eq!(actual.dimensions(), (960, 720));
    let (status, expected) = export_png(&app, &still, FD, "seq9", None).await;
    assert_eq!(status, 200);
    assert!(
        actual.as_raw()
            == image::load_from_memory(&expected)
                .unwrap()
                .into_rgba8()
                .as_raw(),
        "sequence export must match selected still through the same effects/masks"
    );
    let (_, first) = export_png(&app, &document, 0, "seq8", None).await;
    assert_ne!(first, bytes);
    let preview = preview(&app, &document, FD, "seq8").await;
    assert_eq!(preview["width"], 360);
    assert_eq!(preview["height"], 270);
    let owner = state
        .db
        .register_auth_user("png-sequence-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("png-sequence-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "PNG sequence")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "PNG claim",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"},"seq9":{"id":"seq9"}}}),
            &["seq8".into(), "seq9".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    assert_eq!(export_png(&app, &document, FD, "seq8", None).await.0, 401);
    assert_eq!(
        export_png(&app, &document, FD, "seq8", Some(&outsider.token))
            .await
            .0,
        403
    );
    let (status, authorized) = export_png(&app, &document, FD, "seq8", Some(&owner.token)).await;
    assert_eq!(status, 200);
    assert_eq!(authorized, bytes);
}

#[tokio::test]
async fn native_sequence_video_bridge_preserves_every_frame_exact_clock_and_existing_outputs() {
    use tokio_util::sync::CancellationToken;
    use video_kadr_backend::handlers::filmcraft_video_export::{encode_video, VideoRequest};
    let (state, dir) = support::make_state(false, false).await;
    for (n, dx) in [(8, 0.), (9, 3.), (11, 9.)] {
        add(&state, dir.path(), n, dx, 0.).await;
    }
    let start = 9_007_199_254_740_993i64;
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["start"] =
        json!(start);
    let request:VideoRequest=serde_json::from_value(json!({"document":document.to_string(),"sequenceId":"6","startTicks":start.to_string(),"frames":4,"sources":{"1":"seq8"}})).unwrap();
    let output = dir.path().join("outputs/source-sequence.mkv");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let cancel = CancellationToken::new();
    encode_video(
        &state,
        &axum::http::HeaderMap::new(),
        &request,
        &output,
        std::time::Duration::from_secs(30),
        &cancel,
        &tx,
    )
    .await
    .unwrap();
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let info: Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_name"], "ffv1");
    assert_eq!(streams[0]["nb_read_frames"], "4");
    assert_eq!(streams[0]["r_frame_rate"], "24/1");
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&output)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    let app = support::router(state.clone());
    let mut expected = Vec::new();
    let mut still = document.clone();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    for (i, selected) in ["seq8", "seq9", "seq9", "seq11"].into_iter().enumerate() {
        let (status, png) = export_png(&app, &still, start + i as i64 * FD, selected, None).await;
        assert_eq!(status, 200);
        expected.extend(
            image::load_from_memory(&png)
                .unwrap()
                .into_rgba8()
                .into_raw(),
        );
    }
    assert!(
        decoded.stdout == expected,
        "all decoded video pixels must match exact-tick full-size frame renders"
    );
    let mut last = 0.;
    while let Ok(value) = rx.try_recv() {
        assert!(value >= last);
        last = value;
    }
    assert_eq!(last, 100.);
    let bytes = std::fs::read(&output).unwrap();
    assert!(encode_video(
        &state,
        &axum::http::HeaderMap::new(),
        &request,
        &output,
        std::time::Duration::from_secs(30),
        &cancel,
        &tx
    )
    .await
    .is_err());
    assert_eq!(std::fs::read(&output).unwrap(), bytes);
    let (changed_tx, mut changed_rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let changed_source = dir.path().join("sources/shot_0009.png");
    let mutate = tokio::spawn(async move {
        while let Some(value) = changed_rx.recv().await {
            if value > 0. && value < 50. {
                let pixels = frame(-15., 6.);
                image::save_buffer(
                    changed_source,
                    &pixels.rgba,
                    240,
                    180,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                return;
            }
        }
        panic!("export produced no frame progress")
    });
    let refused = dir.path().join("outputs/changed-source.mkv");
    assert!(
        encode_video(
            &state,
            &axum::http::HeaderMap::new(),
            &request,
            &refused,
            std::time::Duration::from_secs(30),
            &cancel,
            &changed_tx
        )
        .await
        .is_err(),
        "source changes during export must refuse publication"
    );
    mutate.await.unwrap();
    assert!(!refused.exists());
    cancel.cancel();
    let cancelled = dir.path().join("outputs/cancelled.mkv");
    assert!(encode_video(
        &state,
        &axum::http::HeaderMap::new(),
        &request,
        &cancelled,
        std::time::Duration::from_secs(30),
        &cancel,
        &tx
    )
    .await
    .is_err());
    assert!(!cancelled.exists());
    assert!(!std::fs::read_dir(dir.path().join("outputs"))
        .unwrap()
        .any(|e| e.unwrap().file_name().to_string_lossy().starts_with('.')));
}

#[tokio::test]
async fn durable_private_video_job_publishes_owned_output_cancels_and_recovers() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{
        config::resource_classes::ResourceClass, jobs::JobKind, model::JobStatus, state::AppState,
    };
    async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let job = state.get_job(id).await.unwrap().unwrap();
                if matches!(
                    job.status,
                    JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                ) {
                    return job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    let (mut state, dir) = support::make_state(true, false).await;
    std::sync::Arc::make_mut(&mut state.tools)
        .ffmpeg_encoders
        .push("ffv1".into());
    std::sync::Arc::make_mut(&mut state.tools)
        .ffmpeg_muxers
        .push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let owner = state
        .db
        .register_auth_user("video-export-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("video-export-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Video export")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Video sources",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"},"seq9":{"id":"seq9"}}}),
            &["seq8".into(), "seq9".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let input = json!({"document":String::from_utf8(source().encode_preserved().unwrap()).unwrap(),"sequenceId":"6","startFrames":"1","frames":2,"sources":{"1":"seq8"}});
    let make = |token: Option<&str>| {
        let mut r = Request::builder()
            .method("POST")
            .uri("/api/compositions/filmcraft/video-jobs")
            .header("content-type", "application/json");
        if let Some(token) = token {
            r = r.header("authorization", format!("Bearer {token}"));
        }
        r.body(Body::from(input.to_string())).unwrap()
    };
    let app = support::router(state.clone());
    assert_eq!(app.clone().oneshot(make(None)).await.unwrap().status(), 401);
    assert_eq!(
        app.clone()
            .oneshot(make(Some(&outsider.token)))
            .await
            .unwrap()
            .status(),
        403
    );
    async fn submit(app: &axum::Router, request: Request<Body>) -> String {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let result: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(status, 202, "{result}");
        assert_eq!(result["mode"], "video_only");
        result["jobId"].as_str().unwrap().into()
    }
    let id = submit(&app, make(Some(&owner.token))).await;
    let job = terminal(&state, &id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let result = job.result.unwrap();
    assert_eq!(result["withAudio"], false);
    assert_eq!(result["videoCodec"], "ffv1");
    let output_id = result["id"].as_str().unwrap();
    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/compositions/filmcraft/video-jobs")
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), 200);
    let history: Value =
        serde_json::from_slice(&to_bytes(history.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(history["jobs"][0]["jobId"], id);
    assert_eq!(history["jobs"][0]["job"]["status"], "done");
    assert_eq!(history["jobs"][0]["job"]["result"]["id"], output_id);

    assert!(state
        .db
        .can_access_output(output_id, &owner.user.username)
        .await
        .unwrap());
    assert!(!state
        .db
        .can_access_output(output_id, &outsider.user.username)
        .await
        .unwrap());
    for (token, status) in [(&owner.token, 200), (&outsider.token, 403)] {
        let request = Request::builder()
            .uri(format!("/api/jobs/{id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(request).await.unwrap().status(), status);
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(result["url"].as_str().unwrap())
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(!to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap()
        .is_empty());
    let mut still: Value = serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    let (status, expected) = export_png(&app, &still, FD, "seq9", Some(&owner.token)).await;
    assert_eq!(status, 200);
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(
            state
                .outputs_dir()
                .join(result["filename"].as_str().unwrap()),
        )
        .args([
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert!(
        decoded.stdout
            == image::load_from_memory(&expected)
                .unwrap()
                .into_rgba8()
                .into_raw(),
        "startFrames must select the next source frame in Rust"
    );
    let imported = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/filmcraft/import")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"document":input["document"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(imported.status(), 200);
    let imported: Value = serde_json::from_slice(
        &to_bytes(imported.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let seq = imported["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "6")
        .unwrap();
    assert_eq!(seq["durationFrames"], "120");
    for start in ["9007199254740993", "not-a-frame"] {
        let mut invalid = input.clone();
        invalid["startFrames"] = json!(start);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/filmcraft/video-jobs")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::from(invalid.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let cancelled = submit(&app, make(Some(&owner.token))).await;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/jobs/{cancelled}/cancel"))
        .header("authorization", format!("Bearer {}", owner.token))
        .body(Body::empty())
        .unwrap();
    assert_eq!(app.clone().oneshot(request).await.unwrap().status(), 200);
    assert_eq!(
        terminal(&state, &cancelled).await.status,
        JobStatus::Cancelled
    );
    let changed = submit(&app, make(Some(&owner.token))).await;
    let path = state
        .library
        .resolve_media_path(&state.library.get("seq9").await.unwrap())
        .await
        .unwrap();
    let pixels = frame(10., 5.);
    image::save_buffer(path, &pixels.rgba, 240, 180, image::ColorType::Rgba8).unwrap();
    drop(permits);
    let failed = terminal(&state, &changed).await;
    assert_eq!(failed.status, JobStatus::Error, "{failed:?}");
    assert!(failed.result.is_none());
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let recovery = submit(&app, make(Some(&owner.token))).await;
    let requests = state
        .job_store
        .active_requests(JobKind::FilmcraftVideo, 256)
        .await
        .unwrap();
    let payload = requests
        .iter()
        .find(|r| r.job.id == recovery)
        .unwrap()
        .payload
        .to_string();
    assert!(payload.contains("snapshot"));
    assert!(!payload.contains(&owner.token));
    state.begin_shutdown();
    drop(permits);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let restarted = AppState::new_with_runtime(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
        state.encode_budget.clone(),
        2,
    )
    .unwrap();
    restarted.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&restarted).await;
    let done = terminal(&restarted, &recovery).await;
    assert_eq!(done.status, JobStatus::Done, "{done:?}");
    restarted.begin_shutdown();
    assert!(
        restarted
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn audiovisual_job_muxes_exact_length_native_nested_dsp_from_wav_and_flac() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{
        domain::{
            filmcraft_project_audio::SourceProjectAudioRuntime, filmcraft_wav_audio::SourceWavPool,
        },
        model::JobStatus,
    };
    let (mut state, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools
        .ffmpeg_encoders
        .extend(["ffv1".into(), "pcm_f32le".into()]);
    tools.ffmpeg_muxers.push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-wav-project.json")).unwrap();
    let wav: Vec<u8> = fixture["wav"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u8)
        .collect();
    let wave = dir.path().join("sources/signal.wav");
    std::fs::write(&wave, &wav).unwrap();
    let flac = dir.path().join("sources/signal.flac");
    let result = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(&wave)
        .args(["-c:a", "flac"])
        .arg(&flac)
        .output()
        .unwrap();
    assert!(result.status.success());
    let movie = dir.path().join("sources/signal.mkv");
    let encoded = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=8x8:rate=24:duration=3",
            "-i",
        ])
        .arg(&wave)
        .args(["-c:v", "ffv1", "-c:a", "pcm_s16le", "-shortest"])
        .arg(&movie)
        .output()
        .unwrap();
    assert!(encoded.status.success());
    for (id, filename) in [
        ("audio-wav", "signal.wav"),
        ("audio-flac", "signal.flac"),
        ("audio-video", "signal.mkv"),
    ] {
        assert!(
            state
                .library
                .add(video_kadr_backend::library::MediaEntry::from_result(
                    "source",
                    &json!({"id":id,"filename":filename,"mediaType":"audio"})
                ))
                .await
        );
    }
    let owner = state
        .db
        .register_auth_user("av-export-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Audiovisual sources")
        .await
        .unwrap();
    let ids: Vec<String> = ["audio-flac", "audio-video", "audio-wav", "seq8", "seq9"]
        .map(str::to_owned)
        .to_vec();
    state.db.create_space_composition_project("AV sources",&json!({"schemaVersion":1,"sources":ids.iter().map(|id|(id.clone(),json!({"id":id}))).collect::<serde_json::Map<String,Value>>()}),&ids,&owner.user.username,&space.id).await.unwrap();
    fn remap(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if matches!(key.as_str(), "item" | "parent") && value.as_u64() == Some(1) {
                        *value = json!(3)
                    } else {
                        remap(value)
                    }
                }
            }
            Value::Array(values) => {
                for value in values {
                    remap(value)
                }
            }
            _ => {}
        }
    }
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let source_items = &fixture["cases"][0]["document"]["project"]["items"];
    let mut audio_source = source_items["1"].clone();
    audio_source["id"] = json!(3);
    document["project"]["items"]["3"] = audio_source;
    let mut nested = source_items["7"].clone();
    remap(&mut nested);
    document["project"]["items"]["7"] = nested;
    let video = document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"].clone();
    let mut sequence = source_items["9"]["kind"]["Sequence"].clone();
    remap(&mut sequence);
    sequence["video_tracks"] = video;
    sequence["settings"]["width"] = json!(240);
    sequence["settings"]["height"] = json!(180);
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    assert_eq!(sequence["settings"]["sample_rate"], 44100);
    document["project"]["items"]["6"]["kind"]["Sequence"] = sequence;
    // Original AudioOut::new resolves the root rate before graph/DSP evaluation.
    // The authored project remains at 44.1 kHz; no post-DSP interpolation is the reference.
    let mut resolved = document.clone();
    resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"] = json!(48000);
    let mut runtime = SourceProjectAudioRuntime::new(
        SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap(),
    ).unwrap();
    let mut pool = SourceWavPool::default();
    pool.bind(3, wav.into()).unwrap();
    let reference = runtime.render_wav(6, 2000, 8000, &pool).unwrap();
    let mut expected = Vec::new();
    for i in 0..8000 {for channel in &reference {expected.push(channel[i]);}}
    assert_eq!(document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"],44100);
    assert!(expected.iter().any(|v| v.abs() > 0.001));
    let app = support::router(state.clone());
    for source in ["audio-wav", "audio-flac", "audio-video"] {
        let input = json!({"document":document.to_string(),"sequenceId":"6","startFrames":"1","frames":4,"withAudio":true,"sources":{"1":"seq8","3":source}});
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/filmcraft/video-jobs")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(status, 202, "{body}");
        assert_eq!(body["mode"], "video_audio");
        let id = body["jobId"].as_str().unwrap();
        let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let job = state.get_job(id).await.unwrap().unwrap();
                if matches!(
                    job.status,
                    JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                ) {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(job.status, JobStatus::Done, "{source}: {job:?}");
        let result = job.result.unwrap();
        assert_eq!(result["withAudio"], true);
        assert_eq!(result["audioCodec"], "pcm_f32le");
        let output = state
            .outputs_dir()
            .join(result["filename"].as_str().unwrap());
        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&output)
            .output()
            .unwrap();
        assert!(probe.status.success());
        let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
        let streams = probe["streams"].as_array().unwrap();
        assert_eq!(streams.len(), 2);
        let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
        assert_eq!(audio["sample_rate"], "48000");
        assert_eq!(audio["channels"], 2);
        assert_eq!(audio["codec_name"], "pcm_f32le");
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&output)
            .args([
                "-map",
                "0:a:0",
                "-f",
                "f32le",
                "-c:a",
                "pcm_f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(
            decoded.stdout.len(),
            8000 * 2 * 4,
            "exact audio extent {source}"
        );
        let actual: Vec<f32> = decoded
            .stdout
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(error < 0.00001, "{source}: maximum sample error {error}");
    }
    assert!(!std::fs::read_dir(state.outputs_dir()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with('.')));
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

struct VideoWorker(std::process::Child);
impl Drop for VideoWorker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn video_worker(storage: &std::path::Path, id: &str, label: &str) -> VideoWorker {
    let log = std::fs::File::create(storage.join(format!("video-worker-{label}.log"))).unwrap();
    VideoWorker(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "filmcraft_video_recovery_worker",
                "--nocapture",
            ])
            .env("VIDEO_KADR_VIDEO_RECOVERY_STORAGE", storage)
            .env("VIDEO_KADR_VIDEO_RECOVERY_JOB", id)
            .stdout(std::process::Stdio::from(log.try_clone().unwrap()))
            .stderr(std::process::Stdio::from(log))
            .spawn()
            .unwrap(),
    )
}
#[tokio::test]
#[ignore]
async fn filmcraft_video_recovery_worker() {
    use video_kadr_backend::{
        config::encode_budget::{EncodeBudget, EncodeProfile, RuntimeLimits},
        model::JobStatus,
        state::{AppState, ToolInfo},
    };
    let storage =
        std::path::PathBuf::from(std::env::var_os("VIDEO_KADR_VIDEO_RECOVERY_STORAGE").unwrap());
    let id = std::env::var("VIDEO_KADR_VIDEO_RECOVERY_JOB").unwrap();
    let tools = ToolInfo {
        ffmpeg: true,
        ffmpeg_encoders: vec!["ffv1".into(), "pcm_f32le".into()],
        ffmpeg_muxers: vec!["matroska".into()],
        ..ToolInfo::default()
    };
    let budget = EncodeBudget::for_profile(
        EncodeProfile::Balanced,
        RuntimeLimits {
            logical_cpus: 2,
            memory_mib: Some(1024),
        },
    )
    .unwrap();
    let state = AppState::new_with_runtime(
        storage.clone(),
        2,
        tools,
        video_kadr_backend::library::Library::load(storage.clone()).await,
        video_kadr_backend::db::Db::open(&storage).await.unwrap(),
        budget,
        2,
    )
    .unwrap();
    state.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&state).await;
    let job = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            let job = state.get_job(&id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    std::fs::write(
        storage.join("video-worker-result.json"),
        serde_json::to_vec(&job).unwrap(),
    )
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
#[tokio::test]
async fn killed_video_worker_recomputes_in_owned_staging_and_preserves_other_work() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{config::resource_classes::ResourceClass, jobs::JobKind};
    let (mut state, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let owner = state
        .db
        .register_auth_user("video-crash-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]
        ["duration"] = json!(FD * 480);
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let app = support::router(state.clone());
    let response=app.oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(json!({"document":document.to_string(),"sequenceId":"6","startFrames":"0","frames":480,"sources":{"1":"seq8"}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 202);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = body["jobId"].as_str().unwrap();
    let requests = state
        .job_store
        .active_requests(JobKind::FilmcraftVideo, 256)
        .await
        .unwrap();
    let output_id = requests.iter().find(|r| r.job.id == id).unwrap().payload["output_id"]
        .as_str()
        .unwrap()
        .to_owned();
    state.begin_shutdown();
    drop(permits);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let workdir = state
        .storage
        .join("cache/filmcraft-exports")
        .join(&output_id);
    let peer = state.storage.join("cache/filmcraft-exports/unrelated");
    std::fs::create_dir_all(&peer).unwrap();
    std::fs::write(peer.join("preserve"), b"other job").unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let mut first = video_worker(&state.storage, id, "killed");
    let partial = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if let Some(status) = first.0.try_wait().unwrap() {
                panic!(
                    "worker finished before kill: {status}; {}",
                    std::fs::read_to_string(state.storage.join("video-worker-killed.log")).unwrap()
                )
            }
            let ready = std::fs::read_dir(&workdir).ok().is_some_and(|entries| {
                entries.flatten().any(|entry| {
                    entry.path().extension().and_then(|v| v.to_str()) == Some("rgba")
                        && entry.metadata().is_ok_and(|m| {
                            m.len() >= 240 * 180 * 4 && m.len() < 480 * 240 * 180 * 4
                        })
                })
            });
            if ready {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(partial);
    first.0.kill().unwrap();
    let status = first.0.wait().unwrap();
    assert!(!status.success());
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));
    }
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id=?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "running");
    assert!(!state.storage.join("video-worker-result.json").exists());
    assert!(workdir.exists());
    std::fs::write(workdir.join("orphan-audio.f32le"), b"partial audio").unwrap();
    std::fs::write(workdir.join("render.mkv"), b"partial video").unwrap();
    let mut second = video_worker(&state.storage, id, "recovered");
    let status = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        loop {
            if let Some(status) = second.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(state.storage.join("video-worker-recovered.log")).unwrap()
    );
    let job: Value = serde_json::from_slice(
        &std::fs::read(state.storage.join("video-worker-result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(job["status"], "done");
    let result = &job["result"];
    assert_eq!(result["id"], output_id);
    assert_eq!(std::fs::read(peer.join("preserve")).unwrap(), b"other job");
    assert!(
        !workdir.exists(),
        "owned raw/audio/codec leftovers must be removed"
    );
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames,codec_name,width,height",
            "-of",
            "json",
        ])
        .arg(
            state
                .outputs_dir()
                .join(result["filename"].as_str().unwrap()),
        )
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["nb_read_frames"], "480");
    assert_eq!(probe["streams"][0]["codec_name"], "ffv1");
    assert_eq!(probe["streams"][0]["width"], 240);
    assert_eq!(probe["streams"][0]["height"], 180);
}

#[tokio::test]
async fn queued_video_cancellation_removes_own_interrupted_artifacts_without_export_slot() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{config::resource_classes::ResourceClass, jobs::JobKind};
    let (mut state, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    let owner = state
        .db
        .register_auth_user("queued-video-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let app = support::router(state.clone());
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(json!({"document":String::from_utf8(source().encode_preserved().unwrap()).unwrap(),"sequenceId":"6","startFrames":"0","frames":2,"sources":{"1":"seq8"}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 202);
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = result["jobId"].as_str().unwrap();
    let requests = state
        .job_store
        .active_requests(JobKind::FilmcraftVideo, 256)
        .await
        .unwrap();
    let output_id = requests.iter().find(|r| r.job.id == id).unwrap().payload["output_id"]
        .as_str()
        .unwrap();
    let workdir = state
        .storage
        .join("cache/filmcraft-exports")
        .join(output_id);
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::write(workdir.join("old.rgba"), b"interrupted").unwrap();
    let output = state.outputs_dir().join(format!("{output_id}.mkv"));
    std::fs::write(&output, b"unpublished").unwrap();
    state
        .db
        .grant_output_access(output_id, &owner.user.username)
        .await
        .unwrap();
    let peer = state.storage.join("cache/filmcraft-exports/another-job");
    std::fs::create_dir_all(&peer).unwrap();
    std::fs::write(peer.join("keep"), b"preserve").unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/jobs/{id}/cancel"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if !workdir.exists()
                && !output.exists()
                && !state
                    .db
                    .can_access_output(output_id, &owner.user.username)
                    .await
                    .unwrap()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        state.get_job(id).await.unwrap().unwrap().status,
        video_kadr_backend::model::JobStatus::Cancelled
    );
    assert_eq!(std::fs::read(peer.join("keep")).unwrap(), b"preserve");
    drop(permits);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn video_history_paginates_owner_rows_survives_restart_and_hides_revoked_sources() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{config::resource_classes::ResourceClass, state::AppState};
    async fn page(
        app: &axum::Router,
        token: Option<&str>,
        query: &str,
    ) -> (axum::http::StatusCode, Value) {
        let mut request =
            Request::builder().uri(format!("/api/compositions/filmcraft/video-jobs{query}"));
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    async fn submit(app: &axum::Router, token: &str, input: &Value) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/filmcraft/video-jobs")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::from(input.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(status, 202, "{body}");
        body["jobId"].as_str().unwrap().into()
    }
    let (mut state, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    let owner = state
        .db
        .register_auth_user("video-history-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("video-history-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let input = json!({"document":String::from_utf8(source().encode_preserved().unwrap()).unwrap(),"sequenceId":"6","startFrames":"0","frames":2,"sources":{"1":"seq8"}});
    let app = support::router(state.clone());
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(submit(&app, &owner.token, &input).await);
    }
    assert_eq!(page(&app, None, "").await.0, 401);
    assert_eq!(page(&app, Some(&owner.token), "?limit=0").await.0, 400);
    assert_eq!(page(&app, Some(&owner.token), "?limit=51").await.0, 400);
    let (status, foreign) = page(&app, Some(&outsider.token), "").await;
    assert_eq!(status, 200);
    assert!(foreign["jobs"].as_array().unwrap().is_empty());
    let before = state.library.get("seq8").await.unwrap().storage_key;
    let mut seen = Vec::new();
    let mut cursor = None;
    let mut first_cursor = String::new();
    for index in 0..3 {
        let query = cursor.as_ref().map_or("?limit=1".to_owned(), |cursor| {
            format!("?limit=1&cursor={cursor}")
        });
        let (status, body) = page(&app, Some(&owner.token), &query).await;
        assert_eq!(status, 200, "{body}");
        let row = &body["jobs"].as_array().unwrap()[0];
        assert_eq!(body["jobs"].as_array().unwrap().len(), 1);
        assert_eq!(row["job"]["status"], "pending");
        assert_eq!(row["job"]["id"], row["jobId"]);
        seen.push(row["jobId"].as_str().unwrap().to_owned());
        cursor = body["nextCursor"].as_str().map(str::to_owned);
        if index == 0 {
            first_cursor = cursor.clone().unwrap();
        }
    }
    assert!(cursor.is_none());
    seen.sort();
    ids.sort();
    assert_eq!(seen, ids);
    assert_eq!(
        state.library.get("seq8").await.unwrap().storage_key,
        before,
        "history must not migrate or hydrate files"
    );
    assert_eq!(
        page(
            &app,
            Some(&outsider.token),
            &format!("?cursor={first_cursor}")
        )
        .await
        .0,
        400
    );
    let space = state
        .db
        .create_space(&outsider.user.username, "Revoked history")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Claim history source",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"}}}),
            &["seq8".into()],
            &outsider.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let (status, revoked) = page(&app, Some(&owner.token), "?limit=1").await;
    assert_eq!(status, 200);
    assert!(revoked["jobs"].as_array().unwrap().is_empty());
    assert!(
        revoked["nextCursor"].is_string(),
        "empty filtered pages must remain pageable"
    );
    let (_, all_revoked) = page(&app, Some(&owner.token), "?limit=50").await;
    assert!(all_revoked["jobs"].as_array().unwrap().is_empty());
    let foreign_id = submit(&app, &outsider.token, &input).await;
    let (_, foreign) = page(&app, Some(&outsider.token), "").await;
    assert_eq!(foreign["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(foreign["jobs"][0]["jobId"], foreign_id);
    assert_eq!(
        page(&app, Some(&owner.token), &format!("?cursor={foreign_id}"))
            .await
            .0,
        400
    );
    state.begin_shutdown();
    drop(permits);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let reopened = AppState::new_with_runtime(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
        state.encode_budget.clone(),
        2,
    )
    .unwrap();
    let router = support::router(reopened.clone());
    let (status, history) = page(&router, Some(&outsider.token), "").await;
    assert_eq!(status, 200);
    assert_eq!(history["jobs"][0]["jobId"], foreign_id);
    let (_, hidden) = page(&router, Some(&owner.token), "").await;
    assert!(hidden["jobs"].as_array().unwrap().is_empty());
    reopened.begin_shutdown();
    assert!(
        reopened
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn owner_retry_after_committed_transient_failure_uses_original_export_request() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{
        config::resource_classes::ResourceClass,
        jobs::{ErrorKind, JobEvent, JobKind},
        model::JobStatus,
        state::AppState,
    };
    let (mut original, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut original.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    add(&original, dir.path(), 8, 0., 0.).await;
    let owner = original
        .db
        .register_auth_user("video-retry-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = original
        .db
        .register_auth_user("video-retry-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = original
        .db
        .create_space(&owner.user.username, "Retry source")
        .await
        .unwrap();
    original
        .db
        .create_space_composition_project(
            "Retry",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"}}}),
            &["seq8".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let mut permits = Vec::new();
    for _ in 0..original.resource_limit(ResourceClass::Export) {
        permits.push(
            original
                .acquire_job_slot(ResourceClass::Export)
                .await
                .unwrap(),
        );
    }
    let response=support::router(original.clone()).oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(json!({"document":String::from_utf8(source().encode_preserved().unwrap()).unwrap(),"sequenceId":"6","startFrames":"0","frames":2,"sources":{"1":"seq8"}}).to_string())).unwrap()).await.unwrap();
    assert_eq!(response.status(), 202);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    let id = body["jobId"].as_str().unwrap().to_owned();
    let requests = original
        .job_store
        .active_requests(JobKind::FilmcraftVideo, 256)
        .await
        .unwrap();
    let output_id = requests.iter().find(|r| r.job.id == id).unwrap().payload["output_id"]
        .as_str()
        .unwrap()
        .to_owned();
    original.begin_shutdown();
    drop(permits);
    assert!(
        original
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
    let state = AppState::new_with_runtime(
        original.storage.clone(),
        2,
        (*original.tools).clone(),
        video_kadr_backend::library::Library::load(original.storage.clone()).await,
        video_kadr_backend::db::Db::open(&original.storage)
            .await
            .unwrap(),
        original.encode_budget.clone(),
        2,
    )
    .unwrap();
    state.recover_jobs().await;
    // Reproduce the persisted crash window: failure committed, automatic retry scheduling not yet run.
    let attempt = state.job_store.claim(&id, 60).await.unwrap().unwrap();
    assert_eq!(attempt.kind, JobKind::FilmcraftVideo);
    assert!(state
        .transition_job(
            &id,
            JobEvent::Started {
                stage: "filmcraft_video".into(),
                attempt: attempt.attempt
            }
        )
        .await
        .unwrap());
    assert!(state
        .transition_job(
            &id,
            JobEvent::Failed {
                kind: ErrorKind::Storage,
                message: "temporary export storage outage".into()
            }
        )
        .await
        .unwrap());
    assert_eq!(
        state.get_job(&id).await.unwrap().unwrap().status,
        JobStatus::Error
    );
    let app = support::router(state.clone());
    let retry = |token: &str| {
        Request::builder()
            .method("POST")
            .uri(format!("/api/jobs/{id}/retry"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(retry(&outsider.token))
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(state.job_store.operator_action_count(&id).await.unwrap(), 0);
    let response = app.clone().oneshot(retry(&owner.token)).await.unwrap();
    let status = response.status();
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["jobId"], id);
    let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(&id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let result = job.result.unwrap();
    assert_eq!(result["id"], output_id);
    assert_eq!(result["width"], 240);
    assert_eq!(result["height"], 180);
    assert_eq!(result["duration"].as_f64().unwrap(), 2. / 24.);
    assert!(state
        .outputs_dir()
        .join(result["filename"].as_str().unwrap())
        .exists());
    assert_eq!(state.job_store.operator_action_count(&id).await.unwrap(), 1);
    let attempts = state.job_store.attempt_history(&id).await.unwrap();
    assert!(attempts
        .iter()
        .any(|a| a.attempt == attempt.attempt && a.status == "failed"));
    assert_eq!(attempts.last().unwrap().status, "succeeded");
    assert!(attempts.last().unwrap().attempt > attempt.attempt);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn independent_queued_exports_publish_their_own_ranges_and_artifacts() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::{config::resource_classes::ResourceClass, model::JobStatus};
    let (mut state, dir) = support::make_state(true, false).await;
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let owner = state
        .db
        .register_auth_user("video-multiple-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Multiple exports")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Queued sources",
            &json!({"schemaVersion":1,"sources":{"seq8":{"id":"seq8"},"seq9":{"id":"seq9"}}}),
            &["seq8".into(), "seq9".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let mut permits = Vec::new();
    for _ in 0..state.resource_limit(ResourceClass::Export) {
        permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());
    }
    let app = support::router(state.clone());
    let document = String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    let mut ids = Vec::new();
    for frame in 0..2 {
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(json!({"document":document,"sequenceId":"6","startFrames":frame.to_string(),"frames":1,"sources":{"1":"seq8"}}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), 202);
        let body: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        ids.push(body["jobId"].as_str().unwrap().to_owned());
    }
    assert_ne!(ids[0], ids[1]);
    for id in &ids {
        assert_eq!(
            state.get_job(id).await.unwrap().unwrap().status,
            JobStatus::Pending
        );
    }
    drop(permits);
    let mut output_ids = Vec::new();
    let mut pixels = Vec::new();
    let mut still: Value = serde_json::from_str(&document).unwrap();
    still["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"] = json!("Image");
    for (index, id) in ids.iter().enumerate() {
        let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let job = state.get_job(id).await.unwrap().unwrap();
                if matches!(
                    job.status,
                    JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
                ) {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(job.status, JobStatus::Done, "{job:?}");
        let result = job.result.unwrap();
        let output_id = result["id"].as_str().unwrap();
        assert!(state
            .db
            .can_access_output(output_id, &owner.user.username)
            .await
            .unwrap());
        output_ids.push(output_id.to_owned());
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(
                state
                    .outputs_dir()
                    .join(result["filename"].as_str().unwrap()),
            )
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        let (status, expected) = export_png(
            &app,
            &still,
            index as i64 * FD,
            if index == 0 { "seq8" } else { "seq9" },
            Some(&owner.token),
        )
        .await;
        assert_eq!(status, 200);
        assert!(
            decoded.stdout
                == image::load_from_memory(&expected)
                    .unwrap()
                    .into_rgba8()
                    .into_raw(),
            "each queued request must retain its own source clock"
        );
        pixels.push(decoded.stdout);
    }
    assert_ne!(output_ids[0], output_ids[1]);
    assert!(
        pixels[0] != pixels[1],
        "different queued ranges must not share a cached raster"
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[test]
fn original_h264_sdr_pixel_vectors_and_adaptive_parameters() {
    use video_kadr_backend::{
        domain::{composition::FrameRateRatio, filmcraft_h264_pixels::yuv420},
        handlers::filmcraft_video_export::ExportPreset,
        tools::native_rgba_export::{RasterEncoding, RasterVideoSpec},
    };
    let surface = GroupSurface {
        width: 2,
        height: 2,
        rgba: vec![
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
        ],
    };
    assert_eq!(yuv420(&surface).unwrap(), vec![63, 173, 32, 235, 128, 128]);
    let red = GroupSurface {
        width: 2,
        height: 2,
        rgba: [255, 0, 0, 255].repeat(4),
    };
    assert_eq!(yuv420(&red).unwrap(), vec![63, 63, 63, 63, 102, 240]);
    let spec = RasterVideoSpec {
        width: 1920,
        height: 1080,
        frame_rate: FrameRateRatio {
            numerator: 24,
            denominator: 1,
        },
        frames: 1,
        staging_bytes: 16 * 1024 * 1024,
    };
    for (preset, target, max) in [
        (ExportPreset::AdaptiveHigh, 9953, 14929),
        (ExportPreset::AdaptiveMedium, 4977, 7465),
        (ExportPreset::AdaptiveLow, 2488, 3732),
    ] {
        match preset.encoding(spec) {
            RasterEncoding::H264 {
                target_kbps,
                max_kbps,
                keyint,..
            } => {
                assert_eq!(target_kbps, target);
                assert_eq!(max_kbps, max);
                assert_eq!(keyint, 48)
            }
            _ => panic!("wrong encoding"),
        }
    }
}
#[tokio::test]
async fn adaptive_h264_job_exports_bt709_high_profile_mp4_with_aac_and_owner_download() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let (state, dir) = support::make_state(true, false).await;
    add(&state, dir.path(), 8, 0., 0.).await;
    add(&state, dir.path(), 9, 3., -2.).await;
    let owner = state
        .db
        .register_auth_user("h264-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("h264-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let mut document: Value =
        serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone = document["project"]["items"]["1"].clone();
    tone["id"] = json!(3);
    let media = &mut tone["kind"]["Media"];
    media["media"] = json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});
    media["info"]["kind"] = json!("AudioOnly");
    media["info"]["video"] = Value::Null;
    media["info"]["audio"] =
        json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});
    document["project"]["items"]["3"] = tone;
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track = sequence["audio_tracks"][0].clone();
    let mut clip = track["items"][0].clone();
    clip["item"] = json!(3);
    clip["start"] = json!(0);
    clip["source_in"] = json!(0);
    clip["duration"] = json!(FD * 4);
    clip["effects"] = json!([]);
    track["items"] = json!([clip]);
    track["effects"] = json!([]);
    track["volume_db"] = json!(0.);
    track["pan"] = json!(0.);
    sequence["audio_tracks"] = json!([track]);
    let app = support::router(state.clone());
    let input = json!({"document":document.to_string(),"sequenceId":"6","startFrames":"0","frames":4,"withAudio":true,"preset":"adaptive_high","sources":{"1":"seq8"}});
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/filmcraft/video-jobs")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["container"], "mp4");
    let id = body["jobId"].as_str().unwrap();
    let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(id).await.unwrap().unwrap();
            if matches!(
                job.status,
                JobStatus::Done | JobStatus::Error | JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    let result = job.result.unwrap();
    assert_eq!(result["container"], "mp4");
    assert_eq!(result["preset"], "adaptive_high");
    assert_eq!(result["videoCodec"], "h264");
    assert_eq!(result["audioCodec"], "aac");
    assert!(result["filename"].as_str().unwrap().ends_with(".mp4"));
    let path = state
        .outputs_dir()
        .join(result["filename"].as_str().unwrap());
    let probe = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-count_frames",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(probe.status.success());
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(video["codec_name"], "h264");
    assert_eq!(video["profile"], "High");
    assert_eq!(video["pix_fmt"], "yuv420p");
    assert_eq!(video["color_space"], "bt709");
    assert_eq!(video["color_range"], "tv");
    assert_eq!(video["nb_read_frames"], "4");
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(audio["channels"], 2);
    assert_eq!(audio["duration_ts"], 8000);
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&path)
        .args([
            "-map", "0:v:0", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout.len(), 240 * 180 * 4 * 4);
    let mut expected = Vec::new();
    for i in 0..4 {
        let (status, png) = export_png(&app, &document, i * FD, "seq8", Some(&owner.token)).await;
        assert_eq!(status, 200);
        expected.extend(
            image::load_from_memory(&png)
                .unwrap()
                .into_rgba8()
                .into_raw(),
        );
    }
    let error = decoded
        .stdout
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(i, _)| i % 4 != 3)
        .map(|(_, (&a, &b))| a.abs_diff(b) as f64)
        .sum::<f64>()
        / (240. * 180. * 3. * 4.);
    assert!(error < 12., "H264 luma/range error {error}");
    let pcm = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&path)
        .args([
            "-map",
            "0:a:0",
            "-c:a",
            "pcm_f32le",
            "-f",
            "f32le",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(pcm.status.success());
    assert!(pcm
        .stdout
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
        .any(|v| v.abs() > 0.01));
    for (token, allowed) in [(&owner.token, true), (&outsider.token, false)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(result["url"].as_str().unwrap())
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().is_success(), allowed);
    }
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[tokio::test]
async fn output_download_acl_covers_mkv_mp4_head_ranges_and_legacy_public_files() {
    use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state.db.register_auth_user("download-owner", "test password").await.unwrap().unwrap();
    let outsider = state.db.register_auth_user("download-outsider", "test password").await.unwrap().unwrap();
    tokio::fs::create_dir_all(state.storage.join("outputs")).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    state.db.grant_output_access(&id, "download-owner").await.unwrap();
    for extension in ["mkv", "mp4"] {
        tokio::fs::write(state.storage.join("outputs").join(format!("{id}.{extension}")), b"0123456789").await.unwrap();
    }
    tokio::fs::write(state.storage.join("outputs/public.mkv"), b"public").await.unwrap();
    let app = video_kadr_backend::build_router(state.clone(), 1024);
    for extension in ["mkv", "mp4"] {
        for method in ["GET", "HEAD"] {
            for token in [None, Some(outsider.token.as_str()), Some(owner.token.as_str())] {
                let mut request = Request::builder().method(method).uri(format!("/files/outputs/{id}.{extension}"));
                if let Some(token) = token { request = request.header("authorization", format!("Bearer {token}")); }
                let response = app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
                assert_eq!(response.status(), if token == Some(owner.token.as_str()) { StatusCode::OK } else if token.is_none() { StatusCode::UNAUTHORIZED } else { StatusCode::FORBIDDEN });
            }
        }
        let response = app.clone().oneshot(Request::builder().uri(format!("/files/outputs/{id}.{extension}"))
            .header("authorization", format!("Bearer {}", owner.token)).header("range", "bytes=2-4")
            .body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(&to_bytes(response.into_body(), 64).await.unwrap()[..], b"234");
    }
    let response = app.oneshot(Request::builder().uri("/files/outputs/public.mkv").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    state.begin_shutdown();
    assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn match_source_video_geometry_and_linear_black_composite() {
    use std::sync::{Arc, atomic::AtomicBool};
    use video_kadr_backend::domain::{filmcraft_export_geometry::{video_dimensions, place}, filmcraft_source_pixels::{encode_srgb, encode_srgb_over_black}, transition_kernel_pack::{context::RenderContext, image::Image}};
    for (source, expected) in [((241,183),(240,182)),((240,183),(240,182)),((241,180),(240,180)),((1,1),(2,2)),((3840,2160),(3840,2160))] {
        assert_eq!(video_dimensions(source.0,source.1),expected);
    }
    let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,64,65536).unwrap();
    let mut image=Image::new(2,1);
    image.px.copy_from_slice(&[0.25,0.,0.,0.25, 0.,0.5,0.,0.5]);
    let placed=place(&context,&image,2,2).unwrap();
    assert_eq!(&placed.px[..8],&image.px[..]);
    assert_eq!(&placed.px[8..],&[0.;8]);
    let opaque=encode_srgb_over_black(&context,&placed).unwrap();
    assert_eq!(opaque.surface().rgba,vec![137,0,0,255,0,188,0,255,0,0,0,255,0,0,0,255]);
    let straight=encode_srgb(&context,&image).unwrap();
    assert_eq!(straight.surface().rgba,vec![255,0,0,64,0,255,0,128]);
    let cropped=place(&context,&image,1,1).unwrap();
    assert_eq!(&cropped.px[..],&image.px[..4]);
    drop((opaque,straight,cropped,placed,image));
    assert_eq!(context.pixel_bytes_in_use(),0);
}

#[tokio::test]
async fn h264_odd_source_canvas_renders_at_original_scale_then_pads_or_crops() {
    use std::{collections::BTreeMap, sync::{Arc,atomic::AtomicBool}};
    use video_kadr_backend::{domain::{filmcraft_source_frame::{render,FrameOptions}, filmcraft_generator_source_provider::ImageSourceProvider, transition_kernel_pack::context::RenderContext}, handlers::filmcraft_video_export::{encode_video,VideoRequest}};
    let (state, dir)=support::make_state(true,false).await;
    add(&state,dir.path(),8,0.,0.).await;
    for (width,height) in [(241usize,180usize),(240,183),(241,183),(1,1)] {
        let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
        let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
        sequence["settings"]["width"]=json!(width);
        sequence["settings"]["height"]=json!(height);
        sequence["video_tracks"][0]["items"][0]["effects"]=json!([{"effect":"opacity","enabled":true,"params":{"opacity":{"value":{"Float":50.}},"blend":{"value":{"Choice":0}}}}]);
        let request:VideoRequest=serde_json::from_value(json!({"document":document,"sequenceId":"6","startFrames":"0","frames":1,"preset":"adaptive_high","sources":{"1":"seq8"}})).unwrap();
        let output=dir.path().join(format!("odd-{width}-{height}.mp4"));
        let (progress,_rx)=tokio::sync::mpsc::unbounded_channel();
        encode_video(&state,&axum::http::HeaderMap::new(),&request,&output,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&progress).await.unwrap();
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(&output).output().await.unwrap();
        assert!(probe.status.success());
        let streams:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let (w,h)=(width.max(2)&!1,height.max(2)&!1);
        assert_eq!(streams["streams"][0]["width"],w);
        assert_eq!(streams["streams"][0]["height"],h);
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(),w*h*4);
        // Independent expected placement: render at the source's width ratio,
        // copy top-left and leave out-of-render pixels black; composite in linear light.
        let doc=SourceProjectDocument::read(&serde_json::to_vec(&request.document).unwrap()).unwrap();
        let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,3840*2160,512*1024*1024).unwrap();
        let mut provider=ImageSourceProvider{images:Arc::new(BTreeMap::from([(1,frame(0.,0.))]))};
        let image=render(&context,&doc,6,0,FrameOptions{scale:w as f32/width as f32,working_output:true,..Default::default()},&mut provider).unwrap();
        let mut error=0u64;
        for y in 0..h {for x in 0..w {for c in 0..3 {
            let linear=if x<image.w&&y<image.h {image.px[(y*image.w+x)*4+c]}else{0.};
            let linear=(linear.clamp(0.,1.)*4095.+0.5).floor()/4095.;
            let srgb=if linear<=0.0031308 {linear*12.92}else{1.055*linear.powf(1./2.4)-0.055};
            let expected=(srgb*255.+0.5).clamp(0.,255.)as u8;
            error+=u64::from(decoded.stdout[(y*w+x)*4+c].abs_diff(expected));
        }}}
        let mae=error as f64/(w*h*3)as f64;
        assert!(mae<10.,"{width}x{height} -> {w}x{h}: mean error {mae}");
        if image.h<h {
            // H.264 may ring across a one-pixel black border. Exact pre-encode
            // zero padding is checked separately; here require a near-black mean.
            let row:Vec<_>=decoded.stdout[(h-1)*w*4..].chunks_exact(4).flat_map(|p|p[..3].iter().copied()).collect();
            let mean=row.iter().map(|v|u64::from(*v)).sum::<u64>()as f64/row.len()as f64;
            assert!(mean<8.,"decoded black border mean {mean}");
        }
    }
    state.begin_shutdown();
    assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn original_export_fit_fill_stretch_pixel_aspect_and_triangle_vectors() {
    use std::sync::{Arc,atomic::AtomicBool};
    use video_kadr_backend::domain::{filmcraft_export_geometry::{Geometry,Scaling},transition_kernel_pack::{context::RenderContext,image::Image}};
    let fit=Geometry::new((1920,1080),(100,100),true,Scaling::ScaleToFit,None,false).unwrap();
    assert_eq!(fit.content,Some((100,56))); assert_eq!(fit.offset,(0,22));
    let fill=Geometry::new((1920,1080),(100,100),true,Scaling::ScaleToFill,None,false).unwrap();
    assert_eq!(fill.content,Some((178,100))); assert_eq!(fill.offset,(-39,0));
    let stretch=Geometry::new((1920,1080),(100,100),true,Scaling::StretchToFill,None,false).unwrap();
    assert_eq!(stretch.content,Some((100,100))); assert_eq!(stretch.offset,(0,0));
    let par=Geometry::new((240,180),(160,90),true,Scaling::ScaleToFit,Some((2,1)),false).unwrap();
    assert_eq!(par.content,Some((60,90))); assert_eq!(par.offset,(50,0));assert_eq!(par.render_scale,0.5);
    let quality=Geometry::new((240,180),(80,60),true,Scaling::ScaleToFit,None,true).unwrap();
    assert_eq!(quality.render_scale,1.);
    assert!(Geometry::new((240,180),(0,80),true,Scaling::ScaleToFit,None,false).is_err());
    assert!(Geometry::new((240,180),(80,2161),true,Scaling::ScaleToFill,None,false).is_err());
    let flag=Arc::new(AtomicBool::new(false));
    let context=RenderContext::new_with_budget(flag.clone(),1,64,65536).unwrap();
    let mut image=Image::new(4,2);
    for y in 0..2 {for x in 0..4 {image.px[(y*4+x)*4..(y*4+x+1)*4].copy_from_slice(&[x as f32/4.,0.,0.,(x+1)as f32/4.]);}}
    let geometry=Geometry::new((4,2),(2,2),true,Scaling::StretchToFill,None,false).unwrap();
    let output=geometry.fit(&context,image).unwrap();
    for y in 0..2 {for (x,expected) in [(0,[5./28.,0.,0.,3./7.]),(1,[4./7.,0.,0.,23./28.])] {
        for c in 0..4 {assert!((output.px[(y*2+x)*4+c]-expected[c]).abs()<1e-6,"triangle ({x},{y},{c})");}
    }}
    drop(output); assert_eq!(context.pixel_bytes_in_use(),0);
    flag.store(true,std::sync::atomic::Ordering::Relaxed);
    assert!(geometry.fit(&context,Image::new(4,2)).is_err());
    assert_eq!(context.pixel_bytes_in_use(),0);
}

#[tokio::test]
async fn custom_export_geometry_preserves_linear_pixels_alpha_and_pixel_aspect_signal() {
    use std::{collections::BTreeMap,sync::{Arc,atomic::AtomicBool}};
    use video_kadr_backend::{domain::{filmcraft_source_frame::{render,FrameOptions},filmcraft_generator_source_provider::ImageSourceProvider,transition_kernel_pack::context::RenderContext},handlers::filmcraft_video_export::{encode_video,VideoRequest}};
    // Independent direct triangle evaluation for the expected raster; no production
    // Geometry/fit/resample/encode helper participates in this reference.
    fn axis(src:&[f32],sw:usize,sh:usize,n:usize,horizontal:bool)->Vec<f32>{
        let(w,h)=if horizontal{(n,sh)}else{(sw,n)};
        let old=if horizontal{sw}else{sh};let scale=old as f32/n as f32;let support=scale.max(1.);
        let mut out=vec![0.;w*h*4];
        for y in 0..h{for x in 0..w{
            let center=((if horizontal{x}else{y})as f32+0.5)*scale-0.5;
            let lo=((center-support).ceil()as i64).max(0)as usize;
            let hi=((center+support).floor()as i64).min(old as i64-1)as usize;
            let sum:f32=(lo..=hi).map(|j|(1.-(j as f32-center).abs()/support).max(0.)).sum();
            for j in lo..=hi{let weight=(1.-(j as f32-center).abs()/support).max(0.)/sum;let(sx,sy)=if horizontal{(j,y)}else{(x,j)};for c in 0..4{out[(y*w+x)*4+c]+=src[(sy*sw+sx)*4+c]*weight;}}
        }}out
    }
    let(state,dir)=support::make_state(true,false).await;
    add(&state,dir.path(),8,0.,0.).await;
    let marker=dir.path().join("metadata-was-executed");
    let metadata=json!({"title":"  Кадр 🎬 = тест  ","creator":"  Автор O'Neil \"X\"  ","copyright":"  © 2026 Video Kadr  ","description":"  Первая строка\nВторая строка  ","comment":format!("  literal $(touch '{}'); -metadata title=changed  ",marker.display())});
    fn check_metadata(probe:&Value,metadata:&Value){
        let tags:std::collections::BTreeMap<_,_>=probe["format"]["tags"].as_object().unwrap().iter().map(|(k,v)|(k.to_ascii_lowercase(),v.as_str().unwrap())).collect();
        for (field,tag)in [("title","title"),("creator","artist"),("copyright","copyright"),("description","description"),("comment","comment")] {
            assert_eq!(tags.get(tag).copied(),Some(metadata[field].as_str().unwrap().trim()),"metadata {field}");
        }
    }
    for (mode,scale,content,offset,par,quality) in [
        ("scaleToFit",1f32/3.,(80usize,60usize),(0i64,10i64),None,false),
        ("scaleToFill",80f32/180.,(107,80),(-13,0),None,false),
        ("stretchToFill",80f32/180.,(80,80),(0,0),None,false),
        ("scaleToFit",1.,(53,80),(13,0),Some((2u32,1u32)),true),
    ]{
        let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
        document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["effects"]=json!([{"effect":"opacity","enabled":true,"params":{"opacity":{"value":{"Float":50.}},"blend":{"value":{"Choice":0}}}}]);
        let mut input=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":1,"sources":{"1":"seq8"},"frameSize":[80,80],"scaling":mode,"pixelAspect":par,"maxRenderQuality":quality,"metadata":metadata});
        if quality{input["videoLimiter"]=json!({"enabled":true,"minPercent":10.,"maxPercent":80.});}
        let request:VideoRequest=serde_json::from_value(input).unwrap();
        let request:VideoRequest=serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
        let output=dir.path().join(format!("custom-{mode}-{quality}.mkv"));
        let(progress,_)=tokio::sync::mpsc::unbounded_channel();
        encode_video(&state,&axum::http::HeaderMap::new(),&request,&output,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&progress).await.unwrap();
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());
        let stream:Value=serde_json::from_slice(&probe.stdout).unwrap();
        check_metadata(&stream,&metadata);
        assert_eq!(stream["streams"][0]["width"],80);assert_eq!(stream["streams"][0]["height"],80);
        if par.is_some(){assert_eq!(stream["streams"][0]["sample_aspect_ratio"],"2:1");assert_eq!(stream["streams"][0]["display_aspect_ratio"],"2:1");}
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();assert!(decoded.status.success());
        let doc=SourceProjectDocument::read(&serde_json::to_vec(&request.document).unwrap()).unwrap();
        let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,3840*2160,512*1024*1024).unwrap();
        let mut provider=ImageSourceProvider{images:Arc::new(BTreeMap::from([(1,frame(0.,0.))]))};
        let image=render(&context,&doc,6,0,FrameOptions{scale,working_output:true,..Default::default()},&mut provider).unwrap();
        let(mut px,mut w,mut h)=(image.px.to_vec(),image.w,image.h);
        if w!=content.0{px=axis(&px,w,h,content.0,true);w=content.0;}
        if h!=content.1{px=axis(&px,w,h,content.1,false);h=content.1;}
        let mut expected=vec![0u8;80*80*4];
        for y in 0..80{for x in 0..80{let(sx,sy)=(x as i64-offset.0,y as i64-offset.1);if sx<0||sy<0||sx>=w as i64||sy>=h as i64{continue}
            let p=&px[(sy as usize*w+sx as usize)*4..][..4];let alpha=p[3].clamp(0.,1.);if alpha<=0.{continue}
            for c in 0..3{let linear=((p[c]/alpha).clamp(0.,1.)*4095.+0.5).floor()/4095.;let srgb=if linear<=0.0031308{linear*12.92}else{1.055*linear.powf(1./2.4)-0.055};expected[(y*80+x)*4+c]=(srgb*255.+0.5).clamp(0.,255.)as u8;}
            expected[(y*80+x)*4+3]=(alpha*255.+0.5)as u8;
        }}
        if quality {
            // Independent reference of the pinned display-encoded BT.709 limiter.
            for p in expected.chunks_exact_mut(4){
                let mut c=[p[0]as f32/255.,p[1]as f32/255.,p[2]as f32/255.];
                let y=c[0]*0.2126+c[1]*0.7152+c[2]*0.0722;
                if y>0.8 {let factor=0.8/y;for v in &mut c{*v*=factor;}}
                else if y<0.1 {for v in &mut c{*v+=0.1-y;}}
                for i in 0..3{p[i]=(c[i].clamp(0.,0.8)*255.).floor()as u8;}
            }
        }
        assert_eq!(decoded.stdout.len(),expected.len());
        let mismatch=decoded.stdout.iter().zip(&expected).position(|(a,b)|a!=b);
        assert!(mismatch.is_none(),"{mode}, pixel aspect {par:?}, quality {quality}, first mismatch {mismatch:?}");
        assert!(decoded.stdout.chunks_exact(4).any(|p|p[3]==128));
        if par.is_some(){
            use axum::{body::{Body,to_bytes},http::Request};
            use tower::ServiceExt;
            use video_kadr_backend::model::JobStatus;
            let owner=state.db.register_auth_user("geometry-owner","test password").await.unwrap().unwrap();
            let app=support::router(state.clone());
            let mut payload=serde_json::to_value(&request).unwrap();payload["preset"]=json!("adaptive_medium");
            let mut invalid=payload.clone();invalid["metadata"]["title"]=json!("invalid\0title");
            let rejected=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs")
                .header("authorization",format!("Bearer {}",owner.token)).header("content-type","application/json")
                .body(Body::from(invalid.to_string())).unwrap()).await.unwrap();
            assert_eq!(rejected.status(),400);
            let response=app.oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs")
                .header("authorization",format!("Bearer {}",owner.token)).header("content-type","application/json")
                .body(Body::from(payload.to_string())).unwrap()).await.unwrap();
            let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
            assert_eq!(status,202,"{accepted}");
            let job=tokio::time::timeout(std::time::Duration::from_secs(30),async{loop{
                let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();
                if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }}).await.unwrap();
            assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();
            assert_eq!(result["width"],80);assert_eq!(result["height"],80);assert_eq!(result["container"],"mp4");
            let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"])
                .arg(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).output().await.unwrap();
            assert!(probe.status.success());let stream:Value=serde_json::from_slice(&probe.stdout).unwrap();
        check_metadata(&stream,&metadata);
            assert_eq!(stream["streams"][0]["sample_aspect_ratio"],"2:1");assert_eq!(stream["streams"][0]["display_aspect_ratio"],"2:1");
            let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"])
                .arg(state.storage.join("outputs").join(result["filename"].as_str().unwrap()))
                .args(["-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();
            assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),80*80*4);
            let mean=(0..80).flat_map(|y|(0..4).map(move |x|(y*80+x)*4))
                .flat_map(|at|decoded.stdout[at..at+3].iter().copied()).map(f64::from).sum::<f64>()/(80.*4.*3.);
            assert!((18. ..32.).contains(&mean),"limited MP4 black border mean {mean}");
            assert!(!marker.exists(),"metadata must stay literal");
        }
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn filmcraft_export_metadata_trims_omits_empty_and_enforces_argument_budgets() {
    use video_kadr_backend::domain::filmcraft_export_metadata::ExportMetadata;
    assert!(ExportMetadata::default().ffmpeg_tags().is_empty());
    let metadata:ExportMetadata=serde_json::from_value(json!({"title":"  🎬 = value  ","creator":"  Автор  ","comment":" \n\t ","description":" one\ntwo "})).unwrap();
    metadata.validate().unwrap();
    assert_eq!(metadata.ffmpeg_tags(),vec![("title".into(),"🎬 = value".into()),("artist".into(),"Автор".into()),("description".into(),"one\ntwo".into())]);
    let mut oversized=ExportMetadata{title:"x".repeat(16*1024),..Default::default()};assert!(oversized.validate().is_ok());
    oversized.title="🎬".repeat(4097);assert!(oversized.validate().is_err());
    oversized.title="a\0b".into();assert!(oversized.validate().is_err());
    let all=ExportMetadata{title:"x".repeat(16384),creator:"x".repeat(16384),copyright:"x".repeat(16384),description:"x".repeat(16384),comment:"x".repeat(16384)};assert!(all.validate().is_err());
}

#[test]
fn original_export_limiter_vectors_clamps_alpha_and_cancellation() {
    use video_kadr_backend::domain::filmcraft_export_limiter::VideoLimiter;
    let input=vec![0,0,0,0,255,255,255,64,255,0,0,128,0,255,0,192,0,0,255,255];
    let mut surface=GroupSurface{width:5,height:1,rgba:input.clone()};
    VideoLimiter::default().apply(&mut surface,||Ok(())).unwrap();assert_eq!(surface.rgba,input);
    let limiter=VideoLimiter{enabled:true,min_percent:10.,max_percent:80.};
    limiter.apply(&mut surface,||Ok(())).unwrap();
    assert_eq!(surface.rgba,vec![25,25,25,0,204,204,204,64,204,0,0,128,0,204,0,192,7,7,204,255]);
    let mut outside=GroupSurface{width:1,height:1,rgba:vec![0,0,0,77]};
    VideoLimiter{enabled:true,min_percent:-20.,max_percent:120.}.apply(&mut outside,||Ok(())).unwrap();assert_eq!(outside.rgba,vec![0,0,0,77]);
    assert!(VideoLimiter{enabled:true,min_percent:f32::NAN,max_percent:100.}.validate().is_err());
    let mut cancelled=GroupSurface{width:1025,height:1,rgba:vec![0;1025*4]};
    let mut checkpoints=0;
    assert!(limiter.apply(&mut cancelled,||{checkpoints+=1;anyhow::ensure!(checkpoints<2,"cancelled");Ok(())}).is_err());
    assert_eq!(checkpoints,2);assert_eq!(&cancelled.rgba[1024*4..],&[0;4]);
}

#[test]
fn original_fixed_h264_presets_keep_sizes_bitrates_and_two_pass_modes() {
    use video_kadr_backend::{handlers::filmcraft_video_export::ExportPreset,tools::native_rgba_export::{RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let spec=RasterVideoSpec{width:640,height:360,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024*1024};
    for(name,size,target,max,two_pass)in [
        ("high_quality1080",(1920,1080),20000,24000,false),("high_quality2160",(3840,2160),60000,72000,false),
        ("youtube1080",(1920,1080),16000,20000,false),("youtube2160",(3840,2160),45000,54000,false),
        ("vimeo1080",(1920,1080),18000,24000,true),("vimeo2160",(3840,2160),50000,65000,true),
        ("social_vertical",(1080,1920),12000,16000,false),
    ]{
        let preset:ExportPreset=serde_json::from_value(json!(name)).unwrap();assert_eq!(preset.frame_size(),Some(size));
        let encoding=preset.encoding(spec);assert_eq!(encoding.two_pass(),two_pass);
        match encoding {RasterEncoding::H264{target_kbps,max_kbps,keyint,..}|RasterEncoding::H264TwoPass{target_kbps,max_kbps,keyint,..}=>{assert_eq!((target_kbps,max_kbps,keyint),(target,max,48));},_=>panic!("wrong preset encoder")}
        assert_eq!(serde_json::to_value(preset.scaling()).unwrap(),if name=="social_vertical"{json!("scaleToFill")}else{json!("scaleToFit")});
    }
}

#[tokio::test]
async fn two_pass_h264_encodes_audio_and_cleans_only_owned_stats_on_success_or_cancel() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec,RasterAudio},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(true,false).await;
    let parent=dir.path().join("outputs");
    let peer=parent.join(".raster-pass-peer");tokio::fs::create_dir(&peer).await.unwrap();tokio::fs::write(peer.join("keep"),b"peer").await.unwrap();
    let audio=dir.path().join("two-pass.f32le");let mut pcm=Vec::new();
    for i in 0..8000{let value=((i as f32*440.*std::f32::consts::TAU/48000.).sin()*0.1).to_le_bytes();pcm.extend(value);pcm.extend(value);}
    tokio::fs::write(&audio,pcm).await.unwrap();
    let spec=RasterVideoSpec{width:64,height:64,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024*1024};
    let encoding=RasterEncoding::H264TwoPass{target_kbps:18000,max_kbps:24000,keyint:48,profile:Default::default(),mode:video_kadr_backend::domain::filmcraft_export_h264::BitrateMode::Vbr2Pass};
    let output=parent.join("two-pass.mp4");let cancel=tokio_util::sync::CancellationToken::new();let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
    encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&audio,frames:8000}),encoding,None,&[("title".into(),"two-pass".into())],|i,_|async move{Ok(GroupSurface{width:64,height:64,rgba:[(i*40)as u8,100,160,255].repeat(64*64)})}).await.unwrap();
    let mut progress=Vec::new();while let Ok(value)=rx.try_recv(){progress.push(value)}
    assert!(progress.iter().any(|v|*v>50.&&*v<=75.));assert!(progress.iter().any(|v|*v>75.));
    assert!(progress.windows(2).all(|p|p[0]<=p[1]));
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());
    let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let streams=probe["streams"].as_array().unwrap();
    let video=streams.iter().find(|v|v["codec_type"]=="video").unwrap();assert_eq!(video["nb_frames"],"4");assert_eq!(video["profile"],"High");
    let audio=streams.iter().find(|v|v["codec_type"]=="audio").unwrap();assert_eq!(audio["codec_name"],"aac");assert_eq!(audio["duration_ts"],8000);assert_eq!(probe["format"]["tags"]["title"],"two-pass");
    let rgba=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-an","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();
    assert!(rgba.status.success());assert_eq!(rgba.stdout.len(),64*64*4*4);
    for(i,frame)in rgba.stdout.chunks_exact(64*64*4).enumerate(){for p in frame.chunks_exact(4){assert!(p[0].abs_diff((i*40)as u8)<=3&&p[1].abs_diff(100)<=3&&p[2].abs_diff(160)<=3,"H.264 colour frame {i}: {p:?}");assert_eq!(p[3],255);}}
    assert_eq!(std::fs::read(peer.join("keep")).unwrap(),b"peer");
    let cancelled=parent.join("two-pass-cancelled.mp4");let cancel=tokio_util::sync::CancellationToken::new();let observer=cancel.clone();let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
    let watcher=tokio::spawn(async move{while let Some(value)=rx.recv().await{if value>50.&&value<=75.{observer.cancel();return}}panic!("first pass produced no progress")});
    let spec=RasterVideoSpec{width:640,height:360,frames:64,..spec};
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("ffmpeg"),&cancelled,spec,std::time::Duration::from_secs(60),&cancel,&tx,None,encoding,None,&[],|i,_|async move{Ok(GroupSurface{width:640,height:360,rgba:[(i*3)as u8,100,160,255].repeat(640*360)})}).await;
    drop(tx);watcher.await.unwrap();assert!(result.is_err());assert!(cancel.is_cancelled());assert!(!cancelled.exists());
    assert_eq!(std::fs::read(peer.join("keep")).unwrap(),b"peer");
    assert!(!std::fs::read_dir(&parent).unwrap().any(|entry|{let name=entry.unwrap().file_name();let name=name.to_string_lossy();name.starts_with(".raster-")&&name!=".raster-pass-peer"}));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn fixed_h264_presets_export_original_default_sizes_through_owned_jobs() {
    use axum::{body::{Body,to_bytes},http::Request};
    use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(true,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("preset-owner","test password").await.unwrap().unwrap();
    let app=support::router(state.clone());let document=String::from_utf8(source().encode_preserved().unwrap()).unwrap();
    for(preset,width,height)in [("high_quality1080",1920,1080),("high_quality2160",3840,2160),("youtube1080",1920,1080),("youtube2160",3840,2160),("vimeo1080",1920,1080),("vimeo2160",3840,2160),("social_vertical",1080,1920)]{
        let request=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":2,"preset":preset,"sources":{"1":"seq8"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs")
            .header("authorization",format!("Bearer {}",owner.token)).header("content-type","application/json")
            .body(Body::from(request.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
        assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mp4");
        let job=tokio::time::timeout(std::time::Duration::from_secs(90),async{loop{
            let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();
            if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();
        assert_eq!(result["width"],width);assert_eq!(result["height"],height);assert_eq!(result["preset"],preset);
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(&output).output().await.unwrap();
        assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let streams=probe["streams"].as_array().unwrap();
        assert_eq!(streams.len(),1);let video=&streams[0];assert_eq!(video["codec_name"],"h264");assert_eq!(video["profile"],"High");
        assert_eq!(video["width"],width);assert_eq!(video["height"],height);assert_eq!(video["nb_frames"],"2");assert_eq!(video["r_frame_rate"],"24/1");assert_eq!(video["color_space"],"bt709");
        assert_eq!(video["display_aspect_ratio"],if preset=="social_vertical"{"9:16"}else{"16:9"});
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn original_prores_yuv422_10_and_pcm24_vectors() {
    use video_kadr_backend::domain::filmcraft_yuv422_pixels::{yuv422_10,pcm24};
    let image=GroupSurface{width:2,height:2,rgba:vec![255,0,0,0,0,255,0,64,0,0,255,128,255,255,255,255]};
    let raw=yuv422_10(&image).unwrap();let planes:Vec<_>=raw.chunks_exact(2).map(|p|u16::from_le_bytes([p[0],p[1]])).collect();
    assert_eq!(planes,vec![250,691,127,940,288,736,533,491]);
    let values=[-1f32,-0.5,0.,0.5,1.,2.,-2.,f32::NAN];let raw:Vec<_>=values.iter().flat_map(|v|v.to_le_bytes()).collect();
    assert_eq!(pcm24(&raw).unwrap(),vec![1,0,128,0,0,192,0,0,0,0,0,64,255,255,127,255,255,127,1,0,128,0,0,0]);
    assert!(pcm24(&[0]).is_err());
}

#[tokio::test]
async fn prores_bridge_writes_source_exact_pcm24_and_removes_owned_conversion_files() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec,RasterAudio},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let mut raw=Vec::new();let mut expected=Vec::new();
    let values=[-1f32,-0.5,0.,0.5,1.,1.5,-1.5];let integers=[-8388607i32,-4194304,0,4194304,8388607,8388607,-8388607];
    for i in 0..8000{for sign in [1.,-1.]{raw.extend_from_slice(&(values[i%7]*sign).to_le_bytes());expected.extend_from_slice(&(integers[i%7]*sign as i32).to_le_bytes()[..3]);}}
    let audio=dir.path().join("pcm24-input.f32le");tokio::fs::write(&audio,raw).await.unwrap();
    let output=dir.path().join("outputs/pcm24.mov");let spec=RasterVideoSpec{width:64,height:64,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024*1024};
    let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
    encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&audio,frames:8000}),RasterEncoding::ProRes{profile:3},None,&[("title".into(),"ProRes PCM24".into())],|i,_|async move{Ok(GroupSurface{width:64,height:64,rgba:[(i*40)as u8,100,160,255].repeat(64*64)})}).await.unwrap();
    let extracted=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();
    assert!(extracted.status.success());assert_eq!(extracted.stdout,expected);
    let rgba=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-an","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();
    assert!(rgba.status.success());assert_eq!(rgba.stdout.len(),64*64*4*4);
    for(i,frame)in rgba.stdout.chunks_exact(64*64*4).enumerate(){for p in frame.chunks_exact(4){
        assert!(p[0].abs_diff((i*40)as u8)<=3&&p[1].abs_diff(100)<=3&&p[2].abs_diff(160)<=3,"ProRes colour frame {i}: {p:?}");assert_eq!(p[3],255);
    }}
    let cancelled=dir.path().join("outputs/pcm24-cancelled.mov");let cancel=tokio_util::sync::CancellationToken::new();let observer=cancel.clone();let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();
    let watcher=tokio::spawn(async move{while let Some(value)=rx.recv().await{if value>=50.{observer.cancel();return}}panic!("raw stage produced no progress")});
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&cancelled,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&audio,frames:8000}),RasterEncoding::ProRes{profile:3},None,&[],|_,_|async move{Ok(GroupSurface{width:64,height:64,rgba:vec![128;64*64*4]})}).await;
    drop(tx);watcher.await.unwrap();assert!(result.is_err());assert!(!cancelled.exists());assert!(output.exists());
    assert!(!std::fs::read_dir(dir.path().join("outputs")).unwrap().any(|e|e.unwrap().file_name().to_string_lossy().starts_with(".native-prores-")));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn prores_presets_publish_owned_mov_with_native_pcm24() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("prores-owner","test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("prores-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;
    media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);
    track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    let app=support::router(state.clone());
    for(preset,profile,tag)in [("prores_proxy","Proxy","apco"),("prores_lt","LT","apcs"),("prores_standard","Standard","apcn"),("prores_hq","HQ","apch")]{
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":4,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"metadata":{"title":"ProRes тест"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mov");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"mov");assert_eq!(result["videoCodec"],"prores");assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let streams=probe["streams"].as_array().unwrap();let video=streams.iter().find(|s|s["codec_type"]=="video").unwrap();let audio=streams.iter().find(|s|s["codec_type"]=="audio").unwrap();
        assert_eq!(video["codec_name"],"prores");assert_eq!(video["profile"],profile);assert_eq!(video["codec_tag_string"],tag);assert_eq!(video["pix_fmt"],"yuv422p10le");assert_eq!(video["width"],240);assert_eq!(video["height"],180);assert_eq!(video["nb_frames"],"4");assert_eq!(video["color_space"],"bt709");
        assert_eq!(audio["codec_name"],"pcm_s24le");assert_eq!(audio["sample_rate"],"48000");assert_eq!(audio["channels"],2);assert_eq!(audio["bits_per_sample"],24);assert_eq!(audio["duration_ts"],8000);assert_eq!(probe["format"]["tags"]["title"],"ProRes тест");
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-f","rawvideo","-pix_fmt","rgba","-an","pipe:1"]).output().await.unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),240*180*4*4);
        assert_ne!(&decoded.stdout[..240*180*4],&decoded.stdout[240*180*4..240*180*8]);
        let pcm=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();assert!(pcm.status.success());assert_eq!(pcm.stdout.len(),8000*2*3);assert!(pcm.stdout.iter().any(|v|*v!=0));
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn native_dnx_encoder_preserves_small_rasters_and_source_depth_rescaling() {
    for profile in [filmcraft_dnx::Profile::Lb,filmcraft_dnx::Profile::Sq,filmcraft_dnx::Profile::Hq]{
        for(width,height)in [(240,180),(64,48),(2,2)]{
            let mut encoder=filmcraft_dnx::Encoder::new(profile,width,height).unwrap();
            let mut frame=filmcraft_dnx::Frame::new(width,height,filmcraft_dnx::ChromaFormat::Yuv422,10,false);
            frame.y.fill(400);frame.cb.fill(512);frame.cr.fill(512);
            let packet=encoder.encode(&frame).unwrap();assert_eq!(packet.len(),encoder.frame_size()as usize);
            let decoded=filmcraft_dnx::decode_frame(&packet).unwrap();
            assert_eq!((decoded.width,decoded.height,decoded.bit_depth),(width,height,8));
            assert_eq!(decoded.cid,profile.cid());assert!(decoded.y.iter().all(|v|v.abs_diff(100)<=1));
            assert!(decoded.cb.iter().chain(decoded.cr.iter()).all(|v|v.abs_diff(128)<=1));
        }
    }
}

#[tokio::test]
async fn native_dnx_mov_matches_original_packets_pcm24_and_uint32_pixel_aspect() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec,RasterAudio},domain::composition::FrameRateRatio};
    use filmcraft_isobmff::TrackKind;
    let(state,dir)=support::make_state(false,false).await;
    let mut raw=Vec::new();let mut expected_audio=Vec::new();
    for i in 0..8000{for(value,integer)in [(0.5f32,4194304i32),(-0.5,-4194304),(1.,8388607),(-1.,-8388607)].into_iter().skip((i%2)*2).take(2){raw.extend_from_slice(&value.to_le_bytes());expected_audio.extend_from_slice(&integer.to_le_bytes()[..3]);}}
    let audio=dir.path().join("native-audio.f32le");tokio::fs::write(&audio,raw).await.unwrap();
    let output=dir.path().join("outputs/native-dnx.mov");let spec=RasterVideoSpec{width:240,height:180,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024*1024};
    let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
    encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&audio,frames:8000}),RasterEncoding::DnxHr{profile:2},Some((u32::MAX,u32::MAX-1)),&[("title".into(),"Native DNxHR 🎬".into())],|i,_|async move{Ok(GroupSurface{width:240,height:180,rgba:[(i*40)as u8,100,160,255].repeat(240*180)})}).await.unwrap();
    let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();
    let video=movie.tracks.iter().position(|t|t.kind==TrackKind::Video).unwrap();let sound=movie.tracks.iter().position(|t|t.kind==TrackKind::Audio).unwrap();
    assert_eq!(movie.tracks[video].entries[0].video.as_ref().unwrap().pixel_aspect,Some((u32::MAX,u32::MAX-1)));
    assert_eq!(movie.tracks[video].timescale,24);assert_eq!(movie.tracks[video].duration,4);assert_eq!(movie.tracks[video].samples.len(),4);
    assert_eq!(movie.tracks[sound].timescale,48000);assert_eq!(movie.tracks[sound].duration,8000);
    let mut original=filmcraft_dnx::Encoder::new(filmcraft_dnx::Profile::Hq,240,180).unwrap();
    for(i,(y,u,v))in [(349,658,327),(379,642,397),(408,625,467),(437,609,537)].into_iter().enumerate(){
        let mut frame=filmcraft_dnx::Frame::new(240,180,filmcraft_dnx::ChromaFormat::Yuv422,10,false);frame.y.fill(y);frame.cb.fill(u);frame.cr.fill(v);
        let expected=original.encode(&frame).unwrap();let packet=movie.read_sample(&bytes,video,i).unwrap();assert_eq!(packet,expected,"source native packet {i}");
    }
    let mut pcm=Vec::new();for i in 0..movie.tracks[sound].samples.len(){pcm.extend(movie.read_sample(&bytes,sound,i).unwrap());}assert_eq!(pcm,expected_audio);
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
    let stream=probe["streams"].as_array().unwrap().iter().find(|s|s["codec_type"]=="video").unwrap();assert_eq!(stream["codec_name"],"dnxhd");assert_eq!(stream["profile"],"DNXHR HQ");assert_eq!(stream["width"],240);assert_eq!(stream["height"],180);assert_eq!(probe["format"]["tags"]["title"],"Native DNxHR 🎬");
    let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-an","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));assert_eq!(decoded.stdout.len(),240*180*4*4);
    for(i,frame)in decoded.stdout.chunks_exact(240*180*4).enumerate(){for p in frame.chunks_exact(4){assert!(p[0].abs_diff((i*40)as u8)<=5&&p[1].abs_diff(100)<=5&&p[2].abs_diff(160)<=5,"DNxHR colour frame {i}: {p:?}");assert_eq!(p[3],255);}}
    assert!(!std::fs::read_dir(dir.path().join("outputs")).unwrap().any(|e|e.unwrap().file_name().to_string_lossy().starts_with(".native-dnx-")));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_dnx_presets_run_without_ffmpeg_tools_and_publish_private_pcm24_movies() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("dnx-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("dnx-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();
    clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    let app=support::router(state.clone());
    for(preset,profile)in [("dnxhr_lb","DNXHR LB"),("dnxhr_sq","DNXHR SQ"),("dnxhr_hq","DNXHR HQ")]{
        let input=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":4,"sources":{"1":"seq8"},"preset":preset,"withAudio":true,"pixelAspect":[2,1],"metadata":{"title":"Native DNxHR job"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(input.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mov");
        let job=tokio::time::timeout(std::time::Duration::from_secs(30),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"mov");assert_eq!(result["videoCodec"],"dnxhd");assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let streams=probe["streams"].as_array().unwrap();let video=streams.iter().find(|s|s["codec_type"]=="video").unwrap();let audio=streams.iter().find(|s|s["codec_type"]=="audio").unwrap();
        assert_eq!(video["profile"],profile);assert_eq!(video["pix_fmt"],"yuv422p");assert_eq!(video["width"],240);assert_eq!(video["height"],180);assert_eq!(video["nb_frames"],"4");assert_eq!(video["display_aspect_ratio"],"8:3");
        assert_eq!(audio["codec_name"],"pcm_s24le");assert_eq!(audio["sample_rate"],"48000");assert_eq!(audio["channels"],2);assert_eq!(audio["duration_ts"],8000);assert_eq!(probe["format"]["tags"]["title"],"Native DNxHR job");
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-an","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().await.unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),240*180*4*4);assert_ne!(&decoded.stdout[..240*180*4],&decoded.stdout[240*180*4..240*180*8]);
        let pcm=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();assert!(pcm.status.success());assert_eq!(pcm.stdout.len(),8000*2*3);assert!(pcm.stdout.iter().any(|v|*v!=0));
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_dnx_cancel_deadline_and_existing_output_preserve_other_files() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio,jobs::{classify_job_error,ErrorKind}};
    let(state,dir)=support::make_state(false,false).await;let parent=dir.path().join("outputs");let peer=parent.join(".native-dnx-peer.keep");tokio::fs::write(&peer,b"peer").await.unwrap();
    let existing=parent.join("existing.mov");tokio::fs::write(&existing,b"existing").await.unwrap();
    let spec=RasterVideoSpec{width:64,height:48,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};let encoding=RasterEncoding::DnxHr{profile:2};
    let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let called=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let observe=called.clone();
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&existing,spec,std::time::Duration::from_secs(5),&cancel,&tx,None,encoding,None,&[],move|_,_|{observe.store(true,std::sync::atomic::Ordering::Relaxed);async{Ok(GroupSurface{width:64,height:48,rgba:vec![128;64*48*4]})}}).await;
    assert!(result.is_err());assert!(!called.load(std::sync::atomic::Ordering::Relaxed));assert_eq!(std::fs::read(&existing).unwrap(),b"existing");
    let output=parent.join("cancelled.mov");let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let observer=cancel.clone();
    let watcher=tokio::spawn(async move{while let Some(value)=rx.recv().await{if value>0.{observer.cancel();return}}panic!("native encoder produced no progress")});
    let result=tokio::time::timeout(std::time::Duration::from_secs(5),encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,encoding,None,&[],|index,_|async move{if index>0{std::future::pending::<()>().await;}Ok(GroupSurface{width:64,height:48,rgba:vec![128;64*48*4]})})).await.unwrap();
    drop(tx);watcher.await.unwrap();assert!(result.is_err());assert!(!output.exists());
    let output=parent.join("deadline.mov");let cancel=tokio_util::sync::CancellationToken::new();let(tx,_)=tokio::sync::mpsc::unbounded_channel();
    let result=tokio::time::timeout(std::time::Duration::from_secs(5),encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_millis(100),&cancel,&tx,None,encoding,None,&[],|index,_|async move{if index>0{std::future::pending::<()>().await;}Ok(GroupSurface{width:64,height:48,rgba:vec![128;64*48*4]})})).await.unwrap();
    assert_eq!(classify_job_error(&result.unwrap_err()),ErrorKind::Timeout);assert!(!output.exists());
    assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert_eq!(std::fs::read(&existing).unwrap(),b"existing");
    assert!(!std::fs::read_dir(&parent).unwrap().any(|e|{let name=e.unwrap().file_name();let name=name.to_string_lossy();name.starts_with(".native-dnx-")&&name!=".native-dnx-peer.keep"}));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_prores_all_profiles_match_original_packets_and_preserve_native_movie_metadata() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:64,height:48,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    let metadata=vec![("title".into(),"Native ProRes 🎬".into()),("artist".into(),"Автор".into()),("copyright".into(),"© 2026".into()),("description".into(),"one\ntwo".into()),("comment".into(),"literal = -metadata".into())];
    for(index,profile)in [filmcraft_prores::Profile::Proxy,filmcraft_prores::Profile::Lt,filmcraft_prores::Profile::Standard,filmcraft_prores::Profile::Hq].into_iter().enumerate(){
        let output=dir.path().join(format!("outputs/native-prores-{index}.mov"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::ProRes{profile:index as u8},Some((u32::MAX,u32::MAX-1)),&metadata,|i,_|async move{Ok(GroupSurface{width:64,height:48,rgba:[(i*40)as u8,100,160,255].repeat(64*48)})}).await.unwrap();
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let video=movie.tracks.iter().position(|v|v.kind==filmcraft_isobmff::TrackKind::Video).unwrap();
        assert_eq!(movie.tracks[video].entries[0].video.as_ref().unwrap().pixel_aspect,Some((u32::MAX,u32::MAX-1)));
        assert_eq!(movie.tracks[video].samples.len(),4);assert_eq!(movie.tracks[video].timescale,24);assert_eq!(movie.tracks[video].duration,4);
        assert_eq!(movie.metadata.get("©des"),Some("one\ntwo"));assert_eq!(movie.metadata.get("description"),Some("one\ntwo"));
        let mut original=filmcraft_prores::Encoder::new(profile,64,48);
        for(i,(y,u,v))in [(349,658,327),(379,642,397),(408,625,467),(437,609,537)].into_iter().enumerate(){
            let mut frame=filmcraft_prores::Frame::new(64,48,filmcraft_prores::ChromaFormat::Yuv422,10,false);frame.y.fill(y);frame.cb.fill(u);frame.cr.fill(v);
            let expected=original.encode(&frame).unwrap();let packet=movie.read_sample(&bytes,video,i).unwrap();assert_eq!(packet,expected,"original profile {profile:?}, frame {i}");
            let decoded=filmcraft_prores::decode_frame(&packet).unwrap();assert_eq!((decoded.width,decoded.height,decoded.bit_depth),(64,48,10));
        }
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        for(key,value)in &metadata{assert_eq!(probe["format"]["tags"][key],*value,"native MOV metadata {key}");}
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_apv_all_profiles_match_original_packets_and_preserve_native_movie_metadata() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:64,height:48,frames:4,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    let metadata=vec![("title".into(),"Native APV 🎬".into()),("artist".into(),"Автор".into()),("copyright".into(),"© 2026".into()),("description".into(),"one\ntwo".into()),("comment".into(),"literal = -metadata".into())];
    for(index,profile)in [filmcraft_apv::Profile::P422_10,filmcraft_apv::Profile::P422_12].into_iter().enumerate(){
        let output=dir.path().join(format!("outputs/native-apv-{index}.mov"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::Apv{profile:index as u8},Some((u32::MAX,u32::MAX-1)),&metadata,|i,_|async move{Ok(GroupSurface{width:64,height:48,rgba:[(i*40)as u8,100,160,255].repeat(64*48)})}).await.unwrap();
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let video=movie.tracks.iter().position(|v|v.kind==filmcraft_isobmff::TrackKind::Video).unwrap();
        assert_eq!(movie.tracks[video].entries[0].video.as_ref().unwrap().pixel_aspect,Some((u32::MAX,u32::MAX-1)));
        assert_eq!(movie.tracks[video].samples.len(),4);assert_eq!(movie.tracks[video].timescale,24);assert_eq!(movie.tracks[video].duration,4);
        assert_eq!(movie.metadata.get("©des"),Some("one\ntwo"));assert_eq!(movie.metadata.get("description"),Some("one\ntwo"));
        let mut original=filmcraft_apv::Encoder::new(profile,64,48).unwrap();
        for(i,(y,u,v))in [(349,658,327),(379,642,397),(408,625,467),(437,609,537)].into_iter().enumerate(){
            let mut frame=filmcraft_apv::Frame::new(64,48,filmcraft_apv::ChromaFormat::Yuv422,10,false);frame.y.fill(y);frame.cb.fill(u);frame.cr.fill(v);
            let expected=original.encode_raw_au(&frame).unwrap();let packet=movie.read_sample(&bytes,video,i).unwrap();assert_eq!(packet,expected,"original profile {profile:?}, frame {i}");
            let decoded=filmcraft_apv::decode_frame(&packet).unwrap();assert_eq!((decoded.width,decoded.height,decoded.bit_depth),(64,48,profile.default_bit_depth()));
        }
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        for(key,value)in &metadata{assert_eq!(probe["format"]["tags"][key],*value,"native MOV metadata {key}");}
    }
    // Cancellation after a published progress event must preserve peer files and remove owned staging.
    for profile in 0..2 {
        let output=dir.path().join(format!("outputs/cancel-apv-{profile}.mov"));
        let peer=dir.path().join("outputs/.native-apv-peer.mov");std::fs::write(&peer,b"peer").unwrap();
        let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::Apv{profile},None,&[],|i,_|{
            if i>0 { assert!(rx.try_recv().unwrap()>0.);stop.cancel(); }
            async move {Ok(GroupSurface{width:64,height:48,rgba:[40,100,160,255].repeat(64*48)})}
        }).await;
        assert!(result.is_err());assert!(!output.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");
        assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|entry|{let name=entry.unwrap().file_name();let name=name.to_string_lossy();!name.starts_with(".native-apv-")||name==".native-apv-peer.mov"}));
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn apv_presets_publish_owned_mov_with_native_pcm24() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("apv-owner","test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("apv-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;
    media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);
    track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    let app=support::router(state.clone());
    for(preset,depth)in [("apv42210",10),("apv42212",12)]{
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":4,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"metadata":{"title":"ProRes тест"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mov");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"mov");assert_eq!(result["videoCodec"],"apv");assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let streams=probe["streams"].as_array().unwrap();let video=streams.iter().find(|s|s["codec_type"]=="video").unwrap();let audio=streams.iter().find(|s|s["codec_type"]=="audio").unwrap();
        assert_eq!(video["codec_tag_string"],"apv1");assert_eq!(video["width"],240);assert_eq!(video["height"],180);assert_eq!(video["nb_frames"],"4");
        assert_eq!(audio["codec_name"],"pcm_s24le");assert_eq!(audio["sample_rate"],"48000");assert_eq!(audio["channels"],2);assert_eq!(audio["bits_per_sample"],24);assert_eq!(audio["duration_ts"],8000);assert_eq!(probe["format"]["tags"]["title"],"ProRes тест");
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let track=movie.tracks.iter().position(|v|v.kind==filmcraft_isobmff::TrackKind::Video).unwrap();
        let packets=(0..4).map(|i|movie.read_sample(&bytes,track,i).unwrap()).collect::<Vec<_>>();
        for packet in &packets{let f=filmcraft_apv::decode_frame(packet).unwrap();assert_eq!((f.width,f.height,f.bit_depth),(240,180,depth));}
        assert_ne!(packets[0],packets[1]);
        let pcm=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();assert!(pcm.status.success());assert_eq!(pcm.stdout.len(),8000*2*3);assert!(pcm.stdout.iter().any(|v|*v!=0));
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn mxf_op1a_presets_publish_owned_mxf_with_native_pcm24() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("mxf-owner","test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("mxf-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;
    media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);
    track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    document["project"]["items"]["6"]["kind"]["Sequence"]["start_timecode"]=json!(86400);
    let app=support::router(state.clone());
    for(preset,codec)in [("mxf_dnxhr_hq","dnxhd"),("mxf_prores_hq","prores")]{
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"1","frames":4,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"metadata":{"title":"ProRes тест"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mxf");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"mxf");assert_eq!(result["videoCodec"],codec);assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let streams=probe["streams"].as_array().unwrap();let video=streams.iter().find(|s|s["codec_type"]=="video").unwrap();let audio=streams.iter().find(|s|s["codec_type"]=="audio").unwrap();
        assert_eq!(video["codec_name"],codec);assert_eq!(video["width"],240);assert_eq!(video["height"],180);assert_eq!(video["color_space"],"bt709");
        assert_eq!(audio["codec_name"],"pcm_s24le");assert_eq!(audio["sample_rate"],"48000");assert_eq!(audio["channels"],2);assert_eq!(audio["bits_per_sample"],24);
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();
        assert_eq!(movie.timecode.as_ref().unwrap().start,86401);
        let pcm=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();assert!(pcm.status.success());assert_eq!(pcm.stdout.len(),8000*2*3);assert!(pcm.stdout.iter().any(|v|*v!=0));
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_mxf_packets_timecode_and_cancellation_preserve_source_contract() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:64,height:48,frames:4,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},staging_bytes:1024*1024};
    for prores in [false,true] {
        let output=dir.path().join(format!("outputs/exact-{prores}.mxf"));
        let encoding=RasterEncoding::Mxf{prores,start:107892,drop_frame:true};
        let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,encoding,Some((4,3)),&[],|i,_|async move{Ok(GroupSurface{width:64,height:48,rgba:[(i*40)as u8,100,160,255].repeat(64*48)})}).await.unwrap();
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();
        assert_eq!(movie.timecode.as_ref().unwrap().format(),"01:00:00;00");
        let video=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(movie.tracks[video].samples.len(),4);
        assert_eq!(movie.index_segments.iter().map(|s|s.entries.len()).sum::<usize>(),4);
        assert_eq!(movie.tracks[video].picture.as_ref().unwrap().aspect_ratio,filmcraft_mxf::Rational::new(16,9));
        let mut original_prores=filmcraft_prores::Encoder::new(filmcraft_prores::Profile::Hq,64,48);
        let mut original_dnx=filmcraft_dnx::Encoder::new(filmcraft_dnx::Profile::Hq,64,48).unwrap();
        for(i,(y,u,v))in [(349,658,327),(379,642,397),(408,625,467),(437,609,537)].into_iter().enumerate(){
            let expected=if prores {
                let mut frame=filmcraft_prores::Frame::new(64,48,filmcraft_prores::ChromaFormat::Yuv422,10,false);frame.y.fill(y);frame.cb.fill(u);frame.cr.fill(v);original_prores.encode(&frame).unwrap()
            } else {
                let mut frame=filmcraft_dnx::Frame::new(64,48,filmcraft_dnx::ChromaFormat::Yuv422,10,false);frame.y.fill(y);frame.cb.fill(u);frame.cr.fill(v);original_dnx.encode(&frame).unwrap()
            };
            assert_eq!(movie.read_sample(&bytes,video,i).unwrap(),expected);
        }
        let peer=dir.path().join("outputs/.native-mxf-peer.mov");std::fs::write(&peer,b"peer").unwrap();
        let cancelled=dir.path().join(format!("outputs/cancel-{prores}.mxf"));let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let stop=cancel.clone();
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&cancelled,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,encoding,None,&[],|i,_|{
            if i>0 {assert!(rx.try_recv().unwrap()>0.);stop.cancel();}
            async move{Ok(GroupSurface{width:64,height:48,rgba:[40,100,160,255].repeat(64*48)})}
        }).await;
        assert!(result.is_err());assert!(!cancelled.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");
        assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|entry|{let name=entry.unwrap().file_name();let name=name.to_string_lossy();!name.starts_with(".native-mxf-")||name==".native-mxf-peer.mov"}));
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_opatom_bundle_preserves_packets_mono_pcm_and_fractional_clocks() {
    use std::io::Read;
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterAudio,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:64,height:48,frames:4,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},staging_bytes:1024*1024};
    let samples=(4u64*48000*1001/30000)as usize;
    let channels=[vec![0.25f32;samples],vec![-0.5f32;samples]];
    let raw=dir.path().join("stereo.f32le");let mut audio=Vec::new();
    for i in 0..samples {audio.extend_from_slice(&0.25f32.to_le_bytes());audio.extend_from_slice(&(-0.5f32).to_le_bytes());assert!(i<7000);}
    std::fs::write(&raw,audio).unwrap();
    for with_audio in [false,true] {
        let output=dir.path().join(format!("outputs/atom-{with_audio}.zip"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,with_audio.then_some(RasterAudio{path:&raw,frames:samples as u64}),RasterEncoding::MxfAtom{start:107892,drop_frame:true},None,&[],|i,_|async move{Ok(GroupSurface{width:64,height:48,rgba:[(i*40)as u8,100,160,255].repeat(64*48)})}).await.unwrap();
        let mut archive=zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();assert_eq!(archive.len(),if with_audio{3}else{1});
        let mut probe_material=None;let mut probe_files=std::collections::HashSet::new();
        let mut original=filmcraft_dnx::Encoder::new(filmcraft_dnx::Profile::Hq,64,48).unwrap();
        for entry in 0..archive.len() {
            let mut file=archive.by_index(entry).unwrap();let name=file.name().to_string();let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();
            assert_eq!(name,if entry==0{"export.mxf".to_string()}else{format!("export_A{entry}.mxf")});
            let movie=filmcraft_mxf::open(&bytes).unwrap();assert_eq!(movie.tracks.len(),1);assert_eq!(movie.timecode.as_ref().unwrap().format(),"01:00:00;00");assert_eq!(movie.tracks[0].duration,Some(4));
            if entry==0 {
                assert_eq!(movie.tracks[0].kind,filmcraft_mxf::TrackKind::Picture);
                for(i,(y,u,v))in [(349,658,327),(379,642,397),(408,625,467),(437,609,537)].into_iter().enumerate(){let mut f=filmcraft_dnx::Frame::new(64,48,filmcraft_dnx::ChromaFormat::Yuv422,10,false);f.y.fill(y);f.cb.fill(u);f.cr.fill(v);assert_eq!(movie.read_sample(&bytes,0,i).unwrap(),original.encode(&f).unwrap());}
            }else{
                let track=&movie.tracks[0];assert_eq!(track.kind,filmcraft_mxf::TrackKind::Sound);let info=track.sound.as_ref().unwrap();assert_eq!((info.channels,info.bits),(1,24));
                let pcm=track.chunks.iter().flat_map(|chunk|bytes[chunk.offset as usize..(chunk.offset+chunk.size)as usize].iter().copied()).collect::<Vec<_>>();
                assert_eq!(pcm,filmcraft_mxf::encode_pcm(&[channels[entry-1].clone()],24));assert_eq!(pcm.len(),samples*3);
            }
            let extracted=dir.path().join(format!("{with_audio}-{name}"));std::fs::write(&extracted,&bytes).unwrap();
            let result=tokio::process::Command::new("ffprobe").args(["-v","error","-show_format","-show_streams","-of","json"]).arg(&extracted).output().await.unwrap();assert!(result.status.success());let probe:Value=serde_json::from_slice(&result.stdout).unwrap();
            let material=probe["format"]["tags"]["material_package_umid"].as_str().unwrap().to_string();if let Some(expected)=&probe_material{assert_eq!(&material,expected)}else{probe_material=Some(material)}
            assert!(probe_files.insert(probe["streams"][0]["tags"]["file_package_umid"].as_str().unwrap().to_string()));
        }
    }
    for with_audio in [false,true] {
        let peer=dir.path().join("outputs/.native-mxf-peer.mov");std::fs::write(&peer,b"peer").unwrap();
        let before=std::fs::read_dir(dir.path().join("outputs")).unwrap().map(|v|v.unwrap().file_name()).collect::<std::collections::HashSet<_>>();
        let output=dir.path().join(format!("outputs/cancel-atom-{with_audio}.zip"));let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,with_audio.then_some(RasterAudio{path:&raw,frames:samples as u64}),RasterEncoding::MxfAtom{start:107892,drop_frame:true},None,&[],|i,_|{
            if i>0 {assert!(rx.try_recv().unwrap()>0.);stop.cancel();}
            async move{Ok(GroupSurface{width:64,height:48,rgba:[40,100,160,255].repeat(64*48)})}
        }).await;
        assert!(result.is_err());assert!(!output.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");
        let after=std::fs::read_dir(dir.path().join("outputs")).unwrap().map(|v|v.unwrap().file_name()).collect::<std::collections::HashSet<_>>();assert_eq!(before,after);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn opatom_job_publishes_owner_only_bundle_with_native_pcm24() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("mxf-owner","test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("mxf-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;
    media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);
    track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    document["project"]["items"]["6"]["kind"]["Sequence"]["start_timecode"]=json!(86400);
    let app=support::router(state.clone());
    for(preset,codec)in [("mxf_atom_dnxhr_hq","dnxhd")]{
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"1","frames":4,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"metadata":{"title":"ProRes тест"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"zip");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"zip");assert_eq!(result["videoCodec"],codec);assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let archive=zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();assert_eq!(archive.len(),3);
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_h264_mxf_job_preserves_audio_frames_and_access_with_native_pcm24() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;
    add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("mxf-owner","test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("mxf-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];
    media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;
    media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);
    track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    document["project"]["items"]["6"]["kind"]["Sequence"]["start_timecode"]=json!(86400);
    let app=support::router(state.clone());
    for(preset,codec)in [("mxf_h264","h264")]{
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"1","frames":4,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"metadata":{"title":"ProRes тест"}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");assert_eq!(accepted["container"],"mxf");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
        assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"mxf");assert_eq!(result["videoCodec"],codec);assert_eq!(result["audioCodec"],"pcm_s24le");
        let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_format","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        let streams=probe["streams"].as_array().unwrap();let video=streams.iter().find(|s|s["codec_type"]=="video").unwrap();let audio=streams.iter().find(|s|s["codec_type"]=="audio").unwrap();
        assert_eq!(video["codec_name"],codec);assert_eq!(video["width"],240);assert_eq!(video["height"],180);assert_eq!(video["color_space"],"bt709");
        assert_eq!(audio["codec_name"],"pcm_s24le");assert_eq!(audio["sample_rate"],"48000");assert_eq!(audio["channels"],2);assert_eq!(audio["bits_per_sample"],24);
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();
        assert_eq!(movie.timecode.as_ref().unwrap().start,86401);
        let pcm=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-c:a","copy","-f","s24le","pipe:1"]).output().await.unwrap();assert!(pcm.status.success());assert_eq!(pcm.stdout.len(),8000*2*3);assert!(pcm.stdout.iter().any(|v|*v!=0));
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_h264_mxf_matches_original_delayed_packets_and_decodes_in_display_order() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:64,height:48,frames:12,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    let output=dir.path().join("outputs/native-h264.mxf");let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
    encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::MxfH264{start:86400,drop_frame:false,keyint:None,profile:Default::default(),bitrate:None},None,&[],|i,_|async move{
        let mut rgba=[0,0,0,255].repeat(64*48);for y in 8..16{for x in i as usize*2..i as usize*2+8{rgba[(y*64+x)*4..(y*64+x)*4+3].fill(255);}}Ok(GroupSurface{width:64,height:48,rgba})
    }).await.unwrap();
    let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let video=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();
    assert_eq!(movie.timecode.as_ref().unwrap().format(),"01:00:00:00");let track=&movie.tracks[video];assert_eq!(track.samples.len(),12);assert!(track.indexed);assert!(track.temporal_offsets);
    let mut config=filmcraft_h264enc::EncoderConfig::new(64,48,24,1);config.format=filmcraft_h264enc::PacketFormat::AnnexB;config.aud=true;config.keyint=48;config.slices=1;config.rate=filmcraft_h264enc::RateControl::Vbr{target_kbps:100,max_kbps:150};
    let mut original=filmcraft_h264enc::Encoder::new(config).unwrap();let mut packets=Vec::new();let c=vec![128u8;64*48/4];
    for i in 0..12{let mut y=vec![16u8;64*48];for row in 8..16{for x in i*2..i*2+8{y[row*64+x]=235;}}packets.extend(original.try_encode(&filmcraft_h264enc::YuvFrame{y:&y,u:&c,v:&c,y_stride:64,uv_stride:32},i as i64).unwrap());}
    packets.extend(original.flush());assert_eq!(packets.len(),12);assert!(packets.iter().any(|p|p.frame_type==filmcraft_h264enc::FrameType::B));
    for(i,packet)in packets.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,video,i).unwrap(),packet.data);}
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-show_frames","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["profile"],"High");let frames=probe["frames"].as_array().unwrap();assert_eq!(frames.len(),12);assert!(frames.iter().any(|f|f["pict_type"]=="B"));
    let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-f","rawvideo","-pix_fmt","yuv420p","-an","pipe:1"]).output().await.unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));assert_eq!(decoded.stdout.len(),12*64*48*3/2);
    for(i,frame)in decoded.stdout.chunks_exact(64*48*3/2).enumerate(){let center=frame[11*64+i*2+3];assert!(center>180,"display frame {i} center {center}");}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_h264_mxf_short_clips_flush_pcm_and_cancel_without_publication() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterAudio,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    for count in 1..=3 {
        let spec=RasterVideoSpec{width:64,height:48,frames:count,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},staging_bytes:1024*1024};let samples=count*48000*1001/30000;
        let raw=dir.path().join(format!("audio-{count}.f32le"));let mut audio=Vec::new();for _ in 0..samples{audio.extend_from_slice(&0.25f32.to_le_bytes());audio.extend_from_slice(&(-0.5f32).to_le_bytes());}std::fs::write(&raw,audio).unwrap();
        let output=dir.path().join(format!("outputs/short-{count}.mxf"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&raw,frames:samples}),RasterEncoding::MxfH264{start:107892,drop_frame:true,keyint:None,profile:Default::default(),bitrate:None},None,&[],|_,_|async move{Ok(GroupSurface{width:64,height:48,rgba:[40,100,160,255].repeat(64*48)})}).await.unwrap();
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let video=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(video.samples.len(),count as usize);assert_eq!(video.duration,Some(count as i64));
        let sound=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Sound).unwrap();assert_eq!(sound.duration,Some(count as i64));
        let pcm=sound.chunks.iter().flat_map(|c|bytes[c.offset as usize..(c.offset+c.size)as usize].iter().copied()).collect::<Vec<_>>();assert_eq!(pcm,filmcraft_mxf::encode_pcm(&[vec![0.25;samples as usize],vec![-0.5;samples as usize]],24));
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-count_frames","-show_streams","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let streams=probe["streams"].as_array().unwrap();let stream=streams.iter().find(|s|s["codec_type"]=="video").unwrap();assert_eq!(stream["nb_read_frames"],count.to_string());
    }
    let spec=RasterVideoSpec{width:64,height:48,frames:12,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    let peer=dir.path().join("outputs/.native-mxf-peer.mov");std::fs::write(&peer,b"peer").unwrap();let output=dir.path().join("outputs/cancel-h264.mxf");let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::MxfH264{start:0,drop_frame:false,keyint:None,profile:Default::default(),bitrate:None},None,&[],|i,_|{if i>0{assert!(rx.try_recv().unwrap()>0.);stop.cancel();}async move{Ok(GroupSurface{width:64,height:48,rgba:[40,100,160,255].repeat(64*48)})}}).await;
    assert!(result.is_err());assert!(!output.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|e|{let n=e.unwrap().file_name();let n=n.to_string_lossy();!n.starts_with(".native-mxf-")||n==".native-mxf-peer.mov"}));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_image_sequence_outputs_match_source_stills_and_preserve_odd_pixels() {
    use std::io::Read;
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;
    let spec=RasterVideoSpec{width:7,height:5,frames:3,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    let surface=|index:u64|GroupSurface{width:7,height:5,rgba:(0..35).flat_map(|p|[p*7,p*3,index as u8*80,[0,127,255][p as usize%3]]).collect()};
    for(format,extension)in [(0,"png"),(1,"tif"),(2,"bmp")] {
        let output=dir.path().join(format!("outputs/images-{format}.zip"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::ImageSequence{format},None,&[],|i,time|{assert_eq!(time,i*1_000_000/24);let frame=surface(i);async move{Ok(frame)}}).await.unwrap();
        let mut archive=zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();assert_eq!(archive.len(),3);
        for i in 0..3 {
            let mut entry=archive.by_index(i).unwrap();assert_eq!(entry.name(),format!("export{i:03}.{extension}"));let mut bytes=Vec::new();entry.read_to_end(&mut bytes).unwrap();let expected=surface(i as u64);
            let mut source=std::io::Cursor::new(Vec::new());
            if format==0{image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut source),&expected.rgba,7,5,image::ExtendedColorType::Rgba8).unwrap();}
            else{let rgb=image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(7,5,expected.rgba.clone()).unwrap()).to_rgb8();rgb.write_to(&mut source,if format==1{image::ImageFormat::Tiff}else{image::ImageFormat::Bmp}).unwrap();}
            assert_eq!(bytes,source.into_inner(),"original still encoder {format}/{i}");
            let decoded=image::load_from_memory(&bytes).unwrap();assert_eq!((decoded.width(),decoded.height()),(7,5));
            if format==0{assert_eq!(decoded.to_rgba8().into_raw(),expected.rgba)}else{assert_eq!(decoded.to_rgb8().into_raw(),expected.rgba.chunks_exact(4).flat_map(|p|p[..3].iter().copied()).collect::<Vec<_>>());}
        }
        let peer=dir.path().join("outputs/.native-images-peer.zip");std::fs::write(&peer,b"peer").unwrap();let target=dir.path().join(format!("outputs/cancel-images-{format}.zip"));let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let stop=cancel.clone();
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&target,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::ImageSequence{format},None,&[],|i,_|{if i>0{assert!(rx.try_recv().unwrap()>0.);stop.cancel();}let frame=surface(i);async move{Ok(frame)}}).await;
        assert!(result.is_err());assert!(!target.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|e|{let n=e.unwrap().file_name();let n=n.to_string_lossy();!n.starts_with(".native-images-")||n==".native-images-peer.zip"}));
        let original_bytes=std::fs::read(&output).unwrap();
        let token=tokio_util::sync::CancellationToken::new();let(tx,_)=tokio::sync::mpsc::unbounded_channel();let mut called=false;
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&token,&tx,None,RasterEncoding::ImageSequence{format},None,&[],|_,_|{called=true;async{anyhow::bail!("must not render")}}).await;
        assert!(result.is_err());assert!(!called);assert_eq!(std::fs::read(&output).unwrap(),original_bytes);
    }
    for(count,first,last)in [(1000,"export000.png","export999.png"),(1001,"export0000.png","export1000.png")] {
        let output=dir.path().join(format!("outputs/numbering-{count}.zip"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let spec=RasterVideoSpec{width:1,height:1,frames:count,..spec};
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::ImageSequence{format:0},None,&[],|_,_|async{Ok(GroupSurface{width:1,height:1,rgba:vec![1,2,3,4]})}).await.unwrap();
        let mut archive=zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();assert_eq!(archive.len(),count as usize);assert_eq!(archive.by_index(0).unwrap().name(),first);assert_eq!(archive.by_index(count as usize-1).unwrap().name(),last);
    }
    for format in [3,255] {
        let output=dir.path().join(format!("outputs/invalid-images-{format}.zip"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();let mut called=false;
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&token,&tx,None,RasterEncoding::ImageSequence{format},None,&[],|_,_|{called=true;async{anyhow::bail!("must not render")}}).await;
        assert!(result.unwrap_err().to_string().contains("invalid image sequence format"));assert!(!called);assert!(!output.exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn image_sequence_jobs_preserve_custom_odd_canvas_and_owner_only_downloads() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("images-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("images-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["width"]=json!(241);document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["height"]=json!(181);
    let app=support::router(state.clone());
    for(preset,codec)in [("png_sequence","png"),("tiff_sequence","tiff"),("bmp_sequence","bmp")] {
        let mut payload=json!({"document":document,"sequenceId":"6","startFrames":"1","frames":3,"preset":preset,"sources":{"1":"seq8"},"frameSize":[57,43]});
        payload["withAudio"]=json!(true);let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),400);
        payload["withAudio"]=json!(false);let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),202);let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(accepted["container"],"zip");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["videoCodec"],codec);assert_eq!(result["mediaType"],"image_sequence");assert_eq!(result["width"],57);assert_eq!(result["height"],43);
        let mut archive=zip::ZipArchive::new(std::fs::File::open(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap()).unwrap();assert_eq!(archive.len(),3);
        for i in 0..3{use std::io::Read;let mut bytes=Vec::new();archive.by_index(i).unwrap().read_to_end(&mut bytes).unwrap();let frame=image::load_from_memory(&bytes).unwrap();assert_eq!((frame.width(),frame.height()),(57,43));}
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_gif_matches_original_palette_alpha_delay_loop_and_cleanup() {
    use std::io::Cursor;use image::AnimationDecoder;
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:7,height:5,frames:3,frame_rate:FrameRateRatio{numerator:15,denominator:1},staging_bytes:1024*1024};
    let surface=|i:u64|GroupSurface{width:7,height:5,rgba:(0..35).flat_map(|p|[p*7,p*3,i as u8*80,[0,127,255][p as usize%3]]).collect()};
    let output=dir.path().join("outputs/native.gif");let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();
    encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::Gif,None,&[],|i,time|{assert_eq!(time,i*1_000_000/15);let frame=surface(i);async move{Ok(frame)}}).await.unwrap();
    let bytes=std::fs::read(&output).unwrap();let mut expected=Vec::new();{
        let mut encoder=image::codecs::gif::GifEncoder::new_with_speed(&mut expected,10);encoder.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        for i in 0..3 {let frame=image::RgbaImage::from_raw(7,5,surface(i).rgba).unwrap();encoder.encode_frame(image::Frame::from_parts(frame,0,0,image::Delay::from_numer_denom_ms(1000,15))).unwrap();}
    }
    assert_eq!(bytes,expected);assert_eq!(bytes.last(),Some(&0x3b));assert!(bytes.windows(19).any(|v|v==b"\x21\xff\x0bNETSCAPE2.0\x03\x01\0\0\0"));
    let frames=image::codecs::gif::GifDecoder::new(Cursor::new(&bytes)).unwrap().into_frames().collect_frames().unwrap();assert_eq!(frames.len(),3);
    for frame in frames {assert_eq!(frame.buffer().dimensions(),(7,5));assert_eq!(frame.delay().numer_denom_ms(),(60,1));assert!(frame.buffer().pixels().any(|p|p[3]==0));assert!(frame.buffer().pixels().any(|p|p[3]==255));}
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-count_frames","-show_streams","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["codec_name"],"gif");assert_eq!(probe["streams"][0]["nb_read_frames"],"3");assert_eq!(probe["streams"][0]["width"],7);assert_eq!(probe["streams"][0]["height"],5);
    let peer=dir.path().join("outputs/.native-images-peer.zip");std::fs::write(&peer,b"peer").unwrap();let cancelled=dir.path().join("outputs/cancel.gif");let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let stop=cancel.clone();
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&cancelled,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::Gif,None,&[],|i,_|{if i>0{assert!(rx.try_recv().unwrap()>0.);stop.cancel();}let frame=surface(i);async move{Ok(frame)}}).await;assert!(result.is_err());assert!(!cancelled.exists());
    let token=tokio_util::sync::CancellationToken::new();let(tx,_)=tokio::sync::mpsc::unbounded_channel();let mut called=false;
    let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&token,&tx,None,RasterEncoding::Gif,None,&[],|_,_|{called=true;async{anyhow::bail!("must not render")}}).await;assert!(result.is_err());assert!(!called);assert_eq!(std::fs::read(&output).unwrap(),bytes);
    let timed=dir.path().join("outputs/timeout.gif");let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&timed,spec,std::time::Duration::from_millis(1),&token,&tx,None,RasterEncoding::Gif,None,&[],|_,_|async{tokio::time::sleep(std::time::Duration::from_millis(10)).await;Ok(GroupSurface{width:7,height:5,rgba:[0,0,0,255].repeat(35)})}).await;assert!(result.unwrap_err().to_string().contains("timeout"));assert!(!timed.exists());assert_eq!(std::fs::read(&peer).unwrap(),b"peer");
    assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|e|{let n=e.unwrap().file_name();let n=n.to_string_lossy();!n.starts_with(".native-images-")||n==".native-images-peer.zip"}));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn gif_jobs_resample_source_ranges_to_original_grid_and_preserve_access() {
    use image::AnimationDecoder;use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::model::JobStatus;
    let(state,dir)=support::make_state(false,false).await;add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("gif-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("gif-outsider","test password").await.unwrap().unwrap();let document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let app=support::router(state.clone());
    let mut first_frames=Vec::new();
    for(start,use_ticks,expected_frames,size)in [(0,false,3,None),(1,false,4,None),(1,true,4,None),(8,false,3,Some((57,43)))] {
        let mut payload=json!({"document":document,"sequenceId":"6","startFrames":start.to_string(),"frames":4,"preset":"gif","sources":{"1":"seq8"}});if let Some((w,h))=size{payload["frameSize"]=json!([w,h]);}
        if use_ticks{payload.as_object_mut().unwrap().remove("startFrames");payload["startTicks"]=json!((start as i64*FD).to_string());}
        payload["withAudio"]=json!(true);let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),400);
        payload["withAudio"]=json!(false);let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),202);let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(accepted["container"],"gif");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["videoCodec"],"gif");assert_eq!(result["fpsRate"]["numerator"],15);assert_eq!(result["duration"],json!(expected_frames as f64*0.06));
        let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();let frames=image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes)).unwrap().into_frames().collect_frames().unwrap();assert_eq!(frames.len(),expected_frames);let dims=size.unwrap_or((640,360));assert!(frames.iter().all(|f|f.buffer().dimensions()==dims&&f.delay().numer_denom_ms()==(60,1)));
        if start==0{first_frames=frames.iter().map(|f|f.buffer().clone()).collect();}else if start==1{for(i,frame)in first_frames.iter().enumerate(){assert_eq!(frames[i].buffer(),frame,"source range must align to output frame grid");}}
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn audio_only_pcm_files_keep_original_quantization_headers_and_owned_cleanup() {
    use video_kadr_backend::tools::native_pcm_export::{encode,PcmPreset};
    let(state,dir)=support::make_state(false,false).await;
    let samples=[-1.5f32,-1.,-0.5/32767.,0.5/32767.,-0.5/8388607.,0.5/8388607.,0.5,-0.5,1.,1.5,f32::NAN,f32::INFINITY];
    let raw=dir.path().join("audio.f32le");std::fs::write(&raw,samples.iter().flat_map(|s|s.to_le_bytes()).collect::<Vec<_>>()).unwrap();
    for(preset,ext,codec,bits)in [(PcmPreset::Wav16,"wav","pcm_s16le",16),(PcmPreset::Wav24,"wav","pcm_s24le",24),(PcmPreset::Aiff16,"aiff","pcm_s16be",16)] {
        let output=dir.path().join(format!("outputs/pcm-{codec}.{ext}"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();
        encode(&raw,6,preset,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap();
        let values:Vec<i32>=if bits==24{vec![-8388607,-8388607,-128,128,-1,1,4194304,-4194304,8388607,8388607,0,8388607]}else{vec![-32767,-32767,-1,1,0,0,16384,-16384,32767,32767,0,32767]};
        let payload=values.iter().flat_map(|q|if ext=="aiff"{q.to_be_bytes()[2..].to_vec()}else{q.to_le_bytes()[..bits/8].to_vec()}).collect::<Vec<_>>();
        let mut expected=Vec::new();if ext=="wav" {
            expected.extend_from_slice(b"RIFF");expected.extend_from_slice(&(36u32+payload.len()as u32).to_le_bytes());expected.extend_from_slice(b"WAVEfmt ");expected.extend_from_slice(&16u32.to_le_bytes());expected.extend_from_slice(&1u16.to_le_bytes());expected.extend_from_slice(&2u16.to_le_bytes());expected.extend_from_slice(&48000u32.to_le_bytes());expected.extend_from_slice(&(48000u32*2*bits as u32/8).to_le_bytes());expected.extend_from_slice(&(2u16*bits as u16/8).to_le_bytes());expected.extend_from_slice(&(bits as u16).to_le_bytes());expected.extend_from_slice(b"data");expected.extend_from_slice(&(payload.len()as u32).to_le_bytes());
        }else{
            expected.extend_from_slice(b"FORM");expected.extend_from_slice(&(46u32+payload.len()as u32).to_be_bytes());expected.extend_from_slice(b"AIFFCOMM");expected.extend_from_slice(&18u32.to_be_bytes());expected.extend_from_slice(&2u16.to_be_bytes());expected.extend_from_slice(&6u32.to_be_bytes());expected.extend_from_slice(&16u16.to_be_bytes());expected.extend_from_slice(&[0x40,0x0e,0xbb,0x80,0,0,0,0,0,0]);expected.extend_from_slice(b"SSND");expected.extend_from_slice(&(8u32+payload.len()as u32).to_be_bytes());expected.extend_from_slice(&[0;8]);
        }expected.extend_from_slice(&payload);assert_eq!(std::fs::read(&output).unwrap(),expected);
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let stream=&probe["streams"][0];assert_eq!(stream["codec_name"],codec);assert_eq!(stream["sample_rate"],"48000");assert_eq!(stream["channels"],2);assert_eq!(stream["duration_ts"],6);
        assert!(encode(&raw,6,preset,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.is_err());assert_eq!(std::fs::read(&output).unwrap(),expected);
        let invalid=dir.path().join("outputs/invalid.wav");assert!(encode(&raw,u64::from(u32::MAX),preset,&invalid,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.is_err());assert!(!invalid.exists());
    }
    let frames=2_000_000;let large=dir.path().join("large.f32le");std::fs::write(&large,[0u8;8].repeat(frames as usize)).unwrap();let output=dir.path().join("outputs/cancel.wav");let peer=dir.path().join("outputs/.native-pcm-peer");std::fs::write(&peer,b"peer").unwrap();
    let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();let stop=token.clone();let observer=tokio::spawn(async move{assert!(rx.recv().await.unwrap()>0.);stop.cancel();});
    let result=encode(&large,frames,PcmPreset::Wav24,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await;observer.await.unwrap();assert!(result.is_err());assert!(!output.exists());
    tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|e|{let n=e.unwrap().file_name();let n=n.to_string_lossy();!n.starts_with(".native-pcm-")||n==".native-pcm-peer"}));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn audio_only_jobs_match_source_graph_at_floor_sample_boundaries_without_video_render() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::{model::JobStatus,domain::{filmcraft_pcm_export::{export_pcm_with_reader,PcmFormat},filmcraft_project_audio::SourceProjectAudioRuntime}};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("audio-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("audio-outsider","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*4);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(-6.);track["pan"]=json!(0.25);sequence["audio_tracks"]=json!([track]);sequence["video_tracks"]=json!([]);sequence["settings"]["width"]=json!(9000);sequence["settings"]["height"]=json!(6000);sequence["settings"]["sample_rate"]=json!(44100);sequence["settings"]["frame_rate"]=json!({"num":30000,"den":1001});
    let app=support::router(state.clone());
    for(preset,ext,bits,codec,start,format)in [("wav16","wav",16,"pcm_s16le",1,PcmFormat::Wav),("wav24","wav",24,"pcm_s24le",0,PcmFormat::Wav),("aiff16","aiff",16,"pcm_s16be",1,PcmFormat::Aiff)]{
        // Audio presets always export sound, even with the legacy optional flag omitted.
        let payload=json!({"document":document,"sequenceId":"6","startFrames":start.to_string(),"frames":1,"preset":preset,"audio":{"sampleRate":48000},"sources":{},"frameSize":[0,0]});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");assert_eq!(accepted["container"],ext);assert_eq!(accepted["mode"],"audio_only");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["mediaType"],"audio");assert!(result["videoCodec"].is_null());assert!(result["width"].is_null());assert!(result["fpsRate"].is_null());assert_eq!(result["audioCodec"],codec);assert_eq!(result["sampleRate"],48000);assert_eq!(result["bitsPerSample"],bits);
        let first=start*48000i64*1001/30000;let end=(start+1)*48000i64*1001/30000;let frames=(end-first)as usize;assert_eq!(frames,if start==0{1601}else{1602});
        let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(48000);let mut runtime=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();let expected=export_pcm_with_reader(&mut runtime,6,first,frames,2,bits,format,|_,_,_,_|Ok(None)).unwrap();let output=state.storage.join("outputs").join(result["filename"].as_str().unwrap());assert_eq!(std::fs::read(&output).unwrap(),expected,"source graph {preset}");assert_eq!(result["duration"],json!(frames as f64/48000.));
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"].as_array().unwrap().len(),1);assert_eq!(probe["streams"][0]["codec_name"],codec);assert_eq!(probe["streams"][0]["duration_ts"],frames);
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn fractional_video_audio_keeps_source_floor_ranges_across_pcm_containers() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::{model::JobStatus,domain::{filmcraft_pcm_export::{export_pcm_with_reader,PcmFormat},filmcraft_project_audio::SourceProjectAudioRuntime}};
    let(mut state,dir)=support::make_state(true,false).await;let tools=std::sync::Arc::make_mut(&mut state.tools);tools.ffmpeg_encoders.extend(["ffv1".into(),"pcm_f32le".into()]);tools.ffmpeg_muxers.push("matroska".into());add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;
    let owner=state.db.register_auth_user("fractional-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*8);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);sequence["settings"]["sample_rate"]=json!(44100);sequence["settings"]["frame_rate"]=json!({"num":30000,"den":1001});
    let app=support::router(state.clone());
    for start in 0..3i64 {
        let first=start*48000*1001/30000;let end=(start+1)*48000*1001/30000;let count=(end-first)as usize;assert_eq!(count,if start==1{1602}else{1601});
        let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(48000);
        let source_document=SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap();let mut runtime=SourceProjectAudioRuntime::new(source_document.clone()).unwrap();let wav=export_pcm_with_reader(&mut runtime,6,first,count,2,24,PcmFormat::Wav,|_,_,_,_|Ok(None)).unwrap();let expected=&wav[44..];
        let mut runtime=SourceProjectAudioRuntime::new(source_document).unwrap();let bus=runtime.render_media(6,first,count,|_,_,_,_|Ok(None)).unwrap();let mut expected_float=Vec::new();for i in 0..count{for channel in &bus{expected_float.extend_from_slice(&channel[i].to_le_bytes());}}
        for preset in ["lossless","prores_hq","dnxhr_hq","apv42210","mxf_dnxhr_hq","mxf_h264","mxf_atom_dnxhr_hq"] {
            let payload=json!({"document":document,"sequenceId":"6","startFrames":start.to_string(),"frames":1,"preset":preset,"withAudio":true,"sources":{"1":"seq8"},"frameSize":[64,48]});
            let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");
            let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{start}: {job:?}");let result=job.result.unwrap();let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let bytes=std::fs::read(&path).unwrap();
            match result["container"].as_str().unwrap() {
                "mov"=>{let movie=filmcraft_isobmff::open(&bytes).unwrap();let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();assert_eq!(movie.tracks[audio].duration,count as u64);let mut pcm=Vec::new();for i in 0..movie.tracks[audio].samples.len(){pcm.extend(movie.read_sample(&bytes,audio,i).unwrap());}assert_eq!(pcm,expected,"MOV PCM {preset}/{start}");},
                "mxf"=>{let movie=filmcraft_mxf::open(&bytes).unwrap();let audio=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Sound).unwrap();let pcm=audio.chunks.iter().flat_map(|c|bytes[c.offset as usize..(c.offset+c.size)as usize].iter().copied()).collect::<Vec<_>>();
                    // Original OP1a frame wrapping writes floor(48000/rate) samples per edit unit;
                    // an excess source endpoint sample is not a second content package.
                    assert_eq!(pcm,&expected[..1601*6],"OP1a PCM {preset}/{start}");assert_eq!(audio.duration,Some(1));},
                "zip"=>{use std::io::Read;let mut zip=zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();assert_eq!(zip.len(),3);for channel in 0..2{let mut mono=Vec::new();zip.by_name(&format!("export_A{}.mxf",channel+1)).unwrap().read_to_end(&mut mono).unwrap();let movie=filmcraft_mxf::open(&mono).unwrap();let audio=&movie.tracks[0];let pcm=audio.chunks.iter().flat_map(|c|mono[c.offset as usize..(c.offset+c.size)as usize].iter().copied()).collect::<Vec<_>>();let wanted=expected.chunks_exact(6).flat_map(|p|p[channel*3..channel*3+3].iter().copied()).collect::<Vec<_>>();assert_eq!(pcm,wanted,"OP-Atom PCM {channel}/{start}");assert_eq!(audio.duration,Some(if count==1602{2}else{1}));}},
                "mkv"=>{let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&path).args(["-map","0:a:0","-c:a","copy","-f","f32le","pipe:1"]).output().await.unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout,expected_float,"float PCM start {start}");},
                other=>panic!("unexpected {other}"),
            }
            assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
        }
    }
    assert_eq!(document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"],44100);
    // Valid endpoint rounding never permits an arbitrary short/long PCM stream.
    for frames in [1600u64,1603] {
        use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterAudio,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
        let raw=dir.path().join(format!("invalid-{frames}.f32le"));let data=vec![0;frames as usize*8];std::fs::write(&raw,&data).unwrap();let output=dir.path().join(format!("outputs/invalid-{frames}.mov"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();let mut called=false;
        let spec=RasterVideoSpec{width:64,height:48,frames:1,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},staging_bytes:1024*1024};
        let result=encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&token,&tx,Some(RasterAudio{path:&raw,frames}),RasterEncoding::DnxHr{profile:2},None,&[],|_,_|{called=true;async{anyhow::bail!("must not render")}}).await;
        assert!(result.unwrap_err().to_string().contains("audio duration does not match"));assert!(!called);assert!(!output.exists());assert_eq!(std::fs::read(&raw).unwrap(),data);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

// Independent single-pull reference from pinned export/audio_out.rs: measure
// in one-second blocks, gain, process look-ahead zeros, discard initial latency.
fn original_loudness_reference(bus:Vec<Vec<f32>>,target:f64,peak:f64)->(Vec<Vec<f32>>,Option<f64>,f64) {
    original_loudness_reference_at_rate(bus,48000,target,peak)
}
fn original_loudness_reference_at_rate(mut bus:Vec<Vec<f32>>,rate:u32,target:f64,peak:f64)->(Vec<Vec<f32>>,Option<f64>,f64) {
    use filmcraft_audio_dsp::{AudioEffect,LoudnessMeter,effects::Limiter};
    let frames=bus[0].len();let mut meter=LoudnessMeter::new(f64::from(rate),bus.len());
    for start in (0..frames).step_by(rate as usize){let end=(start+rate as usize).min(frames);meter.process(&bus.iter().map(|c|&c[start..end]).collect::<Vec<_>>());}
    let measured=meter.integrated();let gain_db=filmcraft_audio_dsp::normalize_gain_db(measured,target);let gain=10f64.powf(gain_db/20.)as f32;
    let mut limiter=Limiter::new(rate as f32,bus.len());limiter.set_param("ceiling",peak.clamp(-24.,0.)as f32);limiter.set_param("true_peak",1.);limiter.set_param("release",80.);limiter.reset();let latency=limiter.latency();
    for channel in &mut bus{channel.resize(frames+latency,0.);if gain!=1.{for sample in channel{*sample*=gain;}}}
    {let mut refs=bus.iter_mut().map(Vec::as_mut_slice).collect::<Vec<_>>();limiter.process(&mut refs);}
    for channel in &mut bus{channel.drain(..latency);channel.resize(frames,0.);}
    (bus,measured.is_finite().then_some(measured),gain_db)
}

#[tokio::test]
async fn loudness_export_resolved_rates_match_original_and_reject_invalid_rates() {
    use video_kadr_backend::tools::native_loudness_export::{normalize_with_rate,LoudnessSettings};
    let(state,dir)=support::make_state(false,false).await;
    let settings=LoudnessSettings{enabled:true,target_lufs:-18.,true_peak_dbtp:-2.};
    let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();
    for rate in [8000u32,44100,48000,96000,192000] {
        // A non-integral second and peaks across streaming boundaries exercise
        // rate-dependent K weighting, look-ahead, release and final zero flush.
        let frames=rate as usize*2+13;let mut bus=vec![vec![0f32;frames];2];
        for i in 0..frames {bus[0][i]=((i as f64*997.*std::f64::consts::TAU/f64::from(rate)).sin()*0.03)as f32;bus[1][i]=((i as f64*440.*std::f64::consts::TAU/f64::from(rate)).sin()*0.02)as f32;}
        for i in [0,4095,4096,rate as usize,frames-1] {bus[0][i]=1.5;bus[1][i]=-1.1;}
        let bytes=(0..frames).flat_map(|i|bus.iter().flat_map(move |c|c[i].to_le_bytes())).collect::<Vec<_>>();
        let raw=dir.path().join(format!("rate-{rate}.f32le"));std::fs::write(&raw,&bytes).unwrap();let output=dir.path().join(format!("outputs/rate-{rate}.f32le"));
        let(expected,measured,gain)=original_loudness_reference_at_rate(bus,rate,-18.,-2.);
        let expected_bytes=(0..frames).flat_map(|i|expected.iter().flat_map(move |c|c[i].to_le_bytes())).collect::<Vec<_>>();
        let report=normalize_with_rate(&raw,frames as u64,rate,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap();
        assert_eq!(report.measured_lufs,measured,"meter {rate}");assert_eq!(report.gain_db,gain,"gain {rate}");
        assert!(std::fs::read(&output).unwrap()==expected_bytes,"original compensated PCM mismatch at {rate}");assert_eq!(std::fs::read(&raw).unwrap(),bytes);
        assert_eq!(std::fs::metadata(&output).unwrap().len(),frames as u64*8);
    }
    // Admission fails before opening input or creating staging/output.
    let missing=dir.path().join("missing.f32le");let output=dir.path().join("outputs/invalid-rate.f32le");
    for rate in [0,7999,192001,u32::MAX] {
        let error=normalize_with_rate(&missing,1,rate,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap_err();
        assert!(error.to_string().contains("unsupported loudness sample rate"));assert!(!output.exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn loudness_export_matches_original_meter_compensated_limiter_and_silence() {
    use video_kadr_backend::tools::native_loudness_export::{normalize,LoudnessSettings};
    let(state,dir)=support::make_state(false,false).await;
    for(case,frames)in [(0,96013usize),(1,128),(2,48000)]{
        let mut bus=vec![vec![0f32;frames];2];if case!=2{for i in 0..frames{bus[0][i]=((i as f64*440.*std::f64::consts::TAU/48000.).sin()*0.03)as f32;bus[1][i]=((i as f64*997.*std::f64::consts::TAU/48000.).sin()*0.02)as f32;}for i in [0,4095,4096,48001,frames-1]{if i<frames{bus[0][i]=1.5;bus[1][i]=-1.1;}}}
        let mut bytes=Vec::new();for i in 0..frames{for channel in &bus{bytes.extend_from_slice(&channel[i].to_le_bytes());}}let raw=dir.path().join(format!("loudness-{case}.f32le"));std::fs::write(&raw,&bytes).unwrap();let output=dir.path().join(format!("outputs/normalized-{case}.f32le"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();let settings=LoudnessSettings{enabled:true,target_lufs:-18.,true_peak_dbtp:-2.};
        let(expected,measured,gain)=original_loudness_reference(bus,-18.,-2.);let mut expected_bytes=Vec::new();for i in 0..frames{for channel in &expected{expected_bytes.extend_from_slice(&channel[i].to_le_bytes());}}
        let report=normalize(&raw,frames as u64,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap();assert_eq!(report.measured_lufs,measured);assert_eq!(report.gain_db,gain);assert_eq!(std::fs::read(&output).unwrap(),expected_bytes,"original compensated output {case}");assert_eq!(std::fs::read(&raw).unwrap(),bytes);
        let mut meter=filmcraft_audio_dsp::LoudnessMeter::new(48000.,2);meter.process(&[&expected[0],&expected[1]]);assert!(meter.true_peak_dbtp()<=-1.8,"true peak {}",meter.true_peak_dbtp());
        if case==2{assert!(report.measured_lufs.is_none());assert_eq!(report.gain_db,0.);assert!(expected_bytes.iter().all(|v|*v==0));}
        assert!(normalize(&raw,frames as u64,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.is_err());assert_eq!(std::fs::read(&output).unwrap(),expected_bytes);
        let invalid=dir.path().join("outputs/invalid-normalization.f32le");assert!(normalize(&raw,frames as u64,LoudnessSettings{target_lufs:f64::NAN,..settings},&invalid,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.is_err());assert!(!invalid.exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn loudness_export_cancellation_releases_cpu_and_preserves_owned_files_in_both_passes() {
    use video_kadr_backend::tools::native_loudness_export::{normalize,LoudnessSettings};
    let(state,dir)=support::make_state(false,false).await;let frames=2_000_000;let raw=dir.path().join("loudness-long.f32le");std::fs::write(&raw,[0u8;8].repeat(frames as usize)).unwrap();let peer=dir.path().join("outputs/.native-loudness-peer");std::fs::write(&peer,b"peer").unwrap();
    for processing_pass in [false,true]{let output=dir.path().join(format!("outputs/cancel-loudness-{processing_pass}.f32le"));let(tx,mut rx)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();let stop=token.clone();let observer=tokio::spawn(async move{loop{let value=rx.recv().await.unwrap();if !processing_pass||value>60.{stop.cancel();break}}});
        let result=normalize(&raw,frames,LoudnessSettings{enabled:true,..Default::default()},&output,std::time::Duration::from_secs(60),&token,&tx,state.cpu_pool.clone()).await;observer.await.unwrap();assert!(result.is_err());assert!(!output.exists());tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::metadata(&raw).unwrap().len(),frames*8);assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|e|{let n=e.unwrap().file_name();let n=n.to_string_lossy();!n.starts_with(".native-loudness-")||n==".native-loudness-peer"}));
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn loudness_export_jobs_match_original_normalized_pcm_and_preserve_disabled_output() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::filmcraft_project_audio::SourceProjectAudioRuntime};
    let(state,dir)=support::make_state(false,false).await;add(&state,dir.path(),8,0.,0.).await;add(&state,dir.path(),9,3.,-2.).await;let owner=state.db.register_auth_user("loudness-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);sequence["settings"]["sample_rate"]=json!(44100);
    let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(48000);let mut runtime=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();let raw=runtime.render_media(6,0,96000,|_,_,_,_|Ok(None)).unwrap();let(normalized,measured,gain)=original_loudness_reference(raw.clone(),-23.,-2.);assert!(measured.is_some());let app=support::router(state.clone());
    for enabled in [false,true]{for preset in ["wav24","prores_hq"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":48,"preset":preset,"audio":{"sampleRate":48000},"withAudio":true,"sources":{"1":"seq8"},"frameSize":[64,48],"loudness":{"enabled":enabled,"targetLufs":-23.,"truePeakDbtp":-2.}});let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();if enabled{assert!((result["loudness"]["measuredLufs"].as_f64().unwrap()-measured.unwrap()).abs()<1e-9);assert!((result["loudness"]["gainDb"].as_f64().unwrap()-gain).abs()<1e-9);}else{assert!(result["loudness"].is_null());}
        let bus=if enabled{&normalized}else{&raw};let mut expected=Vec::new();for i in 0..96000{for channel in bus{expected.extend_from_slice(&((channel[i].clamp(-1.,1.)*8388607.).round()as i32).to_le_bytes()[..3]);}}
        let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();let pcm=if preset=="wav24"{bytes[44..].to_vec()}else{let movie=filmcraft_isobmff::open(&bytes).unwrap();let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();assert_eq!(movie.tracks[audio].duration,96000);let mut pcm=Vec::new();for i in 0..movie.tracks[audio].samples.len(){pcm.extend(movie.read_sample(&bytes,audio,i).unwrap());}pcm};assert_eq!(pcm,expected,"normalized PCM {preset}/{enabled}");assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }}
    assert_eq!(document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"],44100);
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn export_text_jobs_paint_name_and_original_sequence_drop_frame_clock() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use std::io::Read;
    use video_kadr_backend::{model::JobStatus,domain::{filmcraft_export_text::{paint,TextOverlay},filmcraft_source_pixels::encode_srgb,transition_kernel_pack::{context::RenderContext,image::Image}}};
    let(state,_dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("export-label-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["start_timecode"]=json!(107892);
    sequence["settings"]["width"]=json!(320);sequence["settings"]["height"]=json!(180);
    sequence["settings"]["frame_rate"]=json!({"num":30000,"den":1001});sequence["settings"]["drop_frame"]=json!(true);
    let name=json!({"enabled":true,"text":"  Кадр  ","placement":"topLeft","opacity":50,"sizePercent":12});
    let clock=json!({"enabled":true,"text":" TC ","placement":"bottomRight","sizePercent":10});
    let app=support::router(state.clone());
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"1799","frames":2,"preset":"png_sequence","nameOverlay":name,"timecodeOverlay":clock});
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
    let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
    let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");
    let result=job.result.unwrap();let mut zip=zip::ZipArchive::new(std::fs::File::open(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap()).unwrap();assert_eq!(zip.len(),2);
    let context=RenderContext::new_with_budget(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),1,320*180,32*1024*1024).unwrap();
    let name:TextOverlay=serde_json::from_value(name).unwrap();let clock:TextOverlay=serde_json::from_value(clock).unwrap();
    for(i,tc)in ["TC 01;00;59;29","TC 01;01;00;02"].iter().enumerate(){
        let mut expected=Image::new(320,180);paint(&mut expected.px,320,180,&name,"Кадр",&mut ||Ok(())).unwrap();paint(&mut expected.px,320,180,&clock,tc,&mut ||Ok(())).unwrap();
        let encoded=encode_srgb(&context,&expected).unwrap();let expected=encoded.surface();let mut bytes=Vec::new();zip.by_index(i).unwrap().read_to_end(&mut bytes).unwrap();let actual=image::load_from_memory(&bytes).unwrap().into_rgba8();assert!(actual.as_raw()==&expected.rgba,"export frame {i}: first differing byte {:?}",actual.as_raw().iter().zip(&expected.rgba).position(|(a,b)|a!=b));assert!(actual.pixels().any(|p|p.0[3]>0));
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn export_image_overlay_matches_original_linear_resize_anchor_and_group_opacity() {
    use video_kadr_backend::domain::{filmcraft_export_image::{prepare,composite,ImageOverlay},filmcraft_export_text::Placement,transition_kernel_pack::{context::RenderContext,image::Image}};
    let context=RenderContext::new_with_budget(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),1,3840*2160,32*1024*1024).unwrap();
    let source=GroupSurface{width:2,height:1,rgba:vec![255,0,0,255,0,0,255,0]};
    for placement in [Placement::TopLeft,Placement::TopCenter,Placement::TopRight,Placement::CenterLeft,Placement::Center,Placement::CenterRight,Placement::BottomLeft,Placement::BottomCenter,Placement::BottomRight] {
        let settings=ImageOverlay{enabled:true,source_id:"logo".into(),size_percent:20.,placement,offset:(-1.25,0.5),opacity:50.,..Default::default()};
        let resized=prepare(&context,&source,&settings,20).unwrap();assert_eq!((resized.w,resized.h),(4,2));
        for row in resized.px.chunks_exact(16){for(p,a)in row.chunks_exact(4).zip([1.,0.75,0.25,0.]){assert_eq!(p,[a,0.,0.,a]);}}
        let mut output=Image::filled(20,12,[0.1,0.2,0.3,0.4]);composite(&context,&mut output,&resized,&settings).unwrap();
        let(ax,ay)=placement.anchor();let margin=0.03*12.;let ox=(margin+ax*(20.-2.*margin-4.)-1.25).round()as i32;let oy=(margin+ay*(12.-2.*margin-2.)+0.5).round()as i32;
        for y in 0..12{for x in 0..20{let(sx,sy)=(x as i32-ox,y as i32-oy);let a=if (0..4).contains(&sx)&&(0..2).contains(&sy){[1.,0.75,0.25,0.][sx as usize]}else{0.};let mut expected=[0.1,0.2,0.3,0.4];if a>0.{for c in 0..4{expected[c]=if c==0||c==3{a*0.5}else{0.}+expected[c]*(1.-a*0.5);}}assert_eq!(output.get(x,y),expected);}}
    }
    let mut invalid=ImageOverlay{enabled:true,..Default::default()};assert!(invalid.validate().is_err());invalid.source_id="logo".into();invalid.opacity=f32::NAN;assert!(invalid.validate().is_err());
}

#[tokio::test]
async fn export_image_overlay_jobs_keep_asset_acl_and_source_pixels() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use std::io::Read;
    use video_kadr_backend::{model::JobStatus,library::MediaEntry,domain::{filmcraft_export_image::{prepare,composite,ImageOverlay},filmcraft_source_pixels::encode_srgb,transition_kernel_pack::{context::RenderContext,image::Image}}};
    let(state,dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("image-overlay-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("image-overlay-outsider","test password").await.unwrap().unwrap();
    let logo=GroupSurface{width:2,height:1,rgba:vec![255,0,0,128,0,255,0,255]};image::save_buffer(dir.path().join("sources/logo.png"),&logo.rgba,2,1,image::ColorType::Rgba8).unwrap();state.library.add(MediaEntry::from_result("source",&json!({"id":"logo","filename":"logo.png","mediaType":"image"}))).await;
    let space=state.db.create_space(&owner.user.username,"private logo").await.unwrap();state.db.create_space_composition_project("Logo claim",&json!({"schemaVersion":1,"sources":{"logo":{"id":"logo"}}}),&["logo".into()],&owner.user.username,&space.id).await.unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["settings"]["width"]=json!(64);sequence["settings"]["height"]=json!(48);
    let overlay=json!({"enabled":true,"sourceId":"logo","sizePercent":25,"placement":"center","offset":[-3.5,2.25],"opacity":50});let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":1,"preset":"png_sequence","imageOverlay":overlay});let app=support::router(state.clone());
    for(token,allowed)in [(&outsider.token,false),(&owner.token,true)]{
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();if !allowed{assert!(!status.is_success());continue}assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();
        let mut zip=zip::ZipArchive::new(std::fs::File::open(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap()).unwrap();let mut bytes=Vec::new();zip.by_index(0).unwrap().read_to_end(&mut bytes).unwrap();let actual=image::load_from_memory(&bytes).unwrap().into_rgba8();
        let context=RenderContext::new_with_budget(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),1,3840*2160,32*1024*1024).unwrap();let settings:ImageOverlay=serde_json::from_value(overlay.clone()).unwrap();let prepared=prepare(&context,&logo,&settings,64).unwrap();let mut expected=Image::new(64,48);composite(&context,&mut expected,&prepared,&settings).unwrap();let encoded=encode_srgb(&context,&expected).unwrap();assert!(actual.as_raw()==&encoded.surface().rgba);
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn custom_export_clock_matches_original_grid_without_fractional_tick_drift() {
    use video_kadr_backend::domain::{composition::FrameRateRatio,filmcraft_export_clock::{span,sample,gif_delay_ms}};
    for (n,d) in [(15,1),(25,1),(30000,1001),(17,3),(60,1)] {
        let rate=filmcraft_time::FrameRate::new(n,d);let ratio=FrameRateRatio{numerator:n as u32,denominator:d as u32};
        for start_frame in [-19,0,1,1799] {for shift in [0,1,1234567] {
            let start=filmcraft_time::FrameRate::FPS_24.tick_of(start_frame).0+shift;
            let end=filmcraft_time::FrameRate::FPS_24.tick_of(start_frame+7).0+shift;
            let f0=rate.frame_at(filmcraft_time::Tick(start));let f1=rate.frame_at(filmcraft_time::Tick(end-1))+1;
            let(count,aligned)=span(start,end,ratio).unwrap();assert_eq!(count,(f1-f0)as u64);assert_eq!(aligned,rate.tick_of(f0).0);
            for i in 0..=count {assert_eq!(sample(ratio,aligned,i).unwrap(),rate.tick_of(f0+i as i64).0,"{n}/{d} {start_frame} {shift} {i}");}
        }}
        assert_eq!(gif_delay_ms(ratio).unwrap(),(1000*d/n/10*10)as u32);
    }
    assert!(span(1,1,FrameRateRatio{numerator:25,denominator:1}).is_err());assert!(span(0,1,FrameRateRatio{numerator:0,denominator:1}).is_err());
    assert!(span(i64::MAX-1,i64::MAX,FrameRateRatio{numerator:17,denominator:3}).is_err());
}

#[tokio::test]
async fn custom_export_rate_jobs_resample_source_range_and_align_native_audio() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use std::io::Read;use image::AnimationDecoder;
    use video_kadr_backend::{model::JobStatus,domain::{filmcraft_project_audio::SourceProjectAudioRuntime,filmcraft_export_text::{paint,TextOverlay},filmcraft_source_pixels::encode_srgb,transition_kernel_pack::{context::RenderContext,image::Image}}};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("custom-rate-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-18.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["start_timecode"]=json!(86400);sequence["settings"]["drop_frame"]=json!(false);sequence["settings"]["sample_rate"]=json!(48000);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});
    let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*12);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(0.);sequence["audio_tracks"]=json!([track]);
    let mut runtime=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&document).unwrap()).unwrap()).unwrap();let bus=runtime.render_media(6,1920,7680,|_,_,_,_|Ok(None)).unwrap();let mut expected_pcm=Vec::new();for i in 0..7680{for channel in &bus{expected_pcm.extend_from_slice(&((channel[i].clamp(-1.,1.)*8388607.).round()as i32).to_le_bytes()[..3]);}}
    let clock=json!({"enabled":true,"text":"TC","sizePercent":10});let app=support::router(state.clone());
    for preset in ["png_sequence","gif","prores_hq","mxf_dnxhr_hq"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"1","frames":3,"preset":preset,"frameSize":[96,64],"frameRate":{"numerator":25,"denominator":1},"withAudio":matches!(preset,"prores_hq"|"mxf_dnxhr_hq"),"timecodeOverlay":clock});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["fpsRate"],json!({"numerator":25,"denominator":1}));assert!((result["duration"].as_f64().unwrap()-0.16).abs()<1e-9);
        let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();
        if preset=="png_sequence" {
            let mut zip=zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();assert_eq!(zip.len(),4);
            let context=RenderContext::new_with_budget(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),1,3840*2160,32*1024*1024).unwrap();let settings:TextOverlay=serde_json::from_value(clock.clone()).unwrap();
            for i in 0..4{let mut png=Vec::new();zip.by_index(i).unwrap().read_to_end(&mut png).unwrap();let actual=image::load_from_memory(&png).unwrap().into_rgba8();let mut expected=Image::new(96,64);paint(&mut expected.px,96,64,&settings,&format!("TC 01:00:00:{i:02}"),&mut ||Ok(())).unwrap();let encoded=encode_srgb(&context,&expected).unwrap();assert!(actual.as_raw()==&encoded.surface().rgba,"output-grid/source-timecode frame {i}");}
        }else if preset=="gif"{let frames=image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap().into_frames().collect_frames().unwrap();assert_eq!(frames.len(),4);assert!(frames.iter().all(|f|f.delay().numer_denom_ms()==(40,1)));}
        else if preset=="mxf_dnxhr_hq"{let movie=filmcraft_mxf::open(&bytes).unwrap();assert_eq!(movie.timecode.as_ref().unwrap().start,90001);let picture=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(picture.duration,Some(4));let audio=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Sound).unwrap();let pcm=audio.chunks.iter().flat_map(|c|bytes[c.offset as usize..(c.offset+c.size)as usize].iter().copied()).collect::<Vec<_>>();assert_eq!(pcm,expected_pcm);}
        else{let movie=filmcraft_isobmff::open(&bytes).unwrap();let video=movie.tracks.iter().find(|t|t.kind==filmcraft_isobmff::TrackKind::Video).unwrap();assert_eq!(video.samples.len(),4);let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();assert_eq!(movie.tracks[audio].duration,7680);let mut pcm=Vec::new();for i in 0..movie.tracks[audio].samples.len(){pcm.extend(movie.read_sample(&bytes,audio,i).unwrap());}assert_eq!(pcm,expected_pcm);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn keyframe_distance_native_mxf_matches_original_h264_packets() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::composition::FrameRateRatio};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:12,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    for keyint in [None,Some(0),Some(1),Some(3)] {
        let output=dir.path().join(format!("outputs/key-distance-{keyint:?}.mxf"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,None,RasterEncoding::MxfH264{start:0,drop_frame:false,keyint,profile:Default::default(),bitrate:None},None,&[],|_,_|async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}).await.unwrap();
        let effective=keyint.filter(|v|*v>0).unwrap_or(48);let mut config=filmcraft_h264enc::EncoderConfig::new(64,48,24,1);config.format=filmcraft_h264enc::PacketFormat::AnnexB;config.aud=true;config.keyint=effective;config.slices=1;config.rate=filmcraft_h264enc::RateControl::Vbr{target_kbps:100,max_kbps:150};
        let mut original=filmcraft_h264enc::Encoder::new(config).unwrap();let y=vec![16;64*48];let c=vec![128;64*48/4];let mut packets=Vec::new();
        for i in 0..12 {packets.extend(original.try_encode(&filmcraft_h264enc::YuvFrame{y:&y,u:&c,v:&c,y_stride:64,uv_stride:32},i).unwrap());}packets.extend(original.flush());
        let keys=packets.iter().filter(|p|p.keyframe).map(|p|p.pts).collect::<Vec<_>>();assert_eq!(keys,(0..12).filter(|i|i%effective as i64==0).collect::<Vec<_>>());
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let video=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(movie.tracks[video].samples.len(),12);
        for(i,packet)in packets.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,video,i).unwrap(),packet.data,"keyint {keyint:?}, packet {i}");}
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn keyframe_distance_jobs_apply_to_single_pass_two_pass_and_native_h264() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(true,false).await;let owner=state.db.register_auth_user("key-distance-owner","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"]=json!([]);document["project"]["items"]["6"]["kind"]["Sequence"]["audio_tracks"]=json!([]);let app=support::router(state.clone());
    for preset in ["adaptive_high","vimeo1080","mxf_h264"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":12,"frameSize":[64,48],"preset":preset,"keyframeDistance":3});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();
        let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_frames","-of","json"]).arg(path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let frames=probe["frames"].as_array().unwrap();assert_eq!(frames.len(),12);let keys=frames.iter().enumerate().filter(|(_,f)|f["key_frame"]==1).map(|(i,_)|i).collect::<Vec<_>>();assert_eq!(keys,vec![0,3,6,9],"{preset}");
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn h264_profiles_native_mxf_match_original_packets_and_picture_labels() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::H264Profile}};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:8,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    for profile in [H264Profile::Baseline,H264Profile::Main,H264Profile::High] {
        let output=dir.path().join(format!("outputs/profile-{profile:?}.mxf"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,None,RasterEncoding::MxfH264{start:0,drop_frame:false,keyint:None,profile,bitrate:None},None,&[],|_,_|async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}).await.unwrap();
        let mut config=filmcraft_h264enc::EncoderConfig::new(64,48,24,1);config.profile=match profile{H264Profile::Baseline=>filmcraft_h264enc::Profile::Baseline,H264Profile::Main=>filmcraft_h264enc::Profile::Main,H264Profile::High=>filmcraft_h264enc::Profile::High};config.format=filmcraft_h264enc::PacketFormat::AnnexB;config.aud=true;config.keyint=48;config.slices=1;config.rate=filmcraft_h264enc::RateControl::Vbr{target_kbps:100,max_kbps:150};
        let mut original=filmcraft_h264enc::Encoder::new(config).unwrap();let y=vec![16;64*48];let c=vec![128;64*48/4];let mut packets=Vec::new();for i in 0..8{packets.extend(original.try_encode(&filmcraft_h264enc::YuvFrame{y:&y,u:&c,v:&c,y_stride:64,uv_stride:32},i).unwrap());}packets.extend(original.flush());
        assert_eq!(packets.iter().any(|p|p.frame_type==filmcraft_h264enc::FrameType::B),profile!=H264Profile::Baseline);
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let video=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(movie.tracks[video].samples.len(),8);
        let label=movie.tracks[video].picture.as_ref().unwrap().picture_coding.unwrap().0;assert_eq!(label[14],match profile{H264Profile::Baseline=>0x10,H264Profile::Main=>0x20,H264Profile::High=>0x40});
        for(i,packet)in packets.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,video,i).unwrap(),packet.data,"{profile:?} packet {i}");}
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn h264_profiles_jobs_write_selected_profile_in_all_h264_paths() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(true,false).await;let owner=state.db.register_auth_user("h264-profile-owner","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"]=json!([]);document["project"]["items"]["6"]["kind"]["Sequence"]["audio_tracks"]=json!([]);let app=support::router(state.clone());
    for profile in ["baseline","main","high"]{for preset in ["adaptive_high","vimeo1080","mxf_h264"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":8,"frameSize":[64,48],"preset":preset,"h264Profile":profile});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}/{profile}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{profile}: {job:?}");let result=job.result.unwrap();
        let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_streams","-show_frames","-of","json"]).arg(path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let actual=probe["streams"][0]["profile"].as_str().unwrap();match profile{"baseline"=>assert!(actual.ends_with("Baseline")),"main"=>assert_eq!(actual,"Main"),_=>assert_eq!(actual,"High")};let frames=probe["frames"].as_array().unwrap();assert_eq!(frames.len(),8);if profile=="baseline"{assert!(!frames.iter().any(|f|f["pict_type"]=="B"));}
    }}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn bitrate_settings_resolve_original_fixed_adaptive_floor_and_maximum_rules() {
    use video_kadr_backend::domain::{composition::FrameRateRatio,filmcraft_export_h264::{BitrateMode,BitrateSettings}};
    let rate=FrameRateRatio{numerator:24,denominator:1};
    for(mode,target,max,bpp,wanted)in [(BitrateMode::Cbr,10000,Some(20000),None,(10000,10000)),(BitrateMode::Vbr1Pass,0,None,None,(100,150)),(BitrateMode::Vbr2Pass,10000,Some(9000),None,(10000,15000)),(BitrateMode::Vbr1Pass,10000,Some(18000),None,(10000,18000)),(BitrateMode::Vbr2Pass,123,Some(20000),Some(0.2),(9953,20000)),(BitrateMode::Cbr,123,None,Some(0.2),(9953,9953)),(BitrateMode::Vbr1Pass,321,None,Some(0.),(321,481))] {
        let resolved=BitrateSettings{mode,target_kbps:target,max_kbps:max,adaptive_bpp:bpp}.resolve(1920,1080,rate).unwrap();assert_eq!((resolved.target_kbps,resolved.max_kbps),wanted);assert_eq!(resolved.mode,mode);
    }
    assert!(BitrateSettings{adaptive_bpp:Some(f32::NAN),..Default::default()}.resolve(64,48,rate).is_err());assert!(BitrateSettings{target_kbps:u32::MAX,..Default::default()}.resolve(64,48,rate).is_err());
}

#[tokio::test]
async fn bitrate_native_mxf_modes_match_original_single_and_two_pass_packets() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::{BitrateMode,ResolvedBitrate}}};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:8,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};
    for mode in [BitrateMode::Cbr,BitrateMode::Vbr1Pass,BitrateMode::Vbr2Pass] {
        let output=dir.path().join(format!("outputs/bitrate-{mode:?}.mxf"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let mut seen=Vec::new();let resolved=ResolvedBitrate{mode,target_kbps:100,max_kbps:if mode==BitrateMode::Cbr{100}else{150}};
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,None,RasterEncoding::MxfH264{start:0,drop_frame:false,keyint:None,profile:Default::default(),bitrate:Some(resolved)},None,&[],|i,_|{seen.push(i);async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await.unwrap();
        assert_eq!(seen,(0..8).cycle().take(if mode==BitrateMode::Vbr2Pass{16}else{8}).collect::<Vec<_>>());
        let mut config=filmcraft_h264enc::EncoderConfig::new(64,48,24,1);config.format=filmcraft_h264enc::PacketFormat::AnnexB;config.aud=true;config.keyint=48;config.slices=1;config.rate=if mode==BitrateMode::Cbr{filmcraft_h264enc::RateControl::Cbr{kbps:100}}else{filmcraft_h264enc::RateControl::Vbr{target_kbps:100,max_kbps:150}};
        let y=vec![16;64*48];let c=vec![128;64*48/4];let input=filmcraft_h264enc::YuvFrame{y:&y,u:&c,v:&c,y_stride:64,uv_stride:32};
        if mode==BitrateMode::Vbr2Pass {config.pass=filmcraft_h264enc::Pass::First;let mut first=filmcraft_h264enc::Encoder::new(config.clone()).unwrap();for i in 0..8{drop(first.try_encode(&input,i).unwrap());}drop(first.flush());config.pass=filmcraft_h264enc::Pass::Second(first.pass_stats().unwrap());}
        let mut original=filmcraft_h264enc::Encoder::new(config).unwrap();let mut packets=Vec::new();for i in 0..8{packets.extend(original.try_encode(&input,i).unwrap());}packets.extend(original.flush());
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let video=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();assert_eq!(movie.tracks[video].samples.len(),8);for(i,packet)in packets.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,video,i).unwrap(),packet.data,"{mode:?} packet {i}");}
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn bitrate_native_two_pass_cancellation_preserves_peer_files_in_both_passes() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_cpu_pool,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::{BitrateMode,ResolvedBitrate}}};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:8,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};let peer=dir.path().join("outputs/.native-mxf-peer");std::fs::write(&peer,b"peer").unwrap();
    for stop_at in [2,9] {
        let output=dir.path().join(format!("outputs/cancel-pass-{stop_at}.mxf"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();let mut seen=0;
        let result=encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,RasterEncoding::MxfH264{start:0,drop_frame:false,keyint:None,profile:Default::default(),bitrate:Some(ResolvedBitrate{mode:BitrateMode::Vbr2Pass,target_kbps:100,max_kbps:150})},None,&[],Some(state.cpu_pool.clone()),|_,_|{seen+=1;if seen==stop_at{stop.cancel();}async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await;
        assert!(result.is_err());assert!(!output.exists());tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert_eq!(std::fs::read_dir(dir.path().join("outputs")).unwrap().count(),1);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn bitrate_jobs_exercise_vbr_overrides_native_cbr_and_native_two_pass_pcm() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(true,false).await;let owner=state.db.register_auth_user("h264-bitrate-owner","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"]=json!([]);document["project"]["items"]["6"]["kind"]["Sequence"]["audio_tracks"]=json!([]);let app=support::router(state.clone());
    for mode in ["cbr","vbr1Pass","vbr2Pass"]{for preset in ["adaptive_high","vimeo1080","mxf_h264"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":8,"frameSize":[64,48],"preset":preset,"withAudio":preset=="mxf_h264"&&mode=="vbr2Pass","bitrate":{"mode":mode,"targetKbps":100,"maxKbps":150}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}/{mode}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{mode}: {job:?}");let result=job.result.unwrap();let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-count_frames","-show_streams","-of","json"]).arg(&path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["nb_read_frames"],"8");
        if preset=="mxf_h264"&&mode=="vbr2Pass"{let bytes=std::fs::read(&path).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();let audio=movie.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Sound).unwrap();assert_eq!(audio.chunks.iter().map(|c|c.size).sum::<u64>(),16000*6);assert!(audio.chunks.iter().all(|c|bytes[c.offset as usize..(c.offset+c.size)as usize].iter().all(|v|*v==0)));}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_mp4_h264_matches_original_avcc_packets_ctts_edits_and_short_flushes() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_metadata,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::{BitrateMode,H264Profile}}};
    let(state,dir)=support::make_state(false,false).await;
    for (num,den) in [(24,1),(30000,1001),(17,3)]{for count in [1,2,8]{for profile in [H264Profile::Baseline,H264Profile::Main,H264Profile::High]{for mode in [BitrateMode::Cbr,BitrateMode::Vbr1Pass,BitrateMode::Vbr2Pass]{
        let spec=RasterVideoSpec{width:64,height:48,frames:count,frame_rate:FrameRateRatio{numerator:num,denominator:den},staging_bytes:1024*1024};let output=dir.path().join(format!("outputs/native-mp4-{num}-{den}-{count}-{profile:?}-{mode:?}.mp4"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let mut seen=Vec::new();
        encode_frames_with_metadata(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,None,RasterEncoding::NativeH264{target_kbps:100,max_kbps:150,keyint:48,profile,mode,multiplexer:Default::default(),audio:Default::default()},Some((2,1)),&[("title".into(),"Native H264".into())],|i,_|{seen.push(i);async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await.unwrap();
        assert_eq!(seen,(0..count).cycle().take((count*if mode==BitrateMode::Vbr2Pass{2}else{1})as usize).collect::<Vec<_>>());
        let mut config=filmcraft_h264enc::EncoderConfig::new(64,48,num,den);config.format=filmcraft_h264enc::PacketFormat::LengthPrefixed;config.aud=false;config.keyint=48;config.slices=1;config.sar=(2,1);config.profile=profile.native();config.rate=if mode==BitrateMode::Cbr{filmcraft_h264enc::RateControl::Cbr{kbps:100}}else{filmcraft_h264enc::RateControl::Vbr{target_kbps:100,max_kbps:150}};
        let y=vec![16;64*48];let c=vec![128;64*48/4];let input=filmcraft_h264enc::YuvFrame{y:&y,u:&c,v:&c,y_stride:64,uv_stride:32};
        if mode==BitrateMode::Vbr2Pass{config.pass=filmcraft_h264enc::Pass::First;let mut first=filmcraft_h264enc::Encoder::new(config.clone()).unwrap();for i in 0..count{drop(first.try_encode(&input,i as i64*i64::from(den)).unwrap());}drop(first.flush());config.pass=filmcraft_h264enc::Pass::Second(first.pass_stats().unwrap());}
        let mut original=filmcraft_h264enc::Encoder::new(config).unwrap();let avcc=filmcraft_isobmff::AvcConfig::parse(&original.avcc()).unwrap();let mut packets=Vec::new();for i in 0..count{packets.extend(original.try_encode(&input,i as i64*i64::from(den)).unwrap());}packets.extend(original.flush());
        let bytes=std::fs::read(&output).unwrap();assert_eq!(&bytes[8..12],b"isom");let movie=filmcraft_isobmff::open(&bytes).unwrap();assert_eq!(movie.tracks.len(),1);let track=&movie.tracks[0];assert_eq!(track.duration,count*u64::from(den));assert_eq!(track.timescale,num);assert_eq!(track.samples.len(),count as usize);assert_eq!(track.entries[0].codec,filmcraft_isobmff::CodecConfig::Avc(avcc));
        assert_eq!(track.edits.first().map(|e|e.media_time),packets.first().map(|p|p.pts-p.dts).filter(|v|*v>0));
        for(i,packet)in packets.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,0,i).unwrap(),packet.data,"{count}/{profile:?}/{mode:?}/{i}");let sample=&track.samples[i];assert_eq!(sample.duration,den);assert_eq!(sample.pts-sample.dts,packet.pts-packet.dts);assert_eq!(sample.is_sync,packet.keyframe);}
    }}}}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_mp4_h264_cancel_preserves_peer_and_existing_outputs_in_both_passes() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_cpu_pool,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::BitrateMode}};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:8,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};let peer=dir.path().join("outputs/.native-h264-mp4-peer");std::fs::write(&peer,b"peer").unwrap();
    for stop_at in [2,9]{let output=dir.path().join(format!("outputs/cancel-native-mp4-{stop_at}.mp4"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();let mut seen=0;
        let encoding=RasterEncoding::NativeH264{target_kbps:100,max_kbps:150,keyint:48,profile:Default::default(),mode:BitrateMode::Vbr2Pass,multiplexer:Default::default(),audio:Default::default()};
        let result=encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&cancel,&tx,None,encoding,None,&[],Some(state.cpu_pool.clone()),|_,_|{seen+=1;if seen==stop_at{stop.cancel();}async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await;assert!(result.is_err());assert!(!output.exists());
        tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert_eq!(std::fs::read_dir(dir.path().join("outputs")).unwrap().count(),1);
        std::fs::write(&output,b"existing").unwrap();let result=encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,None,encoding,None,&[],Some(state.cpu_pool.clone()),|_,_|async{anyhow::bail!("must refuse before rendering")}).await;assert!(result.is_err());assert_eq!(std::fs::read(&output).unwrap(),b"existing");std::fs::remove_file(output).unwrap();
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_mp4_h264_jobs_work_without_ffmpeg_and_accept_native_audio_cbr() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("native-mp4-owner","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"]=json!([]);document["project"]["items"]["6"]["kind"]["Sequence"]["audio_tracks"]=json!([]);let app=support::router(state.clone());
    for(preset,mode)in [("adaptive_high","vbr1Pass"),("vimeo1080","vbr2Pass"),("high_quality1080","cbr")] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":8,"frameSize":[64,48],"preset":preset,"keyframeDistance":4294967295u32,"bitrate":{"mode":mode,"targetKbps":100,"maxKbps":150}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let bytes=std::fs::read(&path).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();assert_eq!(movie.tracks.len(),1);assert_eq!(movie.tracks[0].samples.len(),8);
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(path).args(["-f","rawvideo","-pix_fmt","yuv420p","pipe:1"]).output().await.unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));assert_eq!(decoded.stdout.len(),8*64*48*3/2);
    }
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":8,"preset":"adaptive_high","withAudio":true,"bitrate":{"mode":"cbr","targetKbps":100}});let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
    let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();assert_eq!(movie.tracks.len(),2);assert!(movie.tracks.iter().any(|t|t.kind==filmcraft_isobmff::TrackKind::Audio));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_aac_mp4_packets_match_original_and_keep_exact_sample_edits() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_cpu_pool,RasterEncoding,RasterAudio,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::BitrateMode}};
    let(state,dir)=support::make_state(false,false).await;
    for(num,den,frames,count)in [(60,1,1,800),(30000,1001,1,1601),(30000,1001,1,1602),(24,1,7,14000),(25,1,16,30720)] {
        let raw=dir.path().join(format!("aac-{count}.f32le"));let mut left=Vec::new();let mut right=Vec::new();let mut bytes=Vec::new();for i in 0..count{let l=(i as f32*440.*std::f32::consts::TAU/48000.).sin()*0.2;let r=(i as f32*227.*std::f32::consts::TAU/48000.).sin()*0.15;left.push(l);right.push(r);bytes.extend_from_slice(&l.to_le_bytes());bytes.extend_from_slice(&r.to_le_bytes());}std::fs::write(&raw,&bytes).unwrap();
        let output=dir.path().join(format!("outputs/aac-{count}.mp4"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let spec=RasterVideoSpec{width:64,height:48,frames,frame_rate:FrameRateRatio{numerator:num,denominator:den},staging_bytes:16*1024*1024};
        encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(60),&tokio_util::sync::CancellationToken::new(),&tx,Some(RasterAudio{path:&raw,frames:count}),RasterEncoding::NativeH264{target_kbps:100,max_kbps:150,keyint:48,profile:Default::default(),mode:BitrateMode::Vbr1Pass,multiplexer:Default::default(),audio:Default::default()},None,&[],Some(state.cpu_pool.clone()),|_,_|async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}).await.unwrap();
        let mut original=filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(48000,2,320000)).unwrap();let asc=original.audio_specific_config();let priming=original.priming_samples();let mut wanted=original.encode(&[&left,&right]);wanted.extend(original.flush());
        let bytes=std::fs::read(&output).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();assert_eq!(movie.timescale,48000);let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();let track=&movie.tracks[audio];assert_eq!(track.timescale,48000);assert_eq!(track.duration,count+u64::from(priming));assert_eq!(track.edits[0].media_time,i64::from(priming));assert_eq!(track.edits[0].segment_duration,count);assert_eq!(track.samples.len(),wanted.len());assert_eq!(wanted.len(),count.div_ceil(1024)as usize+1);
        let filmcraft_isobmff::CodecConfig::Aac(config)=&track.entries[0].codec else{panic!("AAC esds missing")};assert_eq!(config.asc,asc);
        for(i,unit)in wanted.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,audio,i).unwrap().as_slice(),unit.as_slice(),"AAC AU {count}/{i}");assert!(track.samples[i].duration>0&&track.samples[i].duration<=1024);}
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(&output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let stream=&probe["streams"][0];assert_eq!(stream["codec_name"],"aac");assert_eq!(stream["profile"],"LC");assert_eq!(stream["sample_rate"],"48000");assert_eq!(stream["channels"],2);let duration=stream["duration"].as_str().unwrap().parse::<f64>().unwrap();assert!((duration-count as f64/48000.).abs()<1./48000.,"AAC {count} duration {duration}");
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&output).args(["-map","0:a:0","-f","f32le","pipe:1"]).output().await.unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));let got=decoded.stdout.len()/8;assert!((count as usize..=count as usize+1023).contains(&got),"AAC decoded frames {got} for {count}");let energy:f64=decoded.stdout.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())as f64).map(|v|v*v).sum();assert!(energy.is_finite()&&energy>0.01);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn native_aac_mp4_two_pass_cancellation_preserves_pcm_and_peer_files() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_cpu_pool,RasterEncoding,RasterAudio,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::BitrateMode}};
    let(state,dir)=support::make_state(false,false).await;let spec=RasterVideoSpec{width:64,height:48,frames:8,frame_rate:FrameRateRatio{numerator:24,denominator:1},staging_bytes:1024*1024};let raw=dir.path().join("cancel-aac.f32le");std::fs::write(&raw,[0u8;8].repeat(16000)).unwrap();let peer=dir.path().join("outputs/.native-aac-peer");std::fs::write(&peer,b"peer").unwrap();
    for stop_at in [2,11]{let output=dir.path().join(format!("outputs/cancel-aac-{stop_at}.mp4"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();let mut calls=0;
        let result=encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(60),&cancel,&tx,Some(RasterAudio{path:&raw,frames:16000}),RasterEncoding::NativeH264{target_kbps:100,max_kbps:150,keyint:48,profile:Default::default(),mode:BitrateMode::Vbr2Pass,multiplexer:Default::default(),audio:Default::default()},None,&[],Some(state.cpu_pool.clone()),|_,_|{calls+=1;if calls==stop_at{stop.cancel();}async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await;
        assert!(result.is_err());assert!(!output.exists());tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::metadata(&raw).unwrap().len(),16000*8);assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert_eq!(std::fs::read_dir(dir.path().join("outputs")).unwrap().count(),1);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[test]
fn h264_movie_audio_settings_match_original_codec_bit_depth_and_aac_caps() {
    use video_kadr_backend::domain::filmcraft_export_movie::{Multiplexer,AudioCodec,H264AudioSettings};
    for multiplexer in [Multiplexer::Mp4,Multiplexer::Mov]{for codec in [AudioCodec::Auto,AudioCodec::Aac,AudioCodec::Pcm]{
        let settings=H264AudioSettings{codec,..Default::default()};assert_eq!(settings.codec(multiplexer),if multiplexer==Multiplexer::Mp4||codec==AudioCodec::Auto{AudioCodec::Aac}else{codec});
    }}
    for(kbps,bps)in [(0,32000),(32,32000),(128,128000),(320,320000),(512,512000),(u32::MAX,512000)]{assert_eq!(H264AudioSettings{bitrate_kbps:kbps,..Default::default()}.aac_bps(),bps);}
    for(bits,wanted)in [(0,16),(16,16),(23,16),(24,24),(32,24)]{assert_eq!(H264AudioSettings{bits,..Default::default()}.pcm_bits(),wanted);}
    for rate in [0,7999,192001,u32::MAX] {assert_eq!(H264AudioSettings{sample_rate:Some(rate),..Default::default()}.sample_rate(),48000);}
    for sequence_rate in [8000,44100,44117,48000,96000,192000] {
        for sample_rate in [None,Some(0),Some(7999),Some(192001)] {assert_eq!(H264AudioSettings{sample_rate,..Default::default()}.resolve_sample_rate(sequence_rate),sequence_rate);}
        assert_eq!(H264AudioSettings{sample_rate:Some(48000),..Default::default()}.resolve_sample_rate(sequence_rate),48000);
    }
    for (channels,wanted) in [(0,1),(1,1),(2,2),(3,2),(5,2),(6,6),(8,6),(u32::MAX,6)]{assert_eq!(H264AudioSettings{channels,..Default::default()}.channels(),wanted);}
    assert!(H264AudioSettings{channels:6,..Default::default()}.validate_rate(Multiplexer::Mov).is_ok());
    assert_eq!(H264AudioSettings{channels:1,sample_rate:Some(8000),..Default::default()}.aac_bps(),48000);
    let pcm=H264AudioSettings{codec:AudioCodec::Pcm,sample_rate:Some(44117),..Default::default()};assert!(pcm.validate_rate(Multiplexer::Mov).is_ok());assert!(pcm.validate_rate(Multiplexer::Mp4).is_err());
    assert_eq!(H264AudioSettings{sample_rate:Some(8000),..Default::default()}.aac_bps(),96000);

}

#[tokio::test]
async fn h264_movie_native_aac_and_pcm_match_original_packets_and_quantization() {
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_cpu_pool,RasterEncoding,RasterAudio,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_h264::BitrateMode,filmcraft_export_movie::{Multiplexer,AudioCodec,H264AudioSettings}}};
    let(state,dir)=support::make_state(false,false).await;
    for channels in [1u32,2,6] {for rate in [8000u32,11025,12000,16000,22050,24000,32000,44100,44117,48000,64000,88200,96000,192000] {
    let count=u64::from(rate)*2/25;let raw=dir.path().join(format!("movie-codecs-{channels}-{rate}.f32le"));let mut left=Vec::new();let mut right=Vec::new();let mut data=Vec::new();for i in 0..count{let l=(i as f32*440.*std::f32::consts::TAU/rate as f32).sin()*0.2;let r=(i as f32*220.*std::f32::consts::TAU/rate as f32).sin()*0.15;left.push(l);right.push(r);data.extend_from_slice(&l.to_le_bytes());data.extend_from_slice(&r.to_le_bytes());}let planar=if channels==1 {vec![left.iter().zip(&right).map(|(l,r)|0.5*(l+r)).collect::<Vec<_>>()]}else if channels==2{vec![left.clone(),right.clone()]}else{(0..6).map(|c|(0..count).map(|i|(i as f32*(220.+c as f32*110.)*std::f32::consts::TAU/rate as f32).sin()*0.03*(c+1)as f32).collect()).collect()};
    data=(0..count as usize).flat_map(|i|planar.iter().flat_map(move |c|c[i].to_le_bytes())).collect();std::fs::write(&raw,&data).unwrap();
    for(multiplexer,codec,bits,kbps)in [(Multiplexer::Mp4,AudioCodec::Pcm,16,128),(Multiplexer::Mov,AudioCodec::Auto,16,32),(Multiplexer::Mov,AudioCodec::Aac,16,512),(Multiplexer::Mov,AudioCodec::Pcm,16,320),(Multiplexer::Mov,AudioCodec::Pcm,24,320)]{
        let settings=H264AudioSettings{codec,bits,bitrate_kbps:kbps,sample_rate:Some(rate),channels,..Default::default()};
        if settings.validate_rate(multiplexer).is_err(){continue;}let output=dir.path().join(format!("outputs/movie-{channels}-{rate}-{multiplexer:?}-{codec:?}-{bits}-{kbps}.bin"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();let spec=RasterVideoSpec{width:64,height:48,frames:2,frame_rate:FrameRateRatio{numerator:25,denominator:1},staging_bytes:1024*1024};
        encode_frames_with_cpu_pool(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(60),&tokio_util::sync::CancellationToken::new(),&tx,Some(RasterAudio{path:&raw,frames:count}),RasterEncoding::NativeH264{target_kbps:100,max_kbps:150,keyint:48,profile:Default::default(),mode:BitrateMode::Vbr1Pass,multiplexer,audio:settings},None,&[],Some(state.cpu_pool.clone()),|_,_|async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}).await.unwrap();
        let bytes=std::fs::read(&output).unwrap();assert_eq!(&bytes[8..12],if multiplexer==Multiplexer::Mp4{b"isom"}else{b"qt  "});let movie=filmcraft_isobmff::open(&bytes).unwrap();let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();let track=&movie.tracks[audio];
        if settings.codec(multiplexer)==AudioCodec::Aac {
            let mut original=filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(rate,channels as usize,(kbps.clamp(32,512)*1000).min(6*rate*channels))).unwrap();let mut units=original.encode(&planar.iter().map(Vec::as_slice).collect::<Vec<_>>());units.extend(original.flush());assert_eq!(track.samples.len(),units.len());for(i,unit)in units.iter().enumerate(){assert_eq!(movie.read_sample(&bytes,audio,i).unwrap().as_slice(),unit.as_slice(),"AAC {multiplexer:?}/{kbps}/{i}");}assert_eq!(track.duration,count+1024);assert_eq!(track.edits[0].segment_duration,count);
        }else{
            assert_eq!(track.duration,count);assert!(track.edits.is_empty());let mut expected=Vec::new();for i in 0..count as usize{for sample in planar.iter().map(|c|c[i]){let max=if bits==24{8388607.}else{32767.};let q=(sample.clamp(-1.,1.)*max).round()as i32;expected.extend_from_slice(&q.to_le_bytes()[..bits as usize/8]);}}
            let mut actual=Vec::new();for i in 0..track.samples.len(){actual.extend(movie.read_sample(&bytes,audio,i).unwrap());}assert_eq!(actual,expected,"PCM {bits}");
        }
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(output).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["channels"],channels);assert_eq!(probe["streams"][0]["sample_rate"],rate.to_string());assert_eq!(probe["streams"][0]["codec_name"],if settings.codec(multiplexer)==AudioCodec::Aac{"aac"}else if bits==24{"pcm_s24le"}else{"pcm_s16le"});
    }
    }}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn h264_movie_jobs_publish_selected_container_audio_and_owner_only_files() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("movie-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("movie-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();document["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"]=json!([]);document["project"]["items"]["6"]["kind"]["Sequence"]["audio_tracks"]=json!([]);let app=support::router(state.clone());
    document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(44100);
    for channels in [1u32,2,6] {for(codec,bits,rate,sample_rate)in [("auto",16,44100,None),("auto",16,44100,Some(0)),("auto",16,48000,Some(48000)),("pcm",16,44117,Some(44117)),("pcm",24,192000,Some(192000))] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"-1","frames":2,"frameSize":[64,48],"preset":"adaptive_high","multiplexer":"mov","withAudio":true,"loudness":{"enabled":true},"h264Audio":{"codec":codec,"bits":bits,"bitrateKbps":128,"sampleRate":sample_rate,"channels":channels}});let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");assert_eq!(accepted["container"],"mov");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["sampleRate"],rate);assert_eq!(result["channels"],channels);assert_eq!(result["container"],"mov");assert!(result["filename"].as_str().unwrap().ends_with(".mov"));assert_eq!(result["audioCodec"],if codec=="auto"{"aac"}else if bits==24{"pcm_s24le"}else{"pcm_s16le"});
        let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();assert_eq!(&bytes[8..12],b"qt  ");let movie=filmcraft_isobmff::open(&bytes).unwrap();let track=movie.tracks.iter().find(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();let expected=(i128::from(FD)*i128::from(rate)).div_euclid(254_016_000_000)-(-i128::from(FD)*i128::from(rate)).div_euclid(254_016_000_000);assert_eq!(track.duration,expected as u64+if codec=="auto"{1024}else{0});for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
    }}
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn h264_movie_rate_jobs_match_original_negative_range_dsp_and_normalized_pcm() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::filmcraft_project_audio::SourceProjectAudioRuntime};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("rate-tone-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let mut tone=document["project"]["items"]["1"].clone();tone["id"]=json!(3);let media=&mut tone["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":48000,"bits_per_sample":32,"codec":"Tone"});document["project"]["items"]["3"]=tone;
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];let mut track=sequence["audio_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["item"]=json!(3);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["effects"]=json!([]);track["items"]=json!([clip]);track["effects"]=json!([]);track["gain_db"]=json!(0.);track["pan"]=json!(37.);sequence["audio_tracks"]=json!([track]);sequence["settings"]["sample_rate"]=json!(48000);sequence["video_tracks"]=json!([]);
    let app=support::router(state.clone());
    for channels in [1u32,2] {for rate in [44100u32,44117,96000] {for enabled in [false,true] {for sample_rate in [None,Some(0),Some(192001),Some(rate)] {
        let first=(-i128::from(FD)*i128::from(rate)).div_euclid(254_016_000_000) as i64;
        let end=(12*i128::from(FD)*i128::from(rate)).div_euclid(254_016_000_000) as i64;
        let frames=(end-first)as usize;
        let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(rate);
        let mut runtime=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();
        let bus=runtime.render_media(6,first,frames,|_,_,_,_|Ok(None)).unwrap();
        let bus=if channels==1 {vec![bus[0].iter().zip(&bus[1]).map(|(l,r)|0.5*(l+r)).collect::<Vec<_>>()]}else{bus};
        let(bus,measured,gain)=if enabled {original_loudness_reference_at_rate(bus,rate,-23.,-2.)}else{(bus,None,0.)};
        let expected=(0..frames).flat_map(|i|bus.iter().flat_map(move |c|((c[i].clamp(-1.,1.)*8388607.).round()as i32).to_le_bytes()[..3].to_vec())).collect::<Vec<_>>();
        let payload=json!({"document":resolved,"sequenceId":"6","startFrames":"-1","frames":13,"preset":"adaptive_high","multiplexer":"mov","withAudio":true,"sources":{},"frameSize":[64,48],"h264Audio":{"codec":"pcm","bits":24,"sampleRate":sample_rate,"channels":channels},"loudness":{"enabled":enabled,"targetLufs":-23.,"truePeakDbtp":-2.}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{rate}/{enabled}: {job:?}");let result=job.result.unwrap();assert_eq!(result["sampleRate"],rate);assert_eq!(result["channels"],channels);
        if enabled {assert_eq!(result["loudness"]["measuredLufs"].as_f64(),measured);assert_eq!(result["loudness"]["gainDb"].as_f64(),Some(gain));}
        let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();assert_eq!(movie.tracks[audio].duration,frames as u64);
        let pcm=(0..movie.tracks[audio].samples.len()).flat_map(|i|movie.read_sample(&bytes,audio,i).unwrap()).collect::<Vec<_>>();assert!(pcm==expected,"rate DSP/normalization mismatch {rate}/{enabled}: first byte {:?}",pcm.iter().zip(&expected).position(|(a,b)|a!=b));
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }}}}
    assert_eq!(document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"],48000);
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn h264_movie_auto_rate_admission_accounts_for_pcm_storage_and_ignores_silent_rate() {
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("rate-admission-owner","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(192000);let app=support::router(state.clone());
    for sample_rate in [None,Some(192000)] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":80000,"frameSize":[2,2],"preset":"adaptive_high","multiplexer":"mov","withAudio":true,"sources":{},"h264Audio":{"codec":"pcm","bits":24,"sampleRate":sample_rate}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert!(response.status().is_client_error());let body=to_bytes(response.into_body(),1024*1024).await.unwrap();assert!(String::from_utf8_lossy(&body).contains("audiovisual export exceeds staging budget"));
    }
    // No audio packets are produced, so a 192 kHz authored sequence must not
    // prevent a video-only H.264 export through the default AAC settings.
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":2,"frameSize":[64,48],"preset":"adaptive_high","sources":{}});
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
    let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["withAudio"],false);assert!(result["sampleRate"].is_null());
    let bytes=std::fs::read(state.storage.join("outputs").join(result["filename"].as_str().unwrap())).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();assert!(movie.tracks.iter().all(|t|t.kind!=filmcraft_isobmff::TrackKind::Audio));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn loudness_export_mono_and_surround_match_original_channels_meter_and_limiter() {
    use video_kadr_backend::tools::native_loudness_export::{normalize_layout,LoudnessSettings};
    let(state,dir)=support::make_state(false,false).await;let settings=LoudnessSettings{enabled:true,target_lufs:-18.,true_peak_dbtp:-2.};let(tx,_)=tokio::sync::mpsc::unbounded_channel();let token=tokio_util::sync::CancellationToken::new();
    for channels in [1usize,2,6] {for rate in [44100u32,96000] {
        let frames=rate as usize+13;let mut bus=vec![vec![0f32;frames];channels];
        for(c,channel)in bus.iter_mut().enumerate(){for(i,sample)in channel.iter_mut().enumerate(){*sample=((i as f64*(330.+c as f64*77.)*std::f64::consts::TAU/f64::from(rate)).sin()*0.03)as f32;}for i in [0,4095,4096,frames-1]{channel[i]=if c%2==0{1.4}else{-1.2};}}
        let bytes=(0..frames).flat_map(|i|bus.iter().flat_map(move |c|c[i].to_le_bytes())).collect::<Vec<_>>();let raw=dir.path().join(format!("layout-{channels}-{rate}.f32le"));std::fs::write(&raw,&bytes).unwrap();let output=dir.path().join(format!("outputs/layout-{channels}-{rate}.f32le"));
        let(expected,measured,gain)=original_loudness_reference_at_rate(bus,rate,-18.,-2.);let expected_bytes=(0..frames).flat_map(|i|expected.iter().flat_map(move |c|c[i].to_le_bytes())).collect::<Vec<_>>();
        let report=normalize_layout(&raw,frames as u64,rate,channels,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap();assert_eq!(report.measured_lufs,measured);assert_eq!(report.gain_db,gain);assert!(std::fs::read(&output).unwrap()==expected_bytes,"layout PCM mismatch {channels}/{rate}");assert_eq!(std::fs::metadata(&output).unwrap().len(),frames as u64*channels as u64*4);assert_eq!(std::fs::read(&raw).unwrap(),bytes);
        assert!(normalize_layout(&raw,frames as u64,rate,channels,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.is_err());assert!(std::fs::read(&output).unwrap()==expected_bytes);
    }}
    for channels in [0,3,8,usize::MAX] {let output=dir.path().join("outputs/invalid-layout.f32le");let result=normalize_layout(&dir.path().join("missing"),1,48000,channels,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await;assert!(result.unwrap_err().to_string().contains("unsupported loudness channel layout"));assert!(!output.exists());}
    // BS.1770 excludes LFE from integrated loudness; preserve its position.
    let frames=48000;let mut bus=vec![vec![0f32;frames];6];for(i,sample)in bus[3].iter_mut().enumerate(){*sample=(i as f32*60.*std::f32::consts::TAU/48000.).sin()*0.1;}
    let bytes=(0..frames).flat_map(|i|bus.iter().flat_map(move |c|c[i].to_le_bytes())).collect::<Vec<_>>();let raw=dir.path().join("lfe.f32le");std::fs::write(&raw,&bytes).unwrap();let output=dir.path().join("outputs/lfe.f32le");let report=normalize_layout(&raw,frames as u64,48000,6,settings,&output,std::time::Duration::from_secs(30),&token,&tx,state.cpu_pool.clone()).await.unwrap();assert!(report.measured_lufs.is_none());assert_eq!(report.gain_db,0.);
    let actual=std::fs::read(&output).unwrap();assert!(actual.chunks_exact(24).all(|frame|(0..6).filter(|c|*c!=3).all(|c|frame[c*4..c*4+4]==[0,0,0,0])));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn surround_movie_jobs_preserve_nonzero_nested_wav_aac_pcm_and_owner_access(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::{filmcraft_project_audio::SourceProjectAudioRuntime,filmcraft_wav_audio::SourceWavPool}};
    let(state,dir)=support::make_state(false,false).await;
    let sample_rate=44100u32;let count=sample_rate as usize;let mut pcm=Vec::with_capacity(count*12);for i in 0..count{for c in 0..6{let value=((i*13+c*127)%2001)as i16-1000;pcm.extend_from_slice(&value.to_le_bytes());}}
    let mut wav=Vec::new();wav.extend_from_slice(b"RIFF");wav.extend_from_slice(&(36+pcm.len()as u32).to_le_bytes());wav.extend_from_slice(b"WAVEfmt ");wav.extend_from_slice(&16u32.to_le_bytes());wav.extend_from_slice(&1u16.to_le_bytes());wav.extend_from_slice(&6u16.to_le_bytes());wav.extend_from_slice(&sample_rate.to_le_bytes());wav.extend_from_slice(&(sample_rate*12).to_le_bytes());wav.extend_from_slice(&12u16.to_le_bytes());wav.extend_from_slice(&16u16.to_le_bytes());wav.extend_from_slice(b"data");wav.extend_from_slice(&(pcm.len()as u32).to_le_bytes());wav.extend(pcm);let path=dir.path().join("sources/surround.wav");std::fs::write(&path,&wav).unwrap();
    assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":"surround-wave","filename":"surround.wav","mediaType":"audio"}))).await);
    let owner=state.db.register_auth_user("surround-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("surround-outsider","test password").await.unwrap().unwrap();let space=state.db.create_space(&owner.user.username,"Surround source").await.unwrap();let ids=vec!["surround-wave".to_string()];state.db.create_space_composition_project("Surround",&json!({"schemaVersion":1,"sources":{"surround-wave":{"id":"surround-wave"}}}),&ids,&owner.user.username,&space.id).await.unwrap();
    let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-project-audio.json")).unwrap();let mut document=fixture["cases"][0]["document"].clone();
    for id in [7,9] {let s=&mut document["project"]["items"][id.to_string()]["kind"]["Sequence"];s["settings"]["audio_master"]=json!("Surround51");s["settings"]["sample_rate"]=json!(if id==7{44100}else{48000});s["settings"]["frame_rate"]=json!({"num":24,"den":1});s["master_volume_db"]=json!(0.);s["master_effects"]=json!([]);s["master_mixer"]=json!({"mode":"Read"});s["submix_tracks"]=json!([]);s["video_tracks"]=json!([]);s["multicam"]=Value::Null;s["audio_tracks"].as_array_mut().unwrap().truncate(1);let tr=&mut s["audio_tracks"][0];tr["channels"]=json!("Surround51");tr["volume_db"]=json!(0.);tr["pan"]=json!(0.);tr["muted"]=json!(false);tr["solo"]=json!(false);tr["effects"]=json!([]);tr["transitions"]=json!([]);tr["mixer"]=json!({"mode":"Read","input_map":"Stereo"});tr["items"].as_array_mut().unwrap().truncate(1);let clip=&mut tr["items"][0];clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*48);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["gain_db"]=json!(0.);clip["effects"]=json!([]);clip["source_channels"]=json!([]);}
    let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"File":{"path":"/untrusted/surround.wav"}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["duration"]=json!(FD*48);media["info"]["audio"]=json!({"channels":6,"sample_rate":44100,"bits_per_sample":16,"codec":"pcm_s16le"});
    let mut pool=SourceWavPool::default();pool.bind(1,wav.into()).unwrap();let mut reference=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&document).unwrap()).unwrap()).unwrap();let raw=reference.render_media_channels(9,-2000,26000,6,|id,start,n,rate|pool.read(id,start,n,rate)).unwrap();assert_eq!(raw.len(),6);assert!(raw.iter().all(|c|c.iter().any(|v|v.abs()>0.001)));
    let app=support::router(state.clone());
    for(multiplexer,codec,bits,normalized)in [("mp4","aac",16,false),("mov","aac",16,true),("mov","pcm",16,false),("mov","pcm",24,true)] {
        let (expected,measured,gain)=if normalized{original_loudness_reference_at_rate(raw.clone(),48000,-23.,-2.)}else{(raw.clone(),None,0.)};
        let payload=json!({"document":document,"sequenceId":"9","startFrames":"-1","frames":13,"frameSize":[64,48],"preset":"adaptive_high","multiplexer":multiplexer,"withAudio":true,"sources":{"1":"surround-wave"},"h264Audio":{"codec":codec,"bits":bits,"channels":6,"bitrateKbps":128},"loudness":{"enabled":normalized,"targetLufs":-23.,"truePeakDbtp":-2.}});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{multiplexer}/{codec}/{bits}/{normalized}: {job:?}");let result=job.result.unwrap();assert_eq!(result["channels"],6);assert_eq!(result["sampleRate"],48000);if normalized{assert_eq!(result["loudness"]["measuredLufs"].as_f64(),measured);assert_eq!(result["loudness"]["gainDb"].as_f64(),Some(gain));}
        let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let bytes=std::fs::read(&path).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let audio=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();let track=&movie.tracks[audio];
        if codec=="aac"{let mut encoder=filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(48000,6,128000)).unwrap();let mut units=encoder.encode(&expected.iter().map(Vec::as_slice).collect::<Vec<_>>());units.extend(encoder.flush());assert_eq!(track.samples.len(),units.len());assert_eq!(track.duration,27024);assert_eq!(track.edits[0].segment_duration,26000);assert_eq!(track.edits[0].media_time,1024);for(i,unit)in units.iter().enumerate(){assert!(movie.read_sample(&bytes,audio,i).unwrap()==*unit,"surround AAC packet mismatch {i}/{normalized}");}}
        else{assert_eq!(track.duration,26000);let actual=(0..track.samples.len()).flat_map(|i|movie.read_sample(&bytes,audio,i).unwrap()).collect::<Vec<_>>();let expected=(0..26000).flat_map(|i|expected.iter().flat_map(move |c|{let max=if bits==24{8388607.}else{32767.};((c[i].clamp(-1.,1.)*max).round()as i32).to_le_bytes()[..bits as usize/8].to_vec()})).collect::<Vec<_>>();assert!(actual==expected,"surround PCM mismatch {bits}/{normalized}");}
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(&path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["channels"],6);
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }
    assert_eq!(document["project"]["items"]["7"]["kind"]["Sequence"]["settings"]["sample_rate"],44100);state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn audio_only_layout_jobs_resolve_original_rates_channels_bits_and_force_pcm(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::{filmcraft_project_audio::SourceProjectAudioRuntime,filmcraft_pcm_export::{self,PcmFormat}}};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("pcm-layout-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("pcm-layout-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":44117,"bits_per_sample":32,"codec":"Tone"});media["info"]["duration"]=json!(FD*96);
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(44117);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["audio_master"]=json!("Surround51");sequence["master_volume_db"]=json!(0.);sequence["master_effects"]=json!([]);sequence["master_mixer"]=json!({"mode":"Read"});sequence["audio_tracks"].as_array_mut().unwrap().truncate(1);let track=&mut sequence["audio_tracks"][0];track["channels"]=json!("Surround51");track["muted"]=json!(false);track["solo"]=json!(false);track["volume_db"]=json!(0.);track["pan"]=json!(0.);track["effects"]=json!([]);track["transitions"]=json!([]);track["mixer"]=json!({"mode":"Read","input_map":"Stereo"});track["items"].as_array_mut().unwrap().truncate(1);let clip=&mut track["items"][0];clip["item"]=json!(1);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["gain_db"]=json!(0.);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["effects"]=json!([]);clip["source_channels"]=json!([]);let app=support::router(state.clone());
    for preset in ["wav16","wav24","aiff16"]{for(audio,start,n,normalized)in [(Value::Null,1i64,1i64,false),(json!({"sampleRate":0,"channels":6}),1,1,false),(json!({"codec":"aac","sampleRate":192000,"channels":6,"bits":24}),-1,13,true),(json!({"sampleRate":44100,"channels":1,"bits":24}),0,1,false),(json!({"channels":0}),0,1,false),(json!({"channels":u32::MAX}),0,1,false)]{
        let rate=audio.get("sampleRate").map(|value|value.as_u64().filter(|r|(8000..=192000).contains(r)).unwrap_or(44117)).unwrap_or(48000)as u32;let channels=match audio["channels"].as_u64().unwrap_or(2){0|1=>1,6.. =>6,_=>2};let bits=audio["bits"].as_u64().unwrap_or(if preset=="wav24"{24}else{16})as u16;let first=(i128::from(start)*i128::from(rate)).div_euclid(24)as i64;let end=(i128::from(start+n)*i128::from(rate)).div_euclid(24)as i64;let frames=(end-first)as usize;let format=if preset=="aiff16"{PcmFormat::Aiff}else{PcmFormat::Wav};
        let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(rate);let make_reference=||SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();let mut reference=make_reference();let bus=reference.render_media_channels(6,first,frames,channels,|_,_,_,_|panic!("tone source must not decode files")).unwrap();let(bus,measured,gain)=if normalized{original_loudness_reference_at_rate(bus,rate,-23.,-2.)}else{(bus,None,0.)};
        let expected=if !normalized{filmcraft_pcm_export::export_pcm_with_reader(&mut make_reference(),6,first,frames,channels as u16,bits,format,|_,_,_,_|panic!("tone source must not decode files")).unwrap()}else{let mut data=filmcraft_pcm_export::header(frames as u64,rate,channels as u16,bits,format).unwrap();for i in 0..frames{for channel in &bus{let q=(channel[i].clamp(-1.,1.)*if bits==24{8388607.}else{32767.}).round()as i32;if preset=="aiff16"{data.extend_from_slice(&q.to_be_bytes()[4-bits as usize/8..]);}else{data.extend_from_slice(&q.to_le_bytes()[..bits as usize/8]);}}}if preset=="aiff16"&&(frames*channels*bits as usize/8)&1==1{data.push(0);}data};
        let payload=json!({"document":document,"sequenceId":"6","startFrames":start.to_string(),"frames":n,"preset":preset,"audio":audio,"sources":{},"frameSize":[0,0],"loudness":{"enabled":normalized,"targetLufs":-23.,"truePeakDbtp":-2.}});let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");assert_eq!(accepted["mode"],"audio_only");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{audio}: {job:?}");let result=job.result.unwrap();assert_eq!(result["sampleRate"],rate);assert_eq!(result["channels"],channels);assert_eq!(result["bitsPerSample"],bits);assert_eq!(result["duration"],json!(frames as f64/f64::from(rate)));let codec=format!("pcm_s{bits}{}",if preset=="aiff16"{"be"}else{"le"});assert_eq!(result["audioCodec"],codec);assert!(result["width"].is_null()&&result["height"].is_null()&&result["videoCodec"].is_null());if normalized{assert_eq!(result["loudness"]["measuredLufs"].as_f64(),measured);assert_eq!(result["loudness"]["gainDb"].as_f64(),Some(gain));}
        let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());assert!(std::fs::read(&path).unwrap()==expected,"PCM job bytes {preset}/{audio}");let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_streams","-of","json"]).arg(&path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["codec_name"],codec);assert_eq!(probe["streams"][0]["sample_rate"],rate.to_string());assert_eq!(probe["streams"][0]["channels"],channels);assert_eq!(probe["streams"][0]["duration_ts"],frames);
        for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}assert!(!state.storage.join("cache/filmcraft-exports").join(result["id"].as_str().unwrap()).exists());
    }}
    // Normalization temporarily owns both original and normalized float mixes.
    let mut huge=document.clone();huge["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["frame_rate"]=json!({"num":1,"den":1});let payload=json!({"document":huge,"sequenceId":"6","startFrames":"0","frames":6000,"preset":"wav16","audio":{"sampleRate":192000,"channels":1},"sources":{},"loudness":{"enabled":true}});let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert!(response.status().is_client_error());let body=to_bytes(response.into_body(),1024*1024).await.unwrap();assert!(String::from_utf8_lossy(&body).contains("audio staging budget exceeded"));
    assert_eq!(document["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"],44117);state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn intra_mov_jobs_preserve_configured_pcm_aac_layout_rate_and_owner_access(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::filmcraft_project_audio::SourceProjectAudioRuntime};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("intra-layout-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("intra-layout-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":44117,"bits_per_sample":32,"codec":"Tone"});media["info"]["duration"]=json!(FD*96);
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(44117);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["audio_master"]=json!("Surround51");sequence["master_volume_db"]=json!(0.);sequence["master_effects"]=json!([]);sequence["master_mixer"]=json!({"mode":"Read"});sequence["audio_tracks"].as_array_mut().unwrap().truncate(1);let track=&mut sequence["audio_tracks"][0];track["channels"]=json!("Surround51");track["muted"]=json!(false);track["solo"]=json!(false);track["volume_db"]=json!(0.);track["pan"]=json!(0.);track["effects"]=json!([]);track["transitions"]=json!([]);track["mixer"]=json!({"mode":"Read","input_map":"Stereo"});track["items"].as_array_mut().unwrap().truncate(1);let clip=&mut track["items"][0];clip["item"]=json!(1);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["gain_db"]=json!(0.);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["effects"]=json!([]);clip["source_channels"]=json!([]);let app=support::router(state.clone());

    for preset in ["prores_hq","dnxhr_hq","apv42210"] {
        for (audio,rate,channels,bits,aac,normalized) in [
            (Value::Null,48000u32,2usize,24u16,false,false),
            (json!({"codec":"auto","sampleRate":44117,"channels":1,"bits":16}),44117,1,16,false,false),
            (json!({"codec":"pcm","sampleRate":192000,"channels":6,"bits":24}),192000,6,24,false,true),
            (json!({"codec":"aac","sampleRate":8000,"channels":6,"bitrateKbps":512}),8000,6,24,true,false),
            (json!({"codec":"aac","sampleRate":96000,"channels":2,"bitrateKbps":128}),96000,2,24,true,true),
        ] {
            let first=-(i64::from(rate)+23)/24;let end=i64::from(rate)/24;let count=(end-first)as usize;
            let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(rate);
            let mut reference=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();
            let planar=reference.render_media_channels(6,first,count,channels,|_,_,_,_|panic!("tone must not decode files")).unwrap();
            let(planar,_,_)=if normalized{original_loudness_reference_at_rate(planar,rate,-23.,-2.)}else{(planar,None,0.)};
            let expected:Vec<Vec<u8>>=if aac {
                let kbps=audio["bitrateKbps"].as_u64().unwrap()as u32;
                let mut encoder=filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(rate,channels,(kbps.clamp(32,512)*1000).min(6*rate*channels as u32))).unwrap();
                let mut units=encoder.encode(&planar.iter().map(Vec::as_slice).collect::<Vec<_>>());units.extend(encoder.flush());units
            } else {
                let mut raw=Vec::new();for i in 0..count{for channel in &planar{let q=(channel[i].clamp(-1.,1.)*if bits==24{8388607.}else{32767.}).round()as i32;raw.extend_from_slice(&q.to_le_bytes()[..bits as usize/8]);}}vec![raw]
            };
            let payload=json!({"document":document,"sequenceId":"6","startFrames":"-1","frames":2,"preset":preset,"withAudio":true,"audio":audio,"sources":{},"frameSize":[64,48],"loudness":{"enabled":normalized,"targetLufs":-23.,"truePeakDbtp":-2.}});
            let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
            let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}/{audio}: {accepted}");
            let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{audio}: {job:?}");let result=job.result.unwrap();
            let codec=if aac{"aac"}else if bits==24{"pcm_s24le"}else{"pcm_s16le"};assert_eq!(result["audioCodec"],codec);assert_eq!(result["sampleRate"],rate);assert_eq!(result["channels"],channels);assert_eq!(result["container"],"mov");
            let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());let bytes=std::fs::read(&path).unwrap();let movie=filmcraft_isobmff::open(&bytes).unwrap();let index=movie.tracks.iter().position(|t|t.kind==filmcraft_isobmff::TrackKind::Audio).unwrap();let track=&movie.tracks[index];
            assert_eq!(track.timescale,rate);assert_eq!(track.duration,count as u64+if aac{1024}else{0});
            let actual=(0..track.samples.len()).map(|i|movie.read_sample(&bytes,index,i).unwrap()).collect::<Vec<_>>();
            if aac{assert_eq!(actual,expected,"AAC packets {preset}/{audio}");assert_eq!(track.edits[0].segment_duration,count as u64);}else{assert_eq!(actual.concat(),expected.concat(),"PCM bytes {preset}/{audio}");}
            let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(&path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["codec_name"],codec);assert_eq!(probe["streams"][0]["channels"],channels);assert_eq!(probe["streams"][0]["sample_rate"],rate.to_string());
            for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        }
    }
    // AAC rates are validated before a job can be queued; PCM retains arbitrary integer rates.
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":1,"preset":"prores_hq","withAudio":true,"audio":{"codec":"aac","sampleRate":44117},"sources":{},"frameSize":[64,48]});
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert!(response.status().is_client_error());
    // Two float mixes exceed 8 GiB even though float+PCM24 alone would fit.
    let mut huge=document.clone();huge["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["frame_rate"]=json!({"num":1,"den":1});
    let payload=json!({"document":huge,"sequenceId":"6","startFrames":"0","frames":1000,"preset":"prores_hq","withAudio":true,"audio":{"codec":"pcm","sampleRate":192000,"channels":6,"bits":24},"loudness":{"enabled":true},"sources":{},"frameSize":[64,48]});
    let response=app.oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert!(response.status().is_client_error());let body=to_bytes(response.into_body(),1024*1024).await.unwrap();assert!(String::from_utf8_lossy(&body).contains("audiovisual export exceeds staging budget"));
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn mxf_jobs_preserve_configured_pcm_rates_layouts_atom_channels_and_access(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::filmcraft_project_audio::SourceProjectAudioRuntime};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("mxf-layout-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("mxf-layout-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":44117,"bits_per_sample":32,"codec":"Tone"});media["info"]["duration"]=json!(FD*96);
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(44117);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["audio_master"]=json!("Surround51");sequence["master_volume_db"]=json!(0.);sequence["master_effects"]=json!([]);sequence["master_mixer"]=json!({"mode":"Read"});sequence["audio_tracks"].as_array_mut().unwrap().truncate(1);let track=&mut sequence["audio_tracks"][0];track["channels"]=json!("Surround51");track["muted"]=json!(false);track["solo"]=json!(false);track["volume_db"]=json!(0.);track["pan"]=json!(0.);track["effects"]=json!([]);track["transitions"]=json!([]);track["mixer"]=json!({"mode":"Read","input_map":"Stereo"});track["items"].as_array_mut().unwrap().truncate(1);let clip=&mut track["items"][0];clip["item"]=json!(1);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["gain_db"]=json!(0.);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["effects"]=json!([]);clip["source_channels"]=json!([]);let app=support::router(state.clone());


    for preset in ["mxf_dnxhr_hq","mxf_prores_hq","mxf_h264","mxf_atom_dnxhr_hq"] {
        let atom=preset=="mxf_atom_dnxhr_hq";
        for (audio,rate,channels,bits,normalized) in [
            (Value::Null,48000u32,2usize,24u32,false),
            (json!({"codec":"aac","sampleRate":0,"channels":0,"bits":16}),44117,1,16,false),
            (json!({"codec":"auto","sampleRate":192000,"channels":6,"bits":24}),192000,6,24,true),
            (json!({"codec":"aac","sampleRate":8000,"channels":6,"bits":16}),8000,6,16,false),
            (json!({"sampleRate":96000,"channels":2,"bits":24}),96000,2,24,true),
        ] {
            let n=if normalized{13u64}else{2u64};let first=-(i64::from(rate)+23)/24;let end=(u64::from(rate)*(n-1)/24)as i64;let count=(end-first)as usize;
            let mut resolved=document.clone();resolved["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(rate);
            let mut reference=SourceProjectAudioRuntime::new(SourceProjectDocument::read(&serde_json::to_vec(&resolved).unwrap()).unwrap()).unwrap();
            let planar=reference.render_media_channels(6,first,count,channels,|_,_,_,_|panic!("tone must not decode files")).unwrap();
            let(planar,measured,gain)=if normalized{original_loudness_reference_at_rate(planar,rate,-23.,-2.)}else{(planar,None,0.)};if normalized{assert!(measured.is_some());assert!(gain.abs()>0.1);}
            let payload=json!({"document":document,"sequenceId":"6","startFrames":"-1","frames":n,"preset":preset,"withAudio":true,"audio":audio,"sources":{},"frameSize":[64,48],"loudness":{"enabled":normalized,"targetLufs":-23.,"truePeakDbtp":-2.}});
            let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
            let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}/{audio}: {accepted}");
            let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}/{audio}: {job:?}");let result=job.result.unwrap();
            let codec=if bits==24{"pcm_s24le"}else{"pcm_s16le"};assert_eq!(result["audioCodec"],codec);assert_eq!(result["sampleRate"],rate);assert_eq!(result["channels"],channels);assert_eq!(result["container"],if atom{"zip"}else{"mxf"});if normalized{assert_eq!(result["loudness"]["measuredLufs"].as_f64(),measured);assert_eq!(result["loudness"]["gainDb"].as_f64(),Some(gain));}
            let path=state.storage.join("outputs").join(result["filename"].as_str().unwrap());
            let files=if atom {
                use std::io::Read;let mut zip=zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();assert_eq!(zip.len(),channels+1);
                (0..zip.len()).map(|i|{let mut file=zip.by_index(i).unwrap();let name=file.name().to_string();assert_eq!(name,if i==0{"export.mxf".to_string()}else{format!("export_A{i}.mxf")});let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();(name,bytes)}).collect::<Vec<_>>()
            }else{vec![("export.mxf".to_string(),std::fs::read(&path).unwrap())]};
            let mut material=None;let mut packages=std::collections::HashSet::new();
            for (entry,(name,bytes)) in files.iter().enumerate(){
                let movie=filmcraft_mxf::open(bytes).unwrap();
                if atom{assert_eq!(movie.tracks.len(),1);}else{assert_eq!(movie.tracks.len(),2);}
                if !atom||entry>0 {
                    let track=movie.tracks.iter().find(|track|track.kind==filmcraft_mxf::TrackKind::Sound).unwrap();let info=track.sound.as_ref().unwrap();
                    assert_eq!(info.channels,if atom{1}else{channels as u32});assert_eq!(info.bits,bits);assert_eq!(info.sample_rate,filmcraft_mxf::Rational::new(rate as i32,1));
                    let actual=track.chunks.iter().flat_map(|chunk|bytes[chunk.offset as usize..(chunk.offset+chunk.size)as usize].iter().copied()).collect::<Vec<_>>();
                    let expected=if atom{filmcraft_mxf::encode_pcm(&[planar[entry-1].clone()],bits)}else{
                        // The original OP1a writer emits floor-clock edit units, unlike clip-wrapped OP-Atom.
                        let mut cfg=filmcraft_mxf::WriterConfig::new(filmcraft_mxf::Pattern::Op1a,filmcraft_mxf::Rational::new(24,1),filmcraft_mxf::PackageIds::from_seed("audio-oracle","reference"));
                        cfg.picture=Some(filmcraft_mxf::PictureDesc::new(filmcraft_mxf::PictureCoding::Vc3{cid:1272},64,48));
                        cfg.sound=vec![filmcraft_mxf::SoundDesc{sample_rate:rate,channels:channels as u32,bits}];
                        let mut original=filmcraft_mxf::MxfWriter::new(std::io::Cursor::new(Vec::new()),cfg).unwrap();original.push_sound(0,&filmcraft_mxf::encode_pcm(&planar,bits)).unwrap();
                        let picture=movie.tracks.iter().position(|t|t.kind==filmcraft_mxf::TrackKind::Picture).unwrap();
                        for i in 0..movie.tracks[picture].samples.len(){original.push_picture(&movie.read_sample(bytes,picture,i).unwrap(),filmcraft_mxf::FrameInfo::intra()).unwrap();}
                        let reference=original.finish().unwrap().into_inner();let mux=filmcraft_mxf::open(&reference).unwrap();let sound=mux.tracks.iter().find(|t|t.kind==filmcraft_mxf::TrackKind::Sound).unwrap();
                        sound.chunks.iter().flat_map(|chunk|reference[chunk.offset as usize..(chunk.offset+chunk.size)as usize].iter().copied()).collect()
                    };
                    assert!(actual==expected,"PCM {preset}/{audio}/{name}: {} vs {} bytes",actual.len(),expected.len());
                    assert_eq!(actual.len(),if atom{count*bits as usize/8}else{(u64::from(rate)*n/24)as usize*channels*bits as usize/8});
                }
                let extracted=path.with_file_name(format!("probe-{}-{name}",uuid::Uuid::new_v4()));std::fs::write(&extracted,bytes).unwrap();
                let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-show_format","-show_streams","-of","json"]).arg(&extracted).output().await.unwrap();std::fs::remove_file(extracted).unwrap();assert!(probe.status.success(),"ffprobe {preset}/{audio}/{name}: {}",String::from_utf8_lossy(&probe.stderr));let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
                if !atom||entry>0{let stream=probe["streams"].as_array().unwrap().iter().find(|v|v["codec_type"]=="audio").unwrap();assert_eq!(stream["codec_name"],codec);assert_eq!(stream["sample_rate"],rate.to_string());assert_eq!(stream["channels"],if atom{1}else{channels});}
                if atom{let id=probe["format"]["tags"]["material_package_umid"].as_str().unwrap();if let Some(expected)=&material{assert_eq!(id,expected)}else{material=Some(id.to_string());}assert!(packages.insert(probe["streams"][0]["tags"]["file_package_umid"].as_str().unwrap().to_string()));}
            }
            for(token,allowed)in [(&owner.token,true),(&outsider.token,false)]{let response=app.clone().oneshot(Request::builder().uri(result["url"].as_str().unwrap()).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status().is_success(),allowed);}
        }
    }
    // Mono files and their ZIP payload coexist; normalization is not the only peak.
    let mut huge=document.clone();huge["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["frame_rate"]=json!({"num":1,"den":1});
    for normalized in [false,true]{
        let payload=json!({"document":huge,"sequenceId":"6","startFrames":"0","frames":1000,"preset":"mxf_atom_dnxhr_hq","withAudio":true,"audio":{"sampleRate":192000,"channels":6,"bits":24},"loudness":{"enabled":normalized},"sources":{},"frameSize":[64,48]});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert!(response.status().is_client_error());let body=to_bytes(response.into_body(),1024*1024).await.unwrap();assert!(String::from_utf8_lossy(&body).contains("audiovisual export exceeds staging budget"));
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn mxf_atom_distinct_six_channel_pcm_and_cancellation_preserve_owned_files(){
    use std::io::Read;
    use video_kadr_backend::{tools::native_rgba_export::{encode_frames_with_native_audio,RasterAudio,RasterEncoding,RasterVideoSpec},domain::{composition::FrameRateRatio,filmcraft_export_movie::AudioSettings}};
    let(state,dir)=support::make_state(false,false).await;
    for(rate,bits)in [(44117u32,16u16),(192000,24)]{
        let frames=(u64::from(rate)*3*1001/30000)as usize;
        let planar=(0..6).map(|c|(0..frames).map(|i|((i as f32*(110.+c as f32*77.)*std::f32::consts::TAU/rate as f32).sin()*0.02*(c+1)as f32)+c as f32*0.001).collect::<Vec<_>>()).collect::<Vec<_>>();
        let raw_bytes=(0..frames).flat_map(|i|planar.iter().flat_map(move|c|c[i].to_le_bytes())).collect::<Vec<_>>();let raw=dir.path().join(format!("atom-distinct-{rate}-{bits}.f32le"));std::fs::write(&raw,&raw_bytes).unwrap();
        let spec=RasterVideoSpec{width:64,height:48,frames:3,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},staging_bytes:1024*1024};let encoding=RasterEncoding::MxfAtom{start:107892,drop_frame:true};
        let settings=AudioSettings{sample_rate:Some(rate),channels:6,bits,..Default::default()};let output=dir.path().join(format!("outputs/atom-distinct-{rate}-{bits}.zip"));let(tx,_)=tokio::sync::mpsc::unbounded_channel();
        encode_frames_with_native_audio(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,Some(RasterAudio{path:&raw,frames:frames as u64}),encoding,None,&[],Some(state.cpu_pool.clone()),Some(settings),|_,_|async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}).await.unwrap();
        let original_output=std::fs::read(&output).unwrap();let mut zip=zip::ZipArchive::new(std::io::Cursor::new(&original_output)).unwrap();assert_eq!(zip.len(),7);
        for(channel,source)in planar.iter().enumerate(){let mut file=zip.by_name(&format!("export_A{}.mxf",channel+1)).unwrap();let mut bytes=Vec::new();file.read_to_end(&mut bytes).unwrap();let movie=filmcraft_mxf::open(&bytes).unwrap();assert_eq!(movie.timecode.as_ref().unwrap().format(),"01:00:00;00");let track=&movie.tracks[0];let actual=track.chunks.iter().flat_map(|chunk|bytes[chunk.offset as usize..(chunk.offset+chunk.size)as usize].iter().copied()).collect::<Vec<_>>();assert_eq!(actual,filmcraft_mxf::encode_pcm(&[source.clone()],u32::from(bits)),"mono channel {channel}");}
        let peer=dir.path().join("outputs/.native-mxf-peer.mov");std::fs::write(&peer,b"peer").unwrap();let cancelled=dir.path().join(format!("outputs/atom-distinct-cancel-{rate}-{bits}.zip"));let cancel=tokio_util::sync::CancellationToken::new();let stop=cancel.clone();
        let result=encode_frames_with_native_audio(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&cancelled,spec,std::time::Duration::from_secs(30),&cancel,&tx,Some(RasterAudio{path:&raw,frames:frames as u64}),encoding,None,&[],Some(state.cpu_pool.clone()),Some(settings),|i,_|{if i>0{stop.cancel();}async{Ok(GroupSurface{width:64,height:48,rgba:[0,0,0,255].repeat(64*48)})}}).await;
        assert!(result.is_err());assert!(!cancelled.exists());tokio::time::timeout(std::time::Duration::from_secs(5),async{while state.cpu_pool.snapshot().running>0{tokio::time::sleep(std::time::Duration::from_millis(5)).await;}}).await.unwrap();assert_eq!(std::fs::read(&raw).unwrap(),raw_bytes);assert_eq!(std::fs::read(&peer).unwrap(),b"peer");assert_eq!(std::fs::read(&output).unwrap(),original_output);
        assert!(std::fs::read_dir(dir.path().join("outputs")).unwrap().all(|entry|{let name=entry.unwrap().file_name();let name=name.to_string_lossy();!name.starts_with(".native-mxf-")||name==".native-mxf-peer.mov"}));
        assert!(encode_frames_with_native_audio(&state.process_runtime,std::path::Path::new("/never-use-ffmpeg"),&output,spec,std::time::Duration::from_secs(30),&tokio_util::sync::CancellationToken::new(),&tx,Some(RasterAudio{path:&raw,frames:frames as u64}),encoding,None,&[],Some(state.cpu_pool.clone()),Some(settings),|_,_|async{anyhow::bail!("existing output must not render")}).await.is_err());assert_eq!(std::fs::read(&output).unwrap(),original_output);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn source_preset_audio_defaults_publish_48k_without_overrides(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("preset-rate-owner","test password").await.unwrap().unwrap();let _outsider=state.db.register_auth_user("preset-rate-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":44117,"bits_per_sample":32,"codec":"Tone"});media["info"]["duration"]=json!(FD*96);
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(44117);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["audio_master"]=json!("Surround51");sequence["master_volume_db"]=json!(0.);sequence["master_effects"]=json!([]);sequence["master_mixer"]=json!({"mode":"Read"});sequence["audio_tracks"].as_array_mut().unwrap().truncate(1);let track=&mut sequence["audio_tracks"][0];track["channels"]=json!("Surround51");track["muted"]=json!(false);track["solo"]=json!(false);track["volume_db"]=json!(0.);track["pan"]=json!(0.);track["effects"]=json!([]);track["transitions"]=json!([]);track["mixer"]=json!({"mode":"Read","input_map":"Stereo"});track["items"].as_array_mut().unwrap().truncate(1);let clip=&mut track["items"][0];clip["item"]=json!(1);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["gain_db"]=json!(0.);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["effects"]=json!([]);clip["source_channels"]=json!([]);let app=support::router(state.clone());


    for preset in ["adaptive_high","adaptive_medium","adaptive_low","high_quality1080","high_quality2160","youtube1080","youtube2160","vimeo1080","vimeo2160","social_vertical","wav16","wav24","aiff16"] {
        let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":2,"preset":preset,"withAudio":true,"sources":{},"frameSize":[64,48]});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();assert_eq!(result["sampleRate"],48000);assert_eq!(result["channels"],2);
        let file=state.outputs_dir().join(result["filename"].as_str().unwrap());let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(file).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["sample_rate"],"48000");
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_library_preset_exports_snapshot_after_library_is_deleted(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::{model::JobStatus,config::resource_classes::ResourceClass,domain::filmcraft_export_presets::Preset};
    let(state,_dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("preset-export-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);
    let mut preset=video_kadr_backend::domain::filmcraft_export_presets::builtins().remove(0);
    preset.name="Saved review".into();preset.settings["frameSize"]=json!([96,64]);preset.settings["frameRate"]=json!({"num":12,"den":1});preset.settings["includeAudio"]=json!(false);
    state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.save(Preset{builtin:false,..preset},true)).await.unwrap();
    let mut permits=Vec::new();for _ in 0..state.resource_limit(ResourceClass::Export){permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());}
    let app=support::router(state.clone());
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":2,"sources":{},"libraryPreset":"Saved review"});
    let response=app.oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
    let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
    state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.delete("Saved review")).await.unwrap();drop(permits);
    let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
    assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_streams","-of","json"]).arg(state.outputs_dir().join(result["filename"].as_str().unwrap())).output().await.unwrap();assert!(probe.status.success());
    let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["width"],96);assert_eq!(probe["streams"][0]["height"],64);assert_eq!(probe["streams"][0]["r_frame_rate"],"12/1");
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn source_preset_audio_v1_work_keeps_sequence_rate_after_restart(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("legacy-rate-owner","test password").await.unwrap().unwrap();let _outsider=state.db.register_auth_user("legacy-rate-outsider","test password").await.unwrap().unwrap();let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"Tone":{"hz":440.,"db":-30.}}});media["info"]["kind"]=json!("AudioOnly");media["info"]["video"]=Value::Null;media["info"]["audio"]=json!({"channels":2,"sample_rate":44117,"bits_per_sample":32,"codec":"Tone"});media["info"]["duration"]=json!(FD*96);
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["video_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["settings"]["sample_rate"]=json!(44117);sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["audio_master"]=json!("Surround51");sequence["master_volume_db"]=json!(0.);sequence["master_effects"]=json!([]);sequence["master_mixer"]=json!({"mode":"Read"});sequence["audio_tracks"].as_array_mut().unwrap().truncate(1);let track=&mut sequence["audio_tracks"][0];track["channels"]=json!("Surround51");track["muted"]=json!(false);track["solo"]=json!(false);track["volume_db"]=json!(0.);track["pan"]=json!(0.);track["effects"]=json!([]);track["transitions"]=json!([]);track["mixer"]=json!({"mode":"Read","input_map":"Stereo"});track["items"].as_array_mut().unwrap().truncate(1);let clip=&mut track["items"][0];clip["item"]=json!(1);clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(FD*96);clip["gain_db"]=json!(0.);clip["speed"]=json!(1.);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;clip["effects"]=json!([]);clip["source_channels"]=json!([]);let app=support::router(state.clone());


    use video_kadr_backend::{config::resource_classes::ResourceClass,jobs::JobKind,state::AppState};
    let mut permits=Vec::new();for _ in 0..state.resource_limit(ResourceClass::Export){permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());}
    let mut pending=Vec::new();
    for(preset,rate,legacy_audio,legacy_h264,mov)in [
        ("wav16",44117u32,Value::Null,json!({}),false),("wav24",44117,Value::Null,json!({}),false),("aiff16",44117,json!({"bits":24}),json!({}),false),
        ("adaptive_high",44117,Value::Null,json!({"codec":"pcm"}),true),("adaptive_high",44100,Value::Null,Value::Null,false),
    ]{
        let mut doc=document.clone();doc["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["sample_rate"]=json!(rate);
        let mut audio=legacy_audio.clone();if preset!="adaptive_high"{if audio.is_null(){audio=json!({});}audio["sampleRate"]=json!(rate);}
        let h264=if mov{json!({"codec":"pcm","sampleRate":rate})}else{json!({"sampleRate":rate})};
        let payload=json!({"document":doc,"sequenceId":"6","startFrames":"0","frames":2,"preset":preset,"withAudio":true,"sources":{},"frameSize":[64,48],"multiplexer":if mov{"mov"}else{"mp4"},"audio":audio,"h264Audio":h264});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),202);let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();pending.push((accepted["jobId"].as_str().unwrap().to_string(),rate,legacy_audio,legacy_h264));
    }
    state.begin_shutdown();drop(permits);assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
    let requests=state.job_store.active_requests(JobKind::FilmcraftVideo,256).await.unwrap();
    let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db"))).await.unwrap();
    for(id,_,audio,h264)in &pending{let mut work=requests.iter().find(|r|r.job.id==*id).unwrap().payload.clone();assert_eq!(work["schema_version"],2);work["schema_version"]=json!(1);work["request"]["audio"]=audio.clone();work["request"]["h264Audio"]=h264.clone();sqlx::query("UPDATE job_requests SET payload_json=? WHERE job_id=?").bind(work.to_string()).bind(id).execute(&pool).await.unwrap();}
    pool.close().await;
    let reopened=AppState::new_with_runtime(state.storage.clone(),2,(*state.tools).clone(),video_kadr_backend::library::Library::load(state.storage.clone()).await,video_kadr_backend::db::Db::open(&state.storage).await.unwrap(),state.encode_budget.clone(),2).unwrap();
    reopened.recover_jobs().await;video_kadr_backend::handlers::resume_pending_jobs(&reopened).await;
    for(id,rate,_,_)in pending{let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=reopened.get_job(&id).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"legacy {id}: {job:?}");let result=job.result.unwrap();assert_eq!(result["sampleRate"],rate);let path=reopened.outputs_dir().join(result["filename"].as_str().unwrap());let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","a:0","-show_streams","-of","json"]).arg(path).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["sample_rate"],rate.to_string());}
    reopened.begin_shutdown();assert!(reopened.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_library_scale_exports_original_match_source_canvas(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;
    use video_kadr_backend::{model::JobStatus,config::resource_classes::ResourceClass,domain::filmcraft_export_presets::Preset};
    let(state,_dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("preset-export-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();
    let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["width"]=json!(161);sequence["settings"]["height"]=json!(91);sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);
    let mut preset=video_kadr_backend::domain::filmcraft_export_presets::builtins().remove(0);
    preset.name="Saved review".into();preset.settings["frameSize"]=Value::Null;preset.settings["scale"]=json!(0.5);preset.settings["frameRate"]=json!({"num":12,"den":1});preset.settings["includeAudio"]=json!(false);
    state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.save(Preset{builtin:false,..preset},true)).await.unwrap();
    let mut permits=Vec::new();for _ in 0..state.resource_limit(ResourceClass::Export){permits.push(state.acquire_job_slot(ResourceClass::Export).await.unwrap());}
    let app=support::router(state.clone());
    let payload=json!({"document":document,"sequenceId":"6","startFrames":"0","frames":2,"sources":{},"libraryPreset":"Saved review"});
    let response=app.oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
    let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
    state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.delete("Saved review")).await.unwrap();drop(permits);
    let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
    assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();
    let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_streams","-of","json"]).arg(state.outputs_dir().join(result["filename"].as_str().unwrap())).output().await.unwrap();assert!(probe.status.success());
    let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["width"],80);assert_eq!(probe["streams"][0]["height"],46);assert_eq!(probe["streams"][0]["r_frame_rate"],"12/1");
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_library_caption_burn_matches_original_frame_and_default_omits_captions(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use std::io::Read;
    use video_kadr_backend::{model::JobStatus,domain::{filmcraft_source_pixels,transition_kernel_pack::{context::RenderContext,image::Image}}};
    let(state,_dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("caption-preset-owner","test password").await.unwrap().unwrap();
    let fixture:Value=serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-caption-sequence.json")).unwrap()).unwrap();
    assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let case=&fixture["cases"][0];assert_eq!(case["scale"],1.0);assert_eq!(case["at"],0);
    let mut original=Image::new(80,64);original.px=case["bits"].as_array().unwrap().iter().map(|bits|f32::from_bits(bits.as_u64().unwrap()as u32)).collect();
    let context=RenderContext::new_with_budget(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),2,80*64,1024*1024).unwrap();
    let expected=filmcraft_source_pixels::encode_srgb(&context,&original).unwrap().surface().rgba.clone();
    let plain=[255u8,0,0,255].repeat(80*64);assert_ne!(expected,plain);
    let app=support::router(state.clone());
    for burn in [false,true] {
        let mut preset=video_kadr_backend::domain::filmcraft_export_presets::builtins().into_iter().find(|preset|preset.settings["format"]=="png").unwrap();
        preset.name=format!("Caption burn {burn}");
        if burn {preset.settings["burnCaptions"]=json!(true);}else{assert_eq!(preset.settings["burnCaptions"],false);}
        let name=preset.name.clone();state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.save(preset,true)).await.unwrap();
        let payload=json!({"document":case["document"],"sequenceId":"6","startTicks":"0","frames":1,"sources":{},"libraryPreset":name});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
        let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");
        let result=job.result.unwrap();let mut zip=zip::ZipArchive::new(std::fs::File::open(state.outputs_dir().join(result["filename"].as_str().unwrap())).unwrap()).unwrap();assert_eq!(zip.len(),1);
        let mut bytes=Vec::new();zip.by_index(0).unwrap().read_to_end(&mut bytes).unwrap();let decoded=image::load_from_memory(&bytes).unwrap().into_rgba8();assert_eq!(decoded.dimensions(),(80,64));assert_eq!(decoded.as_raw(),if burn{&expected}else{&plain},"burn={burn}");
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_library_h264_levels_match_automatic_floor_and_explicit_output(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;
    let owner=state.db.register_auth_user("level-preset-owner","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);let app=support::router(state.clone());
    for source_name in ["Match Source – Adaptive High Bitrate","Vimeo 1080p Full HD","MXF OP1a H.264"] {
        let mut automatic=None;
        for level in [None,Some(1u8),Some(41)] {
            let mut preset=video_kadr_backend::domain::filmcraft_export_presets::builtins().into_iter().find(|preset|preset.name==source_name).unwrap();
            preset.name=format!("Level {source_name} {level:?}");preset.settings["frameSize"]=json!([64,48]);preset.settings["includeAudio"]=json!(false);preset.settings["h264Level"]=json!(level);
            let name=preset.name.clone();state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.save(preset,true)).await.unwrap();
            let payload=json!({"document":document,"sequenceId":"6","startTicks":"0","frames":2,"sources":{},"libraryPreset":name});
            let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();
            let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");
            let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{source_name} {level:?}: {job:?}");
            let result=job.result.unwrap();let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_streams","-of","json"]).arg(state.outputs_dir().join(result["filename"].as_str().unwrap())).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();let actual=probe["streams"][0]["level"].as_i64().unwrap();
            match level {None=>{assert!(actual>1&&actual<=41);automatic=Some(actual)},Some(1)=>assert_eq!(Some(actual),automatic,"too-low level must rise to original automatic floor"),Some(41)=>assert_eq!(actual,41),_=>unreachable!()}
        }
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn saved_library_caption_sidecars_keep_first_track_offset_tail_and_owner_access(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::graphic_template_fonts::base64_decode};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("sidecar-owner","test password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("sidecar-outsider","test password").await.unwrap().unwrap();
    let fixture:Value=serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-caption-sequence.json")).unwrap()).unwrap();let mut document=fixture["cases"][0]["document"].clone();let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);
    sequence["caption_tracks"][0]["enabled"]=json!(false);sequence["caption_tracks"][0]["captions"]=json!([
        {"id":881,"start":381024000000i64,"duration":254016000000i64,"text":"Hello sidecar","settings":""},
        {"id":885,"start":2540160000000i64,"duration":508032000000i64,"text":"After export end","settings":""},
        {"id":886,"start":0,"duration":254016000000i64,"text":"Ends at offset","settings":""}
    ]);
    let app=support::router(state.clone());
    for kind in ["srt","vtt","stl"]{
        let mut preset=video_kadr_backend::domain::filmcraft_export_presets::builtins().into_iter().find(|preset|preset.settings["format"]=="wav").unwrap();preset.name=format!("Sidecar {kind}");preset.settings["captionSidecar"]=json!(kind);let name=preset.name.clone();state.db.edit_filmcraft_export_presets(&owner.user.username,|library|library.save(preset,true)).await.unwrap();
        let payload=json!({"document":document,"sequenceId":"6","startTicks":"254016000000","frames":2,"sources":{},"libraryPreset":name});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{accepted}");let job_id=accepted["jobId"].as_str().unwrap();
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(job_id).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();assert_eq!(result["container"],"wav");assert_eq!(result["captionSidecar"]["extension"],kind);
        let bytes=base64_decode(result["captionSidecar"]["bytesBase64"].as_str().unwrap()).unwrap();
        if kind=="srt"{let text=String::from_utf8(bytes.clone()).unwrap();assert!(text.contains("00:00:00,500 --> 00:00:01,500"));assert!(text.contains("00:00:09,000 --> 00:00:11,000"));assert!(!text.contains("Ends at offset"));assert!(!text.contains("Top <i>track</i>"));}
        if kind=="vtt"{assert!(bytes.starts_with(b"WEBVTT"));}if kind=="stl"{assert!(bytes.len()>=1024);}
        let own=app.clone().oneshot(Request::builder().uri(format!("/api/jobs/{job_id}")).header("authorization",format!("Bearer {}",owner.token)).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(own.status(),200);let own:Value=serde_json::from_slice(&to_bytes(own.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(own["result"]["captionSidecar"],result["captionSidecar"]);
        let foreign=app.clone().oneshot(Request::builder().uri(format!("/api/jobs/{job_id}")).header("authorization",format!("Bearer {}",outsider.token)).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(foreign.status(),403);
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn exact_source_range_exports_sample_exact_audio_grid_video_and_raw_caption_offset(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::{model::JobStatus,domain::graphic_template_fonts::base64_decode};
    let(state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("exact-range-owner","test password").await.unwrap().unwrap();
    let fixture:Value=serde_json::from_slice(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-caption-sequence.json")).unwrap()).unwrap();let mut document=fixture["cases"][0]["document"].clone();let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);
    sequence["caption_tracks"][0]["captions"]=json!([{"id":881,"start":508032000000i64,"duration":254016000000i64,"text":"Exact offset","settings":""}]);
    let app=support::router(state.clone());
    for(start,end,audio_frames,video_frames,caption_clock)in [
        (381278016000i64,402446016000i64,4000usize,"3","00:00:00,499 --> 00:00:01,499"),
        (10160639999i64,10165932000i64,2usize,"2","00:00:01,960 --> 00:00:02,960"),
    ]{
    for preset in ["wav24","prores_proxy","adaptive_high"]{
        let mut payload=json!({"document":document,"sequenceId":"6","sources":{},"sourceRange":{"startTicks":start.to_string(),"endTicks":end.to_string()},"preset":preset,"withAudio":true,"burnCaptions":false,"captionSidecar":"srt","audio":{"codec":"pcm","bits":24,"sampleRate":48000,"channels":2}});
        if preset!="wav24"{payload["frameSize"]=json!([64,48]);payload["frameRate"]=json!({"numerator":25,"denominator":1});}if preset=="adaptive_high"{payload["multiplexer"]=json!("mov");}
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();let status=response.status();let accepted:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();assert_eq!(status,202,"{preset}: {accepted}");
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{preset}: {job:?}");let result=job.result.unwrap();let file=state.outputs_dir().join(result["filename"].as_str().unwrap());
        let decoded=tokio::process::Command::new("ffmpeg").args(["-v","error","-i"]).arg(&file).args(["-map","0:a:0","-f","f32le","-acodec","pcm_f32le","pipe:1"]).output().await.unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),audio_frames*2*4,"{preset}: exact original audio range");
        let sidecar=String::from_utf8(base64_decode(result["captionSidecar"]["bytesBase64"].as_str().unwrap()).unwrap()).unwrap();assert!(sidecar.contains(caption_clock),"{sidecar}");
        if preset!="wav24"{let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-count_frames","-show_streams","-of","json"]).arg(file).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["nb_read_frames"],video_frames);assert_eq!(probe["streams"][0]["r_frame_rate"],"25/1");}
        payload["frames"]=json!(1);let rejected=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/video-jobs").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(payload.to_string())).unwrap()).await.unwrap();assert_eq!(rejected.status(),400);
    }
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

#[tokio::test]
async fn quick_export_uses_in_out_remembers_only_accepted_presets_and_isolates_owners(){
    use axum::{body::{Body,to_bytes},http::Request};use tower::ServiceExt;use video_kadr_backend::model::JobStatus;
    let(state,_dir)=support::make_state(false,false).await;let alice=state.db.register_auth_user("quick-alice","test password").await.unwrap().unwrap();let bob=state.db.register_auth_user("quick-bob","test password").await.unwrap().unwrap();
    let mut document:Value=serde_json::from_slice(&source().encode_preserved().unwrap()).unwrap();let sequence=&mut document["project"]["items"]["6"]["kind"]["Sequence"];sequence["settings"]["frame_rate"]=json!({"num":24,"den":1});sequence["settings"]["width"]=json!(64);sequence["settings"]["height"]=json!(48);sequence["video_tracks"]=json!([]);sequence["audio_tracks"]=json!([]);sequence["submix_tracks"]=json!([]);sequence["caption_tracks"]=json!([]);sequence["mark_in"]=json!(10584000000i64);sequence["mark_out"]=json!(21168000000i64);
    let app=support::router(state.clone());
    async fn submit(app:axum::Router,token:Option<&str>,body:Value)->(u16,Value){let mut request=Request::builder().method("POST").uri("/api/compositions/filmcraft/quick-export").header("content-type","application/json");if let Some(token)=token{request=request.header("authorization",format!("Bearer {token}"));}let response=app.oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();let status=response.status().as_u16();let body=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();(status,body)}
    let base=json!({"document":document,"sequenceId":"6","sources":{}});
    assert_eq!(submit(app.clone(),None,base.clone()).await.0,401);
    for(token,preset,expected)in [(&alice.token,Some("Waveform Audio 48 kHz 16-bit"),"Waveform Audio 48 kHz 16-bit"),(&alice.token,None,"Waveform Audio 48 kHz 16-bit"),(&bob.token,None,"Match Source – Adaptive High Bitrate")]{
        let mut body=base.clone();if let Some(preset)=preset{body["preset"]=json!(preset);}
        let(status,accepted)=submit(app.clone(),Some(token),body).await;assert_eq!(status,202,"{accepted}");assert_eq!(accepted["presetName"],expected);
        let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(accepted["jobId"].as_str().unwrap()).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{job:?}");let result=job.result.unwrap();
        if expected.starts_with("Waveform"){assert_eq!(result["container"],"wav");assert!((result["duration"].as_f64().unwrap()-4000./48000.).abs()<1e-12);}else{assert_eq!(result["container"],"mp4");assert!((result["duration"].as_f64().unwrap()-2./24.).abs()<1e-12);}
        let file=state.outputs_dir().join(result["filename"].as_str().unwrap());
        let select=if expected.starts_with("Waveform"){ "a:0" }else{ "v:0" };
        let probe=tokio::process::Command::new("ffprobe").args(["-v","error","-select_streams",select,"-count_frames","-show_streams","-of","json"]).arg(file).output().await.unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();
        if expected.starts_with("Waveform"){assert_eq!(probe["streams"][0]["duration_ts"],4000);}else{assert_eq!(probe["streams"][0]["nb_read_frames"],"2");}
        if preset.is_some(){let mut invalid=base.clone();invalid["preset"]=json!("missing preset");assert_eq!(submit(app.clone(),Some(token),invalid).await.0,400);}
    }
    state.begin_shutdown();assert!(state.wait_for_tasks(std::time::Duration::from_secs(5)).await);
}

