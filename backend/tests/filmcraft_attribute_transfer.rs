use serde_json::{json, Value};
use video_kadr_backend::domain::filmcraft_attribute_transfer::{retime_effect, Attributes};

#[test]
fn snapshot_admission_links_attributes_and_full_document_match_original() {
    use sha2::{Digest, Sha256};
    use video_kadr_backend::domain::filmcraft_attribute_transfer::edit;
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-attributes-oracle.json")).unwrap();
    assert_eq!(
        oracle["sourceCommit"],
        "5231852443363f001c3f6b396dd9b1e6461ae2be"
    );
    assert_eq!(oracle["cases"].as_array().unwrap().len(), 1152);
    let mut accepted = 0;
    for (i, case) in oracle["cases"].as_array().unwrap().iter().enumerate() {
        let mut params = case["params"].clone();
        params["document"] =
            json!({"format":"filmcraft.project","schema_version":12,"project":oracle["project"]});
        params["context"] = case["context"].clone();
        params["clipboardText"] = json!(serde_json::to_string(
            &oracle["clipboards"][case["clipboard"].as_u64().unwrap() as usize]
        )
        .unwrap());
        let result = edit(case["command"].as_str().unwrap(), params);
        assert_eq!(
            result.is_ok(),
            case["ok"].as_bool().unwrap(),
            "case {i}: {case}: {result:?}"
        );
        if let Ok(result) = result {
            accepted += 1;
            assert_eq!(case["undo"], 1);
            assert_eq!(result["result"], case["result"], "case {i}");
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())
                ),
                case["afterSha256"],
                "case {i}: {case}"
            );
            assert_eq!(
                serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),
                result["document"]
            );
        } else {
            assert_eq!(case["undo"], 0);
        }
    }
    assert!(accepted > 100);
}

fn clip(start: i64, duration: i64, speed: f64, effects: Value) -> Value {
    json!({"source_in":start,"duration":duration,"speed":speed,"reverse":true,"frame_hold":123,"effects":effects})
}

