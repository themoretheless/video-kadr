mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_mask_commands::run;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-mask-command-oracle.json")).unwrap()
}
fn params(o: &Value, c: &Value) -> Value {
    let mut d = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
    d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]
        ["items"][0]["effects"] = json!([o["instances"][c["instance"].as_u64().unwrap() as usize]]);
    let mut p = c["params"].clone();
    p["context"] = c["context"].clone();
    p["sequence"] = o["sequence"].clone();
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p
}
#[test]
fn mask_commands_match_original_engine_projects_results_and_selection() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert!(o["cases"].as_array().unwrap().len() == 469);
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let p = params(&o, c);
        let before = p.clone();
        let result = run(c["command"].as_str().unwrap(), p.clone());
        assert_eq!(
            result.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i}: {result:?}"
        );
        if let Ok(r) = result {
            if c["command"] == "masks.list" {
                assert_eq!(r, c["result"], "query case {i}");
            } else {
                let hash = format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&r["document"]["project"]).unwrap())
                );
                assert_eq!(
                    hash, c["projectSha256"],
                    "project case {i}: {}",
                    c["params"]
                );
                assert_eq!(r["result"], c["result"], "result case {i}");
                assert_eq!(
                    r["context"]["selectedMask"], c["selectedMask"],
                    "selection case {i}"
                );
                assert_eq!(
                    serde_json::from_str::<Value>(r["documentText"].as_str().unwrap()).unwrap(),
                    r["document"]
                );
            }
        }
        assert_eq!(p, before);
    }
}
async fn call(app: &axum::Router, token: &str, command: &str, p: Value) -> (u16, Value) {
    let r = app
        .clone()
        .oneshot(
            Request::post("/api/compositions/filmcraft/command")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"command":command,"params":p}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        r.status().as_u16(),
        serde_json::from_slice(&to_bytes(r.into_body(), 2 * 1024 * 1024).await.unwrap()).unwrap(),
    )
}
#[tokio::test]
async fn owner_commands_accept_exact_media_ticks_without_export_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("mask-command-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let o = oracle();
    for cmd in [
        "masks.add",
        "masks.remove",
        "masks.set",
        "masks.moveVertex",
        "masks.translate",
        "masks.addVertex",
        "masks.removeVertex",
        "masks.toggleVertexSmooth",
        "masks.list",
        "masks.select",
    ] {
        let c = o["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["command"] == cmd && c["ok"] == true)
            .unwrap();
        let mut p = params(&o, c);
        for field in ["mediaTime", "to"] {
            if let Some(v) = p.get(field).and_then(Value::as_i64) {
                p[field] = json!(v.to_string());
            }
        }
        assert_eq!(call(&app, "invalid", cmd, p.clone()).await.0, 401);
        let (status, result) = call(&app, &owner.token, cmd, p).await;
        assert_eq!(status, 200, "{result}");
        assert!(if cmd == "masks.list" {
            result["masks"].is_array()
        } else {
            result["documentText"].is_string()
        });
    }
    assert!(state.db.load_jobs().await.unwrap().is_empty());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}

#[test]
fn source_mask_context_preserves_large_clip_ids_in_exact_document_text() {
    let o = oracle();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "masks.set" && c["ok"] == true)
        .unwrap();
    let mut p = params(&o, case);
    let mut doc: Value = serde_json::from_str(p["document"].as_str().unwrap()).unwrap();
    doc["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]
        ["items"][0]["id"] = json!(u64::MAX);
    p["document"] = json!(serde_json::to_string(&doc).unwrap());
    p["clip"] = json!(u64::MAX.to_string());
    p["context"]["selectedMask"]["clip"] = json!(u64::MAX.to_string());
    let changed = run("masks.set", p.clone()).unwrap();
    assert!(changed["documentText"]
        .as_str()
        .unwrap()
        .contains("18446744073709551615"));
    assert_eq!(
        serde_json::from_str::<Value>(changed["documentText"].as_str().unwrap()).unwrap(),
        changed["document"]
    );
    p["document"] = changed["documentText"].clone();
    let listed = run("masks.list", p.clone()).unwrap();
    assert_eq!(listed["masks"][0]["clip"], u64::MAX.to_string());
    assert_eq!(listed["masks"][0]["selected"], true);
    let selected = run("masks.select", p).unwrap();
    assert_eq!(
        selected["context"]["selectedMask"]["clip"],
        u64::MAX.to_string()
    );
}
