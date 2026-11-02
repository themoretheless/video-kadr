use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_clip_rename::rename;
fn sample() -> (Value, Value, Value) {
    let o: Value = serde_json::from_str(include_str!("fixtures/filmcraft-effect-edit-oracle.json")).unwrap();
    (json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]}), o["sequence"].clone(), o["clip"].clone())
}
#[test]
fn explicit_and_selected_clip_rename_preserve_every_other_field() {
    let (doc, seq, clip) = sample();
    let path = format!("/project/items/{seq}/kind/Sequence/video_tracks/0/items/0/name");
    for selected in [false, true] {
        let mut p = json!({"document":serde_json::to_string(&doc).unwrap(),"name":"новое 🎬","context":{"activeSequence":seq.to_string(),"selection":[clip.to_string()]}});
        if !selected { p["clip"] = json!(clip.to_string()); p["sequence"] = json!(seq.to_string()); }
        let result = rename(p).unwrap();
        let mut expected = doc.clone();
        *expected.pointer_mut(&path).unwrap() = json!("новое 🎬");
        assert_eq!(result["document"], expected);
        assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(), expected);
    }
}
#[test]
fn item_takes_precedence_and_no_target_is_source_compatible_noop() {
    let (doc, seq, _) = sample();
    let result = rename(json!({"document":doc,"name":"","item":seq.to_string(),"clip":"999999"})).unwrap();
    let mut expected = doc.clone();
    expected["project"]["items"][seq.to_string()]["name"] = json!("");
    assert_eq!(result["document"], expected);
    assert_eq!(rename(json!({"document":doc,"name":"unused"})).unwrap()["document"], doc);
}
#[test]
fn missing_target_and_invalid_name_refuse_without_mutation() {
    let (doc, seq, _) = sample();
    for extra in [json!({"item":"18446744073709551615"}),json!({"sequence":seq.to_string(),"clip":"18446744073709551615"}),json!({"item":-1}),json!({"name":null})] {
        let mut p = json!({"document":doc,"name":"rename"});
        p.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        assert!(rename(p).is_err());
    }
}

#[test]
fn command_addressing_matches_actual_original_engine() {
    use sha2::{Digest, Sha256};
    let oracle: Value = serde_json::from_str(include_str!("fixtures/filmcraft-clip-rename-oracle.json")).unwrap();
    assert_eq!(oracle["sourceCommit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 320);
    for case in oracle["cases"].as_array().unwrap() {
        let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":oracle["project"]});
        doc["project"]["items"][oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["locked"] = case["locked"].clone();
        let mut params = case["params"].clone();
        params["document"] = doc.clone();
        params["context"] = json!({"activeSequence":if case["active"] == true { oracle["sequence"].clone() } else {Value::Null},"selection":if case["selected"] == true {vec![oracle["clip"].clone()]} else {vec![]}});
        let actual = rename(params.clone());
        assert_eq!(actual.is_ok(), case["ok"].as_bool().unwrap(), "params {params}: {actual:?}");
        if let Ok(actual) = actual {
            let hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap()));
            assert_eq!(hash, case["afterSha256"], "params {params}");
        }
    }
}
