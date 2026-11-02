use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_media_modify::edit;
fn ids(v: &Value) -> Value {
    json!(v
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap().to_string())
        .collect::<Vec<_>>())
}
#[tokio::test]
async fn mono_file_aliases_and_exact_timecodes_import_without_opening_source_paths() {
    use video_kadr_backend::{error::ApiJson, handlers::filmcraft_project::import};
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-media-modify-oracle.json")).unwrap();
    let case = o["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["command"] == "clip.breakoutToMono" && c["ok"] == true)
        .unwrap();
    let source = case["afterSelection"].as_array().unwrap();
    assert!(!source.is_empty());
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["projects"][0],"extension":{"id":u64::MAX}});
    let audio = doc["project"]["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| {
            item.pointer("/kind/Media/info/start_timecode")
                .and_then(Value::as_i64)
                == Some(1000)
        })
        .unwrap()
        .0
        .clone();
    doc["project"]["items"][&audio]["kind"]["Media"]["media"] =
        json!({"File":{"path":"/not-opened/source.wav"}});
    doc["project"]["next_id"] = json!(u64::MAX - 10);
    let params = json!({"document":serde_json::to_string(&doc).unwrap(),"items":[audio],"context":{"projectSelection":[]}});
    let result = edit("clip.breakoutToMono", params.clone()).unwrap();
    let made = result["result"]["items"].as_array().unwrap();
    assert!(!made.is_empty());
    assert_eq!(made[0], (u64::MAX - 10).to_string());
    let exact: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(exact["extension"], doc["extension"]);
    let imported = import(ApiJson(
        serde_json::from_value(json!({"document":result["documentText"]})).unwrap(),
    ))
    .await
    .unwrap()
    .0;
    for (id, copy) in made.iter().zip(result["copies"].as_array().unwrap()) {
        let id = id.as_str().unwrap();
        assert_eq!(copy["id"], id);
        assert_eq!(copy["sourceId"], audio);
        let item = imported["projectItems"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap();
        assert_eq!(item["mediaInterpretation"]["channelMap"]["format"], "Mono");
        assert_eq!(exact["project"]["items"][id]["metadata"], json!({}));
        assert_eq!(
            exact["project"]["items"][id]["kind"]["Media"]["markers"],
            json!([])
        );
    }
    let timed = edit(
        "clip.modifyTimecode",
        json!({"document":doc,"item":audio,"frame":i64::MAX.to_string(),"tapeName":"Tape 🎬"}),
    )
    .unwrap();
    assert_eq!(timed["result"]["startTimecode"], i64::MAX.to_string());
    let imported = import(ApiJson(
        serde_json::from_value(json!({"document":timed["documentText"]})).unwrap(),
    ))
    .await
    .unwrap()
    .0;
    let item = imported["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == audio)
        .unwrap();
    assert_eq!(
        item["mediaInterpretation"]["startTimecode"],
        i64::MAX.to_string()
    );
    assert_eq!(item["mediaInterpretation"]["tapeName"], "Tape 🎬");
    let reset = edit(
        "clip.modifyTimecode",
        json!({"document":timed["documentText"],"item":audio,"reset":true}),
    )
    .unwrap();
    assert_eq!(reset["result"]["startTimecode"], "1000");
    let mut exhausted = params;
    let mut d = exact;
    d["project"]["next_id"] = json!(u64::MAX);
    exhausted["document"] = d;
    assert!(edit("clip.breakoutToMono", exhausted).is_err());
}
#[test]
fn channel_maps_timecode_reset_mono_aliases_and_full_projects_match_original() {
    let o: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-media-modify-oracle.json")).unwrap();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 2856);
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
            if c["command"] == "clip.modifyTimecode" {
                expected["item"] = json!(expected["item"].as_u64().unwrap().to_string());
                if let Some(tc) = expected["startTimecode"].as_i64() {
                    expected["startTimecode"] = json!(tc.to_string());
                }
            } else if c["command"] == "clip.breakoutToMono" {
                expected["items"] = ids(&expected["items"]);
            } else if expected.get("channels").is_some() {
                expected["clips"] = ids(&expected["clips"]);
            } else {
                for item in expected["items"].as_array_mut().unwrap() {
                    item["item"] = json!(item["item"].as_u64().unwrap().to_string());
                }
            }
            assert_eq!(r["result"], expected, "case {i}: {c}");
            assert_eq!(
                r["context"]["projectSelection"],
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
    assert!(accepted > 500);
}
