mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_mask_track::{self, Direction},
    filmcraft_parameter_edit::Address,
    filmcraft_project_file::SourceProjectDocument,
    group_surface::GroupSurface,
    warp_stabilizer::{Affine, Vec2},
};
const FD: i64 = 10_584_000_000;
fn source() -> SourceProjectDocument {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut doc = fixture["cases"][0]["document"].clone();
    let media = &mut doc["project"]["items"]["1"]["kind"]["Media"];
    media["media"] = json!({"File":{"path":"/untrusted/original.mov"}});
    media["info"]["video"]["width"] = json!(240);
    media["info"]["video"]["height"] = json!(180);
    let seq = &mut doc["project"]["items"]["6"]["kind"]["Sequence"];
    seq["settings"]["width"] = json!(240);
    seq["settings"]["height"] = json!(180);
    let clip = &mut seq["video_tracks"][0]["items"][0];
    clip["start"] = json!(0);
    clip["duration"] = json!(FD * 8);
    clip["source_in"] = json!(0);
    clip["speed"] = json!(1.);
    clip["effects"] = json!([{"effect":"opacity","enabled":true,"params":{"opacity":{"value":{"Float":100.}},"blend":{"value":{"Choice":0}}},"masks":[{"name":"Track","mode":"Add","track_method":"PositionScaleRotation","path":{"value":{"Path":{"closed":true,"vertices":[{"p":{"x":40.,"y":30.}},{"p":{"x":200.,"y":30.}},{"p":{"x":200.,"y":150.}},{"p":{"x":40.,"y":150.}}]}}},"feather":{"value":{"Float":0.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}}}]}]);
    SourceProjectDocument::read(&serde_json::to_vec(&doc).unwrap()).unwrap()
}
fn address() -> Address {
    Address {
        sequence_id: "6".into(),
        clip_id: "1010".into(),
        effect_index: 0,
        effect_kind: "opacity".into(),
        mask_index: Some(0),
        parameter: "path".into(),
    }
}
fn clip() -> &'static str {
    "/project/items/6/kind/Sequence/video_tracks/0/items/0"
}
fn frame(pose: Affine) -> GroupSurface {
    let inv = pose.inverse().unwrap();
    let mut rgba = Vec::with_capacity(240 * 180 * 4);
    for y in 0..180 {
        for x in 0..240 {
            let p = inv.apply(Vec2::new(x as f64, y as f64));
            let v = (0.5
                + 0.2 * (p.x * 0.21).sin() * (p.y * 0.17).cos()
                + 0.1 * ((p.x + p.y) * 0.43).sin()
                + 0.08 * (p.x * 0.9 - p.y * 0.6).sin())
            .clamp(0., 1.);
            let q = (v * 255.).round() as u8;
            rgba.extend([q, q, q, 255]);
        }
    }
    GroupSurface {
        width: 240,
        height: 180,
        rgba,
    }
}
fn pose(x: f64, y: f64, rotation: f64, scale: f64) -> Affine {
    Affine::motion(
        Vec2::new(120. + x, 90. + y),
        Vec2::new(scale, scale),
        rotation,
        Vec2::new(120., 90.),
    )
}
#[test]
fn original_three_tracking_methods_write_exact_source_keys_with_measured_geometry() {
    for (method, transform) in [
        ("Position", pose(3., -2., 0., 1.)),
        ("PositionRotation", pose(3., -2., 1., 1.)),
        ("PositionScaleRotation", pose(3., -2., 1., 1.01)),
    ] {
        let mut source = source();
        source.envelope.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0]["track_method"] =
            json!(method);
        let origin = 9007199254740993i64;
        source.envelope.pointer_mut(clip()).unwrap()["source_in"] = json!(origin);
        let plan =
            filmcraft_mask_track::plan(&source, &address(), 0, None, Direction::Forward).unwrap();
        assert_eq!(plan.from, origin);
        assert_eq!(plan.to, Some(origin + FD));
        let before = source.envelope.clone();
        let (edited, inliers) = filmcraft_mask_track::apply(
            &source,
            &address(),
            &plan,
            &frame(Affine::default()),
            &frame(transform),
        )
        .unwrap()
        .expect("textured region must track");
        assert!(inliers >= 3);
        assert_eq!(source.envelope, before);
        let keys = &edited.envelope.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"]
            ["keyframes"];
        assert_eq!(keys[0]["time"].as_i64(), Some(origin));
        assert_eq!(keys[1]["time"].as_i64(), Some(origin + FD));
        for (i, (x, y)) in [(40., 30.), (200., 30.), (200., 150.), (40., 150.)]
            .into_iter()
            .enumerate()
        {
            let expected = transform.apply(Vec2::new(x, y));
            let p = &keys[1]["value"]["Path"]["vertices"][i]["p"];
            let error = (p["x"].as_f64().unwrap() - expected.x)
                .hypot(p["y"].as_f64().unwrap() - expected.y);
            assert!(error < 0.55, "{method} vertex {i} error {error}");
        }
    }
}
#[test]
fn boundaries_reverse_hold_and_feature_loss_never_invent_motion() {
    let mut source = source();
    let end = FD * 8 - 1;
    let plan =
        filmcraft_mask_track::plan(&source, &address(), 0, None, Direction::Backward).unwrap();
    assert!(plan.to.is_none());
    source.envelope.pointer_mut(clip()).unwrap()["reverse"] = json!(true);
    let plan =
        filmcraft_mask_track::plan(&source, &address(), 0, None, Direction::Backward).unwrap();
    assert_eq!(plan.from, end);
    assert_eq!(plan.to, Some(end - FD));
    source.envelope.pointer_mut(clip()).unwrap()["frame_hold"] = json!(FD);
    assert!(
        filmcraft_mask_track::plan(&source, &address(), 0, None, Direction::Forward)
            .unwrap()
            .to
            .is_none()
    );
    source.envelope.pointer_mut(clip()).unwrap()["frame_hold"] = Value::Null;
    source.envelope.pointer_mut(clip()).unwrap()["reverse"] = json!(false);
    let plan =
        filmcraft_mask_track::plan(&source, &address(), 0, None, Direction::Forward).unwrap();
    let flat = GroupSurface {
        width: 240,
        height: 180,
        rgba: vec![128; 240 * 180 * 4],
    };
    assert!(
        filmcraft_mask_track::apply(&source, &address(), &plan, &flat, &flat)
            .unwrap()
            .is_none()
    );
}
async fn post(app: &axum::Router, route: &str, value: Value) -> Value {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}
#[tokio::test]
async fn real_library_movie_tracks_and_renders_without_client_supplied_pixels() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    use video_kadr_backend::library::MediaEntry;
    let (state, dir) = support::make_state(false, false).await;
    let movie = dir.path().join("sources/track.mov");
    let mut encoder = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "240x180",
            "-framerate",
            "24",
            "-i",
            "pipe:0",
            "-c:v",
            "qtrle",
            "-pix_fmt",
            "argb",
            "-y",
        ])
        .arg(&movie)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = encoder.stdin.take().unwrap();
        for i in 0..4 {
            input
                .write_all(&frame(pose(i as f64 * 3., i as f64 * -2., 0., 1.)).rgba)
                .unwrap();
        }
    }
    let output = encoder.wait_with_output().unwrap();
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
                &json!({"id":"tracked","filename":"track.mov","mediaType":"video"})
            ))
            .await
    );
    let app = support::router(state);
    let source = source();
    let mut text = String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let mut cursor = None;
    for step in 1..=2 {
        let mut input = json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":"tracked"},"direction":"forward","timeTicks":"0"});
        if let Some(cursor) = cursor {
            input["cursorTicks"] = json!(cursor);
        }
        let result = post(&app, "/api/compositions/filmcraft/track-mask", input).await;
        assert_eq!(result["tracking"]["status"], "tracked");
        assert_eq!(result["tracking"]["sourceTicks"], (FD * step).to_string());
        cursor = Some((FD * step).to_string());
        text = result["documentText"].as_str().unwrap().into();
    }
    let edited: Value = serde_json::from_str(&text).unwrap();
    let key = &edited.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"]["keyframes"][2];
    assert!(
        (key["value"]["Path"]["vertices"][0]["p"]["x"]
            .as_f64()
            .unwrap()
            - 46.)
            .abs()
            < 0.55
    );
    let actual=post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":text,"sequenceId":"6","timeTicks":(FD*2).to_string(),"sources":{"1":"tracked"}})).await;
    let mut reference = edited.clone();
    reference.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0]["path"] =
        json!({"value":key["value"]});
    let expected=post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":reference.to_string(),"sequenceId":"6","timeTicks":(FD*2).to_string(),"sources":{"1":"tracked"}})).await;
    assert_eq!(actual["rgbaHex"], expected["rgbaHex"]);
}
#[tokio::test]
async fn long_source_mask_paths_render_beyond_the_old_32_key_scalar_admission() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut source = fixture["cases"][0]["document"].clone();
    let path = &mut source.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0];
    path["mode"] = json!("Add");
    let value = path["path"]["value"].clone();
    path["path"]["keyframes"] = json!((0..80)
        .map(|i| json!({"time":i*FD,"value":value,"interp":"Linear"}))
        .collect::<Vec<_>>());
    let actual = post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":source.to_string(),"sequenceId":"6","timeTicks":(FD*40).to_string()}),
    )
    .await;
    source.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0]["path"] = json!({"value":value});
    let expected = post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":source.to_string(),"sequenceId":"6","timeTicks":(FD*40).to_string()}),
    )
    .await;
    assert_eq!(actual["rgbaHex"], expected["rgbaHex"]);
}
#[test]
fn all_original_1960_path_samples_keep_exact_easing_and_topology_after_tracking_admission_change() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-mask-path.json")).unwrap();
    assert_eq!(
        fixture["commit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1960);
    for (index, case) in cases.iter().enumerate() {
        let (path, _) =
            filmcraft_mask_track::tracking_path(&case["parameter"], case["at"].as_i64().unwrap())
                .unwrap();
        assert_eq!(path["Path"]["closed"], case["closed"]);
        let bits = path["Path"]["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|v| {
                [
                    v["p"]["x"].as_f64().unwrap(),
                    v["p"]["y"].as_f64().unwrap(),
                    v["t_in"]["x"].as_f64().unwrap(),
                    v["t_in"]["y"].as_f64().unwrap(),
                    v["t_out"]["x"].as_f64().unwrap(),
                    v["t_out"]["y"].as_f64().unwrap(),
                ]
            })
            .map(f64::to_bits)
            .collect::<Vec<_>>();
        assert_eq!(json!(bits), case["bits"], "original mask path {index}");
    }
}
#[tokio::test]
async fn tracking_refuses_unbound_or_unknown_library_sources_before_publication() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let source = source();
    for sources in [json!({}), json!({"1":"unknown"})] {
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/track-mask").header("content-type","application/json").body(Body::from(json!({"document":String::from_utf8(source.encode_preserved().unwrap()).unwrap(),"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":sources,"direction":"forward","timeTicks":"0"}).to_string())).unwrap()).await.unwrap();
        assert!(response.status().is_client_error());
    }
}

