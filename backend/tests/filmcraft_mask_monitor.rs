mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    filmcraft_mask_monitor::{self, Operation, Part},
    filmcraft_parameter_edit::Address,
    filmcraft_project_file::SourceProjectDocument,
};
fn source() -> SourceProjectDocument {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    SourceProjectDocument::read(&serde_json::to_vec(&f["cases"][0]["document"]).unwrap()).unwrap()
}
fn address() -> Address {
    Address {
        sequence_id: "6".into(),
        clip_id: "1010".into(),
        effect_index: 0,
        effect_kind: "opacity".into(),
        mask_index: Some(0),
        parameter: "path".into(),
    }
}
fn clip() -> &'static str {
    "/project/items/6/kind/Sequence/video_tracks/0/items/0"
}
fn path() -> String {
    format!("{}/effects/0/masks/0/path", clip())
}
fn op(value: Value) -> Operation {
    serde_json::from_value(value).unwrap()
}
#[test]
fn all_original_256_motion_matrices_project_mask_controls_without_js_geometry() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-appearance.json")).unwrap();
    assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 256);
    for (index, c) in cases.iter().enumerate() {
        let mut doc = c["document"].clone();
        let pointer = format!(
            "/project/items/6/kind/Sequence/video_tracks/0/items/{}",
            if c["clip"] == 1010 { 0 } else { 1 }
        );
        let effects = &mut doc.pointer_mut(&pointer).unwrap()["effects"];
        effects[1]["masks"] = json!([{"path":{"value":{"Path":{"closed":true,"vertices":[{"p":{"x":1.25,"y":2.5},"t_in":{"x":-0.75,"y":0.5},"t_out":{"x":0.75,"y":-0.5}}]}}}}]);
        let doc = SourceProjectDocument::read(&serde_json::to_vec(&doc).unwrap()).unwrap();
        let mut address = address();
        address.effect_index = 1;
        address.clip_id = c["clip"].as_u64().unwrap().to_string();
        let (view, changed) = filmcraft_mask_monitor::evaluate(
            &doc,
            &address,
            c["at"].as_i64().unwrap(),
            Operation::Read,
        )
        .unwrap();
        assert!(changed.is_none());
        let matrix = c["matrixBits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| f64::from_bits(n.as_u64().unwrap()))
            .collect::<Vec<_>>();
        for (name, x, y) in [
            ("point", 1.25, 2.5),
            ("incoming", 0.5, 3.),
            ("outgoing", 2., 2.),
        ] {
            let expected = [
                matrix[0] * x + matrix[2] * y + matrix[4],
                matrix[1] * x + matrix[3] * y + matrix[5],
            ];
            for axis in 0..2 {
                assert_eq!(
                    view["vertices"][0][name][axis].as_f64().unwrap().to_bits(),
                    expected[axis].to_bits(),
                    "matrix {index} {name} {axis}"
                );
            }
        }
    }
}
#[test]
fn monitor_move_handles_translate_and_smooth_are_native_and_static_paths_stay_static() {
    let source = source();
    let (view, _) =
        filmcraft_mask_monitor::evaluate(&source, &address(), 0, Operation::Read).unwrap();
    let p = [
        view["vertices"][0]["point"][0].as_f64().unwrap() + 2.,
        view["vertices"][0]["point"][1].as_f64().unwrap() - 1.,
    ];
    let before = source.envelope.clone();
    let (_, changed) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        Operation::Move {
            index: 0,
            part: Part::Vertex,
            point: p,
            break_handles: false,
        },
    )
    .unwrap();
    let changed = changed.unwrap();
    assert_eq!(source.envelope, before);
    let parameter = changed.envelope.pointer(&path()).unwrap();
    assert!(parameter.get("keyframes").is_none());
    assert_eq!(
        parameter["value"]["Path"]["vertices"][0]["p"],
        json!({"x":3.25,"y":-0.25})
    );
    let (view, changed) = filmcraft_mask_monitor::evaluate(
        &changed,
        &address(),
        0,
        op(json!({"kind":"move","index":0,"part":"out","point":[9.,1.],"breakHandles":false})),
    )
    .unwrap();
    let changed = changed.unwrap();
    let v = &changed.envelope.pointer(&path()).unwrap()["value"]["Path"]["vertices"][0];
    assert_eq!(
        v["t_in"]["x"].as_f64().unwrap(),
        -v["t_out"]["x"].as_f64().unwrap()
    );
    let incoming = v["t_in"].clone();
    let (_, changed) = filmcraft_mask_monitor::evaluate(
        &changed,
        &address(),
        0,
        op(json!({"kind":"move","index":0,"part":"out","point":[10.,1.],"breakHandles":true})),
    )
    .unwrap();
    assert_eq!(
        changed.as_ref().unwrap().envelope.pointer(&path()).unwrap()["value"]["Path"]["vertices"]
            [0]["t_in"],
        incoming
    );
    assert!(view["svgPath"].as_str().unwrap().contains(" C "));
    let (_, translated) = filmcraft_mask_monitor::evaluate(
        &changed.unwrap(),
        &address(),
        0,
        op(json!({"kind":"translate","delta":[2.,3.]})),
    )
    .unwrap();
    let translated = translated.unwrap();
    let (_, smooth) = filmcraft_mask_monitor::evaluate(
        &translated,
        &address(),
        0,
        Operation::Smooth { index: 0 },
    )
    .unwrap();
    assert_eq!(
        smooth.unwrap().envelope.pointer(&path()).unwrap()["value"]["Path"]["vertices"][0]["t_in"],
        json!({"x":0.,"y":0.})
    );
}
#[test]
fn monitor_edits_use_original_frame_clock_and_inherit_animation_interpolation() {
    let mut source = source();
    let origin = 9007199254740993i64;
    let item = source.envelope.pointer_mut(clip()).unwrap();
    item["source_in"] = json!(origin);
    item["speed"] = json!(2.);
    item["frame_hold"] = json!(origin + 77);
    item["hold_filters"] = json!(false);
    let original = source.envelope.pointer(&path()).unwrap()["value"].clone();
    source.envelope.pointer_mut(&path()).unwrap()["keyframes"] = json!([{"time":origin,"value":original,"interp":"Hold","opaque":"first"},{"time":origin+1000,"value":original,"interp":"Bezier"}]);
    let (view, changed) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        1,
        Operation::Translate { delta: [1., 0.], origin: None },
    )
    .unwrap();
    assert_eq!(view["sampleTicks"], (origin + 77).to_string());
    let changed = changed.unwrap();
    let keys = changed.envelope.pointer(&path()).unwrap()["keyframes"]
        .as_array()
        .unwrap();
    assert_eq!(keys.len(), 3);
    assert_eq!(keys[1]["time"].as_i64(), Some(origin + 77));
    assert_eq!(keys[1]["interp"], "Hold");
    assert_eq!(keys[0]["opaque"], "first");
}
#[test]
fn monitor_topology_changes_preserve_all_key_times_and_unedited_siblings() {
    let mut source = source();
    let value = source.envelope.pointer(&path()).unwrap()["value"].clone();
    source.envelope.pointer_mut(&path()).unwrap()["keyframes"] = json!([{"time":0,"value":value,"interp":"Linear","opaque":"a"},{"time":100,"value":value,"interp":"Linear","opaque":"b"}]);
    let (_, appended) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        Operation::Append { point: [8., 3.] },
    )
    .unwrap();
    let appended = appended.unwrap();
    let p = appended.envelope.pointer(&path()).unwrap();
    assert_eq!(
        p["keyframes"][0]["value"]["Path"]["vertices"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        p["keyframes"][1]["value"]["Path"]["vertices"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(p["keyframes"][1]["time"], 100);
    assert_eq!(p["keyframes"][1]["opaque"], "b");
    let (_, deleted) =
        filmcraft_mask_monitor::evaluate(&appended, &address(), 0, Operation::Delete { index: 0 })
            .unwrap();
    let p = deleted.as_ref().unwrap().envelope.pointer(&path()).unwrap();
    assert_eq!(p["value"]["Path"]["vertices"].as_array().unwrap().len(), 3);
    assert_eq!(
        p["keyframes"][0]["value"]["Path"]["vertices"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
#[test]
fn locked_inactive_and_invalid_monitor_operations_refuse_without_mutation() {
    let source = source();
    let before = source.envelope.clone();
    assert!(filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        Operation::Delete { index: 99 }
    )
    .is_err());
    assert!(filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        i64::MAX / 2,
        Operation::Translate { delta: [1., 0.], origin: None }
    )
    .is_err());
    assert_eq!(source.envelope, before);
    let mut locked = source.clone();
    locked.envelope["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["locked"] =
        json!(true);
    let (view, _) =
        filmcraft_mask_monitor::evaluate(&locked, &address(), 0, Operation::Read).unwrap();
    assert_eq!(view["editable"], false);
    assert!(filmcraft_mask_monitor::evaluate(
        &locked,
        &address(),
        0,
        Operation::Translate { delta: [1., 0.], origin: None }
    )
    .is_err());
}
async fn post(app: &axum::Router, route: &str, input: Value) -> Value {
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
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}
#[tokio::test]
async fn monitor_http_edit_reaches_real_program_frame_and_emits_optional_browser_fixture() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut source = source();
    source.envelope["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["width"] = json!(320);
    source.envelope["project"]["items"]["6"]["kind"]["Sequence"]["settings"]["height"] = json!(180);
    let info = &mut source.envelope["project"]["items"]["1"]["kind"]["Media"]["info"]["video"];
    info["width"] = json!(320);
    info["height"] = json!(180);
    let mask = &mut source.envelope.pointer_mut(clip()).unwrap()["effects"][0]["masks"][0];
    mask["mode"] = json!("Add");
    mask["path"] = json!({"value":{"Path":{"closed":true,"vertices":[{"p":{"x":40.,"y":30.}},{"p":{"x":280.,"y":30.}},{"p":{"x":280.,"y":150.}},{"p":{"x":40.,"y":150.}}]}}});
    let text = String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let address = json!({"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","maskIndex":0,"parameter":"path"});
    let read = post(
        &app,
        "/api/compositions/filmcraft/mask-monitor",
        json!({"document":text,"address":address,"timeTicks":"0","operation":{"kind":"read"}}),
    )
    .await;
    assert!(read["project"].is_null());
    let operation =
        json!({"kind":"move","index":0,"part":"vertex","point":[80.,60.],"breakHandles":false});
    let edited = post(
        &app,
        "/api/compositions/filmcraft/mask-monitor",
        json!({"document":text,"address":address,"timeTicks":"0","operation":operation}),
    )
    .await;
    let changed: Value =
        serde_json::from_str(edited["project"]["documentText"].as_str().unwrap()).unwrap();
    let sampled_path = changed.pointer(&path()).unwrap()["value"].clone();
    let mut reference = source.envelope.clone();
    reference.pointer_mut(&path()).unwrap()["value"] = sampled_path;
    let mut frames = vec![];
    for document in [&source.envelope, &changed, &reference] {
        frames.push(
            post(
                &app,
                "/api/compositions/filmcraft/preview-frame",
                json!({"document":document.to_string(),"sequenceId":"6","timeTicks":"0"}),
            )
            .await,
        );
    }
    assert_ne!(frames[0]["rgbaHex"], frames[1]["rgbaHex"]);
    assert_eq!(frames[1]["rgbaHex"], frames[2]["rgbaHex"]);
    let p1 = json!({"point":[40.,30.]});
    let h1 = json!({"point":[40.,30.],"handle":[70.,30.]});
    let p2 = json!({"point":[280.,30.]});
    let p3 = json!({"point":[160.,150.]});
    let h3 = json!({"point":[160.,150.],"handle":[130.,150.]});
    let operations = [
        json!({"kind":"penDraft","closed":false,"points":[p1]}),
        json!({"kind":"penDraft","closed":false,"points":[h1]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2,p3]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2,h3]}),
        json!({"kind":"penDraft","closed":true,"points":[h1,p2,h3]}),
    ];
    let mut pen_cases = vec![];
    for operation in operations {
        let result = post(
            &app,
            "/api/compositions/filmcraft/mask-monitor",
            json!({"document":text,"address":address,"timeTicks":"0","operation":operation}),
        )
        .await;
        pen_cases.push(json!({"operation":operation,"result":result}));
    }
    let pen_result = pen_cases.last().unwrap()["result"].clone();
    let pen_frame = post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":pen_result["project"]["documentText"],"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    let insert_operation = json!({"kind":"insertNear","point":[160.,30.],"pixelScale":[2.25,2.25]});
    let inserted=post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":pen_result["project"]["documentText"],"address":address,"timeTicks":"0","operation":insert_operation})).await;
    assert_eq!(inserted["overlay"]["selectedVertex"], 1);
    assert_eq!(inserted["overlay"]["vertices"].as_array().unwrap().len(), 4);
    let insert_frame = post(
        &app,
        "/api/compositions/filmcraft/preview-frame",
        json!({"document":inserted["project"]["documentText"],"sequenceId":"6","timeTicks":"0"}),
    )
    .await;
    let mut falloff_cases=vec![];
    for (parameter,delta) in [("feather",[0.,-20.]),("expansion",[0.,20.])] {
        let operation=json!({"kind":"falloff","parameter":parameter,"delta":delta});
        let result=post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":text,"address":address,"timeTicks":"0","operation":operation})).await;
        let frame=post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":result["project"]["documentText"],"sequenceId":"6","timeTicks":"0"})).await;
        assert_ne!(frame["rgbaHex"],frames[0]["rgbaHex"]);
        falloff_cases.push(json!({"operation":operation,"result":result,"frame":frame}));
    }
    let mut body_cases=vec![];
    for (origin,delta,hit) in [([160.,90.],[20.,10.],true),([10.,10.],[150.,80.],false)] {
        let operation=json!({"kind":"translate","origin":origin,"delta":delta});
        let result=post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":text,"address":address,"timeTicks":"0","operation":operation})).await;
        assert_eq!(!result["project"].is_null(),hit);
        let frame=if hit {post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":result["project"]["documentText"],"sequenceId":"6","timeTicks":"0"})).await} else {frames[0].clone()};
        if hit {assert_ne!(frame["rgbaHex"],frames[0]["rgbaHex"])}
        body_cases.push(json!({"operation":operation,"result":result,"frame":frame,"hit":hit}));
    }
    let mut click_cases=vec![];
    for (point,clear) in [([160.,90.],false),([10.,10.],false),([39.5,90.],true)] {
        let operation=json!({"kind":"bodyClick","point":point,"pixelScale":[2.25,2.25]});
        let result=post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":text,"address":address,"timeTicks":"0","operation":operation})).await;
        assert!(result["project"].is_null());assert_eq!(result["overlay"]["deselectMask"],clear);
        click_cases.push(json!({"operation":operation,"result":result,"clear":clear}));
    }
    if let Ok(path) = std::env::var("FILMCRAFT_MASK_MONITOR_FIXTURE") {
        std::fs::write(path,serde_json::to_vec(&json!({"document":text,"address":address,"read":read,"edited":edited,"operation":operation,"frame":frames[0],"editedFrame":frames[1],"penCases":pen_cases,"penResult":pen_result,"penFrame":pen_frame,"insertOperation":insert_operation,"insertResult":inserted,"insertFrame":insert_frame,"falloffCases":falloff_cases,"bodyCases":body_cases,"clickCases":click_cases})).unwrap()).unwrap();
    }
}
#[tokio::test]
#[ignore = "requires the trace emitted by the real Chromium monitor qualification"]
async fn replay_actual_browser_drag_in_rust() {
    let path = std::env::var("FILMCRAFT_MASK_MONITOR_TRACE").expect("explicit browser trace path");
    let trace: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Vertex {
        index: usize,
        point: [f64; 2],
        incoming: [f64; 2],
        outgoing: [f64; 2],
    }
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct FalloffHandle { parameter:String, point:[f64;2], direction:[f64;2] }
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Falloff { feather:f64, expansion:f64, expansion_contour:Vec<[f64;2]>, feather_contour:Vec<[f64;2]>, handles:Vec<FalloffHandle> }
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Overlay {
        #[serde(default)]
        falloff:Option<Falloff>,
        canvas: [u32; 2],
        matrix: [f64; 6],
        sample_ticks: String,
        locked: bool,
        visible: bool,
        editable: bool,
        closed: bool,
        vertices: Vec<Vertex>,
        svg_path: String,
        #[serde(default)]
        selected_vertex: Option<usize>,
        #[serde(default)]
        created_mask_index: Option<usize>,
        #[serde(default)]
        deselect_mask: Option<bool>,
    }
    let cases = trace
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(std::slice::from_ref(&trace));
    assert!(!cases.is_empty());
    for case in cases {
        let actual = post(
            &app,
            "/api/compositions/filmcraft/mask-monitor",
            case["request"].clone(),
        )
        .await;
        // documentText is the lossless contract; the redundant legacy parsed
        // document object normalizes integer-looking f64s in JavaScript.
        assert_eq!(
            actual["project"]["documentText"],
            case["expectedProject"]["documentText"]
        );
        assert_eq!(
            serde_json::from_value::<Overlay>(actual["overlay"].clone()).unwrap(),
            serde_json::from_value::<Overlay>(case["expectedOverlay"].clone()).unwrap()
        );
    }
}

#[test]
fn rotated_monitor_drag_and_box_drawing_use_the_native_inverse_and_close_semantics() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-appearance.json")).unwrap();
    let c = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["clip"] == 1010
                && c["at"] == 0
                && f64::from_bits(c["matrixBits"][1].as_u64().unwrap()).abs() > 0.01
        })
        .unwrap();
    let mut raw = c["document"].clone();
    raw["project"]["items"]["6"]["kind"]["Sequence"]["video_tracks"][0]["enabled"] = json!(true);
    raw.pointer_mut(clip()).unwrap()["effects"][1]["masks"] =
        source().envelope.pointer(clip()).unwrap()["effects"][0]["masks"].clone();
    let doc = SourceProjectDocument::read(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut address = address();
    address.effect_index = 1;
    let pointer = format!("{}/effects/1/masks/0/path", clip());
    let m = c["matrixBits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| f64::from_bits(n.as_u64().unwrap()))
        .collect::<Vec<_>>();
    let scene = |x: f64, y: f64| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
    let zero=scene(0.,0.);let step=scene(1.,2.);
    let (_,body)=filmcraft_mask_monitor::evaluate(&doc,&address,0,op(json!({"kind":"translate","origin":scene(3.,4.),"delta":[step[0]-zero[0],step[1]-zero[1]]}))).unwrap();
    let body=body.unwrap();let moved=&body.envelope.pointer(&pointer).unwrap()["value"]["Path"]["vertices"][0]["p"];
    assert!((moved["x"].as_f64().unwrap()-2.25).abs()<1e-10);
    assert!((moved["y"].as_f64().unwrap()-2.75).abs()<1e-10);
    let (_,miss)=filmcraft_mask_monitor::evaluate(&doc,&address,0,op(json!({"kind":"translate","origin":scene(7.,4.),"delta":[step[0]-zero[0],step[1]-zero[1]]}))).unwrap();
    assert!(miss.is_none());
    let (_, changed) = filmcraft_mask_monitor::evaluate(
        &doc,
        &address,
        0,
        Operation::Move {
            index: 0,
            part: Part::Vertex,
            point: scene(20.125, -3.5),
            break_handles: false,
        },
    )
    .unwrap();
    let p = &changed
        .as_ref()
        .unwrap()
        .envelope
        .pointer(&pointer)
        .unwrap()["value"]["Path"]["vertices"][0]["p"];
    assert!((p["x"].as_f64().unwrap() - 20.125).abs() < 1e-10);
    assert!((p["y"].as_f64().unwrap() + 3.5).abs() < 1e-10);
    let (_, boxed) = filmcraft_mask_monitor::evaluate(
        &doc,
        &address,
        0,
        Operation::Box {
            shape: filmcraft_mask_monitor::BoxShape::Ellipse,
            from: scene(-2., 3.),
            to: scene(8., 11.),
        },
    )
    .unwrap();
    let boxed = boxed.unwrap();
    let path = &boxed.envelope.pointer(&pointer).unwrap()["value"]["Path"];
    assert_eq!(path["closed"], true);
    assert_eq!(path["vertices"].as_array().unwrap().len(), 4);
    let p = &path["vertices"][0]["p"];
    assert!((p["x"].as_f64().unwrap() - 3.).abs() < 1e-10);
    assert!((p["y"].as_f64().unwrap() - 3.).abs() < 1e-10);
    let (view, closed) =
        filmcraft_mask_monitor::evaluate(&boxed, &address, 0, Operation::Close { closed: false })
            .unwrap();
    assert_eq!(view["closed"], false);
    assert!(!view["svgPath"].as_str().unwrap().ends_with(" Z"));
    assert_eq!(
        closed.unwrap().envelope.pointer(&pointer).unwrap()["value"]["Path"]["closed"],
        false
    );
}
#[test]
fn pen_tangent_threshold_is_in_clip_pixels_and_static_paths_keep_the_original_value_contract() {
    let source = source();
    let before = source.envelope.clone();
    let(_,draft)=filmcraft_mask_monitor::evaluate(&source,&address(),0,op(json!({"kind":"penDraft","closed":false,"points":[{"point":[8.,3.],"handle":[10.,3.]},{"point":[9.,4.],"handle":[11.01,4.]},{"point":[7.,5.]}]}))).unwrap();
    let draft = draft.unwrap();
    let parameter = draft.envelope.pointer(&path()).unwrap();
    assert!(parameter.get("keyframes").is_none());
    let vertices = parameter["value"]["Path"]["vertices"].as_array().unwrap();
    assert_eq!(vertices[0]["p"], json!({"x":4.,"y":3.}));
    assert_eq!(vertices[0]["t_out"]["x"].as_f64().unwrap(), 0.);
    assert!((vertices[1]["t_out"]["x"].as_f64().unwrap() - 2.01).abs() < 1e-12);
    assert_eq!(
        vertices[1]["t_in"]["x"].as_f64().unwrap(),
        -vertices[1]["t_out"]["x"].as_f64().unwrap()
    );
    assert_eq!(parameter["value"]["Path"]["closed"], false);
    assert_eq!(source.envelope, before);
}
#[test]
fn native_nearest_segment_insertion_uses_css_distance_and_misses_are_noops() {
    let source = source();
    let before = source.envelope.clone();
    let (_, miss) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        op(json!({"kind":"insertNear","point":[4.75,4.],"pixelScale":[16.,16.]})),
    )
    .unwrap();
    assert!(miss.is_none());
    assert_eq!(source.envelope, before);
    let (view, changed) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        op(json!({"kind":"insertNear","point":[4.7501,4.],"pixelScale":[16.,16.]})),
    )
    .unwrap();
    let changed = changed.unwrap();
    assert_eq!(view["selectedVertex"], 4);
    let vertices = changed.envelope.pointer(&path()).unwrap()["value"]["Path"]["vertices"]
        .as_array()
        .unwrap();
    assert_eq!(vertices.len(), 5);
    assert!((vertices[4]["p"]["x"].as_f64().unwrap() - 0.7501).abs() < 1e-12);
    assert_eq!(vertices[4]["p"]["y"], 4.);
}
#[test]
fn curved_segment_insertion_is_native_and_retains_original_neighbour_handles() {
    let mut source = source();
    source.envelope.pointer_mut(&path()).unwrap()["value"] = json!({"Path":{"closed":true,"vertices":[{"p":{"x":1.,"y":1.},"t_out":{"x":2.,"y":4.}},{"p":{"x":7.,"y":1.},"t_in":{"x":-2.,"y":4.}},{"p":{"x":7.,"y":7.}},{"p":{"x":1.,"y":7.}}]}});
    let (view, changed) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        op(json!({"kind":"insertNear","point":[8.,4.],"pixelScale":[10.,10.]})),
    )
    .unwrap();
    assert_eq!(view["selectedVertex"], 1);
    let changed = changed.unwrap();
    let vertices = changed.envelope.pointer(&path()).unwrap()["value"]["Path"]["vertices"]
        .as_array()
        .unwrap();
    assert_eq!(vertices[1]["p"], json!({"x":4.,"y":4.}));
    assert_eq!(vertices[0]["t_out"], json!({"x":2.,"y":4.}));
    assert_eq!(vertices[2]["t_in"], json!({"x":-2.,"y":4.}));
}
#[test]
fn mixed_nonempty_key_topology_insertion_matches_original_midpoint_and_hold_rules() {
    let mut source = source();
    let first = source.envelope.pointer(&path()).unwrap()["value"].clone();
    let mut second = first.clone();
    second["Path"]["vertices"].as_array_mut().unwrap().remove(2);
    source.envelope.pointer_mut(&path()).unwrap()["keyframes"] = json!([{"time":0,"value":first,"interp":"Hold","opaque":"a"},{"time":100,"value":second,"interp":"Linear","opaque":"b"}]);
    let (view, changed) = filmcraft_mask_monitor::evaluate(
        &source,
        &address(),
        0,
        op(json!({"kind":"insertNear","point":[7.75,0.75],"pixelScale":[10.,10.]})),
    )
    .unwrap();
    assert_eq!(view["selectedVertex"], 1);
    let changed = changed.unwrap();
    let p = changed.envelope.pointer(&path()).unwrap();
    assert_eq!(p["value"], first);
    assert_eq!(
        p["keyframes"][0]["value"]["Path"]["vertices"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        p["keyframes"][1]["value"]["Path"]["vertices"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(p["keyframes"][1]["time"], 100);
    assert_eq!(p["keyframes"][1]["opaque"], "b");
    assert_eq!(
        p["keyframes"][1]["value"]["Path"]["vertices"][1]["p"],
        json!({"x":3.75,"y":0.75})
    );
}

#[test]
fn new_mask_monitor_stages_without_mutation_and_commits_complete_named_path() {
    let original = source();
    let mut doc = original.clone();
    let masks_pointer = format!("{}/effects/0/masks", clip());
    let old = doc.envelope.pointer(&masks_pointer).unwrap()[0].clone();
    let mut named = old.clone();
    named["name"] = json!("Mask (7)");
    *doc.envelope.pointer_mut(&masks_pointer).unwrap() = json!([old, named]);
    let before = doc.envelope.clone();
    let mut address = address();
    address.mask_index = None;
    let (view, result) = filmcraft_mask_monitor::evaluate_new(&doc, &address, 0, Operation::Read).unwrap();
    assert!(result.is_none());
    assert_eq!(view["createdMaskIndex"], 2);
    assert_eq!(view["vertices"], json!([]));
    let operation = op(json!({"kind":"penDraft","closed":true,"points":[{"point":[4,0]},{"point":[8,0]},{"point":[8,4]}]}));
    let (view, result) = filmcraft_mask_monitor::evaluate_new(&doc, &address, 0, operation).unwrap();
    let result = result.unwrap();
    let masks = result.envelope.pointer(&masks_pointer).unwrap().as_array().unwrap();
    assert_eq!(masks.len(), 3);
    assert_eq!(&masks[..2], before.pointer(&masks_pointer).unwrap().as_array().unwrap());
    assert_eq!(masks[2]["name"], "Mask (8)");
    assert_eq!(masks[2]["mode"], "Add");
    assert_eq!(masks[2]["path"]["value"]["Path"]["closed"], true);
    assert_eq!(masks[2]["path"]["value"]["Path"]["vertices"].as_array().unwrap().len(), 3);
    assert_eq!(view["createdMaskIndex"], 2);
    assert_eq!(doc.envelope, before);
    address.mask_index = Some(0);
    assert!(filmcraft_mask_monitor::evaluate_new(&doc, &address, 0, Operation::Read).is_err());
    address.mask_index = None;
    for kind in ["motion", "volume", "unknown"] {
        address.effect_kind = kind.into();
        assert!(filmcraft_mask_monitor::evaluate_new(&doc, &address, 0, Operation::Read).is_err());
    }
    assert_eq!(doc.envelope, before);
}

#[test]
fn mask_admission_matches_every_pinned_original_effect_kind() {
    use video_kadr_backend::domain::filmcraft_parameter_edit::maskable_effect;
    let catalogue: Value = serde_json::from_slice(&std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/filmcraft-catalog.json")
    ).unwrap()).unwrap();
    let mut admitted = 0;
    for effect in catalogue["runtime"]["effects"].as_array().unwrap() {
        let id = effect["id"].as_str().unwrap();
        let expected = effect["kind"] == "Video" && id != "motion";
        assert_eq!(maskable_effect(id), expected, "{id}");
        admitted += usize::from(expected);
    }
    assert_eq!(admitted, 129);
    assert!(!maskable_effect("unknown"));
}

#[tokio::test]
async fn http_new_mask_box_publishes_one_complete_document_from_an_empty_effect() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut source = source();
    source.envelope.pointer_mut(clip()).unwrap()["effects"][0]["masks"] = json!([]);
    let text = String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let address = json!({"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","parameter":"path"});
    let request = json!({"document":text,"address":address,"createMask":true,"timeTicks":"0","operation":{"kind":"read"}});
    let read = post(&app, "/api/compositions/filmcraft/mask-monitor", request.clone()).await;
    assert!(read["project"].is_null());
    assert_eq!(read["overlay"]["createdMaskIndex"], 0);
    for shape in ["rectangle", "ellipse"] {
        let mut request = request.clone();
        request["operation"] = json!({"kind":"box","shape":shape,"from":[4,0],"to":[8,4]});
        let result = post(&app, "/api/compositions/filmcraft/mask-monitor", request).await;
        let changed: Value = serde_json::from_str(result["project"]["documentText"].as_str().unwrap()).unwrap();
        let masks = &changed.pointer(clip()).unwrap()["effects"][0]["masks"];
        assert_eq!(masks.as_array().unwrap().len(), 1);
        assert_eq!(masks[0]["name"], "Mask (1)");
        assert_eq!(masks[0]["path"]["value"]["Path"]["closed"], true);
        assert_eq!(masks[0]["path"]["value"]["Path"]["vertices"].as_array().unwrap().len(), 4);
        assert_eq!(result["overlay"]["createdMaskIndex"], 0);
    }
    assert_eq!(source.envelope.pointer(clip()).unwrap()["effects"][0]["masks"], json!([]));
}

#[tokio::test]
async fn new_mask_pen_http_emits_optional_native_browser_fixture() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let mut source = source();
    let settings = &mut source.envelope["project"]["items"]["6"]["kind"]["Sequence"]["settings"];
    settings["width"] = json!(320); settings["height"] = json!(180);
    let video = &mut source.envelope["project"]["items"]["1"]["kind"]["Media"]["info"]["video"];
    video["width"] = json!(320); video["height"] = json!(180);
    source.envelope.pointer_mut(clip()).unwrap()["effects"][0]["masks"] = json!([]);
    let text = String::from_utf8(source.encode_preserved().unwrap()).unwrap();
    let address = json!({"sequenceId":"6","clipId":"1010","effectIndex":0,"effectKind":"opacity","parameter":"path"});
    let read = post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":text,"address":address,"createMask":true,"timeTicks":"0","operation":{"kind":"read"}})).await;
    assert!(read["project"].is_null());
    let p1=json!({"point":[40.,30.]});let h1=json!({"point":[40.,30.],"handle":[70.,30.]});
    let p2=json!({"point":[280.,30.]});let p3=json!({"point":[160.,150.]});let h3=json!({"point":[160.,150.],"handle":[130.,150.]});
    let mut cases=vec![];
    for operation in [
        json!({"kind":"penDraft","closed":false,"points":[p1]}),
        json!({"kind":"penDraft","closed":false,"points":[h1]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2,p3]}),
        json!({"kind":"penDraft","closed":false,"points":[h1,p2,h3]}),
        json!({"kind":"penDraft","closed":true,"points":[h1,p2,h3]})
    ] {
        let result=post(&app,"/api/compositions/filmcraft/mask-monitor",json!({"document":text,"address":address,"createMask":true,"timeTicks":"0","operation":operation})).await;
        assert_eq!(result["overlay"]["createdMaskIndex"],0);
        cases.push(json!({"operation":operation,"result":result}));
    }
    let pen_result=cases.last().unwrap()["result"].clone();
    let mut frames=vec![];
    for document in [json!(text),pen_result["project"]["documentText"].clone()] {
        frames.push(post(&app,"/api/compositions/filmcraft/preview-frame",json!({"document":document,"sequenceId":"6","timeTicks":"0"})).await);
    }
    assert_ne!(frames[0]["rgbaHex"],frames[1]["rgbaHex"]);
    if let Ok(path)=std::env::var("FILMCRAFT_NEW_MASK_FIXTURE") {
        std::fs::write(path,serde_json::to_vec(&json!({"document":text,"address":address,"read":read,"frame":frames[0],"penCases":cases,"penResult":pen_result,"penFrame":frames[1]})).unwrap()).unwrap();
    }
}

