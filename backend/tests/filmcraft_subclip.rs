use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_subclip::{defaults, edit};
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-subclip-oracle.json")).unwrap()
}
fn strings(v: &Value) -> Value {
    json!(v
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_u64().unwrap().to_string())
        .collect::<Vec<_>>())
}
#[test]
fn targeting_ranges_conversion_and_defaults_match_actual_original() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(o["cases"].as_array().unwrap().len(), 1152);
    let mut accepted = 0;
    for (i, case) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut params = case["params"].clone();
        params["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
        params["context"] = case["context"].clone();
        if case["command"] == "clip.makeSubclip" {
            let plan = defaults(params.clone());
            assert_eq!(
                plan.is_ok(),
                case["defaults"].is_object(),
                "defaults {i}: {case}: {plan:?}"
            );
            if let Ok(plan) = plan {
                assert_eq!(plan["name"], case["defaults"]["name"], "defaults {i}");
                assert_eq!(
                    plan["startFrame"],
                    case["defaults"]["startFrame"].as_i64().unwrap().to_string()
                );
                assert_eq!(
                    plan["endFrame"],
                    case["defaults"]["endFrame"].as_i64().unwrap().to_string()
                );
                assert_eq!(plan["fps"], case["defaults"]["fps"]);
            }
        }
        let result = edit(case["command"].as_str().unwrap(), params);
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {i}: {case}: {:?}",
            result.as_ref().err()
        );
        if let Ok(result) = result {
            accepted += 1;
            assert_eq!(case["undo"], 1);
            assert_eq!(
                result["result"]["item"],
                case["result"]["item"].as_u64().unwrap().to_string()
            );
            assert_eq!(result["result"]["converted"], case["result"]["converted"]);
            assert_eq!(
                result["context"]["projectSelection"],
                strings(&case["afterSelection"])
            );
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())
                ),
                case["afterSha256"],
                "case {i}: {case}"
            );
            assert_eq!(result["subclipEdit"]["itemId"], result["result"]["item"]);
        } else {
            assert_eq!(case["undo"], 0);
        }
    }
    assert!(accepted > 400);
}
#[tokio::test]
async fn converted_file_media_and_exact_new_identity_import_without_path_authority() {
    use video_kadr_backend::{
        error::ApiJson,
        handlers::filmcraft_project::{import, ImportRequest},
    };
    let o = oracle();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"],"extension":{"id":u64::MAX}});
    let outer = doc["project"]["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| item["name"] == "Outer")
        .unwrap()
        .0
        .clone();
    let parent = doc["project"]["items"][&outer]["kind"]["Subclip"]["parent"]
        .as_u64()
        .unwrap()
        .to_string();
    doc["project"]["items"][&parent]["kind"]["Media"]["media"] =
        json!({"File":{"path":"/unopened/source.mov"}});
    let proposal = defaults(json!({"document":doc,"item":outer,"forEdit":true})).unwrap();
    assert_eq!(proposal["startFrame"], "100");
    assert_eq!(proposal["endFrame"], "150");
    assert_eq!(proposal["restrictTrims"], false);
    let result = edit(
        "clip.editSubclip",
        json!({"document":doc,"item":outer,"convertToMaster":true}),
    )
    .unwrap();
    assert_eq!(result["subclipEdit"]["bindingSourceId"], parent);
    assert_eq!(
        result["document"]["project"]["items"][&outer]["metadata"],
        doc["project"]["items"][&outer]["metadata"]
    );
    assert_eq!(
        result["document"]["project"]["items"][&outer]["split"],
        doc["project"]["items"][&outer]["split"]
    );
    let markers = result["document"]["project"]["items"][&outer]["kind"]["Media"]["markers"]
        .as_array()
        .unwrap();
    assert_eq!(markers.len(), 2);
    assert_eq!(result["document"]["extension"]["id"], u64::MAX);
    let request: ImportRequest =
        serde_json::from_value(json!({"document":result["documentText"]})).unwrap();
    let manifest = import(ApiJson(request)).await.unwrap().0;
    for id in [&outer, &parent] {
        let media = manifest["media"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == *id)
            .unwrap();
        assert_eq!(media["path"], "/unopened/source.mov");
        assert_eq!(media["offline"], true);
    }
    let mut max_parent = doc["project"]["items"][&parent].clone();
    max_parent["id"] = json!(u64::MAX);
    doc["project"]["items"][u64::MAX.to_string()] = max_parent;
    doc["project"]["root"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"Item":u64::MAX}));
    doc["project"]["next_id"] = json!(u64::MAX - 1);
    let made = edit(
        "clip.makeSubclip",
        json!({"document":serde_json::to_string(&doc).unwrap(),"item":u64::MAX.to_string()}),
    )
    .unwrap();
    assert_eq!(made["result"]["item"], (u64::MAX - 1).to_string());
    assert_eq!(
        made["document"]["project"]["items"][(u64::MAX - 1).to_string()]["kind"]["Subclip"]
            ["parent"],
        u64::MAX
    );
    let request: ImportRequest =
        serde_json::from_value(json!({"document":made["documentText"]})).unwrap();
    let imported = import(ApiJson(request)).await.unwrap().0;
    let item = imported["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == (u64::MAX - 1).to_string())
        .unwrap();
    assert_eq!(item["subclip"]["parentId"], u64::MAX.to_string());
    assert!(item["subclip"]["startTicks"].is_string());
    assert!(edit(
        "clip.makeSubclip",
        json!({"document":made["documentText"],"item":u64::MAX.to_string()})
    )
    .is_err());
    doc["project"]["next_id"] = json!(parent.parse::<u64>().unwrap());
    assert!(edit("clip.makeSubclip", json!({"document":doc,"item":parent})).is_err());
}
#[test]
fn extreme_frame_rationals_refuse_without_panicking() {
    let o = oracle();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
    let outer = doc["project"]["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| item["name"] == "Outer")
        .unwrap()
        .0
        .clone();
    let parent = doc["project"]["items"][&outer]["kind"]["Subclip"]["parent"]
        .as_u64()
        .unwrap()
        .to_string();
    doc["project"]["items"][&parent]["kind"]["Media"]["interpret"]["frame_rate"] =
        json!({"num":1,"den":u32::MAX});
    let result = std::panic::catch_unwind(|| {
        edit(
            "clip.editSubclip",
            json!({"document":doc,"item":outer,"startFrame":i64::MAX.to_string(),"endFrame":i64::MAX.to_string()}),
        )
    });
    assert!(result.is_ok());
    assert!(result.unwrap().is_err());
}
