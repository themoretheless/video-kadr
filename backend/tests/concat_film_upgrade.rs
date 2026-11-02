mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
#[tokio::test]
async fn converts_static_film_without_losing_original_links(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let links=json!([{"id":"concat.film"},{"id":"concat.film","params":{"grain":75.25,"warmth":80.5}},{"id":"concat.film","enabled":false}]);
 let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{body}");
 assert_eq!(body["nativeEffects"],json!([{"kind":"concat_film","parameters":{"grain":40.,"warmth":50.}},{"kind":"concat_film","parameters":{"grain":75.25,"warmth":80.5}},null]));
 let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());
 let mut stripped=body.clone();stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
}
#[tokio::test]
async fn rejects_unknown_controls_out_of_bounds_and_unrepresentable_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for link in [json!({"id":"concat.film","params":{"grain":100.0000001}}),json!({"id":"concat.film","params":{"warmth":-0.0000001}}),json!({"id":"concat.film","params":{"unknown":1}}),json!({"id":"concat.film","keys":{"grain":[{"at":0,"value":60}]}})] {
  let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");
 }
}
