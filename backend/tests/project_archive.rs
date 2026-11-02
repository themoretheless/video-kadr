//! HTTP round-trip coverage for portable `.veproj` composition archives.

mod support;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use video_kadr_backend::library::MediaEntry;
use video_kadr_backend::project_archive::ARCHIVE_MEDIA_TYPE;
use video_kadr_backend::tools;

use support::{assert_api_error, make_state, router};

const BOUNDARY: &str = "VEPROJ_API_BOUNDARY";

fn tiny_wav() -> Vec<u8> {
    let sample_rate = 8_000_u32;
    let samples = vec![128_u8; 800];
    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36_u32 + samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&8_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(samples.len() as u32).to_le_bytes());
    wav.extend_from_slice(&samples);
    wav
}

fn multipart_archive(bytes: &[u8]) -> Request<Body> {
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"portable.veproj\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/vnd.video-editor.project\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/composition-projects/import")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

fn authenticated(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    request
}

fn in_space(mut request: Request<Body>, space_id: &str) -> Request<Body> {
    request
        .headers_mut()
        .insert("x-space-id", space_id.parse().unwrap());
    request
}

fn document(source_id: &str) -> Value {
    json!({
        "schemaVersion": 1,
        "timeBase": 1_000_000,
        "sources": {
            (source_id): {
                "id": source_id,
                "kind": "audio",
                "durationTicks": 100_000,
                "width": 0,
                "height": 0,
                "hasAudio": true
            }
        },
        "tracks": [{
            "kind": "audio",
            "id": "dialogue",
            "name": "Dialogue",
            "clips": [{
                "id": "voice-clip",
                "sourceId": source_id,
                "timelineStartTicks": 0,
                "sourceInTicks": 0,
                "sourceOutTicks": 100_000
            }]
        }]
    })
}

async fn local_probe_available(state: &video_kadr_backend::state::AppState) -> bool {
    tools::check_tool(&state.process_runtime, "ffmpeg", "-version")
        .await
        .0
        && tools::check_tool(&state.process_runtime, "ffprobe", "-version")
            .await
            .0
}

#[tokio::test]
async fn composition_project_archive_http_round_trip_relinks_sources_and_metadata() {
    archive_roundtrip(false, false).await;
}

#[tokio::test]
async fn nested_project_archive_http_roundtrip_preserves_instances_and_relinks_child_media() {
    archive_roundtrip(true, false).await;
}

#[tokio::test]
async fn nested_archive_relinks_physical_library_binding_without_renaming_logical_slots() {
    archive_roundtrip(true, true).await;
}

