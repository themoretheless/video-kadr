mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
#[tokio::test]
async fn converts_vhs_and_dust_defaults_explicit_values_and_disabled_links(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for(id,kind,a,b)in[("concat.vhs","concat_vhs","bleed","lines"),("concat.falling-dust","concat_falling_dust","dust","leak")]{
  let links=json!([{"id":id},{"id":id,"params":{a:75.25,b:80.5}},{"id":id,"enabled":false}]);
  let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{body}");
  assert_eq!(body["nativeEffects"],json!([{"kind":kind,"parameters":{a:60.,b:50.}},{"kind":kind,"parameters":{a:75.25,b:80.5}},null]));
  let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());let mut stripped=body;stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
 }
}
#[tokio::test]
async fn rejects_bounds_unknown_controls_and_unrepresentable_animation(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for(id,a,b)in[("concat.vhs","bleed","lines"),("concat.falling-dust","dust","leak")]{
  for link in [json!({"id":id,"params":{a:100.0000001}}),json!({"id":id,"params":{b:-0.0000001}}),json!({"id":id,"params":{"unknown":1}}),json!({"id":id,"keys":{a:[{"at":0,"value":60}]}})]{
   let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");
  }
 }
}