#[tokio::test]
async fn library_still_image_tracks_exact_source_ticks_and_retains_private_source_access() {
    use axum::{body::Body,http::{Request,StatusCode}};
    use tower::ServiceExt;
    use video_kadr_backend::library::MediaEntry;
    let (state,dir)=support::make_state(false,false).await;
    let surface=frame(Affine::default());
    image::save_buffer(dir.path().join("sources/still.png"),&surface.rgba,240,180,image::ColorType::Rgba8).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"still","filename":"still.png","mediaType":"image"}))).await);
    let app=support::router(state.clone());
    let mut source=source();let origin=9007199254740993i64;
    source.envelope["project"]["items"]["1"]["kind"]["Media"]["info"]["kind"]=json!("Image");
    source.envelope.pointer_mut(clip()).unwrap()["source_in"]=json!(origin);
    let text=String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let input=json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":"still"},"direction":"forward","timeTicks":"0"});
    let result=post(&app,"/api/compositions/filmcraft/track-mask",input.clone()).await;
    assert_eq!(result["tracking"]["status"],"tracked");
    assert_eq!(result["tracking"]["sourceTicks"],(origin+FD).to_string());
    let edited:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    let keys=edited.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"]["keyframes"].as_array().unwrap();
    assert_eq!(keys.len(),2);assert_eq!(keys[0]["time"],origin);assert_eq!(keys[1]["time"],origin+FD);
    for (index,(x,y)) in [(40.,30.),(200.,30.),(200.,150.),(40.,150.)].into_iter().enumerate() {
        let p=&keys[1]["value"]["Path"]["vertices"][index]["p"];
        assert!((p["x"].as_f64().unwrap()-x).abs()<1e-6);assert!((p["y"].as_f64().unwrap()-y).abs()<1e-6);
    }
    let actual=post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":result["documentText"],"sequenceId":"6","timeTicks":"0","sources":{"1":"still"}})).await;
    let mut reference=edited.clone();reference.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0]["path"]=json!({"value":keys[0]["value"]});
    let expected=post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":reference.to_string(),"sequenceId":"6","timeTicks":"0","sources":{"1":"still"}})).await;
    assert_eq!(actual["rgbaHex"],expected["rgbaHex"]);
    let owner=state.db.register_auth_user("still-owner","shared test password").await.unwrap().unwrap();
    let outsider=state.db.register_auth_user("still-outsider","shared test password").await.unwrap().unwrap();
    let space=state.db.create_space(&owner.user.username,"Private still").await.unwrap();
    state.db.create_space_composition_project("Still claim",&json!({"schemaVersion":1,"sources":{"still":{"id":"still"}}}),&["still".into()],&owner.user.username,&space.id).await.unwrap();
    let request=||Request::builder().method("POST").uri("/api/compositions/filmcraft/track-mask").header("content-type","application/json").body(Body::from(input.to_string())).unwrap();
    assert_eq!(app.clone().oneshot(request()).await.unwrap().status(),StatusCode::UNAUTHORIZED);
    let mut denied=request();denied.headers_mut().insert("authorization",format!("Bearer {}",outsider.token).parse().unwrap());
    assert_eq!(app.clone().oneshot(denied).await.unwrap().status(),StatusCode::FORBIDDEN);
    let mut allowed=request();allowed.headers_mut().insert("authorization",format!("Bearer {}",owner.token).parse().unwrap());
    assert_eq!(app.oneshot(allowed).await.unwrap().status(),StatusCode::OK);
}