async fn archive_roundtrip(nested: bool, bound: bool) {
    let (state, directory) = make_state(true, false).await;
    if !local_probe_available(&state).await {
        eprintln!("skipping .veproj HTTP roundtrip: ffmpeg/ffprobe not on PATH");
        return;
    }
    let old_id = "portable-source";
    let old_filename = "portable-source.wav";
    let media = tiny_wav();
    tokio::fs::write(state.sources_dir().join(old_filename), &media)
        .await
        .unwrap();
    assert!(
        state
            .library
            .add(MediaEntry {
                id: old_id.into(),
                kind: "source".into(),
                filename: old_filename.into(),
                storage_key: None,
                url: format!("/files/sources/{old_filename}"),
                media_type: Some("audio".into()),
                original_filename: Some("voice_0001.wav".into()),
                title: Some("Imported voice".into()),
                duration: Some(0.1),
                width: None,
                height: None,
                fps: None,
                vcodec: None,
                acodec: Some("pcm_s16le".into()),
                size_bytes: Some(media.len() as u64),
                created_at: 1,
            })
            .await
    );
    state
        .db
        .replace_library_metadata(
            old_id,
            Some("Portable dialogue".into()),
            true,
            vec!["dialogue".into(), "approved".into()],
        )
        .await
        .unwrap();
    let copy_id = "portable-source-copy";
    let mut copy_entry = state.library.get(old_id).await.unwrap();
    copy_entry.id = copy_id.into();
    copy_entry.filename = "portable-copy.wav".into();
    copy_entry.url = "/files/sources/portable-copy.wav".into();
    tokio::fs::write(state.sources_dir().join(&copy_entry.filename), &media).await.unwrap();
    assert!(state.library.add(copy_entry).await);
    let session = state
        .db
        .register_auth_user("archive-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let mut project_document = if nested {
        use video_kadr_backend::domain::composition::{CanvasSpec,Composition,TransformSpec};
        let root_content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
        let mut child_content = root_content.clone();
        child_content["sources"] = document(old_id)["sources"].clone();
        child_content["tracks"] = json!([{ "kind": "audio","id": "dialogue","name": "Dialogue","muted": false,"locked": false,"clips": [{ "id": "voice-clip","sourceId": old_id,"placement": { "timelineStartTick": 0,"sourceInTick": 0,"sourceOutTick": 100000,"speed": 1 },"gain": { "mode": "constant","value": 1 },"pan": { "mode": "constant","value": 0 },"fadeInTicks": 0,"fadeOutTicks": 0 }] }]);
        json!({ "schemaVersion": 1,"root": "root","compositions": {
            "root": { "content": root_content,"instances": [{ "id": "group","timing": { "composition": "child","timelineStartUs": 0,"sourceInUs": 0,"sourceOutUs": 100000 },"transform": TransformSpec::default(),"opacity": { "mode": "constant","value": 0.5 },"blendMode": "normal" }] },
            "child": { "content": child_content,"instances": [] }
        } })
    } else { document(old_id) };
    if nested { project_document["projectMedia"]=json!({old_id:{"source":project_document["compositions"]["child"]["content"]["sources"][old_id],"name":"Dialogue slot","placeholder":true,"concatMetadata":{"audioCodec":"pcm_u8","audioTracks":[{"index":0,"codec":"pcm_u8","channels":1,"sampleRate":8000,"title":"Dialogue"}],"origin":"speech"}}}); }
    if bound {
        project_document["projectMedia"][old_id]["librarySourceId"]=json!(old_id);
        let mut alias=project_document["projectMedia"][old_id].clone();
        alias["source"]["id"]=json!("second-slot");alias["name"]=json!("Independent slot");
        project_document["projectMedia"]["second-slot"]=alias;
    }
    let content = if nested { &mut project_document["compositions"]["child"]["content"] } else { &mut project_document };
    let mut copy_source = content["sources"][old_id].clone();
    copy_source["id"] = json!(copy_id);
    content["sources"][copy_id] = copy_source;
    let mut linked=content["tracks"][0]["clips"][0].clone();linked["id"]=json!("linked-voice");
    content["tracks"].as_array_mut().unwrap().push(json!({"kind":"audio","id":"linked-track","name":"Linked","muted":false,"locked":false,"clips":[linked]}));
    content["metadata"] = json!({"clipLinks":{"voice-link":["voice-clip","linked-voice"]},"mediaVersions":{"voice-clip":{"originalSourceId":old_id,"copySourceId":copy_id}}});
    if nested {
        project_document["timelineOrder"] = json!(["root", "child"]);
        let text = format!("{}LUT_1D_SIZE 2\n0 0 0\n1 1 1\n", format!("#{}\n", "x".repeat(1000)).repeat(3300));
        project_document["luts"] = json!([{ "id": "shared", "name": "Portable LUT", "sourcePath": "identity.cube", "lut": { "format": "cube", "text": text, "intensity": 100 } }]);
        assert!(serde_json::to_vec(&project_document).unwrap().len() > 3 * 1024 * 1024);
    }
    let saved = state
        .db
        .create_owned_composition_project(
            "Portable edit",
            &project_document,
            &[old_id.into(), copy_id.into()],
            &session.user.username,
        )
        .await
        .unwrap();
    let target_space = state
        .db
        .create_space(&session.user.username, "Archive team")
        .await
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("archive-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let app = router(state.clone());

    let export = app
        .clone()
        .oneshot(authenticated(
            Request::builder()
                .uri(format!("/api/composition-projects/{}/archive", saved.id))
                .body(Body::empty())
                .unwrap(),
            &session.token,
        ))
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    assert_eq!(export.headers()[header::CONTENT_TYPE], ARCHIVE_MEDIA_TYPE);
    assert_eq!(
        export.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"portable-edit.veproj\""
    );
    let archive = to_bytes(export.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(&archive[..8], b"VEPROJ\r\n");
    let archive_text = String::from_utf8_lossy(&archive);
    assert!(!archive_text.contains(directory.path().to_string_lossy().as_ref()));
    assert!(!archive_text.contains("/files/sources"));
    assert!(!archive_text.contains(&saved.id));

    let rejected = app
        .clone()
        .oneshot(in_space(
            authenticated(multipart_archive(&archive), &outsider.token),
            &target_space.id,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
    assert_eq!(state.library.list().await.len(), 2);

    let imported = app
        .clone()
        .oneshot(in_space(
            authenticated(multipart_archive(&archive), &session.token),
            &target_space.id,
        ))
        .await
        .unwrap();
    assert_eq!(imported.status(), StatusCode::CREATED);
    let body: Value = serde_json::from_slice(
        &to_bytes(imported.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let new_id = body["sourceMapping"][old_id].as_str().unwrap();
    assert_ne!(new_id, old_id);
    assert_eq!(body["project"]["name"], "Portable edit");
    assert_eq!(body["project"]["spaceId"], target_space.id);
    let new_copy = body["sourceMapping"][copy_id].as_str().unwrap();
    assert_ne!(new_copy, copy_id);
    assert_ne!(new_copy, new_id);
    let ids = body["project"]["sourceIds"].as_array().unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&json!(new_id)) && ids.contains(&json!(new_copy)));
    let restored = if nested {
        let logical=if bound{old_id}else{new_id};
        if bound {
            assert_eq!(body["project"]["document"]["projectMedia"][old_id]["librarySourceId"],new_id);
            assert_eq!(body["project"]["document"]["projectMedia"]["second-slot"]["librarySourceId"],new_id);
            assert_eq!(body["project"]["document"]["projectMedia"]["second-slot"]["source"]["id"],"second-slot");
        } else {assert!(body["project"]["document"]["projectMedia"].get(old_id).is_none());}
        assert_eq!(body["project"]["document"]["projectMedia"][logical]["source"]["id"],logical);
        assert_eq!(body["project"]["document"]["projectMedia"][logical]["placeholder"],true);
        assert_eq!(body["project"]["document"]["projectMedia"][logical]["concatMetadata"],project_document["projectMedia"][old_id]["concatMetadata"]);
        assert_eq!(body["project"]["document"]["timelineOrder"], project_document["timelineOrder"]);
        assert_eq!(body["project"]["document"]["luts"], project_document["luts"]);
        assert_eq!(body["project"]["document"]["compositions"]["root"]["instances"], project_document["compositions"]["root"]["instances"]);
        &body["project"]["document"]["compositions"]["child"]["content"]
    } else { &body["project"]["document"] };
    let logical=if bound{old_id}else{new_id};
    assert!(restored["sources"].get(logical).is_some());
    assert_eq!(restored["sources"][logical]["id"], logical);
    assert_eq!(restored["tracks"][0]["clips"][0]["sourceId"], logical);

    assert_eq!(restored["metadata"]["clipLinks"]["voice-link"],json!(["voice-clip","linked-voice"]));
    assert_eq!(restored["tracks"][1]["clips"][0]["sourceId"],logical);
    assert_eq!(restored["metadata"]["mediaVersions"]["voice-clip"], json!({"originalSourceId":logical,"copySourceId":new_copy}));
    assert!(restored["sources"].get(new_copy).is_some());
    let imported_copy = state.library.get(new_copy).await.unwrap();
    assert_eq!(tokio::fs::read(state.library.resolve_media_path(&imported_copy).await.unwrap()).await.unwrap(), media);
    let imported_entry = state.library.get(new_id).await.unwrap();
    assert_eq!(imported_entry.media_type.as_deref(), Some("audio"));
    assert_eq!(imported_entry.original_filename.as_deref(), Some("voice_0001.wav"));
    assert_eq!(
        imported_entry.storage_key.as_deref(),
        Some(format!("spaces/{}/{}", target_space.id, imported_entry.filename).as_str())
    );
    assert_eq!(
        tokio::fs::read(
            state
                .library
                .resolve_media_path(&imported_entry)
                .await
                .unwrap()
        )
        .await
        .unwrap(),
        media
    );
    let metadata = state
        .db
        .get_library_metadata(new_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metadata.title.as_deref(), Some("Portable dialogue"));
    assert!(metadata.favorite);
    assert_eq!(metadata.tags, ["dialogue", "approved"]);
    assert!(tokio::fs::read_dir(state.staging_dir())
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn project_archive_import_rejects_invalid_magic_before_probe_or_publish() {
    let (state, _directory) = make_state(true, false).await;
    let session = state
        .db
        .register_auth_user("invalid-archive-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let app = router(state.clone());
    let unauthorized = app
        .clone()
        .oneshot(multipart_archive(b"not-a-veproj"))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let response = app
        .oneshot(authenticated(
            multipart_archive(b"not-a-veproj"),
            &session.token,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_api_error(&body, "bad_request");
    assert!(state.library.list().await.is_empty());
    assert!(state
        .db
        .list_composition_projects()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn project_archive_routes_are_cors_preflight_enabled() {
    let (state, _directory) = make_state(false, false).await;
    let app = router(state);
    for (uri, method) in [
        ("/api/composition-projects/import", "POST"),
        ("/api/composition-projects/project-id/archive", "GET"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri(uri)
                    .header(header::ORIGIN, "http://localhost:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "http://localhost:5173"
        );
        assert!(response.headers()[header::ACCESS_CONTROL_ALLOW_METHODS]
            .to_str()
            .unwrap()
            .split(',')
            .map(str::trim)
            .any(|allowed| allowed == method));
    }
}

#[tokio::test]
async fn deleted_original_root_project_roundtrips_without_sources_or_ffprobe() {
    let (state, _directory) = make_state(false, false).await;
    let session = state.db.register_auth_user("deleted-root-owner", "shared test password").await.unwrap().unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/concat-merge-preview.json")).unwrap();
    let document = &fixture["removedTimelines"]["root"];
    let saved = state.db.create_owned_composition_project("Remaining cut", document, &[], &session.user.username).await.unwrap();
    let app = router(state.clone());
    let response = app.clone().oneshot(authenticated(Request::builder().uri(format!("/api/composition-projects/{}/archive", saved.id)).body(Body::empty()).unwrap(), &session.token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap();
    let response = app.oneshot(authenticated(multipart_archive(&archive), &session.token)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4 * 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(body["project"]["document"], *document);
    assert_eq!(body["project"]["sourceIds"], json!([]));
    assert_eq!(body["sourceMapping"], json!({}));
    assert!(state.library.list().await.is_empty());
}
