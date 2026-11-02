use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_clip_edit::edit;
fn ids(v: &Value) -> Value {
    json!(v
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap().to_string())
        .collect::<Vec<_>>())
}
#[test]
fn exact_hold_allocation_selection_and_collision_guards_preserve_the_document() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-clip-edit-oracle.json")).unwrap();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["command"] == "clip.insertFrameHoldSegment" && c["profile"] == 0 && c["ok"] == true
        })
        .unwrap();
    let active = case["context"]["activeSequence"].as_u64().unwrap();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["projects"][0],"extension":{"id":u64::MAX,"tick":9007199254740993_i64}});
    doc["project"]["next_id"] = json!(u64::MAX - 10);
    let params = json!({"document":serde_json::to_string(&doc).unwrap(),"frame":"48","seconds":1.25,"context":{"activeSequence":active.to_string(),"selection":[],"playhead":"0"}});
    let result = edit("clip.insertFrameHoldSegment", params.clone()).unwrap();
    assert_eq!(result["result"]["clip"], (u64::MAX - 10).to_string());
    assert_eq!(
        result["context"]["selection"],
        json!([(u64::MAX - 10).to_string()])
    );
    let exact: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(exact["extension"], doc["extension"]);
    let clip = exact["project"]["items"][active.to_string()]["kind"]["Sequence"]["video_tracks"][0]
        ["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == u64::MAX - 10)
        .unwrap();
    let expected = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["command"] == "clip.insertFrameHoldSegment"
                && c["profile"] == 0
                && c["params"]["seconds"] == 1.25
                && c["ok"] == true
        })
        .unwrap();
    assert_eq!(clip["duration"], expected["result"]["duration"]);
    assert_eq!(clip["link"], Value::Null);
    assert_eq!(clip["markers"], json!([]));
    let original_id = doc["project"]["items"][active.to_string()]["kind"]["Sequence"]
        ["video_tracks"][0]["items"][0]["id"]
        .as_u64()
        .unwrap();
    doc["project"]["next_id"] = json!(original_id);
    assert!(edit("clip.group",json!({"document":doc,"context":{"activeSequence":active.to_string(),"selection":[original_id.to_string()]}})).is_err());
    assert_eq!(
        edit("clip.insertFrameHoldSegment", params).unwrap()["documentText"],
        result["documentText"]
    );
}
#[test]
fn speed_link_groups_ripple_holds_and_full_project_match_original() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-clip-edit-oracle.json")).unwrap();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 5040);
    let mut accepted = 0;
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut p = c["params"].clone();
        p["document"] = json!({"format":"filmcraft.project","schema_version":12,"project":o["projects"][c["profile"].as_u64().unwrap() as usize]});
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
            if c["command"] == "clip.frameHold" {
                expected["clips"] = ids(&expected["clips"]);
            }
            if c["command"] == "clip.insertFrameHoldSegment" {
                expected["clip"] = json!(expected["clip"].as_u64().unwrap().to_string());
                expected["duration"] = json!(expected["duration"].as_i64().unwrap().to_string());
            }
            assert_eq!(r["result"], expected, "case {i}: {c}");
            assert_eq!(
                r["context"]["selection"],
                ids(&c["afterSelection"]),
                "case {i}"
            );
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
    assert!(accepted > 1000);
}