#[test]
fn original_falloff_normals_contours_and_drag_values_use_clip_pixels() {
    let doc = source();
    let before = doc.envelope.clone();
    let (view, _) = filmcraft_mask_monitor::evaluate(&doc, &address(), 0, Operation::Read).unwrap();
    let falloff = &view["falloff"];
    assert_eq!(falloff["feather"], 0.75);
    assert_eq!(falloff["expansion"], -0.25);
    assert_eq!(falloff["expansionContour"].as_array().unwrap().len(), 4);
    let length = (6.5_f64 * 6.5 + 25.).sqrt();
    let normal = [-6.5 / length, -5. / length];
    for axis in 0..2 {
        assert!((falloff["handles"][0]["direction"][axis].as_f64().unwrap() - normal[axis]).abs() < 1e-12);
    }
    let (_, changed) = filmcraft_mask_monitor::evaluate(&doc, &address(), 0,
        op(json!({"kind":"falloff","parameter":"feather","delta":[normal[0]*2.,normal[1]*2.]}))).unwrap();
    let changed = changed.unwrap();
    let mask_pointer = format!("{}/effects/0/masks/0", clip());
    let mask = changed.envelope.pointer(&mask_pointer).unwrap();
    assert!((mask["feather"]["value"]["Float"].as_f64().unwrap() - 4.75).abs() < 1e-12);
    assert_eq!(mask["path"], before.pointer(&path()).unwrap().clone());
    assert_eq!(mask["expansion"], before.pointer(&mask_pointer).unwrap()["expansion"]);
    let (_, clamped) = filmcraft_mask_monitor::evaluate(&doc, &address(), 0,
        op(json!({"kind":"falloff","parameter":"feather","delta":[-normal[0]*20.,-normal[1]*20.]}))).unwrap();
    assert_eq!(clamped.unwrap().envelope.pointer(&mask_pointer).unwrap()["feather"]["value"]["Float"], 0.);
    let bottom = [-6.5 / length, 5. / length];
    let (_, expanded) = filmcraft_mask_monitor::evaluate(&doc, &address(), 0,
        op(json!({"kind":"falloff","parameter":"expansion","delta":[bottom[0]*3.,bottom[1]*3.]}))).unwrap();
    assert!((expanded.unwrap().envelope.pointer(&mask_pointer).unwrap()["expansion"]["value"]["Float"].as_f64().unwrap()-2.75).abs()<1e-12);
    assert_eq!(doc.envelope, before);
}