#[tokio::test]
async fn textureless_still_image_reports_lost_without_fabricating_keys() {
    use video_kadr_backend::library::MediaEntry;
    let (state,dir)=support::make_state(false,false).await;
    image::save_buffer(dir.path().join("sources/flat.png"),&vec![128;240*180*4],240,180,image::ColorType::Rgba8).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"flat","filename":"flat.png","mediaType":"image"}))).await);
    let app=support::router(state);let source=source();let text=String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let result=post(&app,"/api/compositions/filmcraft/track-mask",json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":"flat"},"direction":"forward","timeTicks":"0"})).await;
    assert_eq!(result["tracking"]["status"],"lost");assert_eq!(result["tracking"]["more"],false);assert_eq!(result["tracking"]["sourceTicks"],"0");
    assert_eq!(result["documentText"],text);
}

#[tokio::test]
async fn library_still_tracking_accepts_all_enabled_static_image_codecs() {
    use video_kadr_backend::library::MediaEntry;
    let (state,dir)=support::make_state(false,false).await;
    let surface=frame(Affine::default());
    let picture=image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(240,180,surface.rgba).unwrap());
    for extension in ["png","jpg","webp","gif","bmp","tiff","opaque"] {
        let filename=if extension=="opaque" {"opaque-image".into()} else {format!("still-codec.{extension}")};let path=dir.path().join("sources").join(&filename);
        if extension=="jpg" {picture.to_rgb8().save(&path).unwrap()} else if extension=="opaque" {picture.save_with_format(&path,image::ImageFormat::Png).unwrap()} else {picture.save(&path).unwrap()}
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":extension,"filename":filename,"mediaType":"image"}))).await);
    }
    let app=support::router(state);let source=source();let text=String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    for extension in ["png","jpg","webp","gif","bmp","tiff","opaque"] {
        let result=post(&app,"/api/compositions/filmcraft/track-mask",json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":extension},"direction":"forward","timeTicks":"0"})).await;
        assert_eq!(result["tracking"]["status"],"tracked","{extension}");assert_eq!(result["tracking"]["sourceTicks"],FD.to_string());
        let changed:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
        for (index,(x,y)) in [(40.,30.),(200.,30.),(200.,150.),(40.,150.)].into_iter().enumerate() {
            let p=&changed.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"]["keyframes"][1]["value"]["Path"]["vertices"][index]["p"];
            assert!((p["x"].as_f64().unwrap()-x).abs()<1e-6,"{extension}");assert!((p["y"].as_f64().unwrap()-y).abs()<1e-6,"{extension}");
        }
    }
}

