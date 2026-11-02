mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
const CONTRACTS:&str=r#"[{"id": "concat.neon", "kind": "concat_neon", "fields": [["glow", 60, 0, 100], ["punch", 50, 0, 100]]}, {"id": "concat.pink-fuzz", "kind": "concat_pink_fuzz", "fields": [["haze", 60, 0, 100], ["blush", 50, 0, 100]]}, {"id": "concat.night", "kind": "concat_night", "fields": [["lift", 60, 0, 100], ["clarity", 40, 0, 100]]}, {"id": "concat.spooky", "kind": "concat_spooky", "fields": [["dread", 60, 0, 100]]}, {"id": "concat.cool", "kind": "concat_cool", "fields": [["temperature", 8500, 7000, 11000]]}, {"id": "concat.warm", "kind": "concat_warm", "fields": [["temperature", 4600, 3000, 6000]]}, {"id": "concat.teal-orange", "kind": "concat_teal_orange", "fields": [["amount", 70, 0, 100], ["contrast", 25, 0, 100], ["skin", 80, 0, 100]]}]"#;
fn request(links:Value,include:bool)->Request<Body>{Request::builder().method("POST").uri("/api/compositions/concat-effects/upgrade").header("content-type","application/json").body(Body::from(json!({"schemaVersion":1,"links":links,"includeNativeEffects":include}).to_string())).unwrap()}
#[tokio::test]
async fn defaults_explicit_parameters_disabled_links_and_original_response(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);let contracts:Vec<Value>=serde_json::from_str(CONTRACTS).unwrap();
 for c in contracts{
  let mut defaults=serde_json::Map::new();let mut settings=serde_json::Map::new();for field in c["fields"].as_array().unwrap(){let key=field[0].as_str().unwrap();defaults.insert(key.into(),field[1].clone());let lo=field[2].as_f64().unwrap();let hi=field[3].as_f64().unwrap();settings.insert(key.into(),json!(lo+(hi-lo)*0.375));}
  let links=json!([{"id":c["id"]},{"id":c["id"],"params":settings},{"id":c["id"],"enabled":false}]);let(status,body,_)=support::send(&app,request(links.clone(),true)).await;assert_eq!(status,StatusCode::OK,"{body}");let native=body["nativeEffects"].as_array().unwrap();assert_eq!(native.len(),3);assert!(native[2].is_null());
  for(index,expected)in [defaults,settings].iter().enumerate(){assert_eq!(native[index]["kind"],c["kind"]);assert_eq!(native[index]["parameters"].as_object().unwrap().len(),expected.len());for(key,value)in expected{assert_eq!(native[index]["parameters"][key].as_f64(),value.as_f64());}}
  let(status,plain,_)=support::send(&app,request(links,false)).await;assert_eq!(status,StatusCode::OK);assert!(plain.get("nativeEffects").is_none());let mut stripped=body;stripped.as_object_mut().unwrap().remove("nativeEffects");assert_eq!(stripped,plain);
 }
}
#[tokio::test]
async fn rejects_unknown_fields_nonempty_keys_and_each_numeric_bound(){
 let(state,_dir)=support::make_state(false,false).await;let app=support::router(state);let contracts:Vec<Value>=serde_json::from_str(CONTRACTS).unwrap();
 for c in contracts{
  let mut links=vec![json!({"id":c["id"],"params":{"unknown":1}}),json!({"id":c["id"],"keys":{"unknown":[{"at":0,"value":0}]}})];
  for field in c["fields"].as_array().unwrap(){let key=field[0].as_str().unwrap();for value in [field[2].as_f64().unwrap()-0.0000001,field[3].as_f64().unwrap()+0.0000001]{links.push(json!({"id":c["id"],"params":{key:value}}));}}
  for link in links{let(status,body,_)=support::send(&app,request(json!([link]),true)).await;assert_eq!(status,StatusCode::BAD_REQUEST,"{body}");}
 }
}
