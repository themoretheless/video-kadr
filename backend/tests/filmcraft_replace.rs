use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_replace::edit;

#[test]
fn exact_ids_preserve_clip_attributes_and_match_frame_overflow_refuses_atomically() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-replace-oracle.json")).unwrap();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "clip.replaceFromSource" && c["ok"] == true)
        .unwrap();
    let active = case["context"]["activeSequence"].as_u64().unwrap();
    let old_source = case["context"]["sourceItem"].as_u64().unwrap().to_string();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"],"extension":{"id":u64::MAX}});
    let mut item = doc["project"]["items"][&old_source].clone();
    item["id"] = json!(u64::MAX);
    doc["project"]["items"][u64::MAX.to_string()] = item;
    let path = format!("/project/items/{active}/kind/Sequence/video_tracks/0/items/0");
    doc.pointer_mut(&path).unwrap()["id"] = json!(u64::MAX - 1);
    let before = doc.pointer(&path).unwrap().clone();
    let params = json!({"document":serde_json::to_string(&doc).unwrap(),"clips":[(u64::MAX-1).to_string()],"context":{"activeSequence":active.to_string(),"sourceItem":u64::MAX.to_string(),"linkedSelection":false}});
    let result = edit("clip.replaceFromSource", params.clone()).unwrap();
    assert_eq!(result["result"]["item"], u64::MAX.to_string());
    assert_eq!(
        result["result"]["clips"],
        json!([(u64::MAX - 1).to_string()])
    );
    let exact: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(exact["extension"], doc["extension"]);
    let after = exact.pointer(&path).unwrap();
    assert_eq!(after["item"], json!(u64::MAX));
    assert!(after["frame_hold"].is_null());
    for field in [
        "start",
        "duration",
        "speed",
        "reverse",
        "effects",
        "markers",
        "gain_db",
        "link",
        "group",
        "scale_to_frame",
    ] {
        assert_eq!(after[field], before[field], "{field}");
    }
    let mut p = params.clone();
    p["context"]["playhead"] = json!(i64::MIN.to_string());
    p["context"]["sourcePlayhead"] = json!(i64::MAX.to_string());
    assert!(edit("clip.replaceFromSourceMatchFrame", p).is_err());
    let again = edit("clip.replaceFromSource", params).unwrap();
    assert_eq!(again["documentText"], result["documentText"]);
}
#[test]
fn replacement_capabilities_targeting_match_frame_and_full_project_match_original() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-replace-oracle.json")).unwrap();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 2016);
    let mut accepted = 0;
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut p = c["params"].clone();
        p["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
        p["context"] = c["context"].clone();
        let r = edit(c["command"].as_str().unwrap(), p);
        assert_eq!(
            r.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i}: {c}: {:?}",
            r.as_ref().err()
        );
        if let Ok(r) = r {
            accepted += 1;
            assert_eq!(c["undo"], 1);
            let mut expected = c["result"].clone();
            for key in ["clips", "sourceInClamped"] {
                if let Some(a) = expected.get_mut(key).and_then(Value::as_array_mut) {
                    for id in a {
                        *id = json!(id.as_u64().unwrap().to_string());
                    }
                }
            }
            expected["item"] = json!(expected["item"].as_u64().unwrap().to_string());
            for entry in expected["short"].as_array_mut().unwrap() {
                entry["clip"] = json!(entry["clip"].as_u64().unwrap().to_string());
                entry["shortByFrames"] =
                    json!(entry["shortByFrames"].as_i64().unwrap().to_string());
            }
            assert_eq!(r["result"], expected, "case {i}: {c}");
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&r["document"]["project"]).unwrap())
                ),
                c["afterSha256"],
                "case {i}: {c}"
            );
        } else {
            assert_eq!(c["undo"], 0, "case {i}: {c}");
        }
    }
    assert!(accepted > 200);
}
