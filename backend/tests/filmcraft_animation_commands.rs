mod support;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::domain::filmcraft_effect_set_param::edit_command;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-animation-oracle.json")).unwrap()
}
fn params(o: &Value, c: &Value) -> Value {
    let mut d = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
    let item = &mut d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0];
    for (k, v) in c["clipChanges"].as_object().unwrap() {
        item[k] = v.clone();
    }
    for k in c["clipRemovals"].as_array().unwrap() {
        item.as_object_mut().unwrap().remove(k.as_str().unwrap());
    }
    item["effects"] = json!([o["instances"][c["instance"].as_u64().unwrap() as usize]]);
    let mut p = c["params"].clone();
    p["context"] = json!({"playhead":c["playhead"].as_i64().unwrap().to_string()});
    p["sequence"] = o["sequence"].clone();
    p["document"] = json!(serde_json::to_string(&d).unwrap());
    p
}
#[test]
fn animation_sampling_matches_original_engine_full_project_outcomes() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert!(o["cases"].as_array().unwrap().len() > 1426);
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let p = params(&o, c);
        let before = p.clone();
        let result = edit_command(c["command"].as_str().unwrap(), p.clone());
        assert_eq!(
            result.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i}: {result:?}"
        );
        if let Ok(r) = result {
            let hash = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&r["document"]["project"]).unwrap())
            );
            assert_eq!(hash, c["projectSha256"], "case {i}: {}", c["params"]);
            assert_eq!(
                serde_json::from_str::<Value>(r["documentText"].as_str().unwrap()).unwrap(),
                r["document"]
            );
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
async fn animation_owner_commands_use_exact_playhead_without_jobs() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("animation-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let o = oracle();
    for cmd in ["effects.toggleAnimation", "effects.addKeyframe"] {
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
        assert!(result["documentText"].is_string());
    }
    assert!(state.db.load_jobs().await.unwrap().is_empty());
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
