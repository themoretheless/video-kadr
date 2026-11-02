mod support;
fn post_json(uri: &str, body: serde_json::Value) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}
use axum::http::StatusCode;
use serde_json::{json, Value};

#[tokio::test]
#[ignore = "requires independently generated CONCAT_EFFECT_UPGRADE_ORACLE"]
async fn actual_api_matches_original_catalogue_with_bypass_and_keys() {
    let path =
        std::env::var("CONCAT_EFFECT_UPGRADE_ORACLE").expect("set CONCAT_EFFECT_UPGRADE_ORACLE");
    let original: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        original["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    let cases = original["cases"].as_array().unwrap();
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for batch in cases.chunks(96) {
        let inputs: Vec<_> = batch.iter().map(|c| c["input"].clone()).collect();
        let expected: Vec<_> = batch.iter().map(|c| c["expected"].clone()).collect();
        let (status, result, _) = support::send(
            &app,
            post_json(
                "/api/compositions/concat-effects/upgrade",
                json!({"schemaVersion":1,"links":inputs}),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result, json!({"schemaVersion":1,"links":expected}));
    }
    let native_body = json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"box-blur","keys":{"radius":[{"at":0,"value":1},{"at":1,"value":6,"ease":"out"}]}},{"id":"box-blur","enabled":false},{"id":"custom.fx"}]});
    let (status, response, _) = support::send(
        &app,
        post_json("/api/compositions/concat-effects/upgrade", native_body),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    let effect: video_kadr_backend::domain::composition::VideoEffect =
        serde_json::from_value(response["nativeEffects"][0].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(effect).unwrap(),
        response["nativeEffects"][0]
    );
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0., 0., 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
    assert!(response["nativeEffects"][2].is_null());
    let chroma = json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"green-screen","keys":{"similarity":[{"at":0,"value":0},{"at":1,"value":30}]}},{"id":"blue-screen"}]});
    let (status, chroma, _) = support::send(
        &app,
        post_json("/api/compositions/concat-effects/upgrade", chroma),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{chroma}");
    assert_eq!(chroma["nativeEffects"][0]["kind"], "concat_chroma_key");
    assert_eq!(
        chroma["nativeEffects"][0]["concatKeys"]["similarity"][1]["value"],
        60.
    );
    assert_eq!(chroma["nativeEffects"][1]["parameters"]["color"], 65535);
    for body in [
        json!({"schemaVersion":2,"links":[]}),
        json!({"schemaVersion":1,"links":vec![json!({"id":"box-blur"});129]}),
        json!({"schemaVersion":1,"links":[{"id":"box-blur","params":{"radius":-0.5}}]}),
    ] {
        let (status, _, _) = support::send(
            &app,
            post_json("/api/compositions/concat-effects/upgrade", body),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    println!(
        "Original Concat migration API: {} exact links, invalid schema/count/domain rejected",
        cases.len()
    );
}

#[tokio::test]
async fn chromatic_native_descriptor_keeps_fractional_source_keys_and_alias() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let link = json!({"id":"chromatic-aberration","params":{"shift":4.5},"keys":{"shift":[{"at":0,"value":1},{"at":1,"value":24,"ease":[0.42,-0.5,0.58,1]}]}});
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[link.clone(),{"id":"concat.chromatic-aberration","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["links"][0]["link"]["id"], "chromatic-aberration");
    assert_eq!(
        response["nativeEffects"][0]["kind"],
        "concat_chromatic_aberration"
    );
    assert_eq!(response["nativeEffects"][0]["parameters"]["shift"], 4.5);
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn fisheye_native_descriptor_keeps_fractional_source_keys_and_alias() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let link = json!({"id":"fisheye","params":{"strength":34.5},"keys":{"strength":[{"at":0,"value":5},{"at":1,"value":100,"ease":[0.42,-0.5,0.58,1]}]}});
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[link.clone(),{"id":"concat.fisheye","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["links"][0]["link"]["id"], "fisheye");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_fisheye");
    assert_eq!(response["nativeEffects"][0]["parameters"]["strength"], 34.5);
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn zoom_blur_native_descriptor_preserves_all_original_channels() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let keys = json!({"amount":[{"at":0,"value":0},{"at":1,"value":100,"ease":[0.42,-0.5,0.58,1]}],"x":[{"at":0,"value":0},{"at":1,"value":50}],"y":[{"at":0,"value":100},{"at":1,"value":50}]});
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.zoom-blur","keys":keys},{"id":"concat.zoom-blur","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_zoom_blur");
    for channel in ["amount", "x", "y"] {
        assert_eq!(
            response["nativeEffects"][0]["concatKeys"][channel][1]["value"].as_f64(),
            keys[channel][1]["value"].as_f64()
        );
    }
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"]["amount"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn motion_blur_native_descriptor_preserves_alias_channels_and_bypass() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let keys = json!({"length":[{"at":0,"value":2},{"at":1,"value":60,"ease":[0.42,-0.5,0.58,1]}],"angle":[{"at":0,"value":-90},{"at":1,"value":90}]});
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"motion-blur","keys":keys},{"id":"concat.motion-blur","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_motion_blur");
    assert_eq!(response["links"][0]["link"]["id"], "motion-blur");
    for channel in ["length", "angle"] {
        assert_eq!(
            response["nativeEffects"][0]["concatKeys"][channel][1]["value"].as_f64(),
            keys[channel][1]["value"].as_f64()
        );
    }
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn halftone_native_descriptor_keeps_size_keys_and_bypass() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"concat.halftone","params":{"size":6.5},"keys":{"size":[{"at":0,"value":4},{"at":1,"value":40,"ease":[0.42,-0.5,0.58,1]}]}},{"id":"concat.halftone","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_halftone");
    assert_eq!(response["nativeEffects"][0]["parameters"]["size"], 6.5);
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn film_grain_native_descriptor_preserves_alias_curve_and_bypass() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"film-grain","params":{"amount":12.5},"keys":{"amount":[{"at":0,"value":2},{"at":1,"value":40,"ease":[0.42,-0.5,0.58,1]}]}},{"id":"concat.film-grain","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_film_grain");
    assert_eq!(response["links"][0]["link"]["id"], "film-grain");
    assert_eq!(response["nativeEffects"][0]["parameters"]["amount"], 12.5);
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}

#[tokio::test]
async fn glow_native_descriptor_keeps_alias_amount_curve_and_bypass() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let(status,response,_)=support::send(&app,post_json("/api/compositions/concat-effects/upgrade",json!({"schemaVersion":1,"includeNativeEffects":true,"links":[{"id":"glow","params":{"amount":45.5},"keys":{"amount":[{"at":0,"value":10},{"at":1,"value":100,"ease":[0.42,-0.5,0.58,1]}]}},{"id":"concat.glow","enabled":false}]}))).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["nativeEffects"][0]["kind"], "concat_glow");
    assert_eq!(response["links"][0]["link"]["id"], "glow");
    assert_eq!(response["nativeEffects"][0]["parameters"]["amount"], 45.5);
    assert_eq!(
        response["nativeEffects"][0]["concatKeys"][1]["ease"],
        json!([0.42, -0.5, 0.58, 1.])
    );
    assert!(response["nativeEffects"][1].is_null());
}
