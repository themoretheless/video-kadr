mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn combines_scalars_original_wheels_and_sparse_curves(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let(status,body,_)=support::send(&app,request(json!([{"id":"concat.adjust"},{"id":"concat.adjust","params":{"temperature":5000,"exposure":1.25,"contrast":-25,"lift":0.8,"lift.x":0.5,"lift.m":-0.25,"red.0.x":1.5,"red.0.y":1.2,"red.7.x":-0.5,"red.7.y":0.1}},{"id":"concat.adjust","enabled":false}]))).await;assert_eq!(status,StatusCode::OK,"{body}");let defaults=&body["nativeEffects"][0]["parameters"];assert_eq!(defaults["temperature"].as_f64(),Some(6500.));assert_eq!(defaults["lift"],json!([0.,0.,0.]));let p=&body["nativeEffects"][1]["parameters"];assert_eq!(p["exposure"].as_f64(),Some(1.25));assert_eq!(p["contrast"].as_f64(),Some(-25.));assert_eq!(p["lift"],json!([0.5,0.,-0.25]));assert_eq!(p["red"],json!([[0.,0.1],[1.,1.]]));assert!(body["nativeEffects"][2].is_null());
}
#[tokio::test]
async fn rejects_scalar_and_compound_bounds_and_unrepresentable_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for params in [json!({"exposure":3.00000001}),json!({"temperature":1999.9999999}),json!({"saturation":-100.0000001}),json!({"lift.m":1.00000001}),json!({"lift.z":0}),json!({"red.8.x":0}),json!({"unknown":0})]{let(status,body,_)=support::send(&app,request(json!([{"id":"concat.adjust","params":params}]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
 let(status,body,_)=support::send(&app,request(json!([{"id":"concat.adjust","keys":{"exposure":[{"at":0,"value":0}]}}]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");
}
