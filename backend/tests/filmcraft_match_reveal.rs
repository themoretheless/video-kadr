use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_navigation::navigate;
fn context_strings(v: &mut Value) {
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
    for key in ["selection", "projectSelection"] {
        for id in v[key].as_array_mut().unwrap() {
            *id = json!(id.as_u64().unwrap().to_string());
        }
    }
}
#[test]
fn exact_source_and_clip_ids_held_clock_and_overflow_refusal_do_not_create_documents() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-match-reveal-oracle.json")).unwrap();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "sequence.matchFrame" && c["profile"] == 0 && c["ok"] == true)
        .unwrap();
    let active = case["context"]["activeSequence"].as_u64().unwrap();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["projects"][0],"extension":{"id":u64::MAX}});
    let path = format!("/project/items/{active}/kind/Sequence/video_tracks/1/items/0");
    let source = doc.pointer(&path).unwrap()["item"]
        .as_u64()
        .unwrap()
        .to_string();
    let mut media = doc["project"]["items"][&source].clone();
    media["id"] = json!(u64::MAX - 1);
    doc["project"]["items"][(u64::MAX - 1).to_string()] = media;
    let clip = doc.pointer_mut(&path).unwrap();
    clip["id"] = json!(u64::MAX);
    clip["item"] = json!(u64::MAX - 1);
    clip["frame_hold"] = json!(9007199254740997_i64);
    let params = json!({"document":serde_json::to_string(&doc).unwrap(),"clip":u64::MAX.to_string(),"context":{"activeSequence":active.to_string(),"selection":[u64::MAX.to_string()],"projectSelection":[],"sourceItem":null,"sourcePlayhead":"55","playhead":"0","targetedTrackIds":[]}});
    let matched = navigate("sequence.matchFrame", params.clone()).unwrap();
    assert_eq!(matched["context"]["sourceItem"], (u64::MAX - 1).to_string());
    assert_eq!(matched["context"]["sourcePlayhead"], "9007199254740997");
    assert!(matched.get("documentText").is_none());
    let revealed = navigate("clip.revealInProject", params.clone()).unwrap();
    assert_eq!(
        revealed["context"]["projectSelection"],
        json!([(u64::MAX - 1).to_string()])
    );
    assert_eq!(revealed["context"]["sourcePlayhead"], "55");
    assert_eq!(revealed["events"][0]["type"], "revealInProject");
    doc.pointer_mut(&path).unwrap()["frame_hold"] = Value::Null;
    doc.pointer_mut(&path).unwrap()["source_in"] = json!(i64::MAX);
    let mut bad = params.clone();
    bad["document"] = json!(serde_json::to_string(&doc).unwrap());
    assert!(navigate("sequence.matchFrame", bad).is_err());
    assert_eq!(navigate("sequence.matchFrame", params).unwrap(), matched);
}
#[test]
fn source_match_and_project_reveal_match_original_without_project_or_history_changes() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-match-reveal-oracle.json")).unwrap();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 5220);
    let mut accepted = 0;
    for (i, c) in o["cases"].as_array().unwrap().iter().enumerate() {
        let project = &o["projects"][c["profile"].as_u64().unwrap() as usize];
        let mut p = c["params"].clone();
        p["document"] = json!({"format":"filmcraft.project","schema_version":12,"project":project});
        p["context"] = c["context"].clone();
        let r = navigate(c["command"].as_str().unwrap(), p);
        assert_eq!(
            r.is_ok(),
            c["ok"].as_bool().unwrap(),
            "case {i}: {c}: {:?}",
            r.as_ref().err()
        );
        assert_eq!(c["undo"], 0);
        assert_eq!(
            format!("{:x}", Sha256::digest(serde_json::to_vec(project).unwrap())),
            c["afterSha256"],
            "original mutated project {i}"
        );
        if let Ok(r) = r {
            accepted += 1;
            assert!(r.get("documentText").is_none());
            assert!(r.get("document").is_none());
            let mut expected = c["afterContext"].clone();
            context_strings(&mut expected);
            assert_eq!(r["context"], expected, "case {i}: {c}");
            let mut events = c["events"].clone();
            for event in events.as_array_mut().unwrap() {
                event["item"] = json!(event["item"].as_u64().unwrap().to_string());
            }
            assert_eq!(r["events"], events, "case {i}");
            assert_eq!(
                r["result"],
                json!({"item":c["result"]["item"].as_u64().unwrap().to_string()})
            );
        }
    }
    assert!(accepted > 1000);
}
