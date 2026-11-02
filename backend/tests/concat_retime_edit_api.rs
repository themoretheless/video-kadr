mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use video_kadr_backend::domain::{composition::ClipPlacement, concat_retime_edit::Clock};
fn request(value: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-retime-edit")
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}
fn command(value: &Value) -> Value {
    match value["op"].as_str().unwrap() {
        "setClipSpeedCurve" => json!({"kind":"setCurve","points":value["curve"]}),
        "splitClips" => json!({"kind":"split","time":value["time"]}),
        "trimClip" => {
            if value["edge"] == "start" {
                json!({"kind":"trimStart","delta":value["delta"],"floor":0.,"ripple":value["ripple"]})
            } else {
                json!({"kind":"trimEnd","delta":value["delta"],"ceiling":8.,"ripple":value["ripple"]})
            }
        }
        "freezeFrame" => json!({"kind":"freeze","time":value["time"],"duration":value["duration"]}),
        _ => panic!(),
    }
}
fn groups(clip: &Value) -> video_kadr_backend::domain::concat_retime_edit::TrackGroups {
    let mut out = video_kadr_backend::domain::concat_retime_edit::TrackGroups::default();
    if let Some(keys) = clip["keys"].as_array() {
        for key in keys {
            out.clip
                .entry(key["property"].as_str().unwrap().into())
                .or_default()
                .push(serde_json::from_value(key.clone()).unwrap());
        }
    }
    for (field, target) in [
        ("filters", &mut out.audio),
        ("videoEffects", &mut out.video),
    ] {
        if let Some(links) = clip[field].as_array() {
            for (index, link) in links.iter().enumerate() {
                if let Some(keys) = link["keys"].as_object() {
                    for (name, run) in keys {
                        target.insert(
                            format!("{index}:{name}"),
                            serde_json::from_value(run.clone()).unwrap(),
                        );
                    }
                }
            }
        }
    }
    out
}
fn without_derived(mut groups: Value) -> Value {
    for scope in ["clip", "audio", "video"] {
        if let Some(runs) = groups[scope].as_object_mut() {
            for run in runs.values_mut() {
                for key in run.as_array_mut().unwrap() {
                    key.as_object_mut().unwrap().remove("derived");
                }
            }
        }
    }
    groups
}
#[tokio::test]
#[ignore = "requires scripts/qualify-concat-retime-commands.py"]
async fn actual_http_plans_match_original_command_clocks() {
    let rows: Vec<Value> = serde_json::from_slice(
        &std::fs::read(std::env::var("CONCAT_RETIME_COMMAND_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(rows.len(), 62);
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for row in rows {
        let before = row["before"].as_array().unwrap();
        let after = row["after"].as_array().unwrap();
        let clock: Clock =
            serde_json::from_value(before.iter().find(|c| c["id"] == "c").unwrap().clone())
                .unwrap();
        let(status,result,_)=support::send(&app,request(json!({"clock":clock,"rateNum":30000,"rateDen":1001,"command":command(&row["command"]),"tracks":groups(before.iter().find(|c|c["id"]=="c").unwrap()),"fadeIn":before.iter().find(|c|c["id"]=="c").unwrap()["fadeIn"].as_f64().unwrap_or(0.),"fadeOut":before.iter().find(|c|c["id"]=="c").unwrap()["fadeOut"].as_f64().unwrap_or(0.)}))).await;
        assert_eq!(status, StatusCode::OK, "{}: {result}", row["name"]);
        assert_eq!(result["changed"], row["applied"], "{} outcome", row["name"]);
        for part in result["parts"].as_array().unwrap() {
            let role = part["role"].as_str().unwrap();
            let expected = match role {
                "head" | "clip" => after.iter().find(|c| c["id"] == "c").unwrap(),
                "tail" => after
                    .iter()
                    .find(|c| c["id"] != "c" && c["id"] != "later" && c["kind"] == "video")
                    .unwrap(),
                "still" => after.iter().find(|c| c["kind"] == "image").unwrap(),
                _ => panic!("bad part role"),
            };
            let expected_tracks = without_derived(serde_json::to_value(groups(expected)).unwrap());
            assert_eq!(
                serde_json::to_vec(&without_derived(part["tracks"].clone())).unwrap(),
                serde_json::to_vec(&expected_tracks).unwrap(),
                "{} {role} source key runs",
                row["name"]
            );
            for field in ["fadeIn", "fadeOut"] {
                assert_eq!(
                    part[field].as_f64().unwrap().to_bits(),
                    expected[field].as_f64().unwrap_or(0.).to_bits(),
                    "{} {role} {field}",
                    row["name"]
                );
            }
            let expected: Clock = serde_json::from_value(expected.clone()).unwrap();
            let actual: Clock = serde_json::from_value(part["clock"].clone()).unwrap();
            assert_eq!(
                serde_json::to_vec(&actual).unwrap(),
                serde_json::to_vec(&expected).unwrap(),
                "{} {role} raw clock",
                row["name"]
            );
            let placement: ClipPlacement =
                serde_json::from_value(part["placement"].clone()).unwrap();
            assert_eq!(
                placement.timeline_start_tick,
                (expected.start * 1e6).round() as u64
            );
            assert_eq!(
                placement.timeline_duration_ticks().unwrap(),
                (expected.duration * 1e6).round() as u64
            );
            if let Some(retime) = placement.concat_retime {
                assert_eq!(retime.rate_num, 30000);
                assert_eq!(retime.rate_den, 1001);
                assert_eq!(
                    serde_json::to_vec(&retime.document_clock.unwrap()).unwrap(),
                    serde_json::to_vec(&expected).unwrap()
                );
            }
        }
        let old_neighbor = before.iter().find(|c| c["id"] == "later").unwrap()["start"]
            .as_f64()
            .unwrap();
        let expected_neighbor = after.iter().find(|c| c["id"] == "later").unwrap()["start"]
            .as_f64()
            .unwrap();
        assert_eq!(
            old_neighbor + result["rippleBy"].as_f64().unwrap(),
            expected_neighbor,
            "{} ripple",
            row["name"]
        );
    }
    println!("62 source command scenes match through actual HTTP, raw clocks/native placements, key/ease windows, fades and ripple plans");
}
#[tokio::test]
async fn rejects_invalid_clock_command_and_budget_without_tools() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let clock = json!({"start":2.,"duration":3.,"sourceStart":1.5,"speed":1.75,"speedCurve":[{"at":0.,"speed":1.},{"at":1.,"speed":3.}]});
    let valid =
        json!({"clock":clock,"rateNum":25,"rateDen":1,"command":{"kind":"split","time":2.}});
    let (status, result, _) = support::send(&app, request(valid.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["changed"], false);
    assert_eq!(result["parts"].as_array().unwrap().len(), 1);
    for kind in 0..11 {
        let mut bad = valid.clone();
        match kind {
            0 => bad["rateNum"] = json!(0),
            1 => bad["clock"]["speed"] = json!(0),
            2 => bad["command"]["time"] = json!("invalid"),
            3 => bad["command"]["unknown"] = json!(true),
            4 => {
                bad["clock"]["speedCurve"] = json!((0..4097)
                    .map(|_| json!({"at":0.,"speed":1.}))
                    .collect::<Vec<_>>())
            }
            5 => bad["clock"]["duration"] = json!(0.001),
            6 => bad["fadeIn"] = json!(-1.),
            7 => bad["tracks"] = json!({"clip":{"unknown":[{"at":0.,"value":1.}]}}),
            8 => bad["tracks"] = json!({"video":{"0:amount":[{"at":2.,"value":1.}]}}),
            9 => {
                bad["tracks"] = json!({"audio":{"0:gain":(0..4097).map(|_|json!({"at":0.,"value":1.})).collect::<Vec<_>>()}})
            }
            _ => bad["tracks"] = json!({"clip":{"opacity":[{"at":0.,"value":2.}]}}),
        }
        let (status, _, text) = support::send(&app, request(bad)).await;
        assert!(
            status.is_client_error(),
            "invalid case{kind}: {status} {text}"
        );
    }
}