#[test]
fn animated_falloff_uses_held_frame_clock_and_keeps_key_siblings_and_path() {
    let mut doc=source();let origin=9007199254740993i64;
    let item=doc.envelope.pointer_mut(clip()).unwrap();
    item["source_in"]=json!(origin);item["frame_hold"]=json!(origin+77);item["hold_filters"]=json!(false);
    let feather_path=format!("{}/effects/0/masks/0/feather",clip());
    *doc.envelope.pointer_mut(&feather_path).unwrap()=json!({"value":{"Float":8.},"opaque":"parameter","keyframes":[{"time":origin,"value":{"Float":10.},"interp":"Hold","opaque":"left"},{"time":origin+1000,"value":{"Float":20.},"interp":"Linear","opaque":"right"}]});
    let before=doc.envelope.clone();
    let (view,_)=filmcraft_mask_monitor::evaluate(&doc,&address(),1,Operation::Read).unwrap();
    let n=&view["falloff"]["handles"][0]["direction"];
    let (view,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),1,op(json!({"kind":"falloff","parameter":"feather","delta":[n[0].as_f64().unwrap(),n[1].as_f64().unwrap()]}))).unwrap();
    let changed=changed.unwrap();let parameter=changed.envelope.pointer(&feather_path).unwrap();
    let keys=parameter["keyframes"].as_array().unwrap();
    assert_eq!(keys.len(),3);assert_eq!(keys[1]["time"],origin+77);assert_eq!(keys[1]["interp"],"Hold");
    assert!((keys[1]["value"]["Float"].as_f64().unwrap()-12.).abs()<1e-10);
    assert_eq!(keys[0],before.pointer(&feather_path).unwrap()["keyframes"][0]);
    assert_eq!(keys[2],before.pointer(&feather_path).unwrap()["keyframes"][1]);
    assert_eq!(parameter["opaque"],"parameter");assert_eq!(parameter["value"],json!({"Float":8.}));
    assert_eq!(view["sampleTicks"],(origin+77).to_string());
    assert_eq!(changed.envelope.pointer(&path()),before.pointer(&path()));
    assert_eq!(doc.envelope,before);
}

