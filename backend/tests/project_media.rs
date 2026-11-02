mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::{CanvasSpec, Composition, SourceId},
    nested_composition::NestedCompositionProject,
    project_media::set_placeholder,
};
fn project() -> NestedCompositionProject {
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    content["canvas"]["width"] = json!(16);
    content["canvas"]["height"] = json!(16);
    content["sources"] = json!({"source-a":{"id":"source-a","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false}});
    serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]},"other":{"content":content,"instances":[]}}})).unwrap()
}
#[test]
fn marking_is_project_wide_metadata_and_retains_unused_media_without_changing_timeline_surfaces() {
    let p = project();
    let id = SourceId::parse("source-a").unwrap();
    let before = serde_json::to_value(&p.compositions).unwrap();
    let plan = serde_json::to_value(p.frame_plan(0).unwrap()).unwrap();
    let (mut marked, applied) = set_placeholder(&p, &id, true).unwrap();
    assert!(applied);
    assert!(marked.project_media[&id].placeholder);
    assert_eq!(serde_json::to_value(&marked.compositions).unwrap(), before);
    assert_eq!(
        serde_json::to_value(marked.frame_plan(0).unwrap()).unwrap(),
        plan
    );
    assert_eq!(
        set_placeholder(&marked, &id, true).unwrap(),
        (marked.clone(), false)
    );
    assert_eq!(
        set_placeholder(&p, &SourceId::parse("missing").unwrap(), true).unwrap(),
        (p.clone(), false)
    );
    for node in marked.compositions.values_mut() {
        node.content.sources.clear();
        node.content.metadata = None;
    }
    marked.dependency_order().unwrap();
    let wire = serde_json::to_value(&marked).unwrap();
    let restored: NestedCompositionProject = serde_json::from_value(wire).unwrap();
    assert_eq!(restored, marked);
    let (unmarked, changed) = set_placeholder(&restored, &id, false).unwrap();
    assert!(changed);
    assert!(!unmarked.project_media[&id].placeholder);
    assert_eq!(unmarked.project_media[&id].source.id, id);
    assert!(
        serde_json::to_value(&unmarked).unwrap()["projectMedia"][id.as_str()]
            .get("placeholder")
            .is_none()
    );
    assert!(
        video_kadr_backend::domain::concat_project_export::export(&marked)
            .unwrap_err()
            .to_string()
            .contains("slot-aware")
    );
}
#[tokio::test]
async fn placeholder_http_is_pure_and_no_op_admission_is_explicit() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let p = project();
    let request = |project: &NestedCompositionProject, placeholder| {
        Request::builder()
            .method("POST")
            .uri("/api/compositions/nested-media-placeholder")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(
                    &json!({"project":project,"sourceId":"source-a","placeholder":placeholder}),
                )
                .unwrap(),
            ))
            .unwrap()
    };
    let response = app.clone().oneshot(request(&p, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let out: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(out["applied"], true);
    let marked: NestedCompositionProject = serde_json::from_value(out["project"].clone()).unwrap();
    let response = app.oneshot(request(&marked, true)).await.unwrap();
    let out: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(out["applied"], false);
    assert_eq!(
        serde_json::from_value::<NestedCompositionProject>(out["project"].clone()).unwrap(),
        marked
    );
    assert!(p.project_media.is_empty());
}
