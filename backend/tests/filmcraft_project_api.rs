mod support;
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;
use video_kadr_backend::library::MediaEntry;
fn request(route:&str,body:Value)->Request<Body>{Request::builder().method("POST").uri(route).header("content-type","application/json").body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()}
#[tokio::test]
async fn source_project_import_preserves_document_and_exact_ids(){
 let (state,_dir)=support::make_state(false,false).await;
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-project-index.json")).unwrap();
 let response=support::router(state).oneshot(request("/api/compositions/filmcraft/import",json!({"document":serde_json::to_string(&fixture["document"]).unwrap()}))).await.unwrap();
 assert_eq!(response.status(),StatusCode::OK);
 let body:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 assert_eq!(body["document"],fixture["document"]);assert_eq!(serde_json::from_str::<Value>(body["documentText"].as_str().unwrap()).unwrap(),fixture["document"]);assert_eq!(body["sequences"][0]["id"],"5");
}
#[tokio::test]
async fn source_project_wav_download_uses_library_and_matches_original(){
 let (state,dir)=support::make_state(false,false).await;
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-wav-project.json")).unwrap();
 let reference:Value=serde_json::from_str(include_str!("fixtures/filmcraft-pcm-export.json")).unwrap();
 let case=&fixture["cases"][0];
 let bytes:Vec<u8>=fixture["wav"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
 tokio::fs::write(dir.path().join("sources/pattern.wav"),bytes).await.unwrap();
 assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"wave-source","filename":"pattern.wav","mediaType":"audio"}))).await);
 let app=support::router(state);
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/import",json!({"document":serde_json::to_string(&case["document"]).unwrap()}))).await.unwrap();
 assert_eq!(response.status(),StatusCode::OK);
 let manifest:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 assert_eq!(manifest["media"][0]["id"],case["source"].as_u64().unwrap().to_string());
 assert_eq!(manifest["media"][0]["path"],"pattern.wav");
 assert_eq!(manifest["media"][0]["offline"],false);
 let sequence=manifest["sequences"].as_array().unwrap().iter().find(|v|v["id"]==case["root"].as_u64().unwrap().to_string()).unwrap();
 assert_eq!(sequence["sampleRate"],44100);assert!(sequence["durationSeconds"].as_f64().unwrap()>=1.);

 let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["root"].as_u64().unwrap().to_string(),"startSample":0,"frames":65,"channels":2,"bits":24,"format":"wav","sources":{case["source"].as_u64().unwrap().to_string():"wave-source"}});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/export-audio",base.clone())).await.unwrap();
 assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()["content-type"],"audio/wav");
 let actual=to_bytes(response.into_body(),1024*1024).await.unwrap();let expected:Vec<u8>=reference["cases"][0][0][3]["wav"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();assert_eq!(actual.as_ref(),expected);
 let mut missing=base.clone();missing["sources"]=json!({});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/export-audio",missing)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
 let mut unknown=base;unknown["outputPath"]=json!("/tmp/forbidden.wav");
 let response=app.oneshot(request("/api/compositions/filmcraft/export-audio",unknown)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn source_project_linked_trim_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-linked-trim.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":case["edge"],"deltaTicks":case["delta"].as_i64().unwrap().to_string(),"linked":case["linked"]});
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim",body)).await.unwrap();
  if case["error"]==true {assert_eq!(response.status(),StatusCode::BAD_REQUEST,"case {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"case {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert!(result["sequences"].as_array().unwrap().iter().any(|sequence|sequence["clips"].as_array().unwrap().iter().any(|clip|clip["id"]=="1001" && clip["startTicks"].is_string() && clip["durationTicks"].is_string())));
  assert_eq!(result["appliedDeltaTicks"],case["applied"].as_i64().unwrap().to_string(),"case {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"case {i}");
 }
 let case=&fixture["cases"][0];let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"in","deltaFrames":"1","linked":true});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim",base.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["appliedDeltaTicks"],"10584000000");
 let mut invalid=base;invalid["deltaTicks"]=json!("1");let response=app.oneshot(request("/api/compositions/filmcraft/trim",invalid)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn source_project_ripple_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for text in [include_str!("fixtures/filmcraft-ripple-trim.json"),include_str!("fixtures/filmcraft-ripple-transition.json"),include_str!("fixtures/filmcraft-ripple-split.json")] {
  let fixture:Value=serde_json::from_str(text).unwrap();
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
   let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":case["edge"],"mode":"ripple","deltaTicks":case["delta"].as_i64().unwrap().to_string(),"linked":case["linked"]});
   let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim",body)).await.unwrap();
   if case["error"]==true {assert_eq!(response.status(),StatusCode::BAD_REQUEST,"case {i}");continue;}
   assert_eq!(response.status(),StatusCode::OK,"case {i}");
   let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
   assert_eq!(result["appliedDeltaTicks"],case["applied"].as_i64().unwrap().to_string(),"case {i}");
   assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"case {i}");
  }
 }
}

#[tokio::test]
async fn source_project_slip_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-slip.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/slip",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","deltaTicks":case["delta"].as_i64().unwrap().to_string()}))).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK,"slip {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["appliedDeltaTicks"],case["applied"].as_i64().unwrap().to_string(),"slip {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"slip {i}");
 }
}

#[tokio::test]
async fn source_project_stretch_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-rate-stretch.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/rate-stretch",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":case["edge"],"deltaTicks":case["delta"].as_i64().unwrap().to_string()}))).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK,"slip {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["appliedSpeed"],case["applied"],"slip {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"slip {i}");
 }
}

#[tokio::test]
async fn source_project_roll_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 for text in [include_str!("fixtures/filmcraft-roll.json"),include_str!("fixtures/filmcraft-roll-right.json")] {
 let fixture:Value=serde_json::from_str(text).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/roll",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"leftClipId":"1001","rightClipId":"1011","deltaTicks":case["delta"].as_i64().unwrap().to_string()}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"roll {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"roll {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["appliedDeltaTicks"],case["applied"].as_i64().unwrap().to_string(),"roll {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"roll {i}");
 }
 }
}

#[tokio::test]
async fn source_project_slide_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-slide.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/slide",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","deltaTicks":case["delta"].as_i64().unwrap().to_string()}))).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK,"slip {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["appliedDeltaTicks"],case["applied"].as_i64().unwrap().to_string(),"slip {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"slip {i}");
 }
}


#[tokio::test]
async fn source_project_delete_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-delete.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let ids:Vec<_>=case["ids"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect();
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/delete",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipIds":ids,"ripple":case["ripple"]}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"delete {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"delete {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["removed"],case["removed"],"delete {i}");
  let spans:Vec<_>=case["closedSpans"].as_array().unwrap().iter().map(|span|json!({"startTicks":span[0].as_i64().unwrap().to_string(),"durationTicks":span[1].as_i64().unwrap().to_string()})).collect();assert_eq!(result["closedSpans"],json!(spans),"delete {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"delete {i}");
 }
}