#[test]
fn falloff_drag_inverse_matches_all_original_motion_matrices() {
    let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-video-appearance.json")).unwrap();
    let mut checked=0;let mut refused=0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut doc=case["document"].clone();
        let pointer=format!("/project/items/6/kind/Sequence/video_tracks/0/items/{}",if case["clip"]==1010 {0}else{1});
        doc.pointer_mut(&pointer).unwrap()["effects"][1]["masks"]=json!([{"feather":{"value":{"Float":10.}},"expansion":{"value":{"Float":0.}},"path":{"value":{"Path":{"closed":true,"vertices":[{"p":{"x":1.,"y":1.}},{"p":{"x":5.,"y":1.}},{"p":{"x":5.,"y":5.}},{"p":{"x":1.,"y":5.}}]}}}}]);
        let doc=SourceProjectDocument::read(&serde_json::to_vec(&doc).unwrap()).unwrap();
        let mut address=address();address.effect_index=1;address.clip_id=case["clip"].as_u64().unwrap().to_string();
        let at=case["at"].as_i64().unwrap();
        let (view,_)=filmcraft_mask_monitor::evaluate(&doc,&address,at,Operation::Read).unwrap();
        if view["editable"]!=true {
            assert!(filmcraft_mask_monitor::evaluate(&doc,&address,at,op(json!({"kind":"falloff","parameter":"feather","delta":[1.,1.]}))).is_err());refused+=1;continue
        }
        let n=&view["falloff"]["handles"][0]["direction"];
        let (_,changed)=filmcraft_mask_monitor::evaluate(&doc,&address,at,op(json!({"kind":"falloff","parameter":"feather","delta":[n[0].as_f64().unwrap()*2.,n[1].as_f64().unwrap()*2.]}))).unwrap();
        let feather=changed.unwrap().envelope.pointer(&format!("{pointer}/effects/1/masks/0/feather/value/Float")).unwrap().as_f64().unwrap();
        assert!((feather-14.).abs()<1e-8,"{feather}");checked+=1;
    }
    assert_eq!((checked,refused),(106,150));
}

