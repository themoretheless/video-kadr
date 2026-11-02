mod support;
use axum::{body::{to_bytes,Body},http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;

#[tokio::test]
#[ignore = "requires the fresh UI conversion clock trace"]
async fn conversion_http_measures_the_clocked_native_motion() {
    let oracle:Value=serde_json::from_slice(&std::fs::read(std::env::var("NATIVE_POSITION_CONVERSION_CLOCK_ORACLE").unwrap()).unwrap()).unwrap();
    assert_eq!(oracle["schemaVersion"],1);
    let (state,_directory)=support::make_state(true,false).await;
    let app=support::router(state);
    let request=|body:Value|Request::builder().method("POST").uri("/api/compositions/position-conversion").header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
    let response=app.clone().oneshot(request(oracle["request"].clone())).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();
    assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
    let result:Value=serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["withinTolerance"],true);
    assert!(result["maxErrorPx"].as_f64().unwrap()<=0.5);
    assert!(result["nodes"].is_null());
    let tracks:Vec<weftcut_conversion::Track>=serde_json::from_value(result["tracks"].clone()).unwrap();
    assert!(tracks.iter().all(|track|track.sample_grid.is_none()));
    let position:video_kadr_backend::domain::motion_path::PositionAnimation=serde_json::from_value(oracle["position"].clone()).unwrap();
    let original=position.compile().unwrap();
    for sample in oracle["samples"].as_array().unwrap(){
        let time=sample["time"].as_f64().unwrap();
        let expected=original.sample_at(video_kadr_backend::domain::animation_sample_time::AnimationSampleTime::SignedMicroseconds(time as i64)).unwrap();
        assert!((expected.x-sample["value"].as_f64().unwrap()*300.).abs()<1e-9);
        let x=weftcut_eval::eval_precise(&tracks[0].keys,tracks[0].extrapolate,time,0.);
        let y=weftcut_eval::eval_precise(&tracks[1].keys,tracks[1].extrapolate,time,0.);
        assert!((x-expected.x).abs()<=0.5,"time={time}: actual={x}, expected={}",expected.x);
        assert_eq!(y,expected.y);
    }
    // A stripped-clock request must not accidentally satisfy the same evidence.
    let mut legacy=oracle["request"].clone();legacy["tracks"][0].as_object_mut().unwrap().remove("sampleGrid");
    let response=app.clone().oneshot(request(legacy)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
    let legacy:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
    let legacy_tracks:Vec<weftcut_conversion::Track>=serde_json::from_value(legacy["tracks"].clone()).unwrap();
    assert!((weftcut_eval::eval_precise(&legacy_tracks[0].keys,legacy_tracks[0].extrapolate,250000.,0.)-150.).abs()>50.);
    let response=app.clone().oneshot(request(oracle["xyRequest"].clone())).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
    let xy:Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(xy["withinTolerance"],true);
    let output:Vec<weftcut_conversion::Track>=serde_json::from_value(xy["tracks"].clone()).unwrap();assert_eq!(output.len(),1);
    let nodes:Vec<weftcut_eval::path::Node>=serde_json::from_value(xy["nodes"].clone()).unwrap();
    let mut path_samples=Vec::new();let (start,end)=weftcut_eval::path::compile(&nodes,|sample|path_samples.push(sample));
    let original:video_kadr_backend::domain::motion_path::PositionAnimation=serde_json::from_value(oracle["xyPosition"].clone()).unwrap();let original=original.compile().unwrap();
    for frame in 0..=30 {
        let time=weftcut_eval::time_us_at_frame(frame,30,1);
        let expected=original.sample_at(video_kadr_backend::domain::animation_sample_time::AnimationSampleTime::SignedMicroseconds(time)).unwrap();
        let progress=weftcut_eval::eval_precise(&output[0].keys,output[0].extrapolate,time as f64,0.);
        let point=weftcut_eval::path::evaluate(&path_samples,progress,start,end);
        assert!((point.x-expected.x).hypot(point.y-expected.y)<=0.5,"frame {frame}");
    }
    let mut invalid=oracle["request"].clone();invalid["tracks"][0]["sampleGrid"]["rateDen"]=json!(0);
    let response=app.oneshot(request(invalid)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
}
