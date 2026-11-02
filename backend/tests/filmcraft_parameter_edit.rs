#![recursion_limit = "256"]
mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_parameter_edit::{self, Address, Command},
    filmcraft_project_file::SourceProjectDocument,
};
const SEQUENCE: u64 = u64::MAX;
const CLIP: u64 = u64::MAX - 1;
const SOURCE: i64 = 9007199254740993;
fn document() -> SourceProjectDocument {
    SourceProjectDocument::read(&serde_json::to_vec(&json!({"format":"filmcraft.project","schema_version":12,"generator":"FilmCraft","extension":{"preserve":true},"project":{"items":{SEQUENCE.to_string():{"id":SEQUENCE,"name":"Source","kind":{"Sequence":{"settings":{"frame_rate":{"num":24,"den":1}},"video_tracks":[{"id":5,"locked":false,"items":[{"id":CLIP,"item":1,"start":100,"duration":1000,"source_in":SOURCE,"speed":2.,"enabled":true,"effects":[{"effect":"gamma_correction","enabled":true,"opaque":"keep","params":{"gamma":{"value":{"Float":1.},"keyframes":[{"time":SOURCE,"value":{"Float":1.},"interp":"Linear","in_influence":0.3,"out_influence":0.4,"opaque":"keep"},{"time":SOURCE+1000,"value":{"Float":2.},"interp":"Bezier","in_influence":0.5,"out_influence":0.6}]}}}]}]}],"audio_tracks":[]}}}},"next_id":1,"opaque":"preserve"}})).unwrap()).unwrap()
}
fn address() -> Address {
    Address {
        sequence_id: SEQUENCE.to_string(),
        clip_id: CLIP.to_string(),
        effect_index: 0,
        effect_kind: "gamma_correction".into(),
        parameter: "gamma".into(),
        mask_index: None,
    }
}
fn item_path() -> String {
    format!("/project/items/{SEQUENCE}/kind/Sequence/video_tracks/0/items/0")
}
#[test]
fn source_ticks_and_ids_are_strings_at_inspection_boundary() {
    let source = document();
    let report = filmcraft_parameter_edit::inspect(&source, SEQUENCE, CLIP, 101).unwrap();
    assert_eq!(report["parameterTicks"], (SOURCE + 2).to_string());
    assert_eq!(
        report["effects"][0]["parameters"][0]["keys"][0]["time"],
        SOURCE.to_string()
    );
    assert!(
        report["effects"][0]["parameters"][0]["sampled"]
            .as_f64()
            .unwrap()
            > 1.
    );
}
#[test]
fn authoring_retains_every_opaque_field_and_exact_tick() {
    let source = document();
    let changed = filmcraft_parameter_edit::edit(
        &source,
        &address(),
        Command::Upsert {
            time: (SOURCE + 1).to_string(),
            value: json!({"Float":1.25}),
        },
        101,
    )
    .unwrap();
    let path = format!("{}/effects/0/params/gamma/keyframes", item_path());
    let keys = changed.envelope.pointer(&path).unwrap().as_array().unwrap();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[1]["time"].as_i64(), Some(SOURCE + 1));
    assert_eq!(keys[0]["opaque"], "keep");
    let mut expected = source.envelope.clone();
    expected
        .pointer_mut(&path)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .insert(1, keys[1].clone());
    assert_eq!(changed.envelope, expected);
    assert_eq!(
        source
            .envelope
            .pointer(&path)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn playhead_insertion_uses_independent_forward_reverse_and_held_parameter_clocks() {
    for (reverse, held, hold_filters, expected) in [
        (false, None, false, SOURCE + 2),
        (true, None, false, SOURCE + 1997),
        (true, Some(SOURCE + 77), false, SOURCE + 1997),
        (true, Some(SOURCE + 77), true, SOURCE + 77),
    ] {
        let mut source = document();
        let item = source.envelope.pointer_mut(&item_path()).unwrap();
        item["reverse"] = json!(reverse);
        item["frame_hold"] = json!(held);
        item["hold_filters"] = json!(hold_filters);
        let edited = filmcraft_parameter_edit::edit(
            &source,
            &address(),
            Command::AtPlayhead {
                value: json!({"Float":1.5}),
            },
            101,
        )
        .unwrap();
        let report = filmcraft_parameter_edit::inspect(&edited, SEQUENCE, CLIP, 101).unwrap();
        assert!(report["effects"][0]["parameters"][0]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key["time"] == expected.to_string()));
    }
}
#[test]
fn collisions_lock_and_unknown_effect_refuse_without_mutation() {
    let source = document();
    let before = source.envelope.clone();
    assert!(filmcraft_parameter_edit::edit(
        &source,
        &address(),
        Command::Move {
            time: SOURCE.to_string(),
            destination: (SOURCE + 1000).to_string()
        },
        101
    )
    .is_err());
    let mut wrong = address();
    wrong.effect_kind = "other".into();
    assert!(filmcraft_parameter_edit::edit(
        &source,
        &wrong,
        Command::Delete {
            time: SOURCE.to_string()
        },
        101
    )
    .is_err());
    assert_eq!(source.envelope, before);
    let mut locked = source.clone();
    *locked
        .envelope
        .pointer_mut(&format!(
            "/project/items/{SEQUENCE}/kind/Sequence/video_tracks/0/locked"
        ))
        .unwrap() = json!(true);
    assert!(filmcraft_parameter_edit::edit(
        &locked,
        &address(),
        Command::SetBase {
            value: json!({"Float":2.})
        },
        101
    )
    .is_err());
}
#[tokio::test]
async fn http_source_parameter_edit_preserves_lossless_document_transport() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let source = document();
    let text = String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let request = |path: &str, value: Value| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    };
    let response=app.clone().oneshot(request("/api/compositions/filmcraft/parameters",json!({"document":text,"sequenceId":SEQUENCE.to_string(),"clipId":CLIP.to_string(),"timeTicks":"101"}))).await.unwrap();
    assert_eq!(response.status(), 200);
    let report: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(report["parameterTicks"], (SOURCE + 2).to_string());
    let response=app.oneshot(request("/api/compositions/filmcraft/edit-parameter",json!({"document":text,"address":{"sequenceId":SEQUENCE.to_string(),"clipId":CLIP.to_string(),"effectIndex":0,"effectKind":"gamma_correction","parameter":"gamma"},"command":{"kind":"atPlayhead","value":{"Float":1.75}},"timeTicks":"101"}))).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let manifest: Value = serde_json::from_slice(&body).unwrap();
    let edited: Value = serde_json::from_str(manifest["documentText"].as_str().unwrap()).unwrap();
    assert!(edited
        .pointer(&format!("{}/effects/0/params/gamma/keyframes", item_path()))
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|key| key["time"].as_i64() == Some(SOURCE + 2) && key["value"]["Float"] == 1.75));
    assert_eq!(manifest["sequences"][0]["id"], SEQUENCE.to_string());
}