#[tokio::test]
async fn source_manifest_preserves_exact_link_ids(){
 let (state,_dir)=support::make_state(false,false).await;
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-linked-trim.json")).unwrap();let case=&fixture["cases"][0];let mut document=case["document"].clone();
 document["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][1]["link"]=json!(u64::MAX);
 let response=support::router(state).oneshot(request("/api/compositions/filmcraft/import",json!({"document":serde_json::to_string(&document).unwrap()}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 let clip=result["sequences"].as_array().unwrap().iter().flat_map(|sequence|sequence["clips"].as_array().unwrap()).find(|clip|clip["id"]=="1001").unwrap();assert_eq!(clip["linkId"],u64::MAX.to_string());
}

#[tokio::test]
async fn source_project_close_gap_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-close-gap.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/close-gap",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackId":case["track"].as_u64().unwrap().to_string(),"timeTicks":case["time"].as_i64().unwrap().to_string()}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"close gap {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"close gap {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"close gap {i}");
 }
}

#[tokio::test]
async fn source_project_close_gap_frame_coordinates_and_strict_contract(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-close-gap.json")).unwrap();
 let cases=fixture["cases"].as_array().unwrap();
 for num in [24_i64,24000] {
  let case=cases.iter().find(|case|case["error"]==false && case["time"]==254016000000_i64 && case["document"]["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["frame_rate"]["num"]==num).unwrap();
  let den=if num==24{1_i128}else{1001};
  let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackId":case["track"].as_u64().unwrap().to_string()});
  for frames in [-1_i64,0,12,24,36,48,72,120] {
   let ticks=((frames as i128)*254016000000_i128*den).div_euclid(num as i128);
   let mut frame_request=base.clone();frame_request["timeFrames"]=json!(frames.to_string());
   let mut tick_request=base.clone();tick_request["timeTicks"]=json!(ticks.to_string());
   let frame_response=app.clone().oneshot(request("/api/compositions/filmcraft/close-gap",frame_request)).await.unwrap();
   let tick_response=app.clone().oneshot(request("/api/compositions/filmcraft/close-gap",tick_request)).await.unwrap();
   assert_eq!(frame_response.status(),tick_response.status(),"rate {num} frame {frames}");
   let frame_body=to_bytes(frame_response.into_body(),32*1024*1024).await.unwrap();
   let tick_body=to_bytes(tick_response.into_body(),32*1024*1024).await.unwrap();
   assert_eq!(serde_json::from_slice::<Value>(&frame_body).unwrap(),serde_json::from_slice::<Value>(&tick_body).unwrap(),"rate {num} frame {frames}");
  }
  for patch in [json!({}),json!({"timeTicks":"0","timeFrames":"0"}),json!({"timeFrames":1}),json!({"timeFrames":"1.5"}),json!({"timeFrames":i64::MAX.to_string()}),json!({"timeTicks":"0","trackId":"-1"}),json!({"timeTicks":"0","deltaFrames":"1"})] {
   let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
   let response=app.clone().oneshot(request("/api/compositions/filmcraft/close-gap",body)).await.unwrap();
   assert_eq!(response.status(),StatusCode::BAD_REQUEST);
  }
 }
}

#[tokio::test]
async fn source_project_razor_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-razor.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let mut body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"timeTicks":case["time"].as_i64().unwrap().to_string()});
  let (field,values)=if case["mode"]==2{("clipIds",&case["ids"])}else{("trackIds",&case["tracks"])};
  body[field]=json!(values.as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>());
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/razor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"razor {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["createdClipIds"],json!(case["created"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"razor IDs {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"razor document {i}");
 }
}

#[tokio::test]
async fn source_project_razor_strict_selection_and_large_allocated_ids(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-razor.json")).unwrap();
 let case=fixture["cases"].as_array().unwrap().iter().find(|c|c["mode"]==2 && c["ids"].as_array().unwrap().len()==1 && c["created"].as_array().unwrap().len()==1).unwrap();
 let mut document=case["document"].clone();document["project"]["next_id"]=json!(u64::MAX-2);
 let base=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"timeTicks":case["time"].as_i64().unwrap().to_string()});
 let mut body=base.clone();body["clipIds"]=json!(["1001"]);
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/razor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 assert_eq!(result["createdClipIds"],json!([(u64::MAX-2).to_string()]));
 let edited:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();assert_eq!(edited["project"]["next_id"],json!(u64::MAX));
 for patch in [json!({}),json!({"trackIds":[],"clipIds":[]}),json!({"clipIds":[1001]}),json!({"trackIds":["-1"]}),json!({"clipIds":["18446744073709551616"]}),json!({"clipIds":vec!["1001";4097]}),json!({"clipIds":[],"linked":true}),json!({"clipIds":[],"timeFrames":"1"})] {
  let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/razor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
 }
 let mut body=base.clone();body["clipIds"]=json!([]);
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/razor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["createdClipIds"],json!([]));assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),document);
}

#[tokio::test]
async fn source_project_range_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-range.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let tracks:Vec<_>=case["tracks"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect();
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/remove-range",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackIds":tracks,"startTicks":case["time"].as_i64().unwrap().to_string(),"durationTicks":case["rangeDuration"].as_i64().unwrap().to_string(),"ripple":case["mode"]==1}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"range {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["removedClipIds"],json!(case["removed"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"range IDs {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"range document {i}");
 }
}

