mod support;
use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;
use video_kadr_backend::{
    domain::composition::{CanvasSpec, Composition},
    library::MediaEntry,
};
#[tokio::test]
async fn saved_native_template_download_excludes_missing_slot_files_and_cleans_streams() {
    let (state, dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("template-owner", "template test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("template-outsider", "template test password")
        .await
        .unwrap()
        .unwrap();
    let path = state.sources_dir().join("design.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([25, 50, 75, 255]))
        .save(&path)
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: "design-file".into(),
                kind: "source".into(),
                filename: "design.png".into(),
                storage_key: None,
                url: "/files/sources/design.png".into(),
                title: None,
                original_filename: None,
                media_type: Some("image".into()),
                duration: None,
                width: Some(16),
                height: Some(8),
                fps: None,
                vcodec: None,
                acodec: None,
                size_bytes: Some(std::fs::metadata(&path).unwrap().len()),
                created_at: 1
            })
            .await
    );
    let source = |id: &str| json!({"id":id,"kind":"image","durationTicks":0,"width":16,"height":8,"hasAudio":false});
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    content["canvas"]["width"] = json!(16);
    content["canvas"]["height"] = json!(8);
    content["tracks"] = json!([{"id":"pictures","kind":"image","name":"Pictures","clips":[{"id":"stand-in","sourceId":"slot","timelineStartTick":0,"durationTicks":2000000,"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}]);
    let subtitle_documents = json!([{"id":"dialogue","name":"dialogue.srt","text":"1\n00:00:00,000 --> 00:00:01,000\nDialogue"}]);
    content["metadata"] = json!({"subtitleDocuments":subtitle_documents});
    content["sources"] = json!({"slot":source("slot"),"design":source("design")});
    let document = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}},"projectMedia":{"slot":{"source":source("slot"),"librarySourceId":"missing-original","name":"Replace","placeholder":true},"design":{"source":source("design"),"librarySourceId":"design-file","name":"Design"}}});
    let saved = state
        .db
        .create_owned_composition_project(
            "Reusable edit",
            &document,
            &["design-file".into(), "missing-original".into()],
            &owner.user.username,
        )
        .await
        .unwrap();
    let reopened = state.db.get_composition_project_for(&saved.id, &owner.user.username).await.unwrap().unwrap();
    assert_eq!(reopened.document.pointer("/compositions/root/content/metadata/subtitleDocuments"), Some(&subtitle_documents));
    let app = support::router(state.clone());
    let mut portable = document.clone();
    let caption = json!({
        "id":"dialogue-caption", "timelineStartTick":1000000, "timelineEndTick":2000000,
        "text":"Dialogue", "textAnchor":[0.5,1.0], "textBoxWidth":14,
        "style":{"fontFamily":"Noto Sans","fontSize":1,"align":"center",
            "color":{"red":1,"green":1,"blue":1,"alpha":1},
            "background":{"red":0,"green":0,"blue":0,"alpha":0},
            "stroke":{"red":0,"green":0,"blue":0,"alpha":0},"strokeWidth":0,
            "shadow":{"red":0.2,"green":0.3,"blue":0.4,"alpha":0.5},"shadowX":2,"shadowY":-1},
        "transform":video_kadr_backend::domain::composition::TransformSpec::default(),
        "opacity":{"mode":"constant","value":1}, "enabled":true
    });
    let mut reflected_caption = caption.clone();
    reflected_caption["id"] = json!("reflected-caption");
    reflected_caption["timelineStartTick"] = json!(2_000_000);
    reflected_caption["timelineEndTick"] = json!(3_000_000);
    reflected_caption["textAnchor"] = json!([0.5, 0.5]);
    reflected_caption["style"]["tracking"] = json!(2.0);
    reflected_caption["style"]["lineHeight"] = json!(1.75);
    reflected_caption["style"]["backgroundRadius"] = json!(2.0);
    reflected_caption["style"]["backgroundPadding"] = json!([2.0, 1.0]);
    reflected_caption["style"]["shadow"]["alpha"] = json!(0.0);
    reflected_caption["style"]["concatShadow"] = json!(true);
    reflected_caption["style"]["fontWeight"] = json!(550.5);
    reflected_caption["style"]["fontFamily"] = json!("Inter");
    reflected_caption["textBoxHeight"] = json!(4.0);
    reflected_caption["transform"]["concatPreparation"] = json!({
        "crop": [0.0, 0.0, 0.0, 0.0], "canvas": [16, 8], "flipH": true, "flipV": true
    });
    portable["compositions"]["root"]["content"]["tracks"] = json!([
        {"id":"dialogue-lane","kind":"text","name":"Dialogue","clips":[caption, reflected_caption]}
    ]);
    portable["compositions"]["root"]["content"]["metadata"]["captionTextTracks"] = json!(["dialogue-lane"]);
    use base64::{engine::general_purpose::STANDARD, Engine};
    let mut cover = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(16, 8, image::Rgb([25, 50, 75])))
        .write_to(&mut cover, image::ImageFormat::Jpeg).unwrap();
    portable["coverJpeg"] = json!(STANDARD.encode(cover.into_inner()));
    let font = include_bytes!("../vendor/concat-text/fonts/Inter-Regular.ttf");
    portable["fonts"] = json!([{"family": "Inter", "data": STANDARD.encode(font)}]);
    let concat = state.db.create_owned_composition_project(
        "Portable Concat", &portable, &["design-file".into(), "missing-original".into()],
        &owner.user.username,
    ).await.unwrap();
    let reloaded = state.db.get_composition_project_for(&concat.id, &owner.user.username).await.unwrap().unwrap();
    assert_eq!(reloaded.document["coverJpeg"], portable["coverJpeg"]);
    let concat_uri = format!("/api/composition-projects/{}/concat-template", concat.id);
    let concat_request = |token: &str| Request::get(&concat_uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty()).unwrap();
    let denied = app.clone().oneshot(concat_request(&outsider.token)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::NOT_FOUND);
    let response = app.clone().oneshot(concat_request(&owner.token)).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 5);
        let packaged_subtitles: serde_json::Value = serde_json::from_reader(zip.by_name("video-kadr-subtitles.json").unwrap()).unwrap();
        assert_eq!(packaged_subtitles, subtitle_documents);
        let mut manifest_bytes = Vec::new();
        zip.by_name("template.json").unwrap().read_to_end(&mut manifest_bytes).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
        let sidecar: serde_json::Value = serde_json::from_reader(zip.by_name("video-kadr-caption-clips.json").unwrap()).unwrap();
        use sha2::{Digest, Sha256};
        assert_eq!(sidecar["manifestSha256"], format!("{:x}", Sha256::digest(&manifest_bytes)));
        assert_eq!(sidecar["clips"][0][0], "root");
        assert_eq!(sidecar["clips"][0][2]["clip"], manifest["timelines"][0]["clips"][0]);
        assert_eq!(sidecar["clips"][0][2]["video"], manifest["timelines"][0]["video"]);
        assert_eq!(sidecar["clips"][0][2]["uniqueClipId"], true);
        let saved_caption = &sidecar["clips"][0][1];
        assert_eq!(saved_caption["style"]["shadow"], caption["style"]["shadow"]);
        assert_eq!(saved_caption["style"]["shadowX"].as_f64(), Some(2.0));
        assert_eq!(saved_caption["style"]["shadowY"].as_f64(), Some(-1.0));
        assert_eq!(manifest["timelines"][0]["clips"][0]["text"]["shadow"], true);
        let reflected = &manifest["timelines"][0]["clips"][1];
        assert_eq!(reflected["id"], "reflected-caption");
        assert_eq!(reflected["flipH"], true);
        assert_eq!(reflected["flipV"], true);
        assert_eq!(reflected["text"]["tracking"], 0.25);
        assert_eq!(reflected["text"]["lineHeight"], 1.75);
        assert_eq!(reflected["text"]["backgroundRadius"], 0.25);
        assert_eq!(reflected["text"]["backgroundPaddingX"], 0.25);
        assert_eq!(reflected["text"]["backgroundPaddingY"], 0.125);
        assert_eq!(reflected["text"]["shadow"], true);
        assert_eq!(reflected["text"]["fontWeight"], 550.5);
        assert_eq!(reflected["text"]["fontFamily"], "Inter");
        assert_eq!(reflected["text"]["maxHeight"], 0.5);
        assert_eq!(reflected["crop"], json!({"left": 0.0, "top": 0.0, "right": 0.0, "bottom": 0.0}));
        assert_eq!(sidecar["clips"][1][1]["transform"]["concatPreparation"], reflected_caption["transform"]["concatPreparation"]);
        assert_eq!(manifest["name"], "Portable Concat");
        assert_eq!(manifest["video"], manifest["timelines"][0]["video"]);
        let media = manifest["media"].as_array().unwrap();
        let slot = media.iter().find(|item| item["id"] == "slot").unwrap();
        assert_eq!(slot["path"], "");
        assert_eq!(slot["placeholder"], true);
        let design = media.iter().find(|item| item["id"] == "design").unwrap();
        let mut contents = Vec::new();
        zip.by_name(design["path"].as_str().unwrap()).unwrap().read_to_end(&mut contents).unwrap();
        assert_eq!(contents, std::fs::read(&path).unwrap());
        let font_path = manifest["fonts"][0]["path"].as_str().unwrap().to_owned();
        assert_eq!(manifest["fonts"][0]["family"], "Inter");
        let mut packaged_font = Vec::new();
        zip.by_name(&font_path).unwrap().read_to_end(&mut packaged_font).unwrap();
        assert_eq!(packaged_font.as_slice(), font);
        let imported = video_kadr_backend::domain::concat_project_import::import(
            serde_json::from_value(json!({
                "schemaVersion": 1,
                "document": manifest,
                "mediaBindings": {"slot": source("slot"), "design": source("design")},
                "fontBindings": {font_path: {"data": STANDARD.encode(packaged_font)}},
                "preserveMediaIds": true
            })).unwrap()
        ).unwrap();
        assert_eq!(imported.status, "converted", "{:?}", imported.diagnostics);
        let restored = imported.project.unwrap();
        assert_eq!(serde_json::to_value(restored.fonts).unwrap(), portable["fonts"]);
    }
    let saved_cover_zip = app.clone().oneshot(
        Request::get(format!("{concat_uri}?useSavedCover=true"))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(saved_cover_zip.status(), StatusCode::OK);
    let bytes = to_bytes(saved_cover_zip.into_body(), 1024 * 1024).await.unwrap();
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut jpeg = Vec::new();
        zip.by_name("poster.jpg").unwrap().read_to_end(&mut jpeg).unwrap();
        assert_eq!(STANDARD.encode(jpeg), portable["coverJpeg"].as_str().unwrap());
    }
    let conflicting = app.clone().oneshot(
        Request::get(format!("{concat_uri}?useSavedCover=true&posterTimeUs=0"))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(conflicting.status(), StatusCode::BAD_REQUEST);
    let poster_zip = app.clone().oneshot(
        Request::get(format!("{concat_uri}?posterTimeUs=0&posterCompositionId=root"))
            .header(header::COOKIE, format!("video_kadr_session={}", owner.token))
            .body(Body::empty()).unwrap()
    ).await.unwrap();
    let status = poster_zip.status();
    let bytes = to_bytes(poster_zip.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    {
        use std::io::Read;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut jpeg = Vec::new();
        zip.by_name("poster.jpg").unwrap().read_to_end(&mut jpeg).unwrap();
        assert_eq!(image::guess_format(&jpeg).unwrap(), image::ImageFormat::Jpeg);
        let poster = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        assert!(poster.width() <= 320 && poster.height() <= 180);
        assert_eq!(poster.get_pixel(poster.width() / 2, poster.height() / 2).0, [0, 0, 0]);
    }
    let native_cover_uri = format!("/api/composition-projects/{}/template?useSavedCover=true", concat.id);
    let response = app.clone().oneshot(
        Request::post(&native_cover_uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let entry: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(entry["hasPoster"], true);
    let response = app.clone().oneshot(
        Request::get(format!("{native_cover_uri}&posterTimeUs=0"))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let response = app.clone().oneshot(
        Request::get(format!("/api/template-library/{}/poster", entry["id"].as_str().unwrap()))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let cover = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap().to_rgb8();
    assert_eq!(cover.dimensions(), (16, 8));
    let original = STANDARD.decode(portable["coverJpeg"].as_str().unwrap()).unwrap();
    assert_eq!(cover, image::load_from_memory(&original).unwrap().to_rgb8());

    let upload_cover = |uri: &str, image: &[u8]| {
        let mut body = b"--cover\r\nContent-Disposition: form-data; name=\"poster\"; filename=\"cover.png\"\r\nContent-Type: image/png\r\n\r\n".to_vec();
        body.extend_from_slice(image);
        body.extend_from_slice(b"\r\n--cover--\r\n");
        Request::post(uri)
            .header(header::COOKIE, format!("video_kadr_session={}", owner.token))
            .header(header::CONTENT_TYPE, "multipart/form-data; boundary=cover")
            .body(Body::from(body)).unwrap()
    };
    let normalization_uri = "/api/composition-projects/concat-template-cover";
    let mut large_jpeg = original[..2].to_vec();
    let marker = [0xff, 0xef, 0xff, 0xff];
    for _ in 0..7 {
        large_jpeg.extend_from_slice(&marker);
        large_jpeg.resize(large_jpeg.len() + 65_533, 0);
    }
    large_jpeg.extend_from_slice(&original[2..]);
    assert!(large_jpeg.len() > 384 * 1024);
    let mut normalize = upload_cover(normalization_uri, &large_jpeg);
    normalize.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", owner.token).parse().unwrap());
    let response = app.clone().oneshot(normalize).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let normalized: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    let jpeg = STANDARD.decode(normalized["coverJpeg"].as_str().unwrap()).unwrap();
    assert!(jpeg.len() <= 384 * 1024);
    let normalized = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).unwrap().to_rgb8();
    assert_eq!(normalized.dimensions(), cover.dimensions());
    assert!(normalized.as_raw().iter().zip(cover.as_raw()).all(|(left, right)| left.abs_diff(*right) <= 3));

    let png = std::fs::read(&path).unwrap();
    let response = app.clone().oneshot(upload_cover(&concat_uri, &png)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/zip");
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut jpeg = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name("poster.jpg").unwrap(), &mut jpeg).unwrap();
    let poster = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
        .unwrap().to_rgb8();
    assert_eq!(poster.dimensions(), (16, 8));
    for (actual, expected) in poster.get_pixel(8, 4).0.into_iter().zip([25_u8, 50, 75]) {
        assert!(actual.abs_diff(expected) <= 3);
    }
    assert_eq!(
        app.clone().oneshot(upload_cover(&format!("{concat_uri}?useSavedCover=true"), &png))
            .await.unwrap().status(),
        StatusCode::BAD_REQUEST,
    );
    let native_upload_uri = format!("/api/composition-projects/{}/template-cover", saved.id);
    let response = app.clone().oneshot(upload_cover(&format!("{native_upload_uri}?download=true&name=Custom"), &png)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().contains("custom.vktemplate"));
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let upload_stage = dir.path().join("uploaded-cover");
    std::fs::create_dir(&upload_stage).unwrap();
    let uploaded = video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &upload_stage).unwrap();
    assert_eq!(uploaded.manifest.name, "Custom");
    assert_eq!(
        image::load_from_memory(uploaded.poster_png.as_ref().unwrap()).unwrap().to_rgba8(),
        image::load_from_memory(&png).unwrap().to_rgba8(),
    );
    drop(uploaded);
    assert_eq!(
        app.clone().oneshot(upload_cover(&format!("{native_upload_uri}?download=true"), b"invalid PNG")).await.unwrap().status(),
        StatusCode::BAD_REQUEST,
    );
    assert_eq!(
        app.clone().oneshot(upload_cover(&native_upload_uri, &png)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED,
    );
    let mut save_cover = upload_cover(&format!("{native_upload_uri}?name=Custom+library+cover"), &png);
    save_cover.headers_mut().insert(header::AUTHORIZATION, format!("Bearer {}", owner.token).parse().unwrap());
    let response = app.clone().oneshot(save_cover).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let uploaded_entry: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(uploaded_entry["hasPoster"], true);

    let uri = format!("/api/composition-projects/{}/template", saved.id);
    let request = |token: Option<&str>| {
        let mut r = Request::get(&uri);
        if let Some(token) = token {
            r = r.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        r.body(Body::empty()).unwrap()
    };
    assert_eq!(
        app.clone().oneshot(request(None)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(request(Some(&outsider.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    let response = app
        .clone()
        .oneshot(request(Some(&owner.token)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/vnd.video-kadr.template"
    );
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .contains(".vktemplate"));
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let stage = dir.path().join("parsed-template");
    std::fs::create_dir(&stage).unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    let project = serde_json::to_value(&parsed.manifest.project).unwrap();
    assert_eq!(project["compositions"]["root"]["content"]["metadata"]["subtitleDocuments"], subtitle_documents);
    assert_eq!(
        std::fs::read(parsed.assets[0].path()).unwrap(),
        std::fs::read(&path).unwrap()
    );
    assert!(parsed.manifest.project.project_media
        [&video_kadr_backend::domain::composition::SourceId::parse("slot").unwrap()]
        .library_source_id
        .is_none());
    drop(parsed);
    let response = app
        .clone()
        .oneshot(
            Request::get(&uri)
                .header(
                    header::COOKIE,
                    format!("video_kadr_session={}", owner.token),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    drop(response);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let poster_uri = format!("{uri}?posterTimeUs=500000&posterCompositionId=root");
    let poster_request = || {
        Request::get(&poster_uri)
            .header(
                header::COOKIE,
                format!("video_kadr_session={}", owner.token),
            )
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(poster_request())
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let auto_report = video_kadr_backend::domain::concat_project_import::import(
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "document": {
                "version": 1, "activeTimelineId": "poster",
                "media": [{"id":"picture","kind":"image","path":"picture.png"}],
                "timelines": [{"id":"poster","video":{"width":16,"height":8,"rateNum":24,"rateDen":1},
                    "tracks":[{"id":"visual"}],
                    "clips":[{"id":"still","kind":"image","mediaId":"picture","trackId":"visual","start":0,"duration":2}]}]
            },
            "mediaBindings": {"picture": source("missing-original")},
            "preserveMediaIds": true
        })).unwrap(),
    ).unwrap();
    assert_eq!(auto_report.status, "converted", "{:?}", auto_report.diagnostics);
    let mut auto_project = auto_report.project.unwrap();
    auto_project.project_media.values_mut().next().unwrap().placeholder = true;
    let auto_root = auto_project.root.clone();
    let auto_saved = state.db.create_owned_composition_project(
        "Automatic cover", &serde_json::to_value(auto_project).unwrap(),
        &["missing-original".into()], &owner.user.username,
    ).await.unwrap();
    let native_auto_uri = format!("/api/composition-projects/{}/template?autoPoster=true&posterCompositionId={auto_root}", auto_saved.id);
    let response = app.clone().oneshot(
        Request::get(&native_auto_uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let data = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed = video_kadr_backend::template_bundle::read(std::io::Cursor::new(data), &stage).unwrap();
    assert!(parsed.poster_png.is_none());
    drop(parsed);

    let automatic = || Request::get(format!("/api/composition-projects/{}/concat-template?autoPoster=true&posterCompositionId={auto_root}", auto_saved.id))
        .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
        .body(Body::empty()).unwrap();
    let response = app.clone().oneshot(automatic()).await.unwrap();
    let status = response.status();
    let zip = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&zip));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    assert!(archive.by_name("poster.jpg").is_err());

    let original_path = state.sources_dir().join("original.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([0, 0, 0, 255]))
        .save(&original_path)
        .unwrap();
    let mut original_entry = state.library.get("design-file").await.unwrap();
    original_entry.id = "missing-original".into();
    original_entry.filename = "original.png".into();
    original_entry.url = "/files/sources/original.png".into();
    original_entry.size_bytes = Some(std::fs::metadata(&original_path).unwrap().len());
    assert!(state.library.add(original_entry).await);
    let response = app.clone().oneshot(automatic()).await.unwrap();
    let status = response.status();
    let zip = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&zip));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    assert!(archive.by_name("poster.jpg").is_err());

    image::RgbaImage::from_pixel(16, 8, image::Rgba([12, 90, 160, 255]))
        .save(&original_path)
        .unwrap();
    let response = app.clone().oneshot(automatic()).await.unwrap();
    let status = response.status();
    let zip = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&zip));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut jpeg = Vec::new();
    std::io::Read::read_to_end(&mut archive.by_name("poster.jpg").unwrap(), &mut jpeg).unwrap();
    let image = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg)
        .unwrap().to_rgb8();
    assert_eq!(image.dimensions(), (480, 240));
    for (actual, expected) in image.get_pixel(240, 120).0.into_iter().zip([12, 90, 160]) {
        assert!(actual.abs_diff(expected) <= 3);
    }

    let response = app.clone().oneshot(
        Request::get(&native_auto_uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let data = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed = video_kadr_backend::template_bundle::read(std::io::Cursor::new(data), &stage).unwrap();
    let poster = parsed.poster_png.as_ref().unwrap();
    let frame = image::load_from_memory_with_format(poster, image::ImageFormat::Png).unwrap().to_rgba8();
    assert_eq!(frame.dimensions(), (320, 160));
    assert_eq!(frame.get_pixel(160, 80).0, [12, 90, 160, 255]);
    let response = app.clone().oneshot(
        Request::post(&native_auto_uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let entry: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(entry["hasPoster"], true);
    let response = app.clone().oneshot(
        Request::get(format!("/api/template-library/{}/poster", entry["id"].as_str().unwrap()))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(to_bytes(response.into_body(), 1 << 20).await.unwrap().as_ref(), poster);
    drop(parsed);

    let video_path = state.sources_dir().join("automatic-video.mkv");
    let generated = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-f", "lavfi",
            "-i", "color=black:s=16x8:r=24:d=2",
            "-vf", "drawbox=color=red:t=fill:enable='gte(t,0.4)'",
            "-c:v", "ffv1"])
        .arg(&video_path).output().unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let mut video = state.library.get("design-file").await.unwrap();
    video.id = "automatic-video".into();
    video.filename = "automatic-video.mkv".into();
    video.url = "/files/sources/automatic-video.mkv".into();
    video.media_type = Some("video".into());
    video.duration = Some(2.0);
    video.size_bytes = Some(std::fs::metadata(&video_path).unwrap().len());
    assert!(state.library.add(video).await);
    let report = video_kadr_backend::domain::concat_project_import::import(
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "document": {
                "version": 1, "activeTimelineId": "video",
                "media": [{"id":"video","kind":"video","path":"automatic-video.mkv"}],
                "timelines": [{"id":"video","video":{"width":16,"height":8,"rateNum":24,"rateDen":1},
                    "tracks":[{"id":"visual"}],
                    "clips":[{"id":"moving","kind":"video","mediaId":"video","trackId":"visual","start":0,"duration":2}]}]
            },
            "mediaBindings": {"video": {"id":"automatic-video","kind":"video","durationTicks":2000000,"width":16,"height":8,"hasAudio":false}},
            "preserveMediaIds": true
        })).unwrap(),
    ).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let video_project = state.db.create_owned_composition_project(
        "Automatic video cover", &serde_json::to_value(report.project.unwrap()).unwrap(),
        &["automatic-video".into()], &owner.user.username,
    ).await.unwrap();
    let response = app.clone().oneshot(
        Request::get(format!("/api/composition-projects/{}/template?autoPoster=true", video_project.id))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let data = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed = video_kadr_backend::template_bundle::read(std::io::Cursor::new(data), &stage).unwrap();
    let frame = image::load_from_memory(parsed.poster_png.as_ref().unwrap()).unwrap().to_rgb8();
    let [red, green, blue] = frame.get_pixel(160, 80).0;
    assert!(red > 240 && green < 10 && blue < 10);
    drop(parsed);

    let response = app.clone().oneshot(poster_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    let png = parsed.poster_png.as_ref().unwrap();
    let image = image::load_from_memory(png).unwrap().to_rgba8();
    assert!(image.width() <= 320 && image.height() <= 180);
    assert_eq!(
        image.get_pixel(image.width() / 2, image.height() / 2).0,
        [12, 90, 160, 255]
    );
    drop(parsed);
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    let save_request = || {
        Request::post(format!("{poster_uri}&name=Alternate+design"))
            .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
            .body(Body::empty())
            .unwrap()
    };
    let response = app.clone().oneshot(save_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let saved_template: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(saved_template["hasPoster"], true);
    assert_eq!(saved_template["name"], "Alternate design");
    let download = Request::get(format!(
        "/api/template-library/{}",
        saved_template["id"].as_str().unwrap()
    ))
    .header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
    .body(Body::empty())
    .unwrap();
    let response = app.clone().oneshot(download).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let parsed =
        video_kadr_backend::template_bundle::read(std::io::Cursor::new(bytes), &stage).unwrap();
    assert!(parsed.poster_png.is_some());
    assert_eq!(parsed.manifest.name, "Alternate design");
    drop(parsed);
    assert_eq!(
        app.clone().oneshot(save_request()).await.unwrap().status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        std::fs::read_dir(state.storage.join("template-library"))
            .unwrap()
            .count(),
        4
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        app.oneshot(request(Some(&owner.token)))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(std::fs::read_dir(state.staging_dir()).unwrap().count(), 0);
    assert_eq!(
        state
            .db
            .get_composition_project_for(&saved.id, &owner.user.username)
            .await
            .unwrap()
            .unwrap()
            .document,
        document
    );
}

#[tokio::test]
async fn concat_template_exports_authored_cutout_for_a_replaceable_video() {
    use video_kadr_backend::domain::{
        concat_cutout_import,
        concat_project_import::{import, ImportRequest},
        cutout::SELFIE_MODEL_HASH,
    };
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state.db.register_auth_user("concat-cutout", "concat mask password")
        .await.unwrap().unwrap();
    let input: ImportRequest = serde_json::from_value(json!({
        "schemaVersion":1, "preserveMediaIds":true,
        "document":{"version":1,"media":[{"id":"slot","name":"Replacement","kind":"video","placeholder":true}],
            "timelines":[{"id":"main","video":{"width":16,"height":8,"rateNum":10,"rateDen":1},
                "tracks":[{"id":"video"}],"clips":[{"id":"cut","trackId":"video","mediaId":"slot",
                    "kind":"video","sourceStart":0.2,"duration":0.4}]}]},
        "mediaBindings":{"slot":{"id":"unavailable-original","kind":"video","durationTicks":1000000,
            "width":16,"height":8,"hasAudio":false}}
    })).unwrap();
    let report = import(input).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let project = report.project.unwrap();
    let root = project.root.clone();
    let mut document = serde_json::to_value(project).unwrap();
    document["compositions"][&root]["content"]["tracks"][0]["clips"][0]["concatCutout"] = json!({
        "sourceHash":"1".repeat(64), "runtimeHash":"2".repeat(64), "modelHash":SELFIE_MODEL_HASH,
        "subject":"person", "correction":{"custom":true,"feather":0.04,"strokes":[
            {"tool":"brush","size":0.1,"points":[[0.25,0.5]],"at":0.45},
            {"tool":"eraser","size":0.08,"points":[[0.75,0.5]]}
        ]}
    });
    let saved = state.db.create_owned_composition_project(
        "Masked Concat slot", &document, &["unavailable-original".into()], &owner.user.username,
    ).await.unwrap();
    let response = support::router(state).oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", saved.id,
    )).header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
        .body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 1);
    let template: serde_json::Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    assert_eq!(template["media"][0]["path"], "");
    assert_eq!(template["media"][0]["placeholder"], true);
    let cutout = &template["timelines"][0]["clips"][0]["cutout"];
    let settings = concat_cutout_import::plan(cutout).unwrap();
    assert_eq!(settings.subject, concat_cutout_import::Subject::Person);
    assert!(settings.correction.custom);
    assert_eq!(settings.correction.feather, 0.04);
    assert_eq!(settings.correction.strokes[0].at, Some(0.25));
    assert_eq!(settings.correction.strokes[0].points, vec![[0.25, 0.5]]);
    assert_eq!(settings.correction.strokes[1].at, None);
    assert!(cutout.get("sourceHash").is_none());
    assert!(cutout.get("runtimeHash").is_none());
    let filled = video_kadr_backend::domain::concat_media_slots::fill(
        &template, "slot", &json!({
            "path":"library:replacement","name":"New video","kind":"video",
            "duration":0.3,"width":16,"height":8,"hasAudio":false,
        }),
    ).unwrap();
    let clip = &filled["timelines"][0]["clips"][0];
    assert_eq!(clip["sourceStart"], 0.0);
    assert_eq!(clip["cutout"], *cutout);
}

#[tokio::test]
async fn templates_package_retimed_video_masks_and_dissolve() {
    use video_kadr_backend::domain::{
        concat_cutout_import::plan,
        concat_project_import::{import, ImportRequest},
        cutout::SELFIE_MODEL_HASH,
    };
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state.db.register_auth_user("fixed-cutout", "fixed mask password").await.unwrap().unwrap();
    let file = b"fixed video asset";
    std::fs::write(state.sources_dir().join("fixed.mkv"), file).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source", &json!({
        "id":"fixed-file","filename":"fixed.mkv","url":"/files/sources/fixed.mkv","mediaType":"video",
    }))).await);
    let request: ImportRequest = serde_json::from_value(json!({
        "schemaVersion":1, "preserveMediaIds":true,
        "document":{"version":1,"media":[{"id":"fixed","name":"Video","kind":"video"}],
            "timelines":[{"id":"main","video":{"width":16,"height":8,"rateNum":10,"rateDen":1},
                "tracks":[{"id":"video"}],"clips":[
                    {"id":"cut","trackId":"video","mediaId":"fixed","kind":"video",
                     "sourceStart":0.2,"duration":0.4,"speedCurve":[{"at":0.0,"speed":0.5},{"at":1.0,"speed":1.5}]},
                    {"id":"next","trackId":"video","mediaId":"fixed","kind":"video","start":0.4,
                     "sourceStart":0.3,"duration":0.4,"speedCurve":[{"at":0.0,"speed":1.5},{"at":1.0,"speed":0.5}],
                     "transitionIn":{"id":"concat.dissolve","duration":0.2}}
                ]}]},
        "mediaBindings":{"fixed":{"id":"fixed-file","kind":"video","durationTicks":1000000,
            "width":16,"height":8,"hasAudio":false}}
    })).unwrap();
    let project = import(request).unwrap().project.unwrap();
    let root = project.root.clone();
    let mut document = serde_json::to_value(project).unwrap();
    document["compositions"][&root]["content"]["tracks"][0]["clips"][0]["concatCutout"] = json!({
        "sourceHash":"1".repeat(64), "runtimeHash":"2".repeat(64), "modelHash":SELFIE_MODEL_HASH,
        "subject":"person", "correction":{"custom":true,"feather":0.04,"strokes":[
            {"tool":"brush","size":0.1,"points":[[0.25,0.5]],"at":0.45}
        ]}
    });
    let mut slot_document = document.clone();
    slot_document["projectMedia"]["fixed"]["placeholder"] = json!(true);
    let slot_project = serde_json::from_value(slot_document).unwrap();
    let native = video_kadr_backend::template_bundle::write(
        std::io::Cursor::new(Vec::new()), "Masked retimed slot", &slot_project, &[],
    ).unwrap();
    let stage = tempfile::tempdir().unwrap();
    let parsed = video_kadr_backend::template_bundle::read(
        std::io::Cursor::new(native.into_inner()), stage.path(),
    ).unwrap();
    assert!(parsed.assets.is_empty());
    assert!(parsed.video_caches.is_empty());
    assert_eq!(parsed.manifest.slot_masks.len(), 1);
    let recipe = serde_json::to_value(&parsed.manifest.slot_masks[0].recipe).unwrap();
    assert_eq!(recipe["correction"]["strokes"][0]["at"], 0.45);
    let native = serde_json::to_value(&parsed.manifest.project).unwrap();
    let track = &native["compositions"][&root]["content"]["tracks"][0];
    assert!(track["clips"][0]["concatCutout"].is_null());
    assert_eq!(track["clips"][0]["placement"]["concatRetime"]["documentClock"]["speedCurve"],
        json!([{"at":0.0,"speed":0.5},{"at":1.0,"speed":1.5}]));
    assert_eq!(track["transitions"][0]["toClipId"], "next");

    let saved = state.db.create_owned_composition_project(
        "Masked fixed video", &document, &["fixed-file".into()], &owner.user.username,
    ).await.unwrap();
    let response = support::router(state).oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", saved.id,
    )).header(header::AUTHORIZATION, format!("Bearer {}", owner.token)).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 2);
    let template: serde_json::Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    let clip = &template["timelines"][0]["clips"][0];
    assert_eq!(clip["sourceStart"], 0.2);
    assert_eq!(clip["speedCurve"], json!([{"at":0.0,"speed":0.5},{"at":1.0,"speed":1.5}]));
    assert_eq!(plan(&clip["cutout"]).unwrap().correction.strokes[0].at, Some(0.45));
    let incoming = &template["timelines"][0]["clips"][1];
    assert_eq!(incoming["id"], "next");
    assert_eq!(incoming["transitionIn"], json!({"id":"concat.dissolve","duration":0.2}));
    assert_eq!(template["media"][0]["placeholder"], false);
    let mut packaged = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name(template["media"][0]["path"].as_str().unwrap()).unwrap(), &mut packaged).unwrap();
    assert_eq!(packaged, file);

}