#[tokio::test]
async fn invalid_still_inputs_refuse_before_publishing_a_source_document() {
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
    use tower::ServiceExt;
    use video_kadr_backend::library::MediaEntry;
    let (state,dir)=support::make_state(false,false).await;
    std::fs::write(dir.path().join("sources/corrupt.png"),b"not an image").unwrap();
    image::save_buffer(dir.path().join("sources/wide.png"),&vec![0;3841*4],3841,1,image::ColorType::Rgba8).unwrap();
    image::save_buffer(dir.path().join("sources/aspect.png"),&vec![128;120*180*4],120,180,image::ColorType::Rgba8).unwrap();
    std::fs::File::create(dir.path().join("sources/large.png")).unwrap().set_len(64*1024*1024+1).unwrap();
    for name in ["corrupt","wide","aspect","large"] {assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":name,"filename":format!("{name}.png"),"mediaType":"image"}))).await);}
    let app=support::router(state);let source=source();let text=String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    for name in ["corrupt","wide","aspect","large"] {
        let input=json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{"1":name},"direction":"forward","timeTicks":"0"});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/track-mask").header("content-type","application/json").body(Body::from(input.to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::BAD_REQUEST,"{name}");
        let body=to_bytes(response.into_body(),1024*1024).await.unwrap();let error:Value=serde_json::from_slice(&body).unwrap();assert!(error.get("documentText").is_none());
    }
    assert_eq!(String::from_utf8(source.encode_preserved().unwrap()).unwrap(),text);
}