fn parameter_path() -> String {
    format!("{}/effects/0/params/gamma", item_path())
}
fn with_parameter(parameter: Value) -> SourceProjectDocument {
    let mut source = document();
    *source.envelope.pointer_mut(&parameter_path()).unwrap() = parameter;
    source
}
#[test]
fn all_original_point_and_rgba_samples_are_exposed_without_tick_lowering() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-component-param.json")).unwrap();
    assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3528);
    for (index, case) in cases.iter().enumerate() {
        let mut source = with_parameter(case["parameter"].clone());
        let at = case["at"].as_str().unwrap().parse::<i64>().unwrap();
        source.envelope.pointer_mut(&item_path()).unwrap()["source_in"] = json!(at);
        let report = filmcraft_parameter_edit::inspect(&source, SEQUENCE, CLIP, 100).unwrap();
        assert_eq!(report["parameterTicks"], at.to_string());
        let values = report["effects"][0]["parameters"][0]["sampledComponents"]
            .as_array()
            .unwrap();
        let expected = case["bits"].as_array().unwrap();
        assert_eq!(values.len(), expected.len());
        for (component, (value, expected)) in values.iter().zip(expected).enumerate() {
            let value = value.as_f64().unwrap();
            let bits = if case["kind"] == "color" {
                (value as f32).to_bits() as u64
            } else {
                value.to_bits()
            };
            assert_eq!(
                bits,
                expected.as_u64().unwrap(),
                "case {index}, component {component}"
            );
        }
    }
}
#[test]
fn point_and_rgba_edit_commands_preserve_components_alpha_and_opaque_siblings() {
    for (baseline, keyed, edited) in [
        (
            json!({"Vec2":{"x":null,"y":0.25}}),
            json!({"Vec2":{"x":0.1,"y":0.2}}),
            json!({"Vec2":{"x":-0.123456789012345,"y":null}}),
        ),
        (
            json!({"Color":[0.1,0.2,0.3,0.4]}),
            json!({"Color":[0.2,0.3,0.4,0.5]}),
            json!({"Color":[-0.5,1.25,0.75,0.125]}),
        ),
    ] {
        let original = with_parameter(
            json!({"value":baseline,"extension":{"retained":true},"keyframes":[{"time":SOURCE,"value":keyed,"interp":"Bezier","in_influence":0.2,"out_influence":0.7,"extension":"key"}]}),
        );
        let before = original.envelope.clone();
        let changed = filmcraft_parameter_edit::edit(
            &original,
            &address(),
            Command::Upsert {
                time: SOURCE.to_string(),
                value: edited.clone(),
            },
            101,
        )
        .unwrap();
        let mut expected = before.clone();
        expected.pointer_mut(&parameter_path()).unwrap()["keyframes"][0]["value"] = edited.clone();
        assert_eq!(changed.envelope, expected);
        assert_eq!(original.envelope, before);
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Move {
                time: SOURCE.to_string(),
                destination: (SOURCE + 1).to_string(),
            },
            101,
        )
        .unwrap();
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Interpolation {
                time: (SOURCE + 1).to_string(),
                interpolation: "EaseOut".into(),
            },
            101,
        )
        .unwrap();
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Influence {
                time: (SOURCE + 1).to_string(),
                incoming: 0.9,
                outgoing: 0.1,
            },
            101,
        )
        .unwrap();
        let parameter = changed.envelope.pointer(&parameter_path()).unwrap();
        assert_eq!(parameter["keyframes"][0]["value"], edited);
        assert_eq!(parameter["keyframes"][0]["extension"], "key");
        assert_eq!(parameter["keyframes"][0]["time"].as_i64(), Some(SOURCE + 1));
        assert_eq!(parameter["keyframes"][0]["interp"], "EaseOut");
        assert_eq!(parameter["keyframes"][0]["in_influence"], 0.9);
        assert_eq!(parameter["keyframes"][0]["out_influence"], 0.1);
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::AtPlayhead {
                value: edited.clone(),
            },
            101,
        )
        .unwrap();
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::SetBase {
                value: edited.clone(),
            },
            101,
        )
        .unwrap();
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Delete {
                time: (SOURCE + 1).to_string(),
            },
            101,
        )
        .unwrap();
        let parameter = changed.envelope.pointer(&parameter_path()).unwrap();
        assert_eq!(parameter["value"], edited);
        assert_eq!(parameter["extension"]["retained"], true);
        assert_eq!(parameter["keyframes"].as_array().unwrap().len(), 1);
        assert_eq!(parameter["keyframes"][0]["time"].as_i64(), Some(SOURCE + 2));
    }
}
#[test]
fn invalid_component_shapes_and_overflow_refuse_without_source_mutation() {
    let source = with_parameter(json!({"value":{"Color":[0.1,0.2,0.3,0.4]},"keyframes":[]}));
    let before = source.envelope.clone();
    for value in [
        json!({"Color":[0.,1.,0.]}),
        json!({"Color":[0.,1.,0.,1e100]}),
        json!({"Color":[0.,1.,null,1.]}),
        json!({"Vec2":{"x":0.}}),
        json!({"Vec2":{"x":0.,"y":1.,"z":2.}}),
        json!({"Vec2":{"x":"auto","y":0.}}),
        json!({"Color":[0.,0.,0.,1.],"Float":1.}),
    ] {
        assert!(filmcraft_parameter_edit::edit(
            &source,
            &address(),
            Command::Upsert {
                time: SOURCE.to_string(),
                value
            },
            101
        )
        .is_err());
        assert_eq!(source.envelope, before);
    }
    let auto = with_parameter(json!({"value":{"Vec2":{"x":null,"y":null}}}));
    let report = filmcraft_parameter_edit::inspect(&auto, SEQUENCE, CLIP, 101).unwrap();
    assert_eq!(
        report["effects"][0]["parameters"][0]["value"],
        json!({"Vec2":{"x":null,"y":null}})
    );
    assert!(report["effects"][0]["parameters"][0]["sampledComponents"].is_null());
}

