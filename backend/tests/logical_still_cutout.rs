//! Actual pinned RVM HTTP cutout with separate logical media/library identities.
mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::{
    domain::{
        composition::CompositionClipId,
        concat_cutout_import::plan,
        concat_project_import::{import, ImportRequest},
    },
    library::MediaEntry,
    services::local_person,
};
fn request(
    project: &str,
    revision: u64,
    root: &str,
    token: &str,
    settings: &Value,
) -> Request<Body> {
    Request::builder().method("POST").uri(format!("/api/composition-projects/{project}/concat-cutout")).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"compositionId":root,"clipId":"clip","expectedRevision":revision,"cutout":settings}).to_string())).unwrap()
}
#[tokio::test]
#[ignore = "requires actual pinned RVM, native vision worker and ORT"]
async fn logical_image_slot_analyses_reanalyses_and_reopens_private_cutout_using_physical_source() {
    run_logical_cutout(false).await;
}
#[tokio::test]
#[ignore = "requires actual pinned RVM and SAM models, native vision worker and ORT"]
async fn logical_smart_regions_generate_reuse_and_replace_against_the_physical_image() {
    run_logical_cutout(true).await;
}
async fn run_logical_cutout(smart: bool) {
    let config = local_person::Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_PERSON_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let (state, _dir) = support::make_state(true, false).await;
    let sam = smart.then(|| video_kadr_backend::services::local_sam::Config {
        executable: config.executable.clone(),
        runtime_library: config.runtime_library.clone(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER_MODEL")
            .unwrap()
            .into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER_MODEL")
            .unwrap()
            .into(),
    });
    let state = state.with_person_config(Some(config)).with_sam_config(sam);
    let owner = state
        .db
        .register_auth_user("logical-cutout-owner", "logical cutout password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("logical-cutout-outsider", "logical cutout password")
        .await
        .unwrap()
        .unwrap();
    for id in ["physical", "slot"] {
        let filename = format!("{id}.png");
        let image = image::RgbaImage::from_fn(37, 23, |x, y| {
            if id == "slot" {
                image::Rgba([0, 0, 255, 255])
            } else {
                let n = y * 37 + x;
                image::Rgba([
                    (n % 151) as u8,
                    ((n * 17) % 180) as u8,
                    ((n * 31) % 199) as u8,
                    (n % 256) as u8,
                ])
            }
        });
        image.save(state.sources_dir().join(&filename)).unwrap();
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"image","width":37,"height":23}))).await);
    }
    let physical_hash = format!(
        "{:x}",
        Sha256::digest(std::fs::read(state.sources_dir().join("physical.png")).unwrap())
    );
    let source = json!({"id":"physical","kind":"image","durationTicks":0,"width":37,"height":23,"hasAudio":false});
    let input:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":true,"document":{"version":1,"media":[{"id":"slot","name":"Logical portrait","kind":"image","placeholder":true,"origin":"speech"},{"id":"alias","name":"Other slot","kind":"image","placeholder":true}],"timelines":[{"id":"main","video":{"width":40,"height":24,"rateNum":4,"rateDen":1},"tracks":[{"id":"images"}],"clips":[{"id":"clip","trackId":"images","mediaId":"slot","kind":"image","duration":1.}]},{"id":"other","video":{"width":40,"height":24,"rateNum":4,"rateDen":1},"tracks":[{"id":"images"}],"clips":[{"id":"other","trackId":"images","mediaId":"alias","kind":"image","duration":1.}]}]},"mediaBindings":{"slot":source,"alias":source}})).unwrap();
    let report = import(input).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let mut native = report.project.unwrap();
    let video_source:video_kadr_backend::domain::composition::CompositionSource=serde_json::from_value(json!({"id":"native-video","kind":"video","durationTicks":1_000_000,"width":16,"height":16,"hasAudio":false})).unwrap();
    let video_path = state.sources_dir().join("native-video.nut");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=4:d=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&video_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    state.library.add(MediaEntry::from_result("source",&json!({"id":"native-video","filename":"native-video.nut","url":"/files/sources/native-video.nut","mediaType":"video","width":16,"height":16,"duration":1}))).await;
    let node = native.compositions.get_mut(&native.root).unwrap();
    node.content
        .sources
        .insert(video_source.id.clone(), video_source);
    node.content.tracks.push(serde_json::from_value(json!({"id":"native-lane","name":"Unrelated native video","kind":"video","clips":[{"id":"native-clip","sourceId":"native-video","placement":{"timelineStartTick":2_000_000,"sourceInTick":0,"sourceOutTick":2_000_000,"speed":1,"concatSourceEof":true},"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false}]})).unwrap());
    node.content.validate_native_surface().unwrap();
    assert!(node.content.validate().is_err());
    native.dependency_order().unwrap();
    let document = serde_json::to_value(native).unwrap();
    let root = document["root"].as_str().unwrap().to_owned();
    let space = state
        .db
        .create_space(&owner.user.username, "Private logical image")
        .await
        .unwrap();
    let mut saved = state
        .db
        .create_space_composition_project(
            "Logical cutout",
            &document,
            &["native-video".into(), "physical".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let clip = CompositionClipId::parse("clip").unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut settings = json!({"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"brush","size":0.2,"points":[[0.5,0.5]]}]});
    let forbidden = app
        .clone()
        .oneshot(request(
            &saved.id,
            saved.revision,
            &root,
            &outsider.token,
            &settings,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    if smart {
        settings["strokes"] =
            json!([{"tool":"smartBrush","size":0.2,"points":[[0.25,0.5]],"at":0.25}]);
    }
    Box::pin(qualify_rebuilt_still_import(&state, &owner.token, &space.id, &source, &settings, &physical_hash, smart)).await;
    let mut last_region = String::new();
    for pass in 0..if smart { 3 } else { 2 } {
        if smart && pass == 2 {
            settings["strokes"][0]["points"] = json!([[0.75, 0.5]]);
        }
        settings["feather"] = json!(if pass == 0 { 0.01 } else { 0.03 });
        let revision = saved.revision;
        let before = serde_json::to_value(&saved).unwrap();
        let response = app
            .clone()
            .oneshot(request(&saved.id, revision, &root, &owner.token, &settings))
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let reply: Value = serde_json::from_slice(&bytes).unwrap();
        if !smart && pass == 1 {
            if let Ok(path) = std::env::var("CONCAT_LOGICAL_CUTOUT_HISTORY_OUTPUT") {
                std::fs::write(
                    path,
                    serde_json::to_vec(&json!({"before":before,"after":reply})).unwrap(),
                )
                .unwrap();
            }
        }
        saved = state
            .db
            .get_composition_project_for(&saved.id, &owner.user.username)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.document, reply["document"]);
        assert_eq!(saved.revision, revision + 1);
        assert_eq!(saved.document["projectMedia"], document["projectMedia"]);
        assert_eq!(
            saved.document["compositions"][&root]["content"]["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|track| track["id"] == "native-lane")
                .unwrap(),
            document["compositions"][&root]["content"]["tracks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|track| track["id"] == "native-lane")
                .unwrap()
        );
        for (id, node) in document["compositions"].as_object().unwrap() {
            if id != &root {
                assert_eq!(saved.document["compositions"][id], *node);
            }
        }
        let pair =
            &saved.document["compositions"][&root]["content"]["metadata"]["mediaVersions"]["clip"];
        assert_eq!(pair["originalSourceId"], "slot");
        assert_eq!(pair["cutout"]["sourceId"], "slot");
        assert_eq!(pair["cutout"]["sourceHash"], physical_hash);
        assert_eq!(pair["cutout"]["correction"]["feather"], settings["feather"]);
        if smart {
            let region = &pair["cutout"]["correction"]["strokes"][0]["region"];
            assert_eq!(region["sourceId"], "slot");
            assert_eq!(region["sourceHash"], physical_hash);
            assert_eq!(
                region["encoderHash"],
                video_kadr_backend::services::local_sam::ENCODER_HASH
            );
            assert_eq!(
                region["decoderHash"],
                video_kadr_backend::services::local_sam::DECODER_HASH
            );
            let id = region["maskSourceId"].as_str().unwrap().to_owned();
            if pass == 1 {
                assert_eq!(id, last_region);
            }
            if pass == 2 {
                assert_ne!(id, last_region);
            }
            last_region = id;
        }
        let native: video_kadr_backend::domain::nested_composition::NestedCompositionProject =
            serde_json::from_value(saved.document.clone()).unwrap();
        let logical = video_kadr_backend::domain::composition::SourceId::parse("slot").unwrap();
        let mut replacement = native.project_media[&logical].source.clone();
        replacement.id =
            video_kadr_backend::domain::composition::SourceId::parse("other-physical").unwrap();
        assert!(video_kadr_backend::domain::project_media_fill::fill(&native,&logical,&json!({"path":"library:other-physical","name":"Other source","kind":"image","hasAudio":false,"width":37,"height":23}),&replacement).unwrap_err().to_string().contains("asset rebinding"));
        assert_eq!(serde_json::to_value(&native).unwrap(), saved.document);
        assert!(saved.source_ids.contains(&"physical".into()));
        assert!(
            !saved.source_ids.contains(&"slot".into())
                && !saved.source_ids.contains(&"alias".into())
        );
        let checked = video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(
            &state,
            &owner.user.username,
            &saved.id,
            Some(&root),
            &clip,
            saved.revision,
            &plan(&settings).unwrap(),
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(checked.assets.original.id.as_str(), "slot");
        let physical_entry = state.library.get("physical").await.unwrap();
        let physical_path = state.library.resolve_media_path(&physical_entry).await.unwrap();
        assert_eq!(checked.paths["slot"], physical_path.canonicalize().unwrap());
        for asset in [&checked.assets.copy, &checked.assets.mask] {
            let entry = state.library.get(asset.id.as_str()).await.unwrap();
            assert!(entry
                .storage_key
                .as_deref()
                .unwrap()
                .starts_with(&format!("spaces/{}/", space.id)));
            assert!(state
                .library
                .resolve_media_path(&entry)
                .await
                .unwrap()
                .is_file());
        }
        assert!(
            video_kadr_backend::handlers::concat_cutout_access::resolve_saved_still(
                &state,
                &outsider.user.username,
                &saved.id,
                Some(&root),
                &clip,
                saved.revision
            )
            .await
            .is_err()
        );
        let count = state.library.list().await.len();
        assert_eq!(
            app.clone()
                .oneshot(request(&saved.id, revision, &root, &owner.token, &settings))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(state.library.list().await.len(), count);
    }
    image::RgbaImage::from_pixel(37, 23, image::Rgba([0, 255, 0, 255]))
        .save(state.sources_dir().join("slot.png"))
        .unwrap();
    video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(
        &state,
        &owner.user.username,
        &saved.id,
        Some(&root),
        &clip,
        saved.revision,
        &plan(&settings).unwrap(),
        &cancel,
    )
    .await
    .unwrap();
    let loaded = state
        .db
        .get_composition_project_for(&saved.id, &owner.user.username)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.document, saved.document);
}

async fn qualify_rebuilt_still_import(
    state: &video_kadr_backend::state::AppState,
    token: &str,
    space: &str,
    source: &Value,
    settings: &Value,
    expected_hash: &str,
    smart: bool,
) {
    let app = support::router(state.clone());
    let request = || Request::post("/api/compositions/concat-project/import-rebuild")
        .header("authorization", format!("Bearer {token}"))
        .header("x-space-id", space)
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "name":"Imported still mask", "rebuildStills":true,
            "request":{"schemaVersion":1,"preserveMediaIds":true,
                "document":{"version":1,"media":[{"id":"slot","kind":"image","path":"never-read.png"}],
                    "timelines":[{"id":"main","video":{"width":40,"height":24,"rateNum":4,"rateDen":1},
                        "tracks":[{"id":"image"}],"clips":[{"id":"clip","kind":"image","mediaId":"slot",
                            "trackId":"image","duration":1,"cutout":settings}]}]},
                "mediaBindings":{"slot":source}}
        }).to_string())).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
    ).await.unwrap();
    let before = state.library.list().await.len();
    sqlx::query("CREATE TRIGGER reject_still_import BEFORE INSERT ON composition_projects WHEN NEW.name='Imported still mask' BEGIN SELECT RAISE(ABORT,'forced image mask rollback'); END")
        .execute(&pool).await.unwrap();
    let failed = app.clone().oneshot(request()).await.unwrap();
    let status = failed.status();
    let bytes = to_bytes(failed.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{}", String::from_utf8_lossy(&bytes));
    assert_eq!(state.library.list().await.len(), before);
    sqlx::query("DROP TRIGGER reject_still_import").execute(&pool).await.unwrap();
    let created = app.oneshot(request()).await.unwrap();
    let status = created.status();
    let bytes = to_bytes(created.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let document = &result["project"]["document"];
    let cutout = &document["compositions"]["concat-timeline-0"]["content"]["metadata"]["mediaVersions"]["clip"]["cutout"];
    assert_eq!(cutout["sourceHash"], expected_hash);
    assert_eq!(cutout["correction"]["strokes"][0]["points"], settings["strokes"][0]["points"]);
    assert_eq!(!cutout["correction"]["strokes"][0]["region"].is_null(), smart);
    assert_eq!(document["projectMedia"]["slot"]["librarySourceId"], "physical");
    assert_eq!(state.library.list().await.len(), before + if smart { 3 } else { 2 });
    if smart {
        for replace in [false, true] {
            Box::pin(qualify_sam_still_zip_round_trip(state, token, space, &result, settings, expected_hash, replace)).await;
        }
    }
}

async fn qualify_sam_still_zip_round_trip(
    state: &video_kadr_backend::state::AppState,
    token: &str,
    space: &str,
    original: &Value,
    settings: &Value,
    expected_hash: &str,
    replace: bool,
) {
    let app = support::router(state.clone());
    let project_id = if replace {
        let mut template = original["project"]["document"].clone();
        template["projectMedia"]["slot"]["placeholder"] = json!(true);
        let source_ids: Vec<String> = serde_json::from_value(original["project"]["sourceIds"].clone()).unwrap();
        state.db.create_space_composition_project(
            "Replaceable SAM image", &template, &source_ids, "logical-cutout-owner", space,
        ).await.unwrap().id
    } else {
        original["project"]["id"].as_str().unwrap().to_owned()
    };
    let exported = app.clone().oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", project_id,
    )).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap())
        .await.unwrap();
    let status = exported.status();
    let bytes = to_bytes(exported.into_body(), 4 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), if replace { 1 } else { 2 });
    let manifest: Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    let cutout = &manifest["timelines"][0]["clips"][0]["cutout"];
    assert_eq!(plan(cutout).unwrap(), plan(settings).unwrap());
    assert!(cutout["strokes"][0].get("region").is_none());
    let (manifest, binding, source_hash) = if replace {
        assert_eq!(manifest["media"][0]["placeholder"], true);
        assert_eq!(manifest["media"][0]["path"], "");
        drop(zip);
        let filled = app.clone().oneshot(Request::post("/api/compositions/concat-project/fill-registered-slot")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({"document":manifest,"mediaId":"slot","sourceId":"slot"}).to_string())).unwrap())
            .await.unwrap();
        let status = filled.status();
        let bytes = to_bytes(filled.into_body(), 4 << 20).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
        let filled: Value = serde_json::from_slice(&bytes).unwrap();
        let entry = state.library.get("slot").await.unwrap();
        let path = state.library.resolve_media_path(&entry).await.unwrap();
        let hash = format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()));
        assert_ne!(hash, expected_hash);
        (filled["document"].clone(), filled["source"].clone(), hash)
    } else {
        let mut image = Vec::new();
        std::io::Read::read_to_end(&mut zip.by_name(manifest["media"][0]["path"].as_str().unwrap()).unwrap(), &mut image).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&image)), expected_hash);
        drop(zip);
        std::fs::write(state.sources_dir().join("sam-zip-image.png"), image).unwrap();
        assert!(state.library.add(MediaEntry::from_result("source", &json!({
            "id":"sam-zip-image","filename":"sam-zip-image.png","url":"/files/sources/sam-zip-image.png",
            "mediaType":"image","width":37,"height":23,
        }))).await);
        let binding = json!({"id":"sam-zip-image","kind":"image","durationTicks":0,"width":37,"height":23,"hasAudio":false});
        (manifest, binding, expected_hash.to_owned())
    };
    let response = app.oneshot(Request::post("/api/compositions/concat-project/import-rebuild")
        .header("authorization", format!("Bearer {token}"))
        .header("x-space-id", space)
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "rebuildStills":true,
            "request":{"schemaVersion":1,"preserveMediaIds":true,"document":manifest,
                "mediaBindings":{"slot":binding}}
        }).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let imported = &result["project"];
    assert_eq!(imported["document"]["projectMedia"]["slot"]["librarySourceId"], binding["id"]);
    let region = |saved: &Value| saved["document"]["compositions"]["concat-timeline-0"]["content"]
        ["metadata"]["mediaVersions"]["clip"]["cutout"]["correction"]["strokes"][0]["region"].clone();
    let restored = region(imported);
    let previous = region(&original["project"]);
    assert_eq!(restored["sourceHash"], source_hash);
    assert_eq!(restored["points"], previous["points"]);
    if !replace {
        assert_eq!(restored["maskHash"], previous["maskHash"]);
    }
    assert_ne!(restored["maskSourceId"], previous["maskSourceId"]);
    let render = support::router(state.clone().with_person_config(None).with_sam_config(None));
    let preview = |project: &Value, document: Value| Request::post("/api/compositions/nested-preview")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "project":document,"timeUs":0,"renderedOutputs":{},
            "savedProject":{"projectId":project["id"],"expectedRevision":project["revision"]},
        }).to_string())).unwrap();
    let mut unmasked = imported["document"].clone();
    unmasked["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["sourceId"] = json!("slot");
    let mut frames = Vec::new();
    for (project, document) in [
        (&original["project"], original["project"]["document"].clone()),
        (imported, imported["document"].clone()),
        (imported, unmasked),
    ] {
        let frame = render.clone().oneshot(preview(project, document)).await.unwrap();
        let status = frame.status();
        let pixels = to_bytes(frame.into_body(), 1 << 20).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pixels));
        frames.push(pixels);
    }
    if replace {
        assert_ne!(frames[0], frames[1]);
    } else {
        assert_eq!(frames[0], frames[1]);
    }
    assert_ne!(frames[1], frames[2], "fixture must visibly exercise its cutout");
}
