mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn preserves_original_pairing_clamping_sorting_dedup_and_identity(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let links=json!([{"id":"concat.curves","params":{"luma":0.5,"luma.00.x":0,"luma.00.y":1,"luma.01.x":1,"luma.01.y":0}},{"id":"concat.curves","params":{"luma.0.x":1.1,"luma.0.y":1.2,"luma.1.x":0.5,"luma.1.y":0.2,"luma.2.x":0.5000005,"luma.2.y":0.8,"luma.3.x":-0.1,"luma.3.y":-2,"red.0.x":0.3,"red.0.y":0.9,"blue.0.x":1}},{"id":"concat.curves","enabled":false}]);
 let(status,body,_)=support::send(&app,request(links)).await;assert_eq!(status,StatusCode::OK,"{body}");let identity=json!([[0.,0.],[1.,1.]]);for channel in ["luma","red","green","blue"]{assert_eq!(body["nativeEffects"][0]["parameters"][channel],identity);}
 assert_eq!(body["nativeEffects"][1]["parameters"]["luma"],json!([[0.,0.],[0.5000005,0.8],[1.,1.]]));for channel in ["red","green","blue"]{assert_eq!(body["nativeEffects"][1]["parameters"][channel],identity);}assert!(body["nativeEffects"][2].is_null());
}
#[tokio::test]
async fn rejects_invalid_curve_keys_and_animation(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for key in ["luma.8.x","luma.-1.y","red.0.z","unknown","lift.x"]{let(status,body,_)=support::send(&app,request(json!([{"id":"concat.curves","params":{key:0}}]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
 let(status,body,_)=support::send(&app,request(json!([{"id":"concat.curves","keys":{"luma.0.y":[{"at":0,"value":0}]}}]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");
}
