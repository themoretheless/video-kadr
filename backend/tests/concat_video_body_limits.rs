mod support;
use axum::{body::Body,http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;
fn points(count:usize)->Vec<[f64;2]>{vec![[-2.2250738585072014e-308,1.2345678901234568e-307];count]}
fn binding()->Value{json!({"sourceHash":"a".repeat(64),"modelHash":"88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828","runtimeHash":"b".repeat(64),"subject":"person","correction":{"custom":false,"feather":0,"strokes":[{"tool":"brush","size":0.005,"points":points(65536)}]}})}
#[test]
fn cutout_allowance_covers_valid_content_without_expanding_unrelated_metadata(){
 let binding=binding();let mut document=json!({"schemaVersion":1,"sources":{},"tracks":[{"kind":"video","clips":[{"concatCutout":binding}]}]});let bytes=serde_json::to_vec(&binding).unwrap().len();assert!(bytes>2*1024*1024);
 let limit=video_kadr_backend::db::composition_document_byte_limit(&document).unwrap();assert_eq!(limit,video_kadr_backend::db::MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES+bytes);assert!(serde_json::to_vec(&document).unwrap().len()<limit);
 document["blob"]=json!("x".repeat(video_kadr_backend::db::MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES+1));assert_eq!(video_kadr_backend::db::composition_document_byte_limit(&document).unwrap(),limit);assert!(serde_json::to_vec(&document).unwrap().len()>limit);
 document["tracks"][0]["clips"][0]["concatCutout"]["correction"]["unknown"]=json!("not extra budget");assert!(video_kadr_backend::db::composition_document_byte_limit(&document).is_err());
}
#[tokio::test]
async fn legitimate_stroke_bodies_reach_authorization_and_oversized_bodies_remain_bounded(){
 let(state,_root)=support::make_state(true,false).await;let owner=state.db.register_auth_user("body-owner","public synthetic password").await.unwrap().unwrap();let app=support::router(state);
 let request=|action:&str,body:String|Request::builder().method("POST").uri(format!("/api/composition-projects/missing/concat-video-cutout/{action}")).header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(body)).unwrap();
 let correction=binding()["correction"].clone();
 for(action,body,limit)in [("binding",json!({"clipId":"clip","expectedRevision":0,"enabled":false,"correction":correction}),video_kadr_backend::handlers::concat_video_binding::MAX_BODY_BYTES),("preview",json!({"clipId":"clip","expectedRevision":0,"timeUs":0,"correction":correction}),video_kadr_backend::handlers::concat_video_binding::MAX_BODY_BYTES),("smart-region",json!({"clipId":"clip","expectedRevision":0,"timeUs":0,"points":points(65536)}),video_kadr_backend::handlers::concat_video_smart::MAX_START_BODY_BYTES)]{
  let encoded=body.to_string();assert!(encoded.len()>2*1024*1024&&encoded.len()<limit);assert_eq!(app.clone().oneshot(request(action,encoded)).await.unwrap().status(),StatusCode::FORBIDDEN,"{action}: valid body must reach project authority");
  let oversized=format!("{}{{}}"," ".repeat(limit));assert_eq!(app.clone().oneshot(request(action,oversized)).await.unwrap().status(),StatusCode::PAYLOAD_TOO_LARGE,"{action}");
 }
 let mut too_many=correction;too_many["strokes"][0]["points"]=json!(points(65537));assert_eq!(app.clone().oneshot(request("binding",json!({"clipId":"clip","expectedRevision":0,"enabled":false,"correction":too_many}).to_string())).await.unwrap().status(),StatusCode::BAD_REQUEST);assert_eq!(app.oneshot(request("smart-region",json!({"clipId":"clip","expectedRevision":0,"timeUs":0,"points":points(65537)}).to_string())).await.unwrap().status(),StatusCode::BAD_REQUEST);
}