#[tokio::test]
async fn http_edited_point_and_color_keys_reach_real_program_raster() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut source = fixture["cases"][0]["document"].clone();
    let path = "/project/items/6/kind/Sequence/video_tracks/0/items/0";
    let clip = source.pointer_mut(path).unwrap();
    let clip_id = clip["id"].as_u64().unwrap().to_string();
    clip["source_in"] = json!(SOURCE);
    clip["speed"] = json!(2.);
    clip["effects"] = json!([{"effect":"ramp","enabled":true,"masks":[],"params":{
      "start":{"value":{"Vec2":{"x":0.,"y":0.}},"keyframes":[{"time":SOURCE,"value":{"Vec2":{"x":0.,"y":0.}},"interp":"Linear"},{"time":SOURCE+4,"value":{"Vec2":{"x":4.,"y":0.}},"interp":"Linear"}]},
      "end":{"value":{"Vec2":{"x":8.,"y":8.}}},
      "start_color":{"value":{"Color":[1.,0.,0.,1.]},"keyframes":[{"time":SOURCE,"value":{"Color":[1.,0.,0.,1.]},"interp":"Linear"},{"time":SOURCE+4,"value":{"Color":[0.,0.,1.,1.]},"interp":"Linear"}]},
      "end_color":{"value":{"Color":[1.,1.,1.,1.]}},"blend":{"value":{"Float":0.}},"shape":{"value":{"Choice":0}}
    }}]);
    let original = source.clone();
    let request = |route: &str, value: Value| {
        Request::builder()
            .method("POST")
            .uri(route)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    };
    for point in [
        json!({"Vec2":{"x":-4.,"y":2.}}),
        json!({"Vec2":{"x":null,"y":null}}),
    ] {
        let mut text = source.to_string();
        let edits = [
            ("start", point),
            ("start_color", json!({"Color":[0.,1.,0.,0.25]})),
        ];
        for (name, value) in &edits {
            let response=app.clone().oneshot(request("/api/compositions/filmcraft/edit-parameter",json!({"document":text,"address":{"sequenceId":"6","clipId":clip_id,"effectIndex":0,"effectKind":"ramp","parameter":name},"command":{"kind":"atPlayhead","value":value},"timeTicks":"1"}))).await.unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
            let manifest: Value = serde_json::from_slice(&body).unwrap();
            text = manifest["documentText"].as_str().unwrap().into();
        }
        let edited: Value = serde_json::from_str(&text).unwrap();
        let mut reference = original.clone();
        for (name, value) in edits {
            reference
                .pointer_mut(&format!("{path}/effects/0/params"))
                .unwrap()[name] = json!({"value":value});
        }
        let mut rasters = vec![];
        for document in [&original, &edited, &reference] {
            let response = app
                .clone()
                .oneshot(request(
                    "/api/compositions/filmcraft/preview-frame",
                    json!({"document":document.to_string(),"sequenceId":"6","timeTicks":"1"}),
                ))
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
            let frame: Value = serde_json::from_slice(&body).unwrap();
            rasters.push(frame["rgbaHex"].as_str().unwrap().to_owned());
        }
        assert_ne!(
            rasters[0], rasters[1],
            "key edits must visibly change the program frame"
        );
        assert_eq!(
            rasters[1], rasters[2],
            "source key pose must match constant parameter reference"
        );
        assert_eq!(
            source, original,
            "pure document editing and preview must not mutate the source"
        );
    }
}