#[test]
fn body_drag_starts_inside_original_winding_fill_and_outside_is_a_noop() {
    let doc=source();let before=doc.envelope.clone();
    for origin in [[0.,0.],[4.,0.],[15.,4.]] {
        let (view,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"translate","origin":origin,"delta":[1.,2.]}))).unwrap();
        assert!(changed.is_none());
        let (original,_)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,Operation::Read).unwrap();
        assert_eq!(view,original);
    }
    let (_,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"translate","origin":[8.,4.],"delta":[1.,2.]}))).unwrap();
    let changed=changed.unwrap();
    let a=before.pointer(&path()).unwrap();let b=changed.envelope.pointer(&path()).unwrap();
    for i in 0..4 {
        assert_eq!(b["value"]["Path"]["vertices"][i]["p"]["x"].as_f64().unwrap(),a["value"]["Path"]["vertices"][i]["p"]["x"].as_f64().unwrap()+1.);
        assert_eq!(b["value"]["Path"]["vertices"][i]["p"]["y"].as_f64().unwrap(),a["value"]["Path"]["vertices"][i]["p"]["y"].as_f64().unwrap()+2.);
    }
    assert_eq!(doc.envelope,before);
}

#[test]
fn body_hit_test_uses_concave_fill_not_bounding_box_and_closes_open_paths() {
    for closed in [false,true] {
        let mut doc=source();
        doc.envelope.pointer_mut(&path()).unwrap()["value"]=json!({"Path":{"closed":closed,"vertices":[{"p":{"x":0.,"y":0.}},{"p":{"x":6.,"y":0.}},{"p":{"x":6.,"y":2.}},{"p":{"x":2.,"y":2.}},{"p":{"x":2.,"y":6.}},{"p":{"x":0.,"y":6.}}]}});
        for (origin,hit) in [([8.,4.],false),([5.,4.],true),([8.,1.],true)] {
            let (_,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"translate","origin":origin,"delta":[1.,1.]}))).unwrap();
            assert_eq!(changed.is_some(),hit,"closed={closed}, origin={origin:?}");
        }
    }
}

