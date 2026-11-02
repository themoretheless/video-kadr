mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
use video_kadr_backend::domain::{concat_effect_upgrade::{native_effect,AppliedFilter},concat_exposure::{sample_concat_keys,ConcatExposure},composition::VideoEffect,flat_surface,group_surface::GroupSurface};
#[test]
fn source_keys_keep_incoming_easing_and_finite_runtime_overshoot(){
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.exposure","params":{"stops":-1},"keys":{"stops":[{"at":0,"value":0},{"at":1,"value":2,"ease":[0.42,3,0.58,3]}]}})).unwrap();
 let effect=native_effect(&link).unwrap().unwrap();let VideoEffect::ConcatExposure{parameters,concat_keys:Some(keys),animation:None}=effect.clone()else{panic!()};assert_eq!(keys.len(),2);assert_eq!(sample_concat_keys(parameters,&keys,0.5).unwrap(),4.75);assert_eq!(sample_concat_keys(parameters,&keys,0.).unwrap(),0.);assert_eq!(sample_concat_keys(parameters,&keys,1.).unwrap(),2.);
 let mut surface=GroupSurface{width:1,height:1,rgba:vec![32,64,128,255]};let before=surface.clone();assert!(flat_surface::apply_effect_stack(&mut surface,&[effect],500000,&[],&mut||Ok(())).is_err());assert_eq!(surface.rgba,before.rgba);
 assert!(sample_concat_keys(ConcatExposure::default(),&keys,f64::NAN).is_err());
}
fn request(links:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn source_exposure_conversion_preserves_keys_and_rejects_invalid_values(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let(status,body,_)=support::send(&app,request(json!([{"id":"concat.exposure","keys":{"stops":[{"at":0,"value":0},{"at":1,"value":2,"ease":[0.42,3,0.58,3]}]}}]))).await;assert_eq!(status,StatusCode::OK,"{body}");assert_eq!(body["nativeEffects"][0]["concatKeys"][1]["ease"],json!([0.42,3.,0.58,3.]));
 for link in [json!({"id":"concat.exposure","keys":{"stops":[{"at":0,"value":4.0000001}]}}),json!({"id":"concat.exposure","keys":{"unknown":[{"at":0,"value":0}]}})]{let(status,body,_)=support::send(&app,request(json!([link]))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
}
