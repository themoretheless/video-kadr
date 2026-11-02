use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_source_edit::edit;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-source-edit-oracle.json")).unwrap()
}
fn context(mut v: Value) -> Value {
    for key in [
        "activeSequence",
        "sourceItem",
        "sourcePlayhead",
        "playhead",
        "playheadFrames",
    ] {
        if let Some(n) = v[key].as_number() {
            v[key] = json!(n.to_string());
        }
    }
    for id in v["selection"].as_array_mut().unwrap() {
        *id = json!(id.as_u64().unwrap().to_string());
    }
    v
}
#[test]
fn insert_overwrite_all_kinds_split_patching_and_flatten_match_original() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert!(o["cases"].as_array().unwrap().len() > 2000);
    let mut accepted = 0;
    for (i, case) in o["cases"].as_array().unwrap().iter().enumerate() {
        let profile = case["profile"].as_u64().unwrap() as usize;
        let doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["profiles"][profile]});
        let actual = edit(
            case["command"].as_str().unwrap(),
            json!({"document":doc,"context":case["context"]}),
        );
        assert_eq!(
            actual.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {i}: {case}: {:?}",
            actual.as_ref().err()
        );
        if let Ok(actual) = actual {
            accepted += 1;
            assert_eq!(case["undo"], 1);
            assert_eq!(actual["sourceEdit"], true);
            let ids = case["result"]["clips"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_u64().unwrap().to_string())
                .collect::<Vec<_>>();
            assert_eq!(actual["result"]["clips"], json!(ids), "case {i}");
            assert_eq!(
                actual["context"],
                context(case["afterContext"].clone()),
                "case {i}: {case}"
            );
            let got = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap())
            );
            if got != case["afterSha256"] {
                std::fs::write(
                    "/tmp/kadr-source-edit-mismatch.json",
                    serde_json::to_vec_pretty(&json!({"case":case,"actual":actual})).unwrap(),
                )
                .unwrap();
                panic!("full project mismatch in case {i}: got {got}; expected {}; /tmp/kadr-source-edit-mismatch.json",case["afterSha256"]);
            }
        } else {
            assert_eq!(case["undo"], 0);
        }
    }
    assert!(accepted > 1000);
}
#[tokio::test]
async fn exact_new_clip_ids_and_imported_tracks_preserve_authoring_and_refuse_allocator_damage() {
    use video_kadr_backend::{
        error::ApiJson,
        handlers::filmcraft_project::{import, ImportRequest},
    };
    let o = oracle();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["profile"] == 0 && c["ok"] == true && c["context"]["sequencesAsClips"] == false)
        .unwrap();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["profiles"][0],"extension":{"id":u64::MAX}});
    let mut ctx = case["context"].clone();
    let sequence = ctx["activeSequence"].as_u64().unwrap();
    let source = ctx["sourceItem"].as_u64().unwrap();
    let mut item = doc["project"]["items"][source.to_string()].clone();
    item["id"] = json!(u64::MAX);
    doc["project"]["items"][u64::MAX.to_string()] = item;
    doc["project"]["root"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Item":u64::MAX}));
    doc["project"]["next_id"] = json!(u64::MAX - 1);
    ctx["sourceItem"] = json!(u64::MAX.to_string());
    ctx["playhead"] = json!((1000i64 * 254016000000).to_string());
    ctx["targeting"]["audioDest"] = Value::Null;
    let actual = edit(
        "source.insert",
        json!({"document":serde_json::to_string(&doc).unwrap(),"context":ctx}),
    )
    .unwrap();
    assert_eq!(
        actual["result"]["clips"],
        json!([(u64::MAX - 1).to_string()])
    );
    assert_eq!(actual["document"]["project"]["next_id"], u64::MAX);
    assert_eq!(actual["document"]["extension"]["id"], u64::MAX);
    let request: ImportRequest =
        serde_json::from_value(json!({"document":actual["documentText"]})).unwrap();
    let manifest = import(ApiJson(request)).await.unwrap().0;
    let q = manifest["sequences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["id"] == sequence.to_string())
        .unwrap();
    assert!(q["clips"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == (u64::MAX - 1).to_string()));
    assert!(!q["tracks"].as_array().unwrap().is_empty());
    assert!(q["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["id"].is_string() && t["locked"].is_boolean()));
    assert!(edit(
        "source.insert",
        json!({"document":actual["documentText"],"context":ctx})
    )
    .is_err());
    doc["project"]["next_id"] = json!(source);
    assert!(edit("source.overwrite", json!({"document":doc,"context":ctx})).is_err());
}