#[test]
fn body_hit_retains_original_nonzero_winding_rule_and_picture_clip() {
    for reverse in [false,true] {
        let mut doc=source();
        let mut points=vec![[-10.,-10.],[20.,-10.],[20.,20.],[-10.,20.]];
        // A twice-wound contour distinguishes nonzero fill from even-odd fill.
        points.extend(points.clone());if reverse {points.reverse()}
        doc.envelope.pointer_mut(&path()).unwrap()["value"]=json!({"Path":{"closed":true,"vertices":points.iter().map(|p|json!({"p":{"x":p[0],"y":p[1]}})).collect::<Vec<_>>()}});
        for (origin,hit) in [([8.,4.],true),([-1.,4.],false),([8.,9.],false)] {
            let (_,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"translate","origin":origin,"delta":[1.,1.]}))).unwrap();
            assert_eq!(changed.is_some(),hit);
        }
    }
}

#[test]
fn body_hit_flattens_bezier_boundary_instead_of_using_vertex_bounds() {
    let (_,ellipse)=filmcraft_mask_monitor::evaluate(&source(),&address(),0,
        op(json!({"kind":"box","shape":"ellipse","from":[4.,0.],"to":[12.,8.]}))).unwrap();
    let ellipse=ellipse.unwrap();let before=ellipse.envelope.clone();
    for (origin,hit) in [([4.5,0.5],false),([8.,4.],true)] {
        let (_,changed)=filmcraft_mask_monitor::evaluate(&ellipse,&address(),0,
            op(json!({"kind":"translate","origin":origin,"delta":[1.,1.]}))).unwrap();
        assert_eq!(changed.is_some(),hit);
    }
    assert_eq!(ellipse.envelope,before);
}