fn generator_source(generator:Value,width:u32,height:u32)->SourceProjectDocument {
    let mut doc=source();
    let media=&mut doc.envelope["project"]["items"]["1"]["kind"]["Media"];
    media["media"]=json!({"Generator":generator});media["offline"]=json!(false);
    media["info"]["video"]["width"]=json!(width);media["info"]["video"]["height"]=json!(height);
    doc
}

#[tokio::test]
async fn all_picture_generators_track_raw_frames_without_library_bindings() {
    use video_kadr_backend::domain::{filmcraft_generator_video,filmcraft_generator_audio::Generator};
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut measured=0;
    for generator in [json!("BarsAndTone"),json!({"ColorMatte":{"color":[0.2,0.4,0.8,1.]}}),json!("BlackVideo"),json!("TransparentVideo"),json!("CountingLeader"),json!({"Demo":"OceanSunset"}),json!({"Demo":"Aurora"}),json!({"Demo":"CityNight"}),json!({"Demo":"Dunes"}),json!({"Demo":"Forest"}),json!({"Demo":"Plasma"})] {
        let doc=generator_source(generator.clone(),240,180);
        let text=String::from_utf8(doc.encode_preserved().unwrap()).unwrap();
        let plan=filmcraft_mask_track::plan(&doc,&address(),0,None,Direction::Forward).unwrap();assert!(plan.generated);
        let generator:Generator=serde_json::from_value(generator).unwrap();
        let rate=filmcraft_time::FrameRate{num:24,den:1};
        let a=filmcraft_generator_video::video_frame(&generator,240,180,rate,0,1.,||false).unwrap();
        let b=filmcraft_generator_video::video_frame(&generator,240,180,rate,FD,1.,||false).unwrap();
        let expected=filmcraft_mask_track::apply(&doc,&address(),&plan,&a.surface,&b.surface).unwrap();
        let result=post(&app,"/api/compositions/filmcraft/track-mask",json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{},"direction":"forward","timeTicks":"0"})).await;
        if let Some((expected,inliers))=expected {
            measured+=1;assert_eq!(result["tracking"]["status"],"tracked");assert_eq!(result["tracking"]["inliers"],inliers);
            assert_eq!(result["documentText"],String::from_utf8(expected.encode_preserved().unwrap()).unwrap());assert_eq!(result["tracking"]["sourceTicks"],FD.to_string());
        } else {assert_eq!(result["tracking"]["status"],"lost");assert_eq!(result["documentText"],text);}
    }
    assert!(measured>0,"at least one detailed generator must track");
}

