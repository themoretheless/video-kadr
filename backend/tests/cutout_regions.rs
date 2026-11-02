//! Real learned region -> authored correction -> portable project -> model-free replay.
mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use video_kadr_backend::{
    config::resource_classes::ResourceClass,
    db::SpaceRole,
    library::MediaEntry,
    model::JobStatus,
    services::{local_cutout, local_sam},
    state::AppState,
};
fn request(method: &str, path: &str, token: &str, data: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(data.to_string()))
        .unwrap()
}
async fn value(response: axum::response::Response, expected: StatusCode) -> Value {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, expected, "{}", String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}
async fn terminal(state: &AppState, id: &str) -> video_kadr_backend::model::Job {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
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
    .unwrap()
}
async fn admit(app: &axum::Router, path: &str, token: &str, body: Value) -> String {
    eprintln!("admit {path}");
    value(
        app.clone()
            .oneshot(request("POST", path, token, body))
            .await
            .unwrap(),
        StatusCode::ACCEPTED,
    )
    .await["jobId"]
        .as_str()
        .unwrap()
        .to_owned()
}
#[tokio::test]
#[ignore = "requires native executable, pinned IS-Net/SAM models and ORT for initial generation only"]
async fn authored_regions_survive_archive_rebind_and_replay_with_per_region_access() {
    let executable =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap());
    let ort =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap());
    let object = local_cutout::Config {
        executable: executable.clone(),
        model: std::env::var("VIDEO_KADR_TEST_OBJECT_MODEL")
            .unwrap()
            .into(),
        runtime_library: ort.clone(),
    };
    let sam = local_sam::Config {
        executable: executable.clone(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: ort,
    };
    let (state, _dir) = support::make_state(true, false).await;
    let state = state
        .with_cutout_config(Some(object))
        .with_sam_config(Some(sam));
    let owner = state
        .db
        .register_auth_user("region-owner", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let editor = state
        .db
        .register_auth_user("region-editor", "public synthetic password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Images")
        .await
        .unwrap();
    let region_space = state
        .db
        .create_space(&owner.user.username, "Learned regions")
        .await
        .unwrap();
    let source = state.sources_dir().join("original.png");
    image::RgbaImage::from_fn(37, 23, |x, y| {
        let n = y * 37 + x;
        image::Rgba([
            (n % 151) as u8,
            ((n * 17) % 180) as u8,
            ((n * 31) % 199) as u8,
            (n % 256) as u8,
        ])
    })
    .save(&source)
    .unwrap();
    state.library.add(MediaEntry::from_result("source",&json!({"id":"original","filename":"original.png","url":"/files/sources/original.png","mediaType":"image","width":37,"height":23}))).await;
    let project = state
        .db
        .create_space_composition_project(
            "Authored region",
            &json!({"schemaVersion":1,"sources":{"original":{"id":"original"}}}),
            &["original".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let region_project = state
        .db
        .create_space_composition_project(
            "Regions",
            &json!({"schemaVersion":1,"sources":{}}),
            &[],
            &owner.user.username,
            &region_space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    let base = admit(
        &app,
        "/api/cutout/jobs",
        &owner.token,
        json!({"sourceId":"original"}),
    )
    .await;
    assert_eq!(terminal(&state, &base).await.status, JobStatus::Done);
    let points: Vec<_> = (0..40).map(|i| [i as f64 / 39., 0.3]).collect();
    let sam_job = admit(
        &app,
        "/api/sam/jobs",
        &owner.token,
        json!({"sourceId":"original","points":points}),
    )
    .await;
    assert_eq!(terminal(&state, &sam_job).await.status, JobStatus::Done);
    let region = value(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/sam/jobs/{sam_job}/asset"),
                &owner.token,
                json!({"projectId":region_project.id}),
            ))
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    // Every correction below has no SAM configuration and deliberately missing IS-Net/ORT paths.
    let editing =
        state
            .clone()
            .with_sam_config(None)
            .with_cutout_config(Some(local_cutout::Config {
                executable,
                model: state.storage.join("missing-object-model"),
                runtime_library: state.storage.join("missing-ort"),
            }));
    let app = support::router(editing.clone());
    let recipe = json!({"custom":true,"strokes":[{"tool":"smartBrush","size":0.005,"points":points,"at":null,"region":region["region"]}],"feather":0.0});
    let corrected = admit(
        &app,
        &format!("/api/cutout/jobs/{base}/corrections"),
        &owner.token,
        recipe.clone(),
    )
    .await;
    let corrected_job = terminal(&editing, &corrected).await;
    assert_eq!(
        corrected_job.status,
        JobStatus::Done,
        "{:?}",
        corrected_job.error
    );
    let saved = value(
        app.clone()
            .oneshot(request(
                "GET",
                &format!("/api/cutout/jobs/{corrected}/correction"),
                &owner.token,
                Value::Null,
            ))
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["correction"], recipe);
    let accepted = value(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/cutout/jobs/{corrected}/asset"),
                &owner.token,
                json!({"projectId":project.id}),
            ))
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    let mut expected = recipe.clone();
    expected["strokes"][0]["region"]["maskSourceId"] =
        accepted["cutout"]["regions"][0]["id"].clone();
    assert_eq!(accepted["cutout"]["snapshot"]["correction"], expected);
    assert_ne!(accepted["cutout"]["regions"][0]["id"], region["id"]);
    let mut plain = recipe.clone();
    plain["strokes"][0]
        .as_object_mut()
        .unwrap()
        .remove("region");
    let fallback = admit(
        &app,
        &format!("/api/cutout/jobs/{base}/corrections"),
        &owner.token,
        plain,
    )
    .await;
    let fallback = terminal(&editing, &fallback).await;
    assert_eq!(fallback.status, JobStatus::Done);
    assert_ne!(
        fallback.result.unwrap()["outputHash"],
        corrected_job.result.as_ref().unwrap()["outputHash"]
    );
    // Save a real, fully validated native project with all four source dependencies.
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/concat-merge-preview.json")).unwrap();
    let mut document = fixtures["createdTimeline"].clone();
    document["compositions"]
        .as_object_mut()
        .unwrap()
        .retain(|id, _| id == "root");
    document["timelineOrder"] = json!(["root"]);
    let transform =
        document["compositions"]["root"]["content"]["tracks"][0]["clips"][0]["transform"].clone();
    let content = &mut document["compositions"]["root"]["content"];
    content["sources"] = json!({});
    for entry in [
        &accepted,
        &accepted["cutout"]["source"],
        &accepted["cutout"]["mask"],
        &accepted["cutout"]["regions"][0],
    ] {
        let id = entry["id"].as_str().unwrap();
        content["sources"][id] = json!({"id":id,"kind":"image","durationTicks":0,"width":entry["width"],"height":entry["height"],"hasAudio":false});
    }
    content["tracks"] = json!([{"id":"images","name":"Images","kind":"image","clips":[{"id":"picture","sourceId":accepted["id"],"timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}]);
    content["metadata"] = json!({"mediaVersions":{"picture":{"originalSourceId":"original","copySourceId":accepted["id"],"cutout":accepted["cutout"]["snapshot"]}}});
    document["compositions"]["root"]["durationUs"] = json!(1000000);
    value(app.clone().oneshot(request("PUT",&format!("/api/composition-projects/{}",project.id),&owner.token,json!({"schemaVersion":2,"mode":"composition","name":"Authored region","baseRevision":project.revision,"document":document}))).await.unwrap(),StatusCode::OK).await;
    state
        .db
        .set_space_member(
            &space.id,
            &owner.user.username,
            &editor.user.username,
            SpaceRole::Editor,
        )
        .await
        .unwrap()
        .unwrap();
    state
        .db
        .set_space_member(
            &region_space.id,
            &owner.user.username,
            &editor.user.username,
            SpaceRole::Viewer,
        )
        .await
        .unwrap()
        .unwrap();
    let mut held = Vec::new();
    for _ in 0..editing.resource_limit(ResourceClass::Analysis) {
        held.push(
            editing
                .acquire_job_slot(ResourceClass::Analysis)
                .await
                .unwrap(),
        );
    }
    let queued = admit(
        &app,
        &format!(
            "/api/composition-projects/{}/cutout/corrections",
            project.id
        ),
        &editor.token,
        json!({"compositionId":"root","clipId":"picture","correction":recipe}),
    )
    .await;
    state
        .db
        .remove_space_member(
            &region_space.id,
            &owner.user.username,
            &editor.user.username,
        )
        .await
        .unwrap()
        .unwrap();
    drop(held);
    let refused = terminal(&editing, &queued).await;
    assert_eq!(refused.status, JobStatus::Error);
    assert!(!editing
        .storage
        .join("cutout-jobs")
        .join(format!("{queued}.png"))
        .exists());
    assert!(state
        .db
        .is_space_member(&space.id, &editor.user.username)
        .await
        .unwrap());
    let external_region = editing
        .library
        .resolve_media_path(
            &editing
                .library
                .get(region["id"].as_str().unwrap())
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    std::fs::write(&external_region, b"corrupt old independent region").unwrap();
    std::fs::remove_file(external_region).unwrap();
    let cached = value(
        app.clone()
            .oneshot(request(
                "POST",
                &format!("/api/cutout/jobs/{corrected}/asset"),
                &owner.token,
                json!({"projectId":project.id}),
            ))
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert_eq!(cached["cutout"]["snapshot"], accepted["cutout"]["snapshot"]);
    let exported = app
        .clone()
        .oneshot(request(
            "GET",
            &format!("/api/composition-projects/{}/archive", project.id),
            &owner.token,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    let bytes = to_bytes(exported.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    let destination = state
        .db
        .create_space(&owner.user.username, "Imported")
        .await
        .unwrap();
    let mut body=b"--REGION_ARCHIVE\r\nContent-Disposition: form-data; name=\"file\"; filename=\"regions.veproj\"\r\nContent-Type: application/vnd.video-editor.project\r\n\r\n".to_vec();
    body.extend_from_slice(&bytes);
    body.extend_from_slice(b"\r\n--REGION_ARCHIVE--\r\n");
    let upload = Request::builder()
        .method("POST")
        .uri("/api/composition-projects/import")
        .header("authorization", format!("Bearer {}", owner.token))
        .header("X-Space-Id", &destination.id)
        .header(
            "content-type",
            "multipart/form-data; boundary=REGION_ARCHIVE",
        )
        .body(Body::from(body))
        .unwrap();
    let imported = value(
        app.clone().oneshot(upload).await.unwrap(),
        StatusCode::CREATED,
    )
    .await;
    let snapshot = &imported["project"]["document"]["compositions"]["root"]["content"]["metadata"]
        ["mediaVersions"]["picture"]["cutout"];
    let learned = &snapshot["correction"]["strokes"][0]["region"];
    assert_eq!(learned["sourceId"], imported["sourceMapping"]["original"]);
    assert_eq!(
        learned["maskSourceId"],
        imported["sourceMapping"][accepted["cutout"]["regions"][0]["id"].as_str().unwrap()]
    );
    assert_ne!(learned["maskSourceId"], region["id"]);
    assert_eq!(learned["maskHash"], region["region"]["maskHash"]);
    assert_eq!(learned["points"], json!(points));
    let imported_region_path = editing
        .library
        .resolve_media_path(
            &editing
                .library
                .get(learned["maskSourceId"].as_str().unwrap())
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&imported_region_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600,
            "imported region privacy"
        );
    }
    let replay=admit(&app,&format!("/api/composition-projects/{}/cutout/corrections",imported["project"]["id"].as_str().unwrap()),&owner.token,json!({"compositionId":"root","clipId":"picture","expectedAttachment":snapshot,"correction":snapshot["correction"]})).await;
    let replay = terminal(&editing, &replay).await;
    assert_eq!(replay.status, JobStatus::Done, "{:?}", replay.error);
    assert_eq!(
        replay.result.unwrap()["outputHash"],
        corrected_job.result.unwrap()["outputHash"]
    );
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}