#[test]
fn body_click_clears_selection_only_in_original_body_hit_bounds_without_editing() {
    let doc=source();let before=doc.envelope.clone();
    // Native source mask screen bounds are x=5.25..10.25 and y=.75..7.25.
    for (point,scale,clear) in [([8.,4.],[1.,1.],false),([4.,4.],[1.,1.],true),([4.,4.],[2.,2.],false),([15.,4.],[1.,1.],false),([-0.1,4.],[1.,1.],false)] {
        let (view,changed)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"bodyClick","point":point,"pixelScale":scale}))).unwrap();
        assert!(changed.is_none());assert_eq!(view["deselectMask"],clear,"point={point:?}, scale={scale:?}");
    }
    assert!(filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"bodyClick","point":[4.,4.],"pixelScale":[0.,1.]}))).is_err());
    assert_eq!(doc.envelope,before);
    let (_,ellipse)=filmcraft_mask_monitor::evaluate(&doc,&address(),0,op(json!({"kind":"box","shape":"ellipse","from":[4.,0.],"to":[12.,8.]}))).unwrap();
    let (view,changed)=filmcraft_mask_monitor::evaluate(&ellipse.unwrap(),&address(),0,op(json!({"kind":"bodyClick","point":[4.5,0.5],"pixelScale":[2.,2.]}))).unwrap();
    assert_eq!(view["deselectMask"],true);assert!(changed.is_none());
}
