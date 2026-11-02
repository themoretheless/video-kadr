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
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 320 + 2 * oracle["project"]["items"].as_object().unwrap().len());
    assert_eq!(oracle["itemKinds"], json!(["AdjustmentLayer", "Graphic", "Media", "Sequence", "Subclip"]));
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

#[tokio::test]
async fn renamed_sequence_and_offline_media_survive_actual_manifest_import() {
    use video_kadr_backend::{error::ApiJson, handlers::filmcraft_project::{import, ImportRequest}};
    let (mut doc, seq, _) = sample();
    let media_id = doc["project"]["items"].as_object().unwrap().iter().find(|(_, value)| value.pointer("/kind/Media").is_some()).unwrap().0.clone();
    doc["project"]["items"][&media_id]["kind"]["Media"]["media"] = json!({"File":{"path":"/offline/source.mov"}});
    doc["project"]["items"][&media_id]["kind"]["Media"]["offline"] = json!(true);
    for (id, collection) in [(seq.to_string(), "sequences"), (media_id, "media")] {
        let changed = rename(json!({"document":doc,"item":id,"name":"Новое имя 🎬"})).unwrap();
        let request: ImportRequest = serde_json::from_value(json!({"document":changed["documentText"]})).unwrap();
        let manifest = import(ApiJson(request)).await.unwrap().0;
        let item = manifest[collection].as_array().unwrap().iter().find(|item| item["id"] == id).unwrap();
        assert_eq!(item["name"], "Новое имя 🎬");
        if collection == "media" { assert_eq!(item["offline"], true); assert_eq!(item["path"], "/offline/source.mov"); }
    }
}

#[tokio::test]
async fn all_project_item_kinds_import_with_exact_ids_names_and_numeric_order() {
    use video_kadr_backend::{error::ApiJson, handlers::filmcraft_project::{import, ImportRequest}};
    let (mut doc, _, _) = sample();
    let kinds = ["Media", "Sequence", "Subclip", "Graphic", "AdjustmentLayer"];
    let template = doc["project"]["items"].as_object().unwrap().values().next().unwrap().clone();
    for (offset, kind) in kinds.iter().enumerate() {
        let id = u64::MAX - offset as u64;
        let mut item = template.clone();
        item["id"] = json!(id); item["name"] = json!(format!("{kind} 🎬"));
        // Sequence/media retain actual demo payloads; remaining variants are metadata-only.
        let payload = doc["project"]["items"].as_object().unwrap().values().find_map(|v| v["kind"].get(*kind)).cloned().unwrap_or(json!({"width":1920,"height":1080,"rate":{"num":24,"den":1},"duration":254016000000i64,"parent":5,"range":{"start":0,"duration":254016000000i64},"restrict_trims":false}));
        item["kind"] = json!({*kind:payload});
        doc["project"]["items"][id.to_string()] = item;
    }
    let request: ImportRequest = serde_json::from_value(json!({"document":serde_json::to_string(&doc).unwrap()})).unwrap();
    let manifest = import(ApiJson(request)).await.unwrap().0;
    let rows = manifest["projectItems"].as_array().unwrap();
    let ids: Vec<u64> = rows.iter().map(|row| row["id"].as_str().unwrap().parse().unwrap()).collect();
    assert!(ids.windows(2).all(|pair|pair[0]<pair[1]));
    for (offset, kind) in kinds.iter().enumerate() {
        let id = (u64::MAX - offset as u64).to_string();
        let row = rows.iter().find(|row|row["id"]==id).unwrap();
        assert_eq!(row["kind"], *kind); assert_eq!(row["name"], format!("{kind} 🎬"));
        let renamed = rename(json!({"document":manifest["documentText"],"item":id,"name":"новое"})).unwrap();
        let request: ImportRequest = serde_json::from_value(json!({"document":renamed["documentText"]})).unwrap();
        let updated = import(ApiJson(request)).await.unwrap().0;
        assert_eq!(updated["projectItems"].as_array().unwrap().iter().find(|row|row["id"]==id).unwrap()["name"], "новое");
    }
}
