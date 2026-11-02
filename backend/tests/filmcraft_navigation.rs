use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_navigation::navigate;
fn oracle() -> Value {
    serde_json::from_str(include_str!("fixtures/filmcraft-navigation-oracle.json")).unwrap()
}
fn strings(v: &mut Value) {
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
fn monitor_span_mark_fallback_and_selection_match_actual_original() {
    let o = oracle();
    assert_eq!(
        o["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert!(o["cases"].as_array().unwrap().len() > 8000);
    for (i, case) in o["cases"].as_array().unwrap().iter().enumerate() {
        let mut project = o["project"].clone();
        if case["profile"] == 1 {
            for item in project["items"].as_object_mut().unwrap().values_mut() {
                item.as_object_mut().unwrap().remove("split");
                if let Some(q) = item.pointer_mut("/kind/Sequence") {
                    q.as_object_mut().unwrap().remove("split");
                }
            }
        }
        let mut params = case["params"].clone();
        params["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":project});
        params["context"] = case["context"].clone();
        let actual = navigate(case["command"].as_str().unwrap(), params);
        assert_eq!(
            actual.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {i}: {case}: {:?}",
            actual.as_ref().err()
        );
        assert_eq!(case["unchanged"], true);
        assert_eq!(case["undo"], 0);
        if let Ok(actual) = actual {
            let mut ctx = case["afterContext"].clone();
            strings(&mut ctx);
            assert_eq!(actual["context"], ctx, "case {i}: {case}");
            let mut result = case["result"].clone();
            if let Some(n) = result["time"].as_number() {
                result["time"] = json!(n.to_string());
            }
            assert_eq!(actual["result"], result, "case {i}");
            let mut events = case["events"].clone();
            for event in events.as_array_mut().unwrap() {
                event["item"] = json!(event["item"].as_u64().unwrap().to_string());
            }
            assert_eq!(actual["events"], events);
            assert!(actual.get("documentText").is_none());
        }
    }
}
#[test]
fn exact_id_tick_transport_and_context_frames_do_not_create_a_document_edit() {
    let o = oracle();
    let mut project = o["project"].clone();
    let id = project["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| item.pointer("/kind/Graphic").is_some())
        .unwrap()
        .0
        .clone();
    let mut item = project["items"]
        .as_object_mut()
        .unwrap()
        .remove(&id)
        .unwrap();
    item["id"] = json!(u64::MAX);
    project["items"][u64::MAX.to_string()] = item;
    let doc = json!({"format":"filmcraft.project","schema_version":12,"project":project});
    let opened=navigate("source.open",json!({"document":serde_json::to_string(&doc).unwrap(),"item":u64::MAX.to_string(),"context":{}})).unwrap();
    assert_eq!(opened["context"]["sourceItem"], u64::MAX.to_string());
    let seek=navigate("source.setPlayhead",json!({"document":doc,"time":"9007199254740997","context":{"sourceItem":u64::MAX.to_string(),"playheadFrames":"12"}})).unwrap();
    assert!(seek["result"]["time"].is_string());
    assert_eq!(seek["context"]["playhead"], "0");
    assert!(seek.get("documentText").is_none());
}

#[test]
fn hostile_frame_and_rational_pairs_refuse_without_panicking_or_returning_a_context() {
    let o = oracle();
    let mut project = o["project"].clone();
    let source = project["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| item.pointer("/kind/Graphic").is_some())
        .unwrap()
        .0
        .clone();
    let sequence = project["items"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, item)| item.pointer("/kind/Sequence").is_some())
        .unwrap()
        .0
        .clone();
    project["items"][&source]["kind"]["Graphic"]["rate"] = json!({"num":1,"den":u32::MAX});
    project["items"][&sequence]["kind"]["Sequence"]["settings"]["frame_rate"] =
        json!({"num":1,"den":u32::MAX});
    let doc = json!({"format":"filmcraft.project","schema_version":12,"project":project});
    for params in [
        json!({"document":doc,"frame":i64::MAX.to_string(),"context":{"sourceItem":source}}),
        json!({"document":doc,"context":{"sourceItem":source,"activeSequence":sequence,"playheadFrames":i64::MAX.to_string()}}),
    ] {
        let result = std::panic::catch_unwind(|| navigate("source.setPlayhead", params));
        assert!(result.is_ok());
        assert!(result.unwrap().is_err());
    }
}
