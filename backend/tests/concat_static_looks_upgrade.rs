mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
fn cases()->Vec<(&'static str,&'static str,Value,Value)>{vec![
 ("concat.mono","concat_mono",json!({"colour":40.,"contrast":40.,"grain":0.}),json!({"colour":-75.25,"contrast":80.5,"grain":33.25})),
 ("concat.midnight","concat_midnight",json!({"depth":60.,"grain":25.}),json!({"depth":75.25,"grain":80.5})),
 ("concat.marquee","concat_marquee",json!({"lights":60.,"depth":40.}),json!({"lights":75.25,"depth":80.5})),
 ("concat.retro-cartoon","concat_retro_cartoon",json!({"levels":5.,"ink":60.}),json!({"levels":4.25,"ink":80.5})),
 ("concat.super-8","concat_super8",json!({}),json!({})),
]}
#[tokio::test]
async fn preserves_defaults_static_settings_disabled_links_and_canonical_response(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for(id,kind,defaults,settings)in cases(){
  let links=json!([{"id":id},{"id":id,"params":settings},{"id":id,"enabled":false}]);
  let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{id} {body}");assert_eq!(body["nativeEffects"],json!([{"kind":kind,"parameters":defaults},{"kind":kind,"parameters":settings},null]));
  let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());let mut stripped=body;stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
 }
}
#[tokio::test]
async fn refuses_unknown_controls_unrepresentable_keys_and_pre_narrowing_bounds(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let mut links=vec![];
 for(id,_,_,_)in cases(){links.push(json!({"id":id,"params":{"unknown":1}}));links.push(json!({"id":id,"keys":{"unknown":[{"at":0,"value":0}]}}));}
 links.extend([json!({"id":"concat.mono","params":{"colour":-100.0000001}}),json!({"id":"concat.midnight","params":{"depth":100.0000001}}),json!({"id":"concat.marquee","params":{"lights":-0.0000001}}),json!({"id":"concat.retro-cartoon","params":{"levels":1.99999999}}),json!({"id":"concat.retro-cartoon","params":{"levels":8.00000001}})]);
 for link in links{let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
}
