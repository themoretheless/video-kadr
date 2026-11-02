mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
#[tokio::test]
async fn converts_static_digicam_without_losing_original_links(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let links=json!([{"id":"concat.digicam"},{"id":"concat.digicam","params":{"flash":75.25,"noise":80.5}},{"id":"concat.digicam","enabled":false}]);
 let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{body}");
 assert_eq!(body["nativeEffects"],json!([{"kind":"concat_digicam","parameters":{"flash":60.,"noise":40.}},{"kind":"concat_digicam","parameters":{"flash":75.25,"noise":80.5}},null]));
 let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());
 let mut stripped=body.clone();stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
}
#[tokio::test]
async fn rejects_unknown_controls_out_of_bounds_and_unrepresentable_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for link in [json!({"id":"concat.digicam","params":{"flash":100.0000001}}),json!({"id":"concat.digicam","params":{"noise":-0.0000001}}),json!({"id":"concat.digicam","params":{"unknown":1}}),json!({"id":"concat.digicam","keys":{"flash":[{"at":0,"value":60}]}})] {
  let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");
 }
}
