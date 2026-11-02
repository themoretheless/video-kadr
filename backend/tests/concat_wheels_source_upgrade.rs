mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn preserves_white_balance_and_original_wheel_uniform_layout(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let(status,body,_)=support::send(&app,request(json!([{"id":"concat.white-balance"},{"id":"concat.white-balance","params":{"temperature":8500,"tint":25}},{"id":"concat.color-wheels","params":{"lift":0.75}},{"id":"concat.color-wheels","params":{"lift.x":0.5,"lift.y":-0.25,"lift.m":0.125,"gamma.m":-0.5,"gain.y":0.75}},{"id":"concat.color-wheels","enabled":false}]))).await;assert_eq!(status,StatusCode::OK,"{body}");
 assert_eq!(body["nativeEffects"],json!([{"kind":"concat_white_balance","parameters":{"temperature":6500.,"tint":0.}},{"kind":"concat_white_balance","parameters":{"temperature":8500.,"tint":25.}},{"kind":"concat_color_wheels","parameters":{"lift":[0.,0.,0.],"gamma":[0.,0.,0.],"gain":[0.,0.,0.]}},{"kind":"concat_color_wheels","parameters":{"lift":[0.5,-0.25,0.125],"gamma":[0.,0.,-0.5],"gain":[0.,0.75,0.]}},null]));
}
#[tokio::test]
async fn rejects_bad_channels_out_of_range_and_unrepresentable_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for link in [json!({"id":"concat.color-wheels","params":{"lift.z":0.1}}),json!({"id":"concat.color-wheels","params":{"gain.x":1.00000001}}),json!({"id":"concat.color-wheels","params":{"gamma.m":-1.00000001}}),json!({"id":"concat.white-balance","params":{"temperature":1999.9999999}}),json!({"id":"concat.white-balance","params":{"tint":100.0000001}}),json!({"id":"concat.white-balance","params":{"unknown":0}}),json!({"id":"concat.color-wheels","keys":{"lift.z":[{"at":0,"value":0}]}}),json!({"id":"concat.white-balance","keys":{"unknown":[{"at":0,"value":6500}]}})]{let(status,body,_)=support::send(&app,request(json!([link]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
}