#[test]
fn text_and_curve_values_keep_unicode_row_order_and_original_hold_sampling() {
    for (baseline, first, last) in [
        (
            json!({"Text":"Base"}),
            json!({"Text":"Текст\n🎬 é"}),
            json!({"Text":"字幕\nfinal"}),
        ),
        (
            json!({"Curve":[]}),
            json!({"Curve":[[1.25,-0.5],[-2.,3.],[0.25,0.75]]}),
            json!({"Curve":[[0.,1.],[1.,0.]]}),
        ),
    ] {
        let source = with_parameter(
            json!({"value":baseline,"opaque":"param","keyframes":[{"time":SOURCE,"value":first,"interp":"Bezier","opaque":"key"},{"time":SOURCE+100,"value":last,"interp":"EaseIn"}]}),
        );
        for (offset, expected) in [
            (-1, &first),
            (0, &first),
            (1, &first),
            (99, &first),
            (100, &last),
            (101, &last),
        ] {
            let mut sampled = source.clone();
            sampled.envelope.pointer_mut(&item_path()).unwrap()["source_in"] =
                json!(SOURCE + offset);
            let report = filmcraft_parameter_edit::inspect(&sampled, SEQUENCE, CLIP, 100).unwrap();
            assert_eq!(
                &report["effects"][0]["parameters"][0]["sampledValue"],
                expected
            );
        }
        let changed = filmcraft_parameter_edit::edit(
            &source,
            &address(),
            Command::Upsert {
                time: SOURCE.to_string(),
                value: last.clone(),
            },
            101,
        )
        .unwrap();
        let mut expected = source.envelope.clone();
        expected.pointer_mut(&parameter_path()).unwrap()["keyframes"][0]["value"] = last.clone();
        assert_eq!(changed.envelope, expected);
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Delete {
                time: (SOURCE + 100).to_string(),
            },
            101,
        )
        .unwrap();
        let changed = filmcraft_parameter_edit::edit(
            &changed,
            &address(),
            Command::Delete {
                time: SOURCE.to_string(),
            },
            101,
        )
        .unwrap();
        let param = changed.envelope.pointer(&parameter_path()).unwrap();
        assert_eq!(
            param["value"], last,
            "deleting the last key keeps its value, as original Param::remove_keyframe_at"
        );
        assert_eq!(param["opaque"], "param");
        assert!(param["keyframes"].as_array().unwrap().is_empty());
    }
}
#[test]
fn mask_path_authoring_keeps_tangents_closure_and_extensions() {
    let path = json!({"Path":{"closed":false,"opaque":"path","vertices":[{"p":{"x":0.123456789012345,"y":2.},"t_in":{"x":-1.,"y":0.5},"t_out":{"x":1.,"y":-0.5},"opaque":"vertex"},{"p":{"x":3.,"y":4.}}]}});
    let mut source = document();
    source
        .envelope
        .pointer_mut(&format!("{}/effects/0", item_path()))
        .unwrap()["masks"] = json!([{"name":"Bezier","mode":"Add","opaque":"mask","path":{"value":path,"opaque":"parameter"},"feather":{"value":{"Float":0.75}},"opacity":{"value":{"Float":65.}},"expansion":{"value":{"Float":-0.25}}}]);
    let report = filmcraft_parameter_edit::inspect(&source, SEQUENCE, CLIP, 101).unwrap();
    assert_eq!(
        report["effects"][0]["masks"][0]["parameters"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let mut address = address();
    address.mask_index = Some(0);
    address.parameter = "path".into();
    let before = source.envelope.clone();
    let mut edited = path.clone();
    edited["Path"]["vertices"][0]["p"]["x"] = json!(-1.25);
    let changed = filmcraft_parameter_edit::edit(
        &source,
        &address,
        Command::AtPlayhead {
            value: edited.clone(),
        },
        101,
    )
    .unwrap();
    let pointer = format!("{}/effects/0/masks/0/path", item_path());
    let param = changed.envelope.pointer(&pointer).unwrap();
    assert_eq!(param["keyframes"][0]["time"].as_i64(), Some(SOURCE + 2));
    assert_eq!(param["keyframes"][0]["value"], edited);
    assert_eq!(param["value"], path);
    let mut expected = before.clone();
    expected.pointer_mut(&pointer).unwrap()["keyframes"] = param["keyframes"].clone();
    assert_eq!(changed.envelope, expected);
    assert_eq!(source.envelope, before);
    address.parameter = "name".into();
    assert!(filmcraft_parameter_edit::edit(
        &source,
        &address,
        Command::SetBase {
            value: json!({"Text":"bad"})
        },
        101
    )
    .is_err());
}
#[test]
fn invalid_text_curve_and_path_values_refuse_before_document_change() {
    let source = document();
    let before = source.envelope.clone();
    for value in [
        json!({"Text":4}),
        json!({"Text":"x".repeat(65537)}),
        json!({"Curve":[[0.,1.,2.]]}),
        json!({"Curve":[[0.,null]]}),
        json!({"Curve":[[0.,1e100]]}),
        json!({"Path":{"vertices":[{"p":{"x":null,"y":0.}}]}}),
        json!({"Path":{"vertices":[{"p":{"x":0.}}]}}),
        json!({"Path":{"closed":"yes","vertices":[]}}),
        json!({"Path":{"vertices":[{"p":{"x":0.,"y":0.},"t_in":{"x":0.,"y":null}}]}}),
    ] {
        assert!(filmcraft_parameter_edit::edit(
            &source,
            &address(),
            Command::Upsert {
                time: SOURCE.to_string(),
                value
            },
            101
        )
        .is_err());
        assert_eq!(source.envelope, before);
    }
}
#[tokio::test]
async fn edited_source_bezier_mask_path_changes_the_real_program_frame() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut source = fixture["cases"][0]["document"].clone();
    let pointer = "/project/items/6/kind/Sequence/video_tracks/0/items/0";
    let clip = source.pointer_mut(pointer).unwrap();
    clip["source_in"] = json!(SOURCE);
    clip["speed"] = json!(2.);
    clip["effects"][0]["masks"][0]["mode"] = json!("Add");
    let clip_id = clip["id"].as_u64().unwrap().to_string();
    let path = json!({"Path":{"closed":true,"vertices":[{"p":{"x":0.5,"y":0.5},"t_out":{"x":2.,"y":0.}},{"p":{"x":4.,"y":1.},"t_in":{"x":-1.,"y":-0.25}},{"p":{"x":1.,"y":4.}}]}});
    let request = |route: &str, value: Value| {
        Request::builder()
            .method("POST")
            .uri(route)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    };
    let response=app.clone().oneshot(request("/api/compositions/filmcraft/edit-parameter",json!({"document":source.to_string(),"address":{"sequenceId":"6","clipId":clip_id,"effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"},"command":{"kind":"atPlayhead","value":path},"timeTicks":"1"}))).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let manifest: Value = serde_json::from_slice(&body).unwrap();
    let edited: Value = serde_json::from_str(manifest["documentText"].as_str().unwrap()).unwrap();
    let mut reference = source.clone();
    reference
        .pointer_mut(&format!("{pointer}/effects/0/masks/0"))
        .unwrap()["path"] = json!({"value":path});
    let mut frames = vec![];
    for document in [&source, &edited, &reference] {
        let response = app
            .clone()
            .oneshot(request(
                "/api/compositions/filmcraft/preview-frame",
                json!({"document":document.to_string(),"sequenceId":"6","timeTicks":"1"}),
            ))
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let frame: Value = serde_json::from_slice(&body).unwrap();
        frames.push(frame["rgbaHex"].clone());
    }
    assert_ne!(frames[0], frames[1]);
    assert_eq!(frames[1], frames[2]);
}

#[tokio::test]
async fn edited_source_text_and_curve_reach_real_graphic_program_frames() {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let graphic: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-graphic.json")).unwrap();
    let text: Value = serde_json::from_str(include_str!(
        "fixtures/filmcraft-graphic-text-authored.json"
    ))
    .unwrap();
    let pointer = "/project/items/6/kind/Sequence/video_tracks/0/items/0";
    let shape = graphic["cases"][0]["document"]
        .pointer(&format!("{pointer}/effects/0"))
        .unwrap()
        .clone();
    let mut text_effect = text["cases"][0]["effect"].clone();
    text_effect["enabled"] = json!(true);
    for (mut effect, name, original_value, edited_value, at_playhead) in [
        (
            shape,
            "points",
            json!({"Curve":[[0.,0.],[20.,0.],[0.,20.]]}),
            json!({"Curve":[[0.,0.],[24.,0.],[24.,14.],[0.,14.]]}),
            false,
        ),
        (
            text_effect,
            "text",
            json!({"Text":"Office AV"}),
            json!({"Text":"Hi"}),
            true,
        ),
    ] {
        if name == "points" {
            effect["params"]["shape"] = json!({"value":{"Choice":3}});
            effect["params"]["rotation"] = json!({"value":{"Float":0.}});
            effect["params"]["position"] = json!({"value":{"Vec2":{"x":10.,"y":10.}}});
        }
        effect["params"][name] = json!({"value":original_value});
        let kind = effect["effect"].as_str().unwrap().to_owned();
        let mut source = graphic["cases"][0]["document"].clone();
        let clip = source.pointer_mut(pointer).unwrap();
        clip["source_in"] = json!(SOURCE);
        clip["speed"] = json!(2.);
        clip["effects"] = json!([effect]);
        let clip_id = clip["id"].as_u64().unwrap().to_string();
        let request = |route: &str, value: Value| {
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap()
        };
        let response=app.clone().oneshot(request("/api/compositions/filmcraft/edit-parameter",json!({"document":source.to_string(),"address":{"sequenceId":"6","clipId":clip_id,"effectIndex":0,"effectKind":kind,"parameter":name},"command":{"kind":if at_playhead{"atPlayhead"}else{"setBase"},"value":edited_value},"timeTicks":"1"}))).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let manifest: Value = serde_json::from_slice(&body).unwrap();
        let edited: Value =
            serde_json::from_str(manifest["documentText"].as_str().unwrap()).unwrap();
        let mut reference = source.clone();
        reference
            .pointer_mut(&format!("{pointer}/effects/0/params"))
            .unwrap()[name] = json!({"value":edited_value});
        let mut frames = vec![];
        for document in [&source, &edited, &reference] {
            let response = app
                .clone()
                .oneshot(request(
                    "/api/compositions/filmcraft/preview-frame",
                    json!({"document":document.to_string(),"sequenceId":"6","timeTicks":"1"}),
                ))
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap();
            assert_eq!(status, 200, "{name}: {}", String::from_utf8_lossy(&body));
            let frame: Value = serde_json::from_slice(&body).unwrap();
            frames.push(frame["rgbaHex"].clone());
        }
        assert_ne!(
            frames[0], frames[1],
            "{name} edit must change visible pixels"
        );
        assert_eq!(
            frames[1], frames[2],
            "{name} edit must match independently built reference"
        );
    }
}

fn effect_address() -> filmcraft_parameter_edit::EffectAddress {
    filmcraft_parameter_edit::EffectAddress {
        sequence_id: SEQUENCE.to_string(),
        clip_id: CLIP.to_string(),
        effect_index: 0,
        effect_kind: "gamma_correction".into(),
    }
}
fn mask_command(value: Value) -> filmcraft_parameter_edit::MaskCommand {
    serde_json::from_value(value).unwrap()
}
#[test]
fn mask_shapes_and_defaults_follow_original_mask_new_and_bezier_ellipse() {
    let source = document();
    let changed=filmcraft_parameter_edit::edit_mask(&source,&effect_address(),mask_command(json!({"kind":"create","name":"Прямоугольник","shape":{"kind":"rectangle","x0":1.,"y0":2.,"x1":7.,"y1":6.}}))).unwrap();
    let pointer = format!("{}/effects/0/masks", item_path());
    let mask = &changed.envelope.pointer(&pointer).unwrap()[0];
    assert_eq!(
        mask,
        &json!({"name":"Прямоугольник","path":{"value":{"Path":{"closed":true,"vertices":[{"p":{"x":1.,"y":2.}},{"p":{"x":7.,"y":2.}},{"p":{"x":7.,"y":6.}},{"p":{"x":1.,"y":6.}}]}}},"feather":{"value":{"Float":10.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}},"inverted":false,"mode":"Add","track_method":"PositionScaleRotation"})
    );
    let mut expected = source.envelope.clone();
    expected
        .pointer_mut(&format!("{}/effects/0", item_path()))
        .unwrap()["masks"] = json!([mask]);
    assert_eq!(changed.envelope, expected);
    let changed=filmcraft_parameter_edit::edit_mask(&changed,&effect_address(),mask_command(json!({"kind":"create","name":"Эллипс","shape":{"kind":"ellipse","x0":1.,"y0":2.,"x1":7.,"y1":6.}}))).unwrap();
    let vertices =
        &changed.envelope.pointer(&pointer).unwrap()[1]["path"]["value"]["Path"]["vertices"];
    let k = 0.552_284_749_830_793_4;
    assert_eq!(
        vertices,
        &json!([{"p":{"x":4.,"y":2.},"t_in":{"x":-3.*k,"y":0.},"t_out":{"x":3.*k,"y":0.}},{"p":{"x":7.,"y":4.},"t_in":{"x":0.,"y":-2.*k},"t_out":{"x":0.,"y":2.*k}},{"p":{"x":4.,"y":6.},"t_in":{"x":3.*k,"y":0.},"t_out":{"x":-3.*k,"y":0.}},{"p":{"x":1.,"y":4.},"t_in":{"x":0.,"y":2.*k},"t_out":{"x":0.,"y":-2.*k}}])
    );
    let changed = filmcraft_parameter_edit::edit_mask(
        &changed,
        &effect_address(),
        mask_command(json!({"kind":"create","name":"Pen","shape":{"kind":"freeDraw"}})),
    )
    .unwrap();
    assert_eq!(
        changed.envelope.pointer(&pointer).unwrap()[2]["path"]["value"]["Path"],
        json!({"closed":false,"vertices":[]})
    );
    assert!(source.envelope.pointer(&pointer).is_none());
}
#[test]
fn mask_order_and_settings_preserve_original_keys_extensions_and_exact_ids() {
    let source = document();
    let mut changed = source.clone();
    for name in ["A", "B", "C"] {
        changed = filmcraft_parameter_edit::edit_mask(
            &changed,
            &effect_address(),
            mask_command(json!({"kind":"create","name":name,"shape":{"kind":"freeDraw"}})),
        )
        .unwrap();
    }
    let pointer = format!("{}/effects/0/masks", item_path());
    let masks = changed
        .envelope
        .pointer_mut(&pointer)
        .unwrap()
        .as_array_mut()
        .unwrap();
    masks[1]["opaqueId"] = json!(u64::MAX);
    masks[1]["path"]["keyframes"] = json!([{"time":SOURCE,"value":{"Path":{"closed":false,"vertices":[]}},"interp":"Hold","opaqueId":u64::MAX}]);
    let preserved = masks[1].clone();
    let moved = filmcraft_parameter_edit::edit_mask(
        &changed,
        &effect_address(),
        mask_command(json!({"kind":"move","maskIndex":1,"destination":0})),
    )
    .unwrap();
    assert_eq!(moved.envelope.pointer(&pointer).unwrap()[0], preserved);
    for mode in [
        "None",
        "Add",
        "Subtract",
        "Intersect",
        "Lighten",
        "Darken",
        "Difference",
    ] {
        for method in ["Position", "PositionRotation", "PositionScaleRotation"] {
            let edited=filmcraft_parameter_edit::edit_mask(&moved,&effect_address(),mask_command(json!({"kind":"settings","maskIndex":0,"name":"字幕 🎬","mode":mode,"inverted":true,"trackMethod":method}))).unwrap();
            let mut expected = moved.envelope.clone();
            let mask = &mut expected.pointer_mut(&pointer).unwrap()[0];
            mask["name"] = json!("字幕 🎬");
            mask["mode"] = json!(mode);
            mask["inverted"] = json!(true);
            mask["track_method"] = json!(method);
            assert_eq!(edited.envelope, expected);
        }
    }
    let removed = filmcraft_parameter_edit::edit_mask(
        &moved,
        &effect_address(),
        mask_command(json!({"kind":"delete","maskIndex":1})),
    )
    .unwrap();
    assert_eq!(removed.envelope.pointer(&pointer).unwrap()[0], preserved);
    assert_eq!(removed.envelope.pointer(&pointer).unwrap()[1]["name"], "C");
    let mut wrong = effect_address();
    wrong.effect_kind = "other".into();
    assert!(filmcraft_parameter_edit::edit_mask(
        &moved,
        &wrong,
        mask_command(json!({"kind":"delete","maskIndex":0}))
    )
    .is_err());
    let mut locked = moved.clone();
    *locked
        .envelope
        .pointer_mut(&format!(
            "/project/items/{SEQUENCE}/kind/Sequence/video_tracks/0/locked"
        ))
        .unwrap() = json!(true);
    assert!(filmcraft_parameter_edit::edit_mask(
        &locked,
        &effect_address(),
        mask_command(json!({"kind":"delete","maskIndex":0}))
    )
    .is_err());
}
#[test]
fn malformed_mask_edits_refuse_without_touching_the_source() {
    let source = document();
    let before = source.envelope.clone();
    for command in [
        json!({"kind":"create","name":"bad","shape":{"kind":"rectangle","x0":2.,"y0":0.,"x1":1.,"y1":1.}}),
        json!({"kind":"delete","maskIndex":0}),
        json!({"kind":"move","maskIndex":0,"destination":1}),
        json!({"kind":"create","name":"x".repeat(4097),"shape":{"kind":"freeDraw"}}),
    ] {
        assert!(filmcraft_parameter_edit::edit_mask(
            &source,
            &effect_address(),
            mask_command(command)
        )
        .is_err());
        assert_eq!(source.envelope, before);
    }
    for command in [
        json!({"kind":"settings","maskIndex":0,"mode":"unknown"}),
        json!({"kind":"settings","maskIndex":0,"trackMethod":"unknown"}),
        json!({"kind":"delete","maskIndex":0,"unexpected":true}),
    ] {
        assert!(serde_json::from_value::<filmcraft_parameter_edit::MaskCommand>(command).is_err());
    }
}

async fn source_post(app: &axum::Router, route: &str, value: Value) -> Value {
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, 200, "{route}: {}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}
#[tokio::test]
async fn http_mask_creation_modes_order_and_deletion_reach_real_program_preview() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut source = fixture["cases"][0]["document"].clone();
    let pointer = "/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks";
    *source.pointer_mut(pointer).unwrap() = json!([]);
    let address = json!({"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity"});
    let mut edited = source.clone();
    for (name, shape) in [
        (
            "Rect",
            json!({"kind":"rectangle","x0":1.,"y0":1.,"x1":7.,"y1":7.}),
        ),
        (
            "Ellipse",
            json!({"kind":"ellipse","x0":2.,"y0":2.,"x1":6.,"y1":6.}),
        ),
    ] {
        let result=source_post(&app,"/api/compositions/filmcraft/edit-mask",json!({"document":edited.to_string(),"address":address,"command":{"kind":"create","name":name,"shape":shape}})).await;
        edited = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    }
    assert_eq!(
        edited.pointer(pointer).unwrap().as_array().unwrap().len(),
        2
    );
    let baseline = source_post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":source.to_string(),"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    let created = source_post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":edited.to_string(),"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    assert_ne!(
        baseline["rgbaHex"], created["rgbaHex"],
        "new masks must affect program pixels"
    );
    for mode in [
        "None",
        "Add",
        "Subtract",
        "Intersect",
        "Lighten",
        "Darken",
        "Difference",
    ] {
        for inverted in [false, true] {
            let mut expected = edited.clone();
            let mask = &mut expected.pointer_mut(pointer).unwrap()[1];
            mask["mode"] = json!(mode);
            mask["inverted"] = json!(inverted);
            mask["name"] = json!("Settings");
            mask["track_method"] = json!("PositionRotation");
            let result=source_post(&app,"/api/compositions/filmcraft/edit-mask",json!({"document":edited.to_string(),"address":address,"command":{"kind":"settings","maskIndex":1,"name":"Settings","mode":mode,"inverted":inverted,"trackMethod":"PositionRotation"}})).await;
            edited = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
            assert_eq!(edited, expected);
            let actual = source_post(
                &app,
                "/api/compositions/filmcraft/preview-frame",
                json!({"document":edited.to_string(),"sequenceId":"6","timeTicks":"0"}),
            )
            .await;
            let reference = source_post(
                &app,
                "/api/compositions/filmcraft/preview-frame",
                json!({"document":expected.to_string(),"sequenceId":"6","timeTicks":"0"}),
            )
            .await;
            assert_eq!(
                actual["rgbaHex"], reference["rgbaHex"],
                "mode {mode}, inverted {inverted}"
            );
        }
    }
    let before = source_post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":edited.to_string(),"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    let mut expected = edited.clone();
    expected
        .pointer_mut(pointer)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    let result=source_post(&app,"/api/compositions/filmcraft/edit-mask",json!({"document":edited.to_string(),"address":address,"command":{"kind":"move","maskIndex":1,"destination":0}})).await;
    edited = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(edited, expected);
    let after = source_post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":edited.to_string(),"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    assert_ne!(
        before["rgbaHex"], after["rgbaHex"],
        "ordered mask combination must reach renderer"
    );
    expected
        .pointer_mut(pointer)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .remove(1);
    let result=source_post(&app,"/api/compositions/filmcraft/edit-mask",json!({"document":edited.to_string(),"address":address,"command":{"kind":"delete","maskIndex":1}})).await;
    edited = serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(edited, expected);
}

#[test]
fn custom_mask_creation_preserves_asymmetric_handles_closure_and_defaults_atomically() {
    let source=document();let before=source.envelope.clone();
    let path=json!({"closed":true,"vertices":[{"p":{"x":1.,"y":2.},"t_in":{"x":-2.,"y":0.5},"t_out":{"x":3.,"y":1.}},{"p":{"x":9.,"y":1.}},{"p":{"x":5.,"y":8.},"t_in":{"x":-1.,"y":-2.}}]});
    let changed=filmcraft_parameter_edit::edit_mask(&source,&effect_address(),mask_command(json!({"kind":"create","name":"Custom","shape":{"kind":"path","path":path}}))).unwrap();
    let mask=&changed.envelope.pointer(&item_path()).unwrap()["effects"][0]["masks"][0];
    assert_eq!(mask["path"]["value"]["Path"],path);
    assert_eq!(mask["name"],"Custom");assert_eq!(mask["feather"]["value"]["Float"],10.);
    assert_eq!(mask["opacity"]["value"]["Float"],100.);assert_eq!(mask["expansion"]["value"]["Float"],0.);
    assert_eq!(mask["mode"],"Add");assert_eq!(mask["inverted"],false);assert!(mask["path"].get("keyframes").is_none());
    assert_eq!(source.envelope,before);
    for path in [json!({"vertices":[],"closed":true}),json!({"vertices":[{"p":{"x":1.,"y":1.}}]}),json!({"closed":false,"vertices":[{"p":{"x":null,"y":1.}}]}),json!({"closed":false,"vertices":vec![json!({"p":{"x":0.,"y":0.}});4097]})] {
        assert!(filmcraft_parameter_edit::edit_mask(&source,&effect_address(),mask_command(json!({"kind":"create","name":"bad","shape":{"kind":"path","path":path}}))).is_err());
        assert_eq!(source.envelope,before);
    }
    let changed=filmcraft_parameter_edit::edit_mask(&source,&effect_address(),mask_command(json!({"kind":"create","name":"Open","shape":{"kind":"path","path":{"closed":false,"vertices":[]}}}))).unwrap();
    assert_eq!(changed.envelope.pointer(&item_path()).unwrap()["effects"][0]["masks"][0]["path"]["value"]["Path"]["closed"],false);
}

#[tokio::test]
async fn custom_mask_http_creation_reaches_raster_with_independently_built_defaults() {
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut source=fixture["cases"][0]["document"].clone();
    let pointer="/project/items/6/kind/Sequence/video_tracks/0/items/0/effects/0/masks";
    *source.pointer_mut(pointer).unwrap()=json!([]);
    let path=json!({"closed":true,"vertices":[{"p":{"x":1,"y":1},"t_in":{"x":-0.5,"y":0},"t_out":{"x":1,"y":0.5}},{"p":{"x":7,"y":1}},{"p":{"x":4,"y":7},"t_in":{"x":-1,"y":-0.5},"t_out":{"x":0.25,"y":-1}}]});
    let result=source_post(&app,"/api/compositions/filmcraft/edit-mask",json!({"document":source.to_string(),"address":{"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity"},"command":{"kind":"create","name":"Bezier","shape":{"kind":"path","path":path}}})).await;
    let changed:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
    assert_eq!(changed.pointer(pointer).unwrap().as_array().unwrap().len(),1);
    assert_eq!(changed.pointer(pointer).unwrap()[0]["path"]["value"]["Path"],path);
    let mut reference=source.clone();
    *reference.pointer_mut(pointer).unwrap()=json!([{"name":"Bezier","path":{"value":{"Path":path}},"feather":{"value":{"Float":10.}},"opacity":{"value":{"Float":100.}},"expansion":{"value":{"Float":0.}},"inverted":false,"mode":"Add","track_method":"PositionScaleRotation"}]);
    assert_eq!(changed,reference);
    let mut frames=vec![];
    for document in [&source,&changed,&reference] {
        frames.push(source_post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":document.to_string(),"sequenceId":"6","timeTicks":"0"})).await);
    }
    assert_ne!(frames[0]["rgbaHex"],frames[1]["rgbaHex"]);
    assert_eq!(frames[1]["rgbaHex"],frames[2]["rgbaHex"]);
    if let Ok(file)=std::env::var("FILMCRAFT_CUSTOM_MASK_FIXTURE") {
        let mut reports=vec![];
        for document in [&source,&changed] {
            reports.push(source_post(&app,"/api/compositions/filmcraft/parameters",json!({"document":document.to_string(),"sequenceId":"6","clipId":"1010","timeTicks":"0"})).await);
        }
        std::fs::write(file,serde_json::to_vec(&json!({"document":source.to_string(),"path":path,"created":result,"reports":reports,"frames":frames})).unwrap()).unwrap();
    }
}

#[tokio::test]
#[ignore = "requires an actual custom-mask browser trace"]
async fn replay_custom_mask_browser_creation_in_rust() {
    let file=std::env::var("FILMCRAFT_CUSTOM_MASK_TRACE").expect("explicit browser trace");
    let trace:Value=serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
    let actual=source_post(&app,"/api/compositions/filmcraft/edit-mask",trace["request"].clone()).await;
    assert_eq!(actual["documentText"],trace["expectedProject"]["documentText"]);
}