#[tokio::test]
async fn source_project_range_frame_clock_and_strict_contract(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-range.json")).unwrap();
 for num in [24_i64,24000] {
  let case=fixture["cases"].as_array().unwrap().iter().find(|c|c["document"]["project"]["items"][c["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["frame_rate"]["num"]==num).unwrap();
  let document=serde_json::to_string(&case["document"]).unwrap();
  let base=json!({"document":document,"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackIds":["3"],"ripple":false});
  let den=if num==24{1_i128}else{1001};let clock=|frames:i64|((frames as i128)*254016000000_i128*den).div_euclid(num as i128).to_string();
  for ripple in [false,true]{for (start,duration) in [(-1,0),(0,12),(12,24),(48,17),(72,48),(120,0)] {
   let mut frame=base.clone();frame["ripple"]=json!(ripple);frame["startFrames"]=json!(start.to_string());frame["durationFrames"]=json!(duration.to_string());
   let mut tick=base.clone();tick["ripple"]=json!(ripple);tick["startTicks"]=json!(clock(start));tick["durationTicks"]=json!(clock(duration));
   let frame=app.clone().oneshot(request("/api/compositions/filmcraft/remove-range",frame)).await.unwrap();let tick=app.clone().oneshot(request("/api/compositions/filmcraft/remove-range",tick)).await.unwrap();
   assert_eq!(frame.status(),StatusCode::OK);assert_eq!(tick.status(),StatusCode::OK);
   let frame:Value=serde_json::from_slice(&to_bytes(frame.into_body(),32*1024*1024).await.unwrap()).unwrap();let tick:Value=serde_json::from_slice(&to_bytes(tick.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(frame,tick,"range rate {num} start {start} duration {duration} ripple {ripple}");
  }}
  for patch in [json!({}),json!({"startFrames":"0"}),json!({"startTicks":"0","durationFrames":"1"}),json!({"startTicks":"0","durationTicks":"1","startFrames":"0","durationFrames":"1"}),json!({"startFrames":0,"durationFrames":"1"}),json!({"startFrames":"0","durationFrames":"-1"}),json!({"startFrames":"0","durationFrames":"0.5"}),json!({"startFrames":i64::MAX.to_string(),"durationFrames":"1"}),json!({"startTicks":i64::MAX.to_string(),"durationTicks":"1"}),json!({"startTicks":"0","durationTicks":"1","trackIds":[3]}),json!({"startTicks":"0","durationTicks":"1","trackIds":vec!["3";4097]}),json!({"startTicks":"0","durationTicks":"1","linked":true})] {
   let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/remove-range",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
  }
 }
}

#[tokio::test]
async fn source_project_placement_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-placement.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let placements:Vec<_>=case["placements"].as_array().unwrap().iter().map(|v|json!({"trackId":v[0].as_u64().unwrap().to_string(),"clipText":serde_json::to_string(&v[1]).unwrap()})).collect();
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/place-clips",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"placements":placements,"insert":case["mode"]==1}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"placement {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"placement {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["placedClipIds"],json!(case["placed"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"placement IDs {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"placement document {i}");
 }
}

#[tokio::test]
async fn source_project_placement_large_ids_and_strict_contract(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-placement.json")).unwrap();
 let case=fixture["cases"].as_array().unwrap().iter().find(|c|c["error"]==false && c["mode"]==0 && c["placements"].as_array().unwrap().len()==1).unwrap();
 let sequence=case["sequence"].as_u64().unwrap();let mut document=case["document"].clone();
 document["project"]["items"][sequence.to_string()]["kind"]["Sequence"]["video_tracks"][0]["id"]=json!(u64::MAX-2);
 let mut clip=case["placements"][0][1].clone();clip["id"]=json!(u64::MAX);clip["link"]=json!(u64::MAX-1);clip["opaque"]=json!({"nestedId":u64::MAX,"tick":9007199254740993_i64});
 let placement=json!({"trackId":(u64::MAX-2).to_string(),"clipText":serde_json::to_string(&clip).unwrap()});
 let base=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":sequence.to_string(),"placements":[placement.clone()],"insert":false});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/place-clips",base.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["placedClipIds"],json!([u64::MAX.to_string()]));
 let edited:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();
 let placed=edited["project"]["items"][sequence.to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"].as_array().unwrap().iter().find(|c|c["id"]==json!(u64::MAX)).unwrap();assert_eq!(placed,&clip);
 for patch in [json!({"placements":[{"trackId":3,"clipText":placement["clipText"]}]}),json!({"placements":[{"trackId":"-1","clipText":placement["clipText"]}]}),json!({"placements":[{"trackId":"3","clipText":"{"}]}),json!({"placements":[{"trackId":"3","clipText":"[]"}]}),json!({"placements":[{"trackId":"3","clip":clip}]}),json!({"placements":vec![placement.clone();4097]}),json!({"insert":"true"}),json!({"linked":true})] {
  let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/place-clips",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
 }
 for (field,value) in [("id",json!("18446744073709551615")),("item",json!(-1)),("start",json!(1.5)),("duration",json!("1")),("source_in",Value::Null),("speed",json!("1")),("name",json!(1)),("label",json!(false)),("enabled",json!(0)),("effects",json!({}))] {
  let mut malformed=clip.clone();malformed[field]=value;let mut body=base.clone();body["placements"][0]["clipText"]=json!(serde_json::to_string(&malformed).unwrap());let response=app.clone().oneshot(request("/api/compositions/filmcraft/place-clips",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST,"invalid clip {field}");
 }
 for insert in [false,true]{let mut body=base.clone();body["placements"]=json!([]);body["insert"]=json!(insert);let response=app.clone().oneshot(request("/api/compositions/filmcraft/place-clips",body)).await.unwrap();assert_eq!(response.status(),if insert{StatusCode::BAD_REQUEST}else{StatusCode::OK});}
}

#[tokio::test]
async fn source_project_clipboard_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-clipboard.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let sequence=case["sequence"].as_u64().unwrap().to_string();
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/copy",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":sequence,"clipIds":["1001"],"linked":case["linked"],"includeMarkers":case["include"]}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"copy {i}");
  let copied:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(serde_json::from_str::<Value>(copied["clipboardText"].as_str().unwrap()).unwrap(),case["clipboard"],"copy {i}");
  assert_eq!(copied["clipCount"],json!(case["clipboard"]["clips"].as_array().unwrap().len()));assert_eq!(copied["markerCount"],json!(case["clipboard"]["markers"].as_array().unwrap().len()));
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/paste",json!({"document":serde_json::to_string(&case["pasteDocument"]).unwrap(),"sequenceId":sequence,"clipboardText":copied["clipboardText"],"insert":case["insert"],"includeMarkers":case["include"],"rippleMarkers":case["ripple"],"timeTicks":case["time"].as_i64().unwrap().to_string()}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"paste {i}");continue;}assert_eq!(response.status(),StatusCode::OK,"paste {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["selectedClipIds"],json!(case["selected"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"paste IDs {i}");assert_eq!(result["playheadTicks"],case["playhead"].as_i64().unwrap().to_string(),"paste playhead {i}");let rate=&case["pasteDocument"]["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["frame_rate"];let frame=(case["playhead"].as_i64().unwrap() as i128*rate["num"].as_i64().unwrap() as i128).div_euclid(254016000000_i128*rate["den"].as_i64().unwrap() as i128);assert_eq!(result["playheadFrames"],frame.to_string(),"paste frame {i}");assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"paste document {i}");
 }
}

#[tokio::test]
async fn source_project_clipboard_frame_clock_and_malformed_http(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-clipboard.json")).unwrap();
 for num in [24_i64,24000]{
  let case=fixture["cases"].as_array().unwrap().iter().find(|c|c["error"]==false && c["include"]==true && c["document"]["project"]["items"][c["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["frame_rate"]["num"]==num).unwrap();
  let base=json!({"document":serde_json::to_string(&case["pasteDocument"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipboardText":serde_json::to_string(&case["clipboard"]).unwrap(),"insert":false,"includeMarkers":true,"rippleMarkers":true});
  for insert in [false,true]{for frames in [-1_i64,0,12,48,72,120]{
   let den=if num==24{1_i128}else{1001};let ticks=((frames as i128)*254016000000_i128*den).div_euclid(num as i128);
   let mut frame=base.clone();frame["insert"]=json!(insert);frame["timeFrames"]=json!(frames.to_string());let mut tick=base.clone();tick["insert"]=json!(insert);tick["timeTicks"]=json!(ticks.to_string());
   let frame=app.clone().oneshot(request("/api/compositions/filmcraft/paste",frame)).await.unwrap();let tick=app.clone().oneshot(request("/api/compositions/filmcraft/paste",tick)).await.unwrap();assert_eq!(frame.status(),StatusCode::OK);assert_eq!(tick.status(),StatusCode::OK);
   let frame:Value=serde_json::from_slice(&to_bytes(frame.into_body(),32*1024*1024).await.unwrap()).unwrap();let tick:Value=serde_json::from_slice(&to_bytes(tick.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(frame,tick,"paste rate {num} frames {frames} insert {insert}");
  }}
  for patch in [json!({}),json!({"timeTicks":"0","timeFrames":"0"}),json!({"timeFrames":0}),json!({"timeFrames":"0.5"}),json!({"timeFrames":i64::MAX.to_string()}),json!({"timeTicks":"0","clipboardText":"{"}),json!({"timeTicks":"0","clipboardText":"null"}),json!({"timeTicks":"0","clipboardText":case["clipboard"]}),json!({"timeTicks":"0","includeMarkers":1}),json!({"timeTicks":"0","linked":true})]{
   let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/paste",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);
  }
  let mut malformed=vec![];
  for (field,value) in [("version",json!(2)),("format",json!("unknown")),("clips",Value::Null),("markers",Value::Null)]{let mut clipboard=case["clipboard"].clone();clipboard[field]=value;malformed.push(clipboard);}
  for entry in [Value::Null,json!(["Video",0]),json!(["Video",0,{},0]),json!(["Unknown",0,{}]),json!(["Video",-1,{}])]{let mut clipboard=case["clipboard"].clone();clipboard["clips"]=json!([entry]);malformed.push(clipboard);}
  for (field,value) in [("item",json!(-1)),("source_in",Value::Null),("speed",json!("1")),("link",json!("18446744073709551615")),("reverse",json!(0)),("frame_hold",json!(0.5)),("source_channels",json!([65536]))]{let mut clipboard=case["clipboard"].clone();clipboard["clips"][0][2][field]=value;malformed.push(clipboard);}
  for (field,value) in [("id",json!("1")),("start",json!(0.5)),("comment",json!(0))]{let mut clipboard=case["clipboard"].clone();clipboard["markers"][0][field]=value;malformed.push(clipboard);}
  let mut clipboard=case["clipboard"].clone();clipboard["clips"]=json!(vec![clipboard["clips"][0].clone();4097]);malformed.push(clipboard);
  let mut clipboard=case["clipboard"].clone();clipboard["markers"]=json!(vec![clipboard["markers"][0].clone();4097]);malformed.push(clipboard);
  for clipboard in malformed{let mut body=base.clone();body["timeTicks"]=json!("0");body["includeMarkers"]=json!(false);body["clipboardText"]=json!(serde_json::to_string(&clipboard).unwrap());let response=app.clone().oneshot(request("/api/compositions/filmcraft/paste",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
  let copy=json!({"document":base["document"],"sequenceId":base["sequenceId"],"clipIds":["1001"],"linked":true,"includeMarkers":true});
  for patch in [json!({"clipIds":[1001]}),json!({"clipIds":["-1"]}),json!({"clipIds":vec!["1001";4097]}),json!({"linked":"true"}),json!({"unknown":true})]{let mut body=copy.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/copy",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
 }
}

#[tokio::test]
async fn source_project_add_transition_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-add-transition.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/add-transition",json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackId":case["track"].as_u64().unwrap().to_string(),"transitionText":serde_json::to_string(&case["transition"]).unwrap()}))).await.unwrap();
  if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"transition {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"transition {i}");
  let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  assert_eq!(result["createdTransitionId"],case["id"].as_u64().unwrap().to_string(),"transition ID {i}");
  assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"transition document {i}");
 }
}

#[tokio::test]
async fn source_project_add_transition_strict_contract(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-add-transition.json")).unwrap();let case=fixture["cases"].as_array().unwrap().iter().find(|case|case["error"]==false).unwrap();
 let mut document=case["document"].clone();document["project"]["next_id"]=json!(9007199254740993_u64);
 let base=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"trackId":case["track"].as_u64().unwrap().to_string(),"transitionText":serde_json::to_string(&case["transition"]).unwrap()});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/add-transition",base.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["createdTransitionId"],"9007199254740993");
 for patch in [json!({"trackId":3}),json!({"sequenceId":1}),json!({"transitionText":{}}),json!({"transitionText":"{"}),json!({"extra":true}),json!({"trackId":"18446744073709551616"})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/add-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
 for (field,value) in [("id",json!("0")),("start",json!(0.5)),("duration",Value::Null),("from",json!("1001")),("to",json!(-1)),("align",json!("unknown")),("reverse",json!(1)),("effect",json!({"effect":"cross_dissolve","enabled":true,"params":[]}))]{let mut transition=case["transition"].clone();transition[field]=value;let mut body=base.clone();body["transitionText"]=json!(serde_json::to_string(&transition).unwrap());let response=app.clone().oneshot(request("/api/compositions/filmcraft/add-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST,"transition field {field}");}
}

#[tokio::test]
async fn source_project_apply_transition_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-apply-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),1920);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let mut body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"kind":if case["kind"]=="video_tracks"{"video"}else{"audio"},"effect":case["effect"],"frames":case["frames"].as_i64().unwrap().to_string(),"timeTicks":case["time"].as_i64().unwrap().to_string(),"targetedTrackIds":case["targeted"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap().to_string()).collect::<Vec<_>>(),"reverse":case["reverse"]});
  if let Some(clip)=case["clip"].as_u64(){body["clipId"]=json!(clip.to_string());}if !case["edge"].is_null(){body["edge"]=case["edge"].clone();}
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();if case["id"].is_null(){assert_eq!(response.status(),StatusCode::BAD_REQUEST,"apply refusal {i}");continue;}
  assert_eq!(response.status(),StatusCode::OK,"apply {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["createdTransitionId"],case["id"].as_u64().unwrap().to_string());assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["applied"],"apply document {i}");
 }
}
#[tokio::test]
async fn source_project_apply_transition_catalog_and_strict_contract(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-apply-transition.json")).unwrap();let case=fixture["cases"].as_array().unwrap().iter().find(|v|!v["id"].is_null()&&v["kind"]=="video_tracks").unwrap();
 let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"kind":"video","effect":"cross_dissolve","frames":"7","clipId":"1001","edge":"in","timeFrames":"0"});
 let catalog:Value=serde_json::from_str(include_str!("../src/domain/filmcraft_transition_catalog.json")).unwrap();
 for entry in catalog.as_array().unwrap(){let mut body=base.clone();body["kind"]=json!(if entry["definition"]["kind"]=="VideoTransition"{"video"}else{"audio"});body["clipId"]=json!(if body["kind"]=="video"{"1001"}else{"1002"});body["effect"]=json!(format!(" {} ",entry["definition"]["name"].as_str().unwrap().to_uppercase()));let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let doc:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();let seq=&doc["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"];let transition=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().unwrap().iter()).flat_map(|track|track["transitions"].as_array().unwrap().iter()).find(|t|t["id"].as_u64()==Some(case["id"].as_u64().unwrap())).unwrap();assert_eq!(transition["effect"],entry["instance"]);}
 for patch in [json!({"frames":7}),json!({"frames":"0.5"}),json!({"frames":i64::MAX.to_string()}),json!({"timeTicks":"0"}),json!({"timeFrames":0}),json!({"kind":"unknown"}),json!({"effect":"constant_power"}),json!({"effect":"unknown"}),json!({"params":null}),json!({"params":[]}),json!({"params":{"unknown":0}}),json!({"reverse":1}),json!({"clipId":1001}),json!({"edge":"middle"}),json!({"targetedTrackIds":[3]}),json!({"targetedTrackIds":vec!["3";4097]}),json!({"unknown":0})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_transition_catalog_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let response=app.oneshot(axum::http::Request::builder().uri("/api/compositions/filmcraft/transitions").body(axum::body::Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let original:Value=serde_json::from_str(include_str!("fixtures/filmcraft-transition-params.json")).unwrap();assert_eq!(result,original["catalog"]);
}

#[tokio::test]
async fn source_project_set_transition_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-set-transition.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let command=&case["request"];let mut body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"transitionId":command["transition"].as_u64().unwrap().to_string(),"reset":command["reset"]});if let Some(params)=command.get("params"){body["params"]=params.clone();}if let Some(reverse)=command.get("reverse"){body["reverse"]=reverse.clone();}
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/set-transition",body)).await.unwrap();if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"set refusal {i}");continue;}assert_eq!(response.status(),StatusCode::OK,"set {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"set document {i}");}
}

#[tokio::test]
async fn source_project_transition_manifest_preserves_ids_and_parameter_text(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-set-transition.json")).unwrap();let case=fixture["cases"].as_array().unwrap().iter().find(|case|case["request"]["transition"].as_u64()==Some(u64::MAX)).unwrap();let mut source=case["document"].clone();let sequence=case["sequence"].as_u64().unwrap().to_string();source["project"]["items"][&sequence]["kind"]["Sequence"]["video_tracks"][0]["transitions"][0]["effect"]["params"]["opaque"]=json!({"value":{"Choice":u64::MAX},"keyframes":[]});
 let response=app.oneshot(request("/api/compositions/filmcraft/import",json!({"document":serde_json::to_string(&source).unwrap()}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),source);let transitions=result["sequences"][0]["transitions"].as_array().unwrap();assert_eq!(transitions.len(),6);for transition in transitions{assert_eq!(transition["id"],u64::MAX.to_string());assert_eq!(transition["startTicks"],"0");assert_eq!(transition["durationTicks"],"100");assert!(transition["trackId"].is_string());}let params:Value=serde_json::from_str(transitions[0]["paramsText"].as_str().unwrap()).unwrap();assert_eq!(params["opaque"]["value"]["Choice"].as_u64(),Some(u64::MAX));
}

#[tokio::test]
async fn source_project_batch_transitions_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-batch-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),864);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipIds":case["clips"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap().to_string()).collect::<Vec<_>>(),"videoEffect":"cross_dissolve","audioEffect":"constant_power","videoFrames":case["frames"].as_i64().unwrap().to_string(),"audioFrames":case["frames"].as_i64().unwrap().to_string()});let response=app.clone().oneshot(request("/api/compositions/filmcraft/batch-transitions",body)).await.unwrap();if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"batch refusal {i}");continue;}assert_eq!(response.status(),StatusCode::OK,"batch {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["createdTransitionIds"],json!(case["result"]["transitions"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap().to_string()).collect::<Vec<_>>()));assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"batch document {i}");}
}

