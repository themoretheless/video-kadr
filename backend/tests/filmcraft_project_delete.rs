use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_project_delete::delete;
fn oracle() -> Value {
    serde_json::from_str(include_str!(
        "fixtures/filmcraft-project-delete-oracle.json"
    ))
    .unwrap()
}
fn wire_context(mut context: Value) -> Value {
    for key in ["activeSequence", "sourceItem"] {
        if let Some(id) = context[key].as_u64() {
            context[key] = json!(id.to_string());
        }
    }
    for key in [
        "selection",
        "projectSelection",
        "openSequences",
        "captionSelection",
    ] {
        for id in context[key].as_array_mut().unwrap() {
            *id = json!(id.as_u64().unwrap().to_string());
        }
    }
    if let Some(id) = context["selectedMask"]["clip"].as_u64() {
        context["selectedMask"]["clip"] = json!(id.to_string());
    }
    context
}
#[test]
fn folders_references_transitions_and_fixed_session_match_original() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 224);
    let mut accepted = 0;
    for (case_id, case) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut params = case["params"].clone();
        params["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
        params["context"] = wire_context(case["context"].clone());
        let result = delete(params);
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {case_id}: {case}: {result:?}"
        );
        if let Ok(result) = result {
            accepted += 1;
            assert_eq!(
                case["undoDelta"], 1,
                "accepted Clear always creates a source Undo frame"
            );
            assert_eq!(result["result"], case["result"], "case {case_id}");
            assert_eq!(
                result["context"],
                wire_context(case["afterContext"].clone()),
                "case {case_id}"
            );
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())
                ),
                case["afterSha256"],
                "case {case_id}"
            );
            for id in result["removedItemIds"].as_array().unwrap() {
                assert!(o["project"]["items"].get(id.as_str().unwrap()).is_some());
                assert!(result["document"]["project"]["items"]
                    .get(id.as_str().unwrap())
                    .is_none());
            }
            assert_eq!(
                serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),
                result["document"]
            );
        } else {
            assert_eq!(case["undoDelta"], 0);
        }
    }
    assert!(accepted > 150);
}
#[tokio::test]
async fn exact_u64_ids_import_after_deletion_and_preserve_envelope() {
    use video_kadr_backend::{
        error::ApiJson,
        handlers::filmcraft_project::{import, ImportRequest},
    };
    let o = oracle();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"],"hostExtension":{"id":u64::MAX}});
    let items = doc["project"]["items"].as_object_mut().unwrap();
    let source = items
        .iter()
        .find(|(_, i)| i.pointer("/kind/Media").is_some())
        .unwrap()
        .0
        .clone();
    let media = items.remove(&source).unwrap();
    items.insert(u64::MAX.to_string(), media);
    let huge = items.get_mut(&u64::MAX.to_string()).unwrap();
    huge["id"] = json!(u64::MAX);
    let result = delete(json!({"document":serde_json::to_string(&doc).unwrap(),"items":[u64::MAX.to_string()],"context":{}})).unwrap();
    assert_eq!(result["removedItemIds"], json!([u64::MAX.to_string()]));
    assert_eq!(result["document"]["hostExtension"]["id"], u64::MAX);
    let request: ImportRequest =
        serde_json::from_value(json!({"document":result["documentText"]})).unwrap();
    let manifest = import(ApiJson(request)).await.unwrap().0;
    assert!(!manifest["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"] == u64::MAX.to_string()));
}
