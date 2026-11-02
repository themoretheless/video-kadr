use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use video_kadr_backend::domain::filmcraft_marks::edit;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-marks-oracle.json")).unwrap()
}
fn exact(v: &mut Value) {
    match v {
        Value::Object(m) => m.values_mut().for_each(exact),
        Value::Array(a) => a.iter_mut().for_each(exact),
        Value::Number(n)
            if n.as_u64().is_some_and(|n| n > 9007199254740991)
                || n.as_i64().is_some_and(|n| n < -9007199254740991) =>
        {
            *v = json!(n.to_string())
        }
        _ => {}
    }
}
#[test]
fn ordinary_and_split_marks_match_actual_original_engine() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert!(o["cases"].as_array().unwrap().len() > 7000);
    for (i, case) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut params = case["params"].clone();
        params["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
        params["context"] = case["context"].clone();
        let actual = edit(case["command"].as_str().unwrap(), params);
        assert_eq!(
            actual.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {i}: {case}: {:?}",
            actual.as_ref().err()
        );
        if let Ok(actual) = actual {
            let mut expected = case["result"].clone();
            exact(&mut expected);
            assert_eq!(actual["result"], expected, "case {i}");
            assert_eq!(actual["markEdit"], true);
            assert_eq!(case["undoDelta"], 1);
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap())
                ),
                case["afterSha256"],
                "case {i}: {case}"
            );
        } else {
            assert_eq!(case["undoDelta"], 0);
        }
    }
}
#[tokio::test]
async fn exact_tick_transport_and_split_omission_preserve_source_fields() {
    let o = oracle();
    let mut d = json!({"format":"filmcraft.project","schema_version":12,"project":o["project"],"extension":{"id":u64::MAX}});
    let source = d["project"]["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, i)| i.pointer("/kind/Media").is_some())
        .unwrap()
        .0
        .clone();
    let result = edit(
        "project.setMarks",
        json!({"document":d,"item":source,"in":i64::MAX.to_string(),"out":null}),
    )
    .unwrap();
    assert_eq!(
        result["document"]["project"]["items"][&source]["kind"]["Media"]["mark_in"],
        i64::MAX
    );
    assert!(result["document"]["project"]["items"][&source]
        .get("split")
        .is_none());
    assert_eq!(result["document"]["extension"]["id"], u64::MAX);
    let request: video_kadr_backend::handlers::filmcraft_project::ImportRequest =
        serde_json::from_value(json!({"document":result["documentText"]})).unwrap();
    let manifest = video_kadr_backend::handlers::filmcraft_project::import(
        video_kadr_backend::error::ApiJson(request),
    )
    .await
    .unwrap()
    .0;
    let source_manifest = manifest["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == source)
        .unwrap();
    assert_eq!(source_manifest["sourceMarks"]["in"], i64::MAX.to_string());
    assert_eq!(
        source_manifest["sourceMarks"]["split"],
        json!({"videoIn":null,"videoOut":null,"audioIn":null,"audioOut":null})
    );
    d = result["document"].clone();
    d["project"]["items"][&source]
        .as_object_mut()
        .unwrap()
        .remove("split");
    let result=edit("markers.markSplitVideoIn",json!({"document":d,"target":"source","time":i64::MIN.to_string(),"context":{"sourceItem":source}})).unwrap();
    assert_eq!(
        result["result"],
        json!({"videoIn":i64::MIN.to_string(),"videoOut":null,"audioIn":null,"audioOut":null})
    );
}