#[tokio::test]
async fn source_project_transition_duration_preferences_match_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-apply-transition.json")).unwrap();let source=&source["cases"][0];let sequence=source["sequence"].as_u64().unwrap().to_string();let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-transition-duration.json")).unwrap();
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate().filter(|(_,case)|case["unit"]!="other"){let mut document=source["document"].clone();document["project"]["items"][&sequence]["kind"]["Sequence"]["settings"]["frame_rate"]=json!({"num":case["num"],"den":case["den"]});let body=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":sequence,"kind":"video","effect":"cross_dissolve","clipId":"1001","edge":"in","timeFrames":"0","duration":{"value":case["value"],"unit":case["unit"]}});let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"duration {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let document:Value=serde_json::from_str(result["documentText"].as_str().unwrap()).unwrap();let id=result["createdTransitionId"].as_str().unwrap().parse::<u64>().unwrap();let transition=document["project"]["items"][&sequence]["kind"]["Sequence"]["video_tracks"][0]["transitions"].as_array().unwrap().iter().find(|t|t["id"].as_u64()==Some(id)).unwrap();assert_eq!(transition["duration"],case["expected"],"preference duration {i}");}
 let base=json!({"document":serde_json::to_string(&source["document"]).unwrap(),"sequenceId":sequence,"kind":"video","effect":"cross_dissolve","clipId":"1001","edge":"in","timeFrames":"0"});
 for patch in [json!({"duration":{"value":1,"unit":"unknown"}}),json!({"duration":{"value":"1","unit":"seconds"}}),json!({"duration":{"value":1,"unit":"seconds","extra":0}}),json!({"frames":"1","duration":{"value":1,"unit":"seconds"}}),json!({"duration":{"value":1e100,"unit":"frames"}})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
 for kind in ["video","audio"]{let mut automatic=base.clone();automatic["kind"]=json!(kind);automatic["effect"]=json!(if kind=="video"{"cross_dissolve"}else{"constant_power"});automatic["clipId"]=json!(if kind=="video"{"1001"}else{"1002"});let mut explicit=automatic.clone();explicit["frames"]=json!(if kind=="video"{"30"}else{"24"});let mut responses=vec![];for body in [automatic,explicit]{let response=app.clone().oneshot(request("/api/compositions/filmcraft/apply-transition",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);responses.push(serde_json::from_slice::<Value>(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap());}assert_eq!(responses[0],responses[1],"default {kind}");}
}

#[tokio::test]
async fn source_project_edit_point_transitions_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),864);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipIds":[],"editPoints":case["points"].as_array().unwrap().iter().map(|p|json!({"clipId":p["clip"].as_u64().unwrap().to_string(),"edge":if p["out"].as_bool().unwrap(){"out"}else{"in"}})).collect::<Vec<_>>(),"videoEffect":"cross_dissolve","audioEffect":"constant_power","videoFrames":case["frames"].as_i64().unwrap().to_string(),"audioFrames":case["frames"].as_i64().unwrap().to_string()});let response=app.clone().oneshot(request("/api/compositions/filmcraft/batch-transitions",body)).await.unwrap();if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"batch refusal {i}");continue;}assert_eq!(response.status(),StatusCode::OK,"batch {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["createdTransitionIds"],json!(case["result"]["transitions"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap().to_string()).collect::<Vec<_>>()));assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"],"batch document {i}");}
}