#[test]
fn generator_tracking_keeps_quantized_odd_dimensions_and_original_raw_gray_analysis() {
    use video_kadr_backend::domain::{filmcraft_generator_video,motion_tracker::{self,Gray,Prepared}};
    let mut measured=0;
    for generator in [json!("CountingLeader"),json!({"Demo":"CityNight"}),json!({"Demo":"Forest"})] {
        let mut doc=generator_source(generator,1101,721);
        doc.envelope.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0]["path"]["value"]=json!({"Path":{"closed":true,"vertices":[{"p":{"x":55.,"y":36.}},{"p":{"x":1045.,"y":36.}},{"p":{"x":1045.,"y":684.}},{"p":{"x":55.,"y":684.}}]}});
        let plan=filmcraft_mask_track::plan(&doc,&address(),0,None,Direction::Forward).unwrap();
        let a=filmcraft_generator_video::source_frame(&doc,1,0,plan.generator_scale(),||false).unwrap();
        let b=filmcraft_generator_video::source_frame(&doc,1,FD,plan.generator_scale(),||false).unwrap();
        assert_eq!((a.surface.width,a.surface.height),(963,631));
        let (path,flat)=filmcraft_mask_track::tracking_path(&doc.envelope.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"],0).unwrap();
        let scale=963./1101.;let region=flat.into_iter().map(|p|p*scale).collect::<Vec<_>>();
        let previous=Prepared::new(Gray::from_rgba8(963,631,&a.surface.rgba));let next=Prepared::new(Gray::from_rgba8(963,631,&b.surface.rgba));
        let original=motion_tracker::track_step(&previous,&next,&region,plan.method);
        let actual=filmcraft_mask_track::apply(&doc,&address(),&plan,&a.surface,&b.surface).unwrap();
        if let Some(step)=original {
            measured+=1;let (actual,inliers)=actual.unwrap();assert_eq!(inliers,step.inliers);
            let vertices=&actual.envelope.pointer(clip()).unwrap()["effects"][0]["masks"][0]["path"]["keyframes"][1]["value"]["Path"]["vertices"];
            for (i,vertex) in path["Path"]["vertices"].as_array().unwrap().iter().enumerate() {
                let p=step.transform.apply(Vec2::new(vertex["p"]["x"].as_f64().unwrap()*scale,vertex["p"]["y"].as_f64().unwrap()*scale));
                assert!((vertices[i]["p"]["x"].as_f64().unwrap()-p.x/scale).abs()<1e-10);
                assert!((vertices[i]["p"]["y"].as_f64().unwrap()-p.y/scale).abs()<1e-10);
            }
        } else {assert!(actual.is_none());}
    }
    assert!(measured>0);
}

