mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::SourceId,
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
};
fn project(physical: &str) -> NestedCompositionProject {
    let media: Vec<_> = ["slot-a", "slot-b"]
        .into_iter()
        .map(|id| json!({"id":id,"kind":"image","name":id,"path":"/inert","placeholder":true}))
        .collect();
    let bindings:serde_json::Map<String,Value>=["slot-a","slot-b"].into_iter().map(|id|(id.into(),json!({"id":id,"kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false}))).collect();
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"document":{"version":1,"media":media,"timelines":[{"id":"timeline","video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"a","trackId":"v","mediaId":"slot-a","kind":"image","start":0.,"duration":1.},{"id":"b","trackId":"v","mediaId":"slot-b","kind":"image","start":1.,"duration":1.}]}]},"mediaBindings":bindings})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let mut project = report.project.unwrap();
    for item in project.project_media.values_mut() {
        item.library_source_id = Some(SourceId::parse(physical).unwrap());
    }
    project.dependency_order().unwrap();
    project
}
#[tokio::test]
async fn logical_aliases_share_one_storage_reference_without_losing_editorial_identity() {
    use std::collections::BTreeMap;
    use video_kadr_backend::domain::{
        concat_project_export::export_with_paths,
        project_media::{remove, set_placeholder},
    };
    let p = project("physical-file");
    let before = serde_json::to_value(&p).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db = video_kadr_backend::db::Db::open(dir.path()).await.unwrap();
    let saved = db
        .create_composition_project("Aliased media", &before, &["physical-file".into()])
        .await
        .unwrap();
    assert_eq!(saved.source_ids, vec!["physical-file"]);
    assert_eq!(saved.document, before);
    let id = SourceId::parse("slot-a").unwrap();
    assert_eq!(p.library_source_id(&id).as_str(), "physical-file");
    let (unmarked, _) = set_placeholder(&p, &id, false).unwrap();
    assert_eq!(
        unmarked.project_media[&id].library_source_id,
        p.project_media[&id].library_source_id
    );
    let output = export_with_paths(
        &p,
        &BTreeMap::from([
            ("slot-a".into(), "/portable/shared.png".into()),
            ("slot-b".into(), "/portable/shared.png".into()),
        ]),
    )
    .unwrap();
    assert_eq!(output["media"].as_array().unwrap().len(), 2);
    assert_eq!(output["media"][0]["id"], "slot-a");
    assert_eq!(output["media"][1]["id"], "slot-b");
    assert!(!output.to_string().contains("librarySourceId"));
    let (remaining, _) = remove(&p, &id).unwrap();
    assert_eq!(remaining.project_media.len(), 1);
    assert_eq!(
        remaining
            .project_media
            .values()
            .next()
            .unwrap()
            .library_source_id
            .as_ref()
            .unwrap()
            .as_str(),
        "physical-file"
    );
    db.create_composition_project(
        "Remaining alias",
        &serde_json::to_value(&remaining).unwrap(),
        &["physical-file".into()],
    )
    .await
    .unwrap();
    let mut bad = p.clone();
    bad.project_media.get_mut(&id).unwrap().source.width = 32;
    assert!(bad.dependency_order().is_err());
    assert_eq!(serde_json::to_value(p).unwrap(), before);
}
#[tokio::test]
async fn preview_authorizes_physical_source_and_renders_both_logical_slots() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    use video_kadr_backend::library::MediaEntry;
    let (state, _dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("alias-owner", "alias test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("alias-outsider", "alias test password")
        .await
        .unwrap()
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([12, 34, 56, 255]))
        .save(state.sources_dir().join(&filename))
        .unwrap();
    state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"image","width":16,"height":16}))).await;
    let p = project(&id);
    let document = serde_json::to_value(&p).unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Private aliases")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Aliased source",
            &document,
            &[id.clone()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = support::router(state.clone());
    for time in [250_000, 1_250_000] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/compositions/nested-preview")
                    .header("content-type", "application/json")
                    .header("accept", "image/png")
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::from(
                        json!({"project":p,"timeUs":time,"renderedOutputs":{}}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let frame = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(frame.dimensions(), (16, 16));
        assert!(frame.pixels().all(|pixel| pixel.0 == [12, 34, 56, 255]));
    }
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/compositions/nested-preview")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", outsider.token))
                .body(Body::from(
                    json!({"project":p,"timeUs":250000,"renderedOutputs":{}}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(state.library.get("slot-a").await.is_none());
    assert!(state.library.get("slot-b").await.is_none());
    let mut export_state = state.clone();
    let mut capabilities = export_state.tools.as_ref().clone();
    capabilities.ffmpeg_encoders.push("ffv1".into());
    capabilities.ffmpeg_muxers.push("matroska".into());
    export_state.tools = std::sync::Arc::new(capabilities);
    let export_app = support::router(export_state.clone());
    let export_request = |token: &str| {
        Request::builder().method("POST").uri("/api/compositions/nested-export").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(json!({"schemaVersion":1,"mode":"video_only","project":p,"compositionId":p.root,"startUs":0,"endUs":2000000,"renderedOutputs":{}}).to_string())).unwrap()
    };
    let response = export_app
        .clone()
        .oneshot(export_request(&outsider.token))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = export_app
        .oneshot(export_request(&owner.token))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let queued: Value = serde_json::from_slice(&bytes).unwrap();
    let job_id = queued["jobId"].as_str().unwrap();
    let job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = export_state.get_job(job_id).await.unwrap().unwrap();
            if job.status.is_terminal() {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owned native export job timeout");
    assert_eq!(
        job.status,
        video_kadr_backend::model::JobStatus::Done,
        "{:?}",
        job.error
    );
    let result = job.result.unwrap();
    let path = export_state
        .outputs_dir()
        .join(result["filename"].as_str().unwrap());
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout.len(), 8 * 16 * 16 * 4);
    for pixel in decoded.stdout.chunks_exact(4) {
        assert_eq!(pixel, [12, 34, 56, 255]);
    }
}