#[tokio::test]
async fn source_project_edit_points_validate_transport_and_preserve_large_ids(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();
 let mut case=fixture["cases"].as_array().unwrap().iter().find(|c|c["error"]==false&&c["points"][0]["clip"]==1001).unwrap().clone();
 fn remap(v:&mut Value){match v{Value::Array(a)=>for x in a{remap(x)},Value::Object(o)=>for x in o.values_mut(){remap(x)},_=>if v.as_u64()==Some(1001){*v=json!(9007199254740993u64)}}}
 remap(&mut case);
 let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipIds":[],"editPoints":case["points"].as_array().unwrap().iter().map(|p|json!({"clipId":p["clip"].as_u64().unwrap().to_string(),"edge":if p["out"].as_bool().unwrap(){"out"}else{"in"}})).collect::<Vec<_>>(),"videoEffect":"cross_dissolve","audioEffect":"constant_power","videoFrames":case["frames"].as_i64().unwrap().to_string(),"audioFrames":case["frames"].as_i64().unwrap().to_string()});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/batch-transitions",base.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
 let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),case["expected"]);
 for points in [Value::Null,json!([{"clipId":9007199254740993u64,"edge":"in"}]),json!([{"clipId":"18446744073709551616","edge":"in"}]),json!([{"clipId":"1001","edge":"middle"}]),json!([{"clipId":"1001","edge":"in","unknown":0}]),json!(vec![json!({"clipId":"1001","edge":"in"});4097])]{let mut body=base.clone();body["editPoints"]=points;let response=app.clone().oneshot(request("/api/compositions/filmcraft/batch-transitions",body)).await.unwrap();assert_eq!(response.status(),StatusCode::BAD_REQUEST);}
 // Nonempty clip selection takes precedence over the trim-monitor fallback.
 let mut body=base.clone();body["clipIds"]=json!(["999999"]);assert_eq!(app.oneshot(request("/api/compositions/filmcraft/batch-transitions",body)).await.unwrap().status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn source_project_trim_monitor_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-side.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),864);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){for (j,point) in case["points"].as_array().unwrap().iter().enumerate(){
  let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":point["clip"].as_u64().unwrap().to_string(),"edge":if point["out"].as_bool().unwrap(){"out"}else{"in"}});
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"monitor {i}/{j}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  let ctx=&case["contexts"][j];let mut expected=json!({"editTime":ctx["editTime"].as_i64().map(|v|v.to_string()),"rollPair":ctx["rollPair"].as_array().map(|ids|ids.iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"outgoing":ctx["outgoingFrame"],"incoming":ctx["incomingFrame"]});
  for side in ["outgoing","incoming"]{if !expected[side].is_null(){for key in ["clip","item","time","mediaTime","mediaFrame","sourceIn","sourceOut"]{expected[side][key]=Value::String(expected[side][key].to_string());}}}
  assert_eq!(result,expected,"monitor response {i}/{j}");
 }}
 let case=&fixture["cases"][0];let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out"});
 for patch in [json!({"edge":"middle"}),json!({"clipId":1001}),json!({"clipId":"18446744073709551616"}),json!({"sequenceId":"999999"}),json!({"unknown":true})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_trim_monitor_custom_rates_match_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-rate.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),432);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){for (j,point) in case["points"].as_array().unwrap().iter().enumerate(){
  let body=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":point["clip"].as_u64().unwrap().to_string(),"edge":if point["out"].as_bool().unwrap(){"out"}else{"in"}});
  let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"monitor {i}/{j}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
  let ctx=&case["contexts"][j];let mut expected=json!({"editTime":ctx["editTime"].as_i64().map(|v|v.to_string()),"rollPair":ctx["rollPair"].as_array().map(|ids|ids.iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"outgoing":ctx["outgoingFrame"],"incoming":ctx["incomingFrame"]});
  for side in ["outgoing","incoming"]{if !expected[side].is_null(){for key in ["clip","item","time","mediaTime","mediaFrame","sourceIn","sourceOut"]{expected[side][key]=Value::String(expected[side][key].to_string());}}}
  assert_eq!(result,expected,"monitor response {i}/{j}");
 }}
 let case=&fixture["cases"][0];let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out"});
 for patch in [json!({"edge":"middle"}),json!({"clipId":1001}),json!({"clipId":"18446744073709551616"}),json!({"sequenceId":"999999"}),json!({"unknown":true})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_nearest_edit_points_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-nearest-edit-point.json")).unwrap();let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),2880);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"timeTicks":case["time"].as_i64().unwrap().to_string(),"kind":if case["kind"]=="unknown"{"roll"}else{case["kind"].as_str().unwrap()},"targetedTrackIds":case["targeted"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/nearest-edit-points",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"nearest {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let mut expected=case["expected"].clone();for point in expected["editPoints"].as_array_mut().unwrap(){point["clip"]=json!(point["clip"].as_u64().unwrap().to_string());}assert_eq!(result,expected,"nearest response {i}");}
 let case=&source["cases"][0];let base=json!({"document":serde_json::to_string(&case["document"]).unwrap(),"sequenceId":case["sequence"].as_u64().unwrap().to_string(),"timeTicks":"0","kind":"trimIn"});
 for patch in [json!({"kind":"unknown"}),json!({"timeFrames":"0"}),json!({"timeTicks":0}),json!({"targetedTrackIds":[3]}),json!({"targetedTrackIds":vec!["3";4097]}),json!({"sequenceId":"999999"}),json!({"unknown":true})]{let mut body=base.clone();body.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/nearest-edit-points",body)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
 let mut frames=base.clone();frames.as_object_mut().unwrap().remove("timeTicks");frames["timeFrames"]=json!("0");let a=app.clone().oneshot(request("/api/compositions/filmcraft/nearest-edit-points",base)).await.unwrap();let b=app.oneshot(request("/api/compositions/filmcraft/nearest-edit-points",frames)).await.unwrap();assert_eq!(b.status(),StatusCode::OK);assert_eq!(to_bytes(a.into_body(),1024*1024).await.unwrap(),to_bytes(b.into_body(),1024*1024).await.unwrap());
}

#[tokio::test]
async fn source_project_trim_points_preserves_original_partial_edits(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-points.json")).unwrap();let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),432);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut points=case["points"].clone();for point in points.as_array_mut().unwrap(){point["clip"]=json!(point["clip"].as_u64().unwrap().to_string());}
 let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":points,"linked":case["linked"],"deltaTicks":case["delta"].as_i64().unwrap().to_string()});let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"trim points {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let mut expected=base["document"].clone();expected["project"]=case["expectedProject"].clone();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),expected,"trim points project {i}");
 if case["error"]==true{assert!(result["refusal"].as_str().is_some_and(|s|!s.is_empty()));assert!(result["appliedTicks"].is_null());}else{assert!(result["refusal"].is_null());assert_eq!(result["appliedTicks"],json!(case["applied"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap().to_string()).collect::<Vec<_>>()));}
 }
 let base=&source["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[{"clip":"1001","out":true,"kind":"trim"}],"linked":false,"deltaTicks":"0"});
 for patch in [json!({"points":[]}),json!({"points":[{"clip":1001,"out":true,"kind":"trim"}]}),json!({"points":[{"clip":"1001","out":true,"kind":"invalid"}]}),json!({"points":vec![json!({"clip":"1001","out":true,"kind":"trim"});4097]}),json!({"deltaFrames":"0"}),json!({"sequenceId":"999999"}),json!({"unknown":true})]{let mut value=body.clone();value.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",value)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_trim_shift_http_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let shifts:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-shift.json")).unwrap();let batches:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-points.json")).unwrap();let sources:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(shifts["cases"].as_array().unwrap().len(),1296);
 for (i,case) in shifts["cases"].as_array().unwrap().iter().enumerate(){let batch=&batches["cases"][case["sourceCase"].as_u64().unwrap() as usize];let base=&sources["cases"][batch["sourceCase"].as_u64().unwrap() as usize];let mut points=batch["points"].clone();for p in points.as_array_mut().unwrap(){p["clip"]=json!(p["clip"].as_u64().unwrap().to_string());}
 let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":points,"linked":batch["linked"],"deltaTicks":batch["delta"].as_i64().unwrap().to_string(),"shift":{"outShift":case["initial"]["outShift"].as_i64().unwrap().to_string(),"inShift":case["initial"]["inShift"].as_i64().unwrap().to_string()}});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"shift {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["shift"],json!({"outShift":case["expected"]["outShift"].as_i64().unwrap().to_string(),"inShift":case["expected"]["inShift"].as_i64().unwrap().to_string()}),"shift {i}");assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap()["project"],batch["expectedProject"]);assert_eq!(!result["refusal"].is_null(),batch["error"].as_bool().unwrap());
 }
 let base=&sources["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[{"clip":"1001","out":true,"kind":"trim"}],"linked":false,"deltaTicks":"0"});
 for shift in [Value::Null,json!({"outShift":0,"inShift":"0"}),json!({"outShift":"9223372036854775808","inShift":"0"}),json!({"outShift":"0","inShift":"0","extra":true})]{let mut value=body.clone();value["shift"]=shift;assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",value)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_trim_shift_frames_match_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let shifts:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-shift-frames.json")).unwrap();let batches:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-points.json")).unwrap();let sources:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(shifts["cases"].as_array().unwrap().len(),1296);
 for (i,case) in shifts["cases"].as_array().unwrap().iter().enumerate(){let batch=&batches["cases"][case["sourceCase"].as_u64().unwrap() as usize];let base=&sources["cases"][batch["sourceCase"].as_u64().unwrap() as usize];let mut points=batch["points"].clone();for p in points.as_array_mut().unwrap(){p["clip"]=json!(p["clip"].as_u64().unwrap().to_string());}
 let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":points,"linked":batch["linked"],"deltaTicks":batch["delta"].as_i64().unwrap().to_string(),"shift":{"outShift":case["initial"]["outShift"].as_i64().unwrap().to_string(),"inShift":case["initial"]["inShift"].as_i64().unwrap().to_string()}});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"shift {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["shift"],json!({"outShift":case["expected"]["outShift"].as_i64().unwrap().to_string(),"inShift":case["expected"]["inShift"].as_i64().unwrap().to_string()}),"shift {i}");assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap()["project"],batch["expectedProject"]);assert_eq!(!result["refusal"].is_null(),batch["error"].as_bool().unwrap());assert_eq!(result["shiftFrames"],json!({"outShift":case["expectedFrames"]["outShift"].as_i64().unwrap().to_string(),"inShift":case["expectedFrames"]["inShift"].as_i64().unwrap().to_string()}),"shift frames {i}");
 }
 let base=&sources["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[{"clip":"1001","out":true,"kind":"trim"}],"linked":false,"deltaTicks":"0"});
 for shift in [Value::Null,json!({"outShift":0,"inShift":"0"}),json!({"outShift":"9223372036854775808","inShift":"0"}),json!({"outShift":"0","inShift":"0","extra":true})]{let mut value=body.clone();value["shift"]=shift;assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",value)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_extend_points_to_playhead_matches_batch_corpus(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-trim-points.json")).unwrap();let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),432);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut points=case["points"].clone();for point in points.as_array_mut().unwrap(){point["clip"]=json!(point["clip"].as_u64().unwrap().to_string());}
 let document=video_kadr_backend::domain::filmcraft_project_file::SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let primary=&case["points"][0];let context=video_kadr_backend::domain::filmcraft_trim::edit_point_context(&document,base["sequence"].as_u64().unwrap(),primary["clip"].as_u64().unwrap(),if primary["out"]==true{video_kadr_backend::domain::filmcraft_trim::Edge::Out}else{video_kadr_backend::domain::filmcraft_trim::Edge::In}).unwrap();let Some(first)=context["editTime"].as_i64()else{continue};let playhead=first.checked_add(case["delta"].as_i64().unwrap()).unwrap();
 let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":points,"linked":case["linked"],"playheadTicks":playhead.to_string()});let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"trim points {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let mut expected=base["document"].clone();expected["project"]=case["expectedProject"].clone();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),expected,"trim points project {i}");
 if case["error"]==true{assert!(result["refusal"].as_str().is_some_and(|s|!s.is_empty()));assert!(result["appliedTicks"].is_null());}else{assert!(result["refusal"].is_null());assert_eq!(result["appliedTicks"],json!(case["applied"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap().to_string()).collect::<Vec<_>>()));}
 }
 let base=&source["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[{"clip":"1001","out":true,"kind":"trim"}],"linked":false,"playheadTicks":"0"});
 for patch in [json!({"points":[]}),json!({"points":[{"clip":1001,"out":true,"kind":"trim"}]}),json!({"points":[{"clip":"1001","out":true,"kind":"invalid"}]}),json!({"points":vec![json!({"clip":"1001","out":true,"kind":"trim"});4097]}),json!({"deltaFrames":"0"}),json!({"playheadFrames":"0"}),json!({"sequenceId":"999999"}),json!({"unknown":true})]{let mut value=body.clone();value.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/trim-points",value)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}


#[tokio::test]
async fn source_project_to_playhead_matches_original(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-to-playhead.json")).unwrap();let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),1536);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let ids=|key:&str|case[key].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>();
 let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipIds":ids("selected"),"targetedTrackIds":ids("targeted"),"side":if case["next"]==true{"next"}else{"previous"},"ripple":case["ripple"],"linked":case["linked"],"timeTicks":case["playhead"].as_i64().unwrap().to_string()});
 let response=app.clone().oneshot(request("/api/compositions/filmcraft/to-playhead",body)).await.unwrap();if case["error"]==true{assert_eq!(response.status(),StatusCode::BAD_REQUEST,"Q/W refusal {i}");continue;}assert_eq!(response.status(),StatusCode::OK,"Q/W {i}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let mut expected=base["document"].clone();expected["project"]=case["expectedProject"].clone();assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap(),expected,"Q/W project {i}");assert_eq!(result["rangeTicks"],json!(case["result"]["range"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap().to_string()).collect::<Vec<_>>()));assert_eq!(result["playheadTicks"],case["expectedPlayhead"].as_i64().unwrap().to_string());
 }
 let base=&source["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipIds":["1001"],"targetedTrackIds":[],"side":"previous","ripple":true,"linked":true,"timeTicks":"762048000000"});
 for patch in [json!({"side":"invalid"}),json!({"clipIds":[1001]}),json!({"targetedTrackIds":["-1"]}),json!({"timeFrames":"72"}),json!({"timeTicks":"9223372036854775808"}),json!({"sequenceId":"999999"}),json!({"extra":true}),json!({"clipIds":vec!["1001";4097]})]{let mut value=body.clone();value.as_object_mut().unwrap().extend(patch.as_object().unwrap().clone());assert_eq!(app.clone().oneshot(request("/api/compositions/filmcraft/to-playhead",value)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_project_to_playhead_frame_clock_matches_exact_ticks(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&source["cases"][0];
 for (num,den) in [(24i64,1i64),(24000,1001)]{let mut document=base["document"].clone();document["project"]["items"][base["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["settings"]["frame_rate"]=json!({"num":num,"den":den});
 for next in [false,true]{for ripple in [false,true]{let mut body=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipIds":["1001"],"targetedTrackIds":[],"side":if next{"next"}else{"previous"},"ripple":ripple,"linked":true,"timeFrames":"72"});let response=app.clone().oneshot(request("/api/compositions/filmcraft/to-playhead",body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let frame:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();body.as_object_mut().unwrap().remove("timeFrames");body["timeTicks"]=json!((72i128*254016000000i128*den as i128/num as i128).to_string());let response=app.clone().oneshot(request("/api/compositions/filmcraft/to-playhead",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let ticks:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(frame,ticks,"Q/W clock {num}/{den} next={next} ripple={ripple}");let position=frame["playheadTicks"].as_str().unwrap().parse::<i64>().unwrap();assert_eq!(frame["playheadFrames"],((position as i128*num as i128).div_euclid(254016000000i128*den as i128)).to_string());assert_eq!(frame["trackIds"],json!(["3","4","5"]));
 }}
 }
}

fn trim_session_request(route:&str,token:&str,body:Value)->Request<Body>{let mut req=request(route,body);req.headers_mut().insert("authorization",format!("Bearer {token}").parse().unwrap());req}
#[tokio::test]
async fn source_trim_sessions_auth_owner_revision_and_recovery(){
 let (state,_dir)=support::make_state(false,false).await;let alice=state.db.register_auth_user("trim-alice","shared test password").await.unwrap().unwrap();let bob=state.db.register_auth_user("trim-bob","shared test password").await.unwrap().unwrap();let app=support::router(state);let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&source["cases"][0];let create=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[{"clip":"1001","out":true,"kind":"trim"}],"linked":true,"shift":{"outShift":"9007199254740993","inShift":"-9007199254740993"},"playheadTicks":"9007199254740993"});let route="/api/compositions/filmcraft/trim-sessions/";
 assert_eq!(app.clone().oneshot(request(&format!("{route}create"),create.clone())).await.unwrap().status(),StatusCode::UNAUTHORIZED);assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}create"),"invalid",create.clone())).await.unwrap().status(),StatusCode::UNAUTHORIZED);
 let response=app.clone().oneshot(trim_session_request(&format!("{route}create"),&alice.token,create)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let created:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let id=created["sessionId"].as_str().unwrap();assert_eq!(created["revision"],"0");assert_eq!(created["shift"]["outShift"],"9007199254740993");assert_eq!(created["status"]["playheadTicks"],"9007199254740993");
 for (op,body) in [("read",json!({"sessionId":id})),("command",json!({"sessionId":id,"revision":"0","action":{"op":"cancel"}})),("close",json!({"sessionId":id,"revision":"0"}))]{assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}{op}"),&bob.token,body)).await.unwrap().status(),StatusCode::NOT_FOUND);}
 let command=json!({"sessionId":id,"revision":"0","action":{"op":"shuttle","clock":0.0,"direction":"forward","slow":false}});assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}command"),&alice.token,command.clone())).await.unwrap().status(),StatusCode::OK);assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}command"),&alice.token,command)).await.unwrap().status(),StatusCode::CONFLICT);
 for action in [json!({"op":"shuttle","clock":0.0,"direction":"invalid","slow":false}),json!({"op":"tick","clock":"NaN"}),json!({"op":"cancel","extra":true})]{assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}command"),&alice.token,json!({"sessionId":id,"revision":"1","action":action}))).await.unwrap().status(),StatusCode::BAD_REQUEST);}
 let response=app.clone().oneshot(trim_session_request(&format!("{route}read"),&alice.token,json!({"sessionId":id}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let current:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(current["revision"],"1");assert_eq!(current["status"]["dynamic"]["speed"],1.0);assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}close"),&alice.token,json!({"sessionId":id,"revision":"0"}))).await.unwrap().status(),StatusCode::CONFLICT);assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}close"),&alice.token,json!({"sessionId":id,"revision":"1"}))).await.unwrap().status(),StatusCode::OK);assert_eq!(app.oneshot(trim_session_request(&format!("{route}read"),&alice.token,json!({"sessionId":id}))).await.unwrap().status(),StatusCode::NOT_FOUND);
}
#[tokio::test]
async fn source_trim_sessions_match_original_dynamic_and_playback(){
 let (state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("trim-oracle","shared test password").await.unwrap().unwrap();let app=support::router(state);let source:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let route="/api/compositions/filmcraft/trim-sessions/";
 for (data,count) in [(include_str!("fixtures/filmcraft-dynamic-trim.json"),192),(include_str!("fixtures/filmcraft-dynamic-trim-extended.json"),1536),(include_str!("fixtures/filmcraft-play-around.json"),192)]{let fixture:Value=serde_json::from_str(data).unwrap();assert_eq!(fixture["cases"].as_array().unwrap().len(),count);
 for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut points=case["points"].clone();for p in points.as_array_mut().unwrap(){p["clip"]=json!(p["clip"].as_u64().unwrap().to_string());}let initial=&case["initialShift"];let create=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":points,"linked":case["linked"],"shift":{"outShift":initial["out_shift"].as_i64().unwrap_or(0).to_string(),"inShift":initial["in_shift"].as_i64().unwrap_or(0).to_string()},"playheadTicks":case["initialPlayhead"].as_i64().unwrap_or(0).to_string()});let response=app.clone().oneshot(trim_session_request(&format!("{route}create"),&owner.token,create)).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"session create {count}/{i}");let created:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let id=created["sessionId"].as_str().unwrap();
 for (j,step) in case["steps"].as_array().unwrap().iter().enumerate(){assert_eq!(step["error"],false);let mut action=step["action"].clone();if action["op"]=="shuttle"{let reverse=match &action["direction"]{Value::String(d)=>d.starts_with('r')||d.starts_with('b')||d.eq_ignore_ascii_case("j"),v=>v.as_f64().is_some_and(|v|v<0.0)};action["direction"]=json!(if reverse{"reverse"}else{"forward"});action["slow"]=json!(action["slow"].as_bool().unwrap_or(false));}if action["op"]=="around"{action["loop"]=json!(action["loop"].as_bool().unwrap_or(true));action["toggle"]=json!(action["toggle"].as_bool().unwrap_or(false));action["centerPlayhead"]=case["centerPlayhead"].clone();action["prerollSeconds"]=case["pre"].clone();action["postrollSeconds"]=case["post"].clone();}
 let response=app.clone().oneshot(trim_session_request(&format!("{route}command"),&owner.token,json!({"sessionId":id,"revision":j.to_string(),"action":action}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK,"session command {count}/{i}/{j}");let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["revision"],(j+1).to_string());assert_eq!(serde_json::from_str::<Value>(result["documentText"].as_str().unwrap()).unwrap()["project"],step["project"],"session project {count}/{i}/{j}");assert_eq!(result["shift"],json!({"outShift":step["shift"]["out_shift"].as_i64().unwrap().to_string(),"inShift":step["shift"]["in_shift"].as_i64().unwrap().to_string()}));assert_eq!(result["status"]["playing"],step["status"]["playing"]);assert_eq!(result["status"]["playheadTicks"],step["status"]["playhead"].as_i64().unwrap().to_string());let mut dynamic=step["status"]["dynamic"].clone();if !dynamic.is_null(){dynamic["offsetFrames"]=json!(dynamic["offsetFrames"].as_i64().unwrap().to_string());}assert_eq!(result["status"]["dynamic"],dynamic);
 if step["result"].get("committed").is_some(){assert_eq!(result["commit"]["committed"],step["result"]["committed"]);assert_eq!(result["commit"]["offsetTicks"],step["result"]["offset"].as_i64().unwrap_or(0).to_string());}else{assert!(result["commit"].is_null());}
 }
 assert_eq!(app.clone().oneshot(trim_session_request(&format!("{route}close"),&owner.token,json!({"sessionId":id,"revision":case["steps"].as_array().unwrap().len().to_string()}))).await.unwrap().status(),StatusCode::OK);
 }
 }
}

#[tokio::test]
async fn source_trim_sessions_accept_exact_frame_playhead(){
 let (state,_dir)=support::make_state(false,false).await;let owner=state.db.register_auth_user("trim-clock","shared test password").await.unwrap().unwrap();let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];let body=json!({"document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[],"linked":false,"shift":{"outShift":"0","inShift":"0"},"playheadFrames":"72"});let route="/api/compositions/filmcraft/trim-sessions/create";let response=app.clone().oneshot(trim_session_request(route,&owner.token,body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(result["status"]["playheadTicks"],"762048000000");assert_eq!(result["status"]["playheadFrames"],"72");let mut ambiguous=body.clone();ambiguous["playheadTicks"]=json!("0");assert_eq!(app.clone().oneshot(trim_session_request(route,&owner.token,ambiguous)).await.unwrap().status(),StatusCode::BAD_REQUEST);let mut missing=body;missing.as_object_mut().unwrap().remove("playheadFrames");assert_eq!(app.oneshot(trim_session_request(route,&owner.token,missing)).await.unwrap().status(),StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn source_trim_sessions_creation_recovery_and_parameter_conflict(){
 let (state,_dir)=support::make_state(false,false).await;let alice=state.db.register_auth_user("creation-alice","shared test password").await.unwrap().unwrap();let bob=state.db.register_auth_user("creation-bob","shared test password").await.unwrap().unwrap();let app=support::router(state);
 let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];
 let body=json!({"creationKey":"lost-response-key","document":serde_json::to_string(&base["document"]).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"points":[],"linked":false,"shift":{"outShift":"0","inShift":"0"},"playheadFrames":"72"});let route="/api/compositions/filmcraft/trim-sessions/create";
 let response=app.clone().oneshot(trim_session_request(route,&alice.token,body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let first:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 let response=app.clone().oneshot(trim_session_request(route,&alice.token,body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let replay:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(first,replay);
 let id=first["sessionId"].as_str().unwrap();let command=app.clone().oneshot(trim_session_request("/api/compositions/filmcraft/trim-sessions/command",&alice.token,json!({"sessionId":id,"revision":"0","action":{"op":"stop"}}))).await.unwrap();assert_eq!(command.status(),StatusCode::OK);
 let response=app.clone().oneshot(trim_session_request(route,&alice.token,body.clone())).await.unwrap();let current:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_eq!(current["revision"],"1");assert_eq!(current["sessionId"],id);
 let mut changed=body.clone();changed["playheadFrames"]=json!("73");assert_eq!(app.clone().oneshot(trim_session_request(route,&alice.token,changed)).await.unwrap().status(),StatusCode::CONFLICT);
 let response=app.clone().oneshot(trim_session_request(route,&bob.token,body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let other:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert_ne!(other["sessionId"],id);
 for key in ["".to_string(),"x".repeat(129),"bad/key".to_string()]{let mut invalid=body.clone();invalid["creationKey"]=json!(key);assert_eq!(app.clone().oneshot(trim_session_request(route,&alice.token,invalid)).await.unwrap().status(),StatusCode::BAD_REQUEST);}
}

#[tokio::test]
async fn source_trim_monitor_renders_generator_media_and_subclips(){
 let (state,_dir)=support::make_state(false,false).await;let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];let mut document=base["document"].clone();
 let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"Generator":{"ColorMatte":{"color":[0.125,0.5,0.875,0.25]}}});media["info"]["video"]=json!({"width":64,"height":36,"frame_rate":{"num":24,"den":1}});
 for item in [1,2]{document["project"]["items"][base["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][1]["item"]=json!(item);let body=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out","includeFrames":true});let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();
 assert_eq!(result["outgoing"]["video"],true);{let frame=&result["outgoing"]["frame"];assert_eq!(frame["width"],8);assert_eq!(frame["height"],5);assert_eq!(frame["rgbaHex"],"1f7fdf3f".repeat(40));assert!(frame["ptsTicks"].as_str().unwrap().parse::<i64>().is_ok());assert!(result["outgoing"].get("frameUnavailable").is_none());}
 }
 document["project"]["items"]["1"]["kind"]["Media"]["offline"]=json!(true);let response=app.oneshot(request("/api/compositions/filmcraft/trim-monitor",json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out","includeFrames":true}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert!(result["outgoing"].get("frame").is_none());assert_eq!(result["outgoing"]["frameUnavailable"],"source media is offline");
}

#[tokio::test]
async fn source_trim_monitor_library_image_ignores_project_paths(){
 let (state,dir)=support::make_state(false,false).await;let path=dir.path().join("sources/monitor.png");image::RgbaImage::from_pixel(16,8,image::Rgba([43,86,129,255])).save(&path).unwrap();assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"monitor-image","filename":"monitor.png","mediaType":"image"}))).await);
 let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];let mut document=base["document"].clone();let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"File":{"path":"/must/not/be/read.png"}});media["info"]["kind"]=json!("Still");media["info"]["video"]=json!({"width":16,"height":8,"frame_rate":{"num":24,"den":1}});
 let body=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out","includeFrames":true,"sources":{"1":"monitor-image"}});let route="/api/compositions/filmcraft/trim-monitor";
 let response=app.clone().oneshot(request(route,body.clone())).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();let frame=&result["outgoing"]["frame"];assert_eq!(frame["kind"],"image");assert_eq!(frame["ptsTicks"],"0");assert_eq!(frame["sampleTicks"],result["outgoing"]["mediaTime"]);assert_eq!(frame["width"],16);assert_eq!(frame["height"],8);assert_eq!(frame["rgbaHex"],"2b5681ff".repeat(128));
 let mut missing=body.clone();missing["sources"]=json!({});let response=app.clone().oneshot(request(route,missing)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert!(result["outgoing"].get("frame").is_none());assert_eq!(result["outgoing"]["frameUnavailable"],"source image is not bound to the library");
 let mut invalid=body.clone();invalid["sources"]=json!({"2":"monitor-image"});assert_eq!(app.clone().oneshot(request(route,invalid)).await.unwrap().status(),StatusCode::BAD_REQUEST);
 let mut unknown=body;unknown["sources"]=json!({"1":"missing-image"});assert_eq!(app.oneshot(request(route,unknown)).await.unwrap().status(),StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn source_trim_monitor_file_video_uses_presentation_timestamps(){
 let (state,dir)=support::make_state(false,false).await;let raw=dir.path().join("input.rgba");let mut bytes=vec![];for color in [[255,0,0,255],[0,255,0,255],[0,0,255,255]]{bytes.extend(color.repeat(32));}std::fs::write(&raw,bytes).unwrap();let movie=dir.path().join("sources/vfr.mkv");
 let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","rawvideo","-pix_fmt","rgba","-s","8x4","-r","25","-i"]).arg(&raw).args(["-vf","setpts='if(eq(N,0),0,if(eq(N,1),2,5))'","-fps_mode","vfr","-c:v","ffv1","-pix_fmt","bgra","-threads","1"]).arg(&movie).output().unwrap();assert!(output.status.success());
 assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"vfr-source","filename":"vfr.mkv","mediaType":"video"}))).await);let app=support::router(state);let fixture:Value=serde_json::from_str(include_str!("fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];let mut document=base["document"].clone();
 let media=&mut document["project"]["items"]["1"]["kind"]["Media"];media["media"]=json!({"File":{"path":"/not/read/vfr.mkv"}});media["info"]["kind"]=json!("Movie");media["info"]["video"]=json!({"width":8,"height":4,"frame_rate":{"num":25,"den":1}});
 for (ticks,hex,ordinal) in [(0,"ff0000ff",0),(20_321_279_999,"ff0000ff",0),(20_321_280_000,"00ff00ff",1),(50_803_199_999,"00ff00ff",1),(50_803_200_000,"0000ffff",2),(60_963_840_000,"",3)]{
  let clip=&mut document["project"]["items"][base["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][1];clip["start"]=json!(0);clip["source_in"]=json!(0);clip["duration"]=json!(ticks+10_584_000_000i64);clip["speed"]=json!(1.0);clip["reverse"]=json!(false);clip["frame_hold"]=Value::Null;
  let body=json!({"document":serde_json::to_string(&document).unwrap(),"sequenceId":base["sequence"].as_u64().unwrap().to_string(),"clipId":"1001","edge":"out","includeFrames":true,"sources":{"1":"vfr-source"}});let response=app.clone().oneshot(request("/api/compositions/filmcraft/trim-monitor",body)).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),32*1024*1024).await.unwrap()).unwrap();assert!(result["outgoing"].get("_videoSource").is_none());
  if hex.is_empty(){assert!(result["outgoing"].get("frame").is_none());assert_eq!(result["outgoing"]["frameUnavailable"],"no video frame at this source time");continue;}
  let frame=&result["outgoing"]["frame"];assert_eq!(frame["kind"],"video");assert_eq!(frame["sampleTicks"],ticks.to_string());assert_eq!(frame["ordinal"],ordinal.to_string());assert_eq!(frame["timeBase"],json!({"num":"1","den":"1000"}));assert_eq!(frame["rgbaHex"],hex.repeat(32));assert!(result["outgoing"].get("frameUnavailable").is_none());
 }
}