#[tokio::test]
async fn generator_tracking_refuses_invalid_or_nonpicture_sources_without_publication() {
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};use tower::ServiceExt;
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut sources=vec![generator_source(json!({"Tone":{"hz":440.,"db":-6.}}),240,180),generator_source(json!({"ColorMatte":{"color":[1e99,0.,0.,1.]}}),240,180),generator_source(json!("Unknown"),240,180),generator_source(json!("BlackVideo"),3841,180)];
    let mut offline=generator_source(json!("BlackVideo"),240,180);offline.envelope["project"]["items"]["1"]["kind"]["Media"]["offline"]=json!(true);sources.push(offline);
    let mut ambiguous=generator_source(json!("BlackVideo"),240,180);ambiguous.envelope["project"]["items"]["1"]["kind"]["Media"]["media"]["File"]=json!({"path":"/untrusted/private.mov"});sources.push(ambiguous);
    for source in sources {
        let text=String::from_utf8(source.encode_preserved().unwrap()).unwrap();
        let input=json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{},"direction":"forward","timeTicks":"0"});
        let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/compositions/filmcraft/track-mask").header("content-type","application/json").body(Body::from(input.to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::BAD_REQUEST);
        let body=to_bytes(response.into_body(),1024*1024).await.unwrap();let error:Value=serde_json::from_slice(&body).unwrap();assert!(error.get("documentText").is_none());
        assert_eq!(String::from_utf8(source.encode_preserved().unwrap()).unwrap(),text);
    }
}

#[tokio::test]
async fn generator_subclips_keep_original_default_frame_rate_and_root_source() {
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let mut doc=generator_source(json!("CountingLeader"),240,180);
    doc.envelope["project"]["items"]["2"]=json!({"id":2,"name":"Subclip","kind":{"Subclip":{"parent":1,"range":{"in":0,"out":FD*8}}}});
    doc.envelope.pointer_mut(clip()).unwrap()["item"]=json!(2);
    let plan=filmcraft_mask_track::plan(&doc,&address(),0,None,Direction::Forward).unwrap();
    let frame_ticks=254016000000i64*1001/24000;
    assert!(plan.generated);assert_eq!(plan.source,1);assert_eq!(plan.to,Some(frame_ticks));
    let text=String::from_utf8(doc.encode_preserved().unwrap()).unwrap();
    let result=post(&app,"/api/compositions/filmcraft/track-mask",json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{},"direction":"forward","timeTicks":"0"})).await;
    if result["tracking"]["status"]=="tracked" {assert_eq!(result["tracking"]["sourceTicks"],frame_ticks.to_string());}
    else {assert_eq!(result["tracking"]["status"],"lost");assert_eq!(result["documentText"],text);}
}

#[tokio::test]
async fn generator_tracking_preserves_unsnapped_signed_clocks_and_ignores_clip_opacity() {
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    for origin in [-FD*2,9007199254740993i64] {
        let mut doc=generator_source(json!("BarsAndTone"),240,180);
        doc.envelope.pointer_mut(clip()).unwrap()["source_in"]=json!(origin);
        doc.envelope.pointer_mut(clip()).unwrap()["effects"][0]["params"]["opacity"]["value"]=json!({"Float":0.});
        let text=String::from_utf8(doc.encode_preserved().unwrap()).unwrap();
        let result=post(&app,"/api/compositions/filmcraft/track-mask",json!({"document":text,"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"sources":{},"direction":"forward","timeTicks":"0"})).await;
        assert_eq!(result["tracking"]["status"],"tracked");assert_eq!(result["tracking"]["sourceTicks"],(origin+FD).to_string());
        let changed:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
        let clip=changed.pointer(clip()).unwrap();let keys=clip["effects"][0]["masks"][0]["path"]["keyframes"].as_array().unwrap();
        assert_eq!(keys.len(),2);assert_eq!(keys[0]["time"],origin);assert_eq!(keys[1]["time"],origin+FD);
        assert_eq!(clip["effects"][0]["params"]["opacity"]["value"]["Float"],0.);
    }
}
