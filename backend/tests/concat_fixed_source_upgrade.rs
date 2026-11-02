mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
const CASES:&[(&str,&str)]=&[("concat.autumn","concat_autumn"),("concat.bleach-bypass","concat_bleach_bypass"),("concat.classic-chrome","concat_classic_chrome"),("concat.cyberpunk","concat_cyberpunk"),("concat.delicious","concat_delicious"),("concat.dreamy","concat_dreamy"),("concat.high-contrast-mono","concat_high_contrast_mono"),("concat.landscape-pop","concat_landscape_pop"),("concat.natural","concat_natural"),("concat.noir","concat_noir"),("concat.retro-70s","concat_retro_70s"),("concat.rosy","concat_rosy"),("concat.sci-fi","concat_sci_fi"),("concat.silver","concat_silver"),("concat.tropical","concat_tropical"),("concat.vaporwave","concat_vaporwave"),("concat.winter-cool","concat_winter_cool")];
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
#[tokio::test]
async fn fixed_presets_preserve_canonical_response_and_disabled_links(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for &(id,kind)in CASES{
  let links=json!([{"id":id},{"id":id,"enabled":false}]);let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{id} {body}");assert_eq!(body["nativeEffects"],json!([{"kind":kind,"parameters":{}},null]));
  let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());let mut stripped=body;stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
 }
}
#[tokio::test]
async fn fixed_presets_refuse_extra_parameters_and_nonempty_keys(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for &(id,_)in CASES{for link in [json!({"id":id,"params":{"unknown":1}}),json!({"id":id,"keys":{"unknown":[{"at":0,"value":0}]}})]{let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{id} {body}");}}
}
