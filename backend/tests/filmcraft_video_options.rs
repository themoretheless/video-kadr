use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_video_options::edit;

#[test]
fn exact_clip_and_hold_ticks_survive_and_source_overflow_refuses_atomically() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-options-oracle.json")).unwrap();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "clip.frameHoldOptions" && c["ok"] == true)
        .unwrap();
    let active = case["context"]["activeSequence"].as_u64().unwrap();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["projects"][0],"extension":{"id":u64::MAX,"tick":9007199254740993_i64}});
    let path = format!("/project/items/{active}/kind/Sequence/video_tracks/0/items/0");
    doc.pointer_mut(&path).unwrap()["id"] = json!(u64::MAX);
    let base = json!({"document":serde_json::to_string(&doc).unwrap(),"clips":[u64::MAX.to_string()],"holdOn":"sourceTimecode","time":"9007199254740997","context":{"activeSequence":active.to_string(),"selection":[]}});
    let result = edit("clip.frameHoldOptions", base.clone()).unwrap();
    let exact: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(exact.pointer(&path).unwrap()["id"], json!(u64::MAX));
    assert_eq!(exact["extension"], doc["extension"]);
    let reported = &result["result"]["clips"][0];
    assert_eq!(reported["clip"], u64::MAX.to_string());
    let hold = reported["hold"].as_str().unwrap().parse::<i64>().unwrap();
    assert!(hold > 9007190000000000);
    assert_eq!(exact.pointer(&path).unwrap()["frame_hold"], json!(hold));
    doc.pointer_mut(&path).unwrap()["source_in"] = json!(i64::MAX - 1);
    let mut hostile = base.clone();
    hostile["document"] = json!(serde_json::to_string(&doc).unwrap());
    hostile["holdOn"] = json!("out");
    assert!(edit("clip.frameHoldOptions", hostile).is_err());
    assert_eq!(
        edit("clip.frameHoldOptions", base).unwrap()["documentText"],
        result["documentText"]
    );
    // Field/interpolation commands do not inspect a source's media clock.
    let item = doc.pointer(&path).unwrap()["item"]
        .as_u64()
        .unwrap()
        .to_string();
    doc["project"]["items"][&item]["kind"]["Media"]["interpret"]["frame_rate"] =
        json!({"num":0,"den":1});
    assert!(edit("clip.fieldOptions",json!({"document":doc,"clips":[u64::MAX.to_string()],"context":{"activeSequence":active.to_string()}})).is_ok());
}
#[test]
fn hold_fields_interpolation_fit_fill_and_video_only_selection_match_original() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-options-oracle.json")).unwrap();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 5568);
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
            if let Some(clips) = expected["clips"].as_array_mut() {
                for clip in clips {
                    clip["clip"] = json!(clip["clip"].as_u64().unwrap().to_string());
                    if clip.get("hold").is_some() {
                        clip["hold"] = json!(clip["hold"].as_i64().unwrap().to_string());
                    }
                }
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
    assert!(accepted > 1000);
}