#[test]
fn exact_max_clip_identity_envelope_and_untrusted_clipboard_are_preserved_or_refused() {
    use video_kadr_backend::domain::filmcraft_attribute_transfer::edit;
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-attributes-oracle.json")).unwrap();
    let sequence = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|c| c["context"]["activeSequence"].as_u64())
        .unwrap();
    let mut doc = json!({"format":"filmcraft.project","schema_version":12,"project":oracle["project"],"extension":{"id":u64::MAX,"tick":9007199254740993_i64}});
    let path = format!("/project/items/{sequence}/kind/Sequence/video_tracks/0/items/0/id");
    *doc.pointer_mut(&path).unwrap() = json!(u64::MAX);
    let params = json!({"document":serde_json::to_string(&doc).unwrap(),"clips":[u64::MAX.to_string()],"context":{"activeSequence":sequence.to_string(),"linkedSelection":false}});
    let result = edit("edit.removeAttributes", params.clone()).unwrap();
    let exact: Value = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(exact.pointer(&path), Some(&json!(u64::MAX)));
    assert_eq!(exact["extension"], doc["extension"]);
    for malformed in [
        "{",
        "null",
        "{}",
        "{\"format\":\"video-kadr.filmcraft-clipboard\",\"version\":1,\"clips\":[],\"markers\":[]}",
    ] {
        let mut p = params.clone();
        p["clipboardText"] = json!(malformed);
        assert!(edit("edit.pasteAttributes", p).is_err());
    }
    let mut hostile = oracle["clipboards"][1].clone();
    hostile["clips"][0][2]["source_in"] = json!("9223372036854775807");
    let mut p = params;
    p["clipboardText"] = json!(serde_json::to_string(&hostile).unwrap());
    assert!(edit("edit.pasteAttributes", p).is_err());
}
#[test]
fn retimes_parameters_and_all_mask_channels_without_changing_animation_shape() {
    let animated = json!({"value":{"Float":1.0},"keyframes":[
        {"time":9,"value":{"Float":0.0},"interpolation":"Bezier","out_tangent":[1,2]},
        {"time":13,"value":{"Float":2.0},"interpolation":"Linear"}
    ]});
    let effect = json!({"effect":"opacity","enabled":false,"essential":true,
        "params":{"opacity":animated},"masks":[{"path":animated,"feather":animated,"opacity":animated,"expansion":animated,"inverted":true}]});
    let from = clip(10, 4, -2.0, json!([]));
    let to = clip(100, 8, 0.5, json!([]));
    let mapped = retime_effect(&effect, &from, &to, true).unwrap();
    for path in [
        "/params/opacity",
        "/masks/0/path",
        "/masks/0/feather",
        "/masks/0/opacity",
        "/masks/0/expansion",
    ] {
        let keys = &mapped.pointer(path).unwrap()["keyframes"];
        assert_eq!(keys[0]["time"], 99); // round(-0.5) is away from zero
        assert_eq!(keys[1]["time"], 102);
        assert_eq!(keys[0]["out_tangent"], json!([1, 2]));
    }
    assert_eq!(mapped["enabled"], false);
    assert_eq!(mapped["masks"][0]["inverted"], true);
    let shifted = retime_effect(&effect, &from, &to, false).unwrap();
    assert_eq!(shifted["params"]["opacity"]["keyframes"][1]["time"], 103);
    assert_eq!(effect["params"]["opacity"]["keyframes"][0]["time"], 9);
}
#[test]
fn zero_span_offsets_and_overflow_refuses_atomically() {
    let effect = json!({"params":{"x":{"keyframes":[{"time":12}]}}});
    let from = clip(10, 10, 0.0, json!([]));
    let to = clip(100, 10, 1.0, json!([]));
    assert_eq!(
        retime_effect(&effect, &from, &to, true).unwrap()["params"]["x"]["keyframes"][0]["time"],
        102
    );
    assert!(retime_effect(&effect, &from, &clip(i64::MAX, 1, 1.0, json!([])), true).is_err());
    let malformed = json!({"params":{"x":{"keyframes":[{"time":12},{"time":"13"}]}}});
    assert!(retime_effect(&malformed, &from, &to, true).is_err());
    assert_eq!(malformed["params"]["x"]["keyframes"][0]["time"], 12);
}
#[test]
fn paste_appends_standard_effects_preserves_unknown_and_essential_and_replaces_only_first_intrinsic(
) {
    let from = clip(
        0,
        10,
        1.0,
        json!([
            {"effect":"opacity","enabled":false,"params":{}},
            {"effect":"brightness_contrast","params":{}},
            {"effect":"graphic_text","params":{}},
            {"effect":"brightness_contrast","essential":true,"params":{}},
            {"effect":"unknown_extension","params":{}}
        ]),
    );
    let to = clip(
        10,
        20,
        1.0,
        json!([
            {"effect":"opacity","enabled":true,"params":{}},
            {"effect":"opacity","enabled":true,"params":{}},
            {"effect":"brightness_contrast","params":{}}
        ]),
    );
    let result = Attributes::new(&json!({})).paste(&from, &to).unwrap();
    assert_eq!(result["effects"].as_array().unwrap().len(), 4);
    assert_eq!(result["effects"][0]["enabled"], false);
    assert_eq!(result["effects"][1]["enabled"], true);
    assert_eq!(result["source_in"], 10);
    let twice = Attributes::new(&json!({})).paste(&from, &result).unwrap();
    assert_eq!(twice["effects"].as_array().unwrap().len(), 5);
    let excluded = Attributes::new(&json!({"effects":false,"opacity":false}))
        .paste(&from, &to)
        .unwrap();
    assert_eq!(excluded, to);
}
#[test]
fn remove_uses_source_anchor_and_target_position_and_preserves_unselected_effects() {
    let source = clip(
        0,
        10,
        1.0,
        json!([
            {"effect":"motion","enabled":false,"essential":true,"params":{},"masks":[{}]},
            {"effect":"brightness_contrast","params":{}},
            {"effect":"brightness_contrast","essential":true,"params":{}},
            {"effect":"unknown_extension","params":{}}
        ]),
    );
    let result = Attributes::new(&json!({}))
        .remove(&source, (1920, 1080), (640, 480))
        .unwrap();
    assert_eq!(result["effects"].as_array().unwrap().len(), 3);
    let motion = &result["effects"][0];
    assert_eq!(
        motion["params"]["anchor"]["value"]["Vec2"],
        json!({"x":320.0,"y":240.0})
    );
    assert_eq!(
        motion["params"]["position"]["value"]["Vec2"],
        json!({"x":960.0,"y":540.0})
    );
    assert_eq!(motion["enabled"], true);
    assert!(motion.get("essential").is_none());
    assert!(motion.get("masks").is_none());
    assert_eq!(
        Attributes::new(&json!({"motion":false,"effects":[]}))
            .remove(&source, (1, 1), (2, 2))
            .unwrap(),
        source
    );
}
