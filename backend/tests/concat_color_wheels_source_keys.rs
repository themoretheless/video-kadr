mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
use video_kadr_backend::domain::{concat_effect_upgrade::{native_effect,AppliedFilter},concat_color_wheels::{sample_keys,ColorWheels},composition::VideoEffect,flat_surface,group_surface::GroupSurface};
#[test]
fn dotted_axes_overshoot_bare_key_and_required_duration(){
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.color-wheels","keys":{"lift.x":[{"at":0,"value":0},{"at":1,"value":1,"ease":[0.42,3,0.58,3]}],"gain.m":[{"at":0,"value":0.25}],"lift":[{"at":0,"value":1}]}})).unwrap();
 let effect=native_effect(&link).unwrap().unwrap();let VideoEffect::ConcatColorWheels{parameters,concat_keys:Some(keys),animation:None}=effect.clone()else{panic!()};let mid=sample_keys(parameters,&keys,0.5).unwrap();assert_eq!(mid.lift,[2.375,0.,0.]);assert_eq!(mid.gain,[0.,0.,0.25]);assert_eq!(sample_keys(parameters,&keys,1.).unwrap().lift,[1.,0.,0.]);assert!(sample_keys(ColorWheels::default(),&keys,f64::NAN).is_err());
 let mut surface=GroupSurface{width:1,height:1,rgba:vec![32,64,128,255]};let before=surface.clone();assert!(flat_surface::apply_effect_stack(&mut surface,&[effect],500000,&[],&mut||Ok(())).is_err());assert_eq!(surface.rgba,before.rgba);
}
fn request(link:Value)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":[link],"includeNativeEffects":true}).to_string())).unwrap()}
#[tokio::test]
async fn source_axis_bounds_before_narrowing_and_incoming_easing(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for(channel,value)in [("lift.x",1.00000001),("gain.m",-1.00000001),("lift.z",0.),("bad.x",0.)]{let(status,body,_)=support::send(&app,request(json!({"id":"concat.color-wheels","keys":{channel:[{"at":0,"value":value}]}}))).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
 let(status,body,_)=support::send(&app,request(json!({"id":"concat.color-wheels","keys":{"gamma.y":[{"at":1,"value":0.5,"ease":"out"}]}}))).await;assert_eq!(status,StatusCode::OK,"{body}");assert_eq!(body["nativeEffects"][0]["concatKeys"]["gamma.y"][0]["ease"],json!([0.,0.,0.58,1.]));
}