#[tokio::test]
async fn concat_template_exports_image_cutout_without_shipping_old_mask_assets() {
    use video_kadr_backend::domain::{
        composition::CompositionClipId,
        concat_cutout_binding::{bind_still, StillAssets},
        concat_cutout_import::{plan, Subject},
        concat_project_import::{import, ImportRequest},
        cutout::SELFIE_MODEL_HASH,
    };
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state.db.register_auth_user("concat-image-mask", "concat image mask password")
        .await.unwrap().unwrap();
    let source = |id: &str, width, height| json!({
        "id":id,"kind":"image","durationTicks":0,"width":width,"height":height,"hasAudio":false,
    });
    let input: ImportRequest = serde_json::from_value(json!({
        "schemaVersion":1, "preserveMediaIds":true,
        "document":{"version":1,"media":[{"id":"slot","name":"Portrait","kind":"image","placeholder":true}],
            "timelines":[{"id":"main","video":{"width":16,"height":8,"rateNum":10,"rateDen":1},
                "tracks":[{"id":"image"}],"clips":[{"id":"cut","trackId":"image","mediaId":"slot",
                    "kind":"image","duration":0.4}]}]},
        "mediaBindings":{"slot":source("unavailable-original",16,8)}
    })).unwrap();
    let mut project = import(input).unwrap().project.unwrap();
    let settings = plan(&json!({"mode":"custom","subject":"person","feather":0.04,"strokes":[
        {"tool":"smartBrush","size":0.1,"points":[[0.25,0.5]],"at":0.2},
        {"tool":"eraser","size":0.08,"points":[[0.75,0.5]]}
    ]})).unwrap();
    use sha2::{Digest, Sha256};
    let prompt_hash = format!("{:x}", Sha256::digest(b"[[0.25,0.5]]"));
    let mut correction = settings.correction.clone();
    correction.strokes[0].region = Some(serde_json::from_value(json!({
        "sourceId":"slot","maskSourceId":"old-sam","sourceHash":"1".repeat(64),
        "maskHash":"3".repeat(64),"maskFileHash":"4".repeat(64),
        "encoderHash":"5".repeat(64),"decoderHash":"6".repeat(64),"runtimeHash":"7".repeat(64),
        "promptHash":prompt_hash,
        "points":[[0.25,0.5]]
    })).unwrap());
    let node = project.compositions.get_mut(&project.root).unwrap();
    let assets = StillAssets {
        original: serde_json::from_value(source("slot",16,8)).unwrap(),
        copy: serde_json::from_value(source("old-copy",16,8)).unwrap(),
        mask: serde_json::from_value(source("old-mask",256,256)).unwrap(),
        regions: vec![serde_json::from_value(source("old-sam",256,256)).unwrap()],
        snapshot: serde_json::from_value(json!({
            "sourceId":"slot","maskSourceId":"old-mask","sourceHash":"1".repeat(64),
            "maskHash":"2".repeat(64),"maskFileHash":"3".repeat(64),
            "modelHash":SELFIE_MODEL_HASH,"runtimeHash":"4".repeat(64),
            "subject":"person","correction":correction,
        })).unwrap(),
    };
    node.content = bind_still(
        &node.content, &CompositionClipId::parse("cut").unwrap(), &settings, &assets,
        &assets.snapshot.source_hash, SELFIE_MODEL_HASH, &assets.snapshot.runtime_hash,
    ).unwrap();
    let original = serde_json::to_value(project).unwrap();
    let saved = state.db.create_owned_composition_project(
        "Masked image slot", &original,
        &["old-copy".into(),"old-mask".into(),"old-sam".into(),"unavailable-original".into()],
        &owner.user.username,
    ).await.unwrap();
    let response = support::router(state.clone()).oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", saved.id,
    )).header(header::AUTHORIZATION, format!("Bearer {}", owner.token))
        .body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 1);
    let template: serde_json::Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    assert_eq!(template["media"].as_array().unwrap().len(), 1);
    assert_eq!(template["media"][0]["path"], "");
    let clip = &template["timelines"][0]["clips"][0];
    assert_eq!(clip["mediaId"], "slot");
    let restored = plan(&clip["cutout"]).unwrap();
    assert_eq!(restored, settings);
    assert_eq!(restored.subject, Subject::Person);
    assert_eq!(restored.region_source_seconds, vec![0.2]);
    assert!(clip["cutout"]["strokes"][0].get("region").is_none());
    let filled = video_kadr_backend::domain::concat_media_slots::fill(
        &template, "slot", &json!({
            "path":"library:portrait","name":"New portrait","kind":"image",
            "width":16,"height":8,"hasAudio":false,
        }),
    ).unwrap();
    assert_eq!(plan(&filled["timelines"][0]["clips"][0]["cutout"]).unwrap(), settings);
    assert_eq!(state.db.get_composition_project_for(&saved.id, &owner.user.username)
        .await.unwrap().unwrap().document, original);
}
