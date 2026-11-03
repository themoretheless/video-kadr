mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    concat_project_import::{import, ImportRequest},
    nested_composition::NestedCompositionProject,
    project_template::inspect,
};
fn project() -> NestedCompositionProject {
    let media: Vec<_> = [
        ("a", "video"),
        ("b", "image"),
        ("unused", "audio"),
        ("ordinary", "image"),
    ]
    .iter()
    .map(|(id, kind)| json!({"id":id,"name":id,"kind":kind,"placeholder":*id!="ordinary"}))
    .collect();
    let bindings:serde_json::Map<String,Value>=[("a","video"),("b","image"),("unused","audio"),("ordinary","image")].iter().map(|(id,kind)|((*id).into(),json!({"id":id,"kind":kind,"durationTicks":if *kind=="image"{0}else{20_000_000},"width":if *kind=="audio"{0}else{16},"height":if *kind=="audio"{0}else{16},"hasAudio":*kind!="image"}))).collect();
    let request:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":true,"document":{"version":1,"media":media,"timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":24,"rateDen":1},"tracks":[{"id":"v","visible":false,"muted":true},{"id":"i"}],"clips":[{"id":"v1","mediaId":"a","kind":"video","trackId":"v","start":4.,"duration":3.,"speed":2.},{"id":"v2","mediaId":"a","kind":"video","trackId":"v","start":9.,"duration":2.,"speed":0.5},{"id":"i1","mediaId":"b","kind":"image","trackId":"i","start":1.,"duration":2.}]},{"id":"other","video":{"width":32,"height":24,"rateNum":24,"rateDen":1},"tracks":[{"id":"v"}],"clips":[{"id":"v3","mediaId":"a","kind":"video","trackId":"v","start":0.,"duration":1.}]}]},"mediaBindings":bindings})).unwrap();
    let report = import(request).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    report.project.unwrap()
}
#[test]
fn slots_follow_active_timeline_first_use_and_sum_authored_not_source_duration() {
    let p = project();
    let before = serde_json::to_value(&p).unwrap();
    let info = inspect(&p, Some("concat-timeline-0")).unwrap();
    assert_eq!(
        info.slots
            .iter()
            .map(|s| s.media_id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a", "unused"]
    );
    assert_eq!(
        info.slots.iter().map(|s| s.seconds).collect::<Vec<_>>(),
        [2., 5., 0.]
    );
    let info = inspect(&p, Some("concat-timeline-1")).unwrap();
    assert_eq!((info.width, info.height), (32, 24));
    assert_eq!(info.slots[0].media_id.as_str(), "a");
    assert_eq!(info.slots[0].seconds, 1.);
    assert_eq!(serde_json::to_value(p).unwrap(), before);
}
#[test]
fn default_root_empty_slots_and_invalid_documents() {
    let mut p = project();
    assert_eq!(inspect(&p, None).unwrap().composition_id, p.root);
    assert!(inspect(&p, Some("missing")).is_err());
    for item in p.project_media.values_mut() {
        item.placeholder = false
    }
    assert!(inspect(&p, None).unwrap().slots.is_empty());
    p.schema_version = 99;
    assert!(inspect(&p, None).is_err());
}
#[tokio::test]
async fn template_info_http_is_pure_and_rejects_unknown_timeline() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state.clone());
    let p = project();
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/compositions/nested-template-info")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"project":p,"compositionId":"concat-timeline-0"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(body["schemaVersion"], 1);
    assert_eq!(body["slots"][0]["mediaId"], "b");
    assert_eq!(body["slots"][1]["seconds"], 5.);
    let response = app
        .oneshot(
            Request::post("/api/compositions/nested-template-info")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"project":p,"compositionId":"missing"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(state.library.list().await.is_empty());
}
