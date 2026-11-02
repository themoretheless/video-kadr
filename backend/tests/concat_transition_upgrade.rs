mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
fn request(body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-effects/upgrade")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
async fn converts_twenty_two_transition_descriptors_without_inventing_placement() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let links = json!([{"id":"concat.dissolve"},{"id":"concat.fade-black"},{"id":"concat.fade-white"},{"id":"concat.linear-wipe","params":{"direction":2,"softness":70.5}},{"id":"concat.clock-wipe"},{"id":"concat.iris","params":{"softness":45.5}},{"id":"concat.shape-wipe","params":{"shape":2,"softness":37.5}},{"id":"concat.push","params":{"direction":2}},{"id":"concat.slide","params":{"direction":3}},{"id":"concat.rgb-split-wipe","params":{"spread":37.5}},{"id":"concat.spin","params":{"turns":1.25}},{"id":"concat.swirl","params":{"twist":35.5}},{"id":"concat.blur-dissolve","params":{"strength":37.5}},{"id":"concat.whip-pan","params":{"speed":39.5}},{"id":"concat.zoom-transition","params":{"direction":1}},{"id":"concat.light-leak-flash","params":{"strength":37.5}},{"id":"concat.light-leak-transition","params":{"warmth":37.5}},{"id":"concat.zoom-blur-transition","params":{"strength":37.5}},{"id":"concat.film-burn","params":{"heat":37.5}},{"id":"concat.cube"},{"id":"concat.glitch-transition","params":{"amount":37.5}},{"id":"concat.glitch-warp","params":{"amount":37.5}},{"id":"concat.linear-wipe","enabled":false},{"id":"unknown.transition"}]);
    let(status,body,_)=support::send(&app,request(json!({"schemaVersion":1,"links":links,"includeNativeTransitions":true,"includeNativeEffects":true}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["nativeTransitions"],
        json!([{"kind":"concat_dissolve"},{"kind":"concat_fade_black"},{"kind":"concat_fade_white"},{"kind":"procedural","kernel":{"effect":"concat.linear-wipe","params":{"direction":{"Choice":2},"softness":{"Float":70.5}}}},{"kind":"procedural","kernel":{"effect":"concat.clock-wipe"}},{"kind":"procedural","kernel":{"effect":"concat.iris","params":{"softness":{"Float":45.5}}}},{"kind":"procedural","kernel":{"effect":"concat.shape-wipe","params":{"shape":{"Choice":2},"softness":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.push","params":{"direction":{"Choice":2}}}},{"kind":"procedural","kernel":{"effect":"concat.slide","params":{"direction":{"Choice":3}}}},{"kind":"procedural","kernel":{"effect":"concat.rgb-split-wipe","params":{"spread":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.spin","params":{"turns":{"Float":1.25}}}},{"kind":"procedural","kernel":{"effect":"concat.swirl","params":{"twist":{"Float":35.5}}}},{"kind":"procedural","kernel":{"effect":"concat.blur-dissolve","params":{"strength":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.whip-pan","params":{"speed":{"Float":39.5}}}},{"kind":"procedural","kernel":{"effect":"concat.zoom-transition","params":{"direction":{"Choice":1}}}},{"kind":"procedural","kernel":{"effect":"concat.light-leak-flash","params":{"strength":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.light-leak-transition","params":{"warmth":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.zoom-blur-transition","params":{"strength":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.film-burn","params":{"heat":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.cube"}},{"kind":"procedural","kernel":{"effect":"concat.glitch-transition","params":{"amount":{"Float":37.5}}}},{"kind":"procedural","kernel":{"effect":"concat.glitch-warp","params":{"amount":{"Float":37.5}}}},null,null])
    );
    assert!(body["nativeEffects"]
        .as_array()
        .unwrap()
        .iter()
        .all(Value::is_null));
    for descriptor in body["nativeTransitions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| !v.is_null())
    {
        let parsed: video_kadr_backend::domain::concat_effect_upgrade::NativeTransition =
            serde_json::from_value(descriptor.clone()).unwrap();
        if let Some(kernel) = parsed.kernel {
            kernel.validate().unwrap();
        }
        assert!(descriptor.get("timeline").is_none());
        assert!(descriptor.get("fromClipId").is_none());
    }
    let (status, plain, _) =
        support::send(&app, request(json!({"schemaVersion":1,"links":links}))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(plain.get("nativeTransitions").is_none());
}
#[tokio::test]
async fn rejects_unrepresentable_transition_settings_and_animation() {
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for link in [json!({"id":"concat.glitch-warp","params":{"amount":101}}),json!({"id":"concat.glitch-warp","params":{"other":1}}),json!({"id":"concat.glitch-warp","keys":{"amount":[{"at":0,"value":0}]}}),json!({"id":"concat.glitch-transition","params":{"amount":101}}),json!({"id":"concat.glitch-transition","params":{"other":1}}),json!({"id":"concat.glitch-transition","keys":{"amount":[{"at":0,"value":0}]}}),json!({"id":"concat.cube","params":{"other":1}}),json!({"id":"concat.cube","keys":{"other":[{"at":0,"value":0}]}}),json!({"id":"concat.film-burn","params":{"heat":101}}),json!({"id":"concat.film-burn","params":{"other":1}}),json!({"id":"concat.film-burn","keys":{"heat":[{"at":0,"value":0}]}}),json!({"id":"concat.zoom-blur-transition","params":{"strength":101}}),json!({"id":"concat.zoom-blur-transition","params":{"other":1}}),json!({"id":"concat.zoom-blur-transition","keys":{"strength":[{"at":0,"value":0}]}}),json!({"id":"concat.light-leak-transition","params":{"warmth":101}}),json!({"id":"concat.light-leak-transition","params":{"other":1}}),json!({"id":"concat.light-leak-transition","keys":{"warmth":[{"at":0,"value":0}]}}),json!({"id":"concat.light-leak-flash","params":{"strength":101}}),json!({"id":"concat.light-leak-flash","params":{"other":1}}),json!({"id":"concat.light-leak-flash","keys":{"strength":[{"at":0,"value":0}]}}),json!({"id":"concat.zoom-transition","params":{"direction":2}}),json!({"id":"concat.zoom-transition","params":{"other":1}}),json!({"id":"concat.zoom-transition","keys":{"direction":[{"at":0,"value":0}]}}),json!({"id":"concat.whip-pan","params":{"speed":19}}),json!({"id":"concat.whip-pan","params":{"other":1}}),json!({"id":"concat.whip-pan","keys":{"speed":[{"at":0,"value":80}]}}),json!({"id":"concat.blur-dissolve","params":{"strength":101}}),json!({"id":"concat.blur-dissolve","params":{"other":1}}),json!({"id":"concat.blur-dissolve","keys":{"strength":[{"at":0,"value":0}]}}),json!({"id":"concat.spin","params":{"turns":0.49}}),json!({"id":"concat.swirl","params":{"twist":101}}),json!({"id":"concat.rgb-split-wipe","params":{"spread":101}}),json!({"id":"concat.rgb-split-wipe","params":{"direction":0}}),json!({"id":"concat.push","params":{"direction":4}}),json!({"id":"concat.slide","params":{"direction":-1}}),json!({"id":"concat.shape-wipe","params":{"shape":3}}),json!({"id":"concat.shape-wipe","params":{"softness":101}}),
        json!({"id":"concat.linear-wipe","params":{"direction":4}}),
        json!({"id":"concat.linear-wipe","params":{"softness":101}}),
        json!({"id":"concat.linear-wipe","params":{"other":1}}),
        json!({"id":"concat.dissolve","params":{"amount":50}}),
        json!({"id":"concat.linear-wipe","keys":{"softness":[{"at":0,"value":20},{"at":1,"value":80}]}}),
    ] {
        let (status, body, _) = support::send(
            &app,
            request(json!({"schemaVersion":1,"includeNativeTransitions":true,"links":[link]})),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
}
