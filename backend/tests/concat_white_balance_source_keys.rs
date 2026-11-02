mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
use video_kadr_backend::domain::{concat_effect_upgrade::{native_effect,AppliedFilter},concat_white_balance::{sample_keys,WhiteBalance},composition::VideoEffect,flat_surface,group_surface::GroupSurface};
#[test]
fn normalized_channels_preserve_overshoot_and_require_duration(){
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.white-balance","keys":{"temperature":[{"at":0,"value":6500},{"at":1,"value":7500,"ease":[0.42,3,0.58,3]}],"tint":[{"at":0,"value":0},{"at":1,"value":100,"ease":[0.42,3,0.58,3]}]}})).unwrap();
 let effect=native_effect(&link).unwrap().unwrap();let VideoEffect::ConcatWhiteBalance{parameters,concat_keys:Some(keys),animation:None}=effect.clone()else{panic!()};let midpoint=sample_keys(parameters,&keys,0.5).unwrap();assert_eq!(midpoint.temperature,8875.);assert_eq!(midpoint.tint,237.5);assert_eq!(sample_keys(parameters,&keys,0.).unwrap(),WhiteBalance::default());assert_eq!(sample_keys(parameters,&keys,1.).unwrap(),WhiteBalance{temperature:7500.,tint:100.});assert!(sample_keys(parameters,&keys,f64::NAN).is_err());
 let mut surface=GroupSurface{width:1,height:1,rgba:vec![32,64,128,255]};let before=surface.clone();assert!(flat_surface::apply_effect_stack(&mut surface,&[effect],500000,&[],&mut||Ok(())).is_err());assert_eq!(surface.rgba,before.rgba);
}
fn request(link:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":[link],"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn bounds_before_narrowing_unknown_channels_and_original_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for(channel,value)in [("temperature",1999.9999999),("temperature",11000.0000001),("tint",100.0000001),("unknown",0.)]{let(status,body,_)=support::send(&app,request(json!({"id":"concat.white-balance","keys":{channel:[{"at":0,"value":value}]}}))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
 let(status,body,_)=support::send(&app,request(json!({"id":"concat.white-balance","keys":{"temperature":[{"at":1,"value":7500,"ease":"out"}],"tint":[{"at":0,"value":25}]}}))).await;assert_eq!(status,StatusCode::OK,"{body}");assert_eq!(body["nativeEffects"][0]["concatKeys"]["tint"][0]["value"],25.);assert_eq!(body["nativeEffects"][0]["concatKeys"]["temperature"][0]["ease"],json!([0.,0.,0.58,1.]));
}
