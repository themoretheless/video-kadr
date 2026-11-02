mod support;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use video_kadr_backend::domain::concat_retime_edit::{Clock, Point};
fn request(value: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/compositions/concat-retime-edit")
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .unwrap()
}
fn bits(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
#[tokio::test]
#[ignore = "requires scripts/qualify-concat-freeze-capture.py"]
async fn original_studio_still_capture_clocks_match_actual_http_for_curved_and_constant_clips() {
    let records =
        std::fs::read_to_string(std::env::var("CONCAT_FREEZE_CAPTURE_ORACLE").unwrap()).unwrap();
    assert_eq!(records.lines().count(), 54);
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let (mut checked, mut premature_millisecond_controls, mut curve_controls) = (0, 0, 0);
    for line in records.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        for curved in [false, true] {
            let clock = Clock {
                start: bits(f[0]),
                source_start: bits(f[1]),
                speed: bits(f[2]),
                duration: 3.,
                speed_curve: curved
                    .then(|| vec![Point { at: 0., speed: 1. }, Point { at: 1., speed: 3. }]),
            };
            let at = bits(f[3]);
            let expected = bits(f[4]);
            let seek: u64 = f[5].parse().unwrap();
            let cache: u64 = f[6].parse().unwrap();
            let(status,body,text)=support::send(&app,request(json!({"clock":clock,"rateNum":25,"rateDen":1,"command":{"kind":"freeze","time":at,"duration":1.}}))).await;
            assert_eq!(status, StatusCode::OK, "{line}: {text}");
            assert_eq!(body["changed"], true);
            let capture = &body["capture"];
            assert_eq!(capture["selection"], "atOrAfterThenFirst");
            assert_eq!(
                capture["sourceSeconds"].as_f64().unwrap().to_bits(),
                expected.to_bits(),
                "{line}"
            );
            assert_eq!(capture["seekTimeUs"], seek);
            assert_eq!(capture["cacheTimeMs"], cache);
            assert_eq!(capture["width"], 1280);
            assert_eq!(capture["jpegQuality"], 4);
            assert_eq!(body["parts"].as_array().unwrap().len(), 3);
            assert_eq!(body["rippleBy"], 1.0);
            if (seek as f64 / 1000.).round() as u64 != cache {
                premature_millisecond_controls += 1;
            }
            if curved {
                let p = clock.placement(25, 1).unwrap();
                let curve_source = p
                    .concat_retime
                    .as_ref()
                    .unwrap()
                    .sample(
                        p.timeline_start_tick,
                        p.source_in_tick,
                        (at * 254016000000.).round() as i128,
                    )
                    .unwrap()
                    .unwrap();
                if (curve_source as f64 / 254016000000. - expected).abs() > 1e-6 {
                    curve_controls += 1;
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 108);
    assert!(premature_millisecond_controls >= 12);
    assert!(curve_controls >= 20);
    println!("{checked} original Studio freeze capture plans through HTTP; {premature_millisecond_controls} cache-rounding controls; {curve_controls} curved-clock controls");
}
#[tokio::test]
async fn noop_and_invalid_freezes_never_return_a_capture_request() {
    let clock = Clock {
        start: 2.,
        duration: 3.,
        source_start: 1.5,
        speed: 1.75,
        speed_curve: None,
    };
    assert!(clock.still_capture(f64::NAN).is_err());
    assert!(clock.still_capture(1.).is_err());
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    for time in [1., 2., 5., 6.] {
        let(status,body,text)=support::send(&app,request(json!({"clock":clock,"rateNum":25,"rateDen":1,"command":{"kind":"freeze","time":time,"duration":1.}}))).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert_eq!(body["changed"], false);
        assert!(body.get("capture").is_none());
    }
    let (status, body, _) = support::send(
        &app,
        request(
            json!({"clock":clock,"rateNum":25,"rateDen":1,"command":{"kind":"split","time":3.}}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("capture").is_none());
}
