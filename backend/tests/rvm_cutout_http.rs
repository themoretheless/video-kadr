mod support;
use serde_json::json;
use video_kadr_backend::domain::{concat_cutout_binding::{bind_still,StillAssets},concat_cutout_import::plan,composition::{Composition,CanvasSpec,TransformSpec,CompositionClipId,CompositionSource},cutout::SELFIE_MODEL_HASH};
fn source(id:&str,width:u32,height:u32)->CompositionSource{serde_json::from_value(json!({"id":id,"kind":"image","durationTicks":1000000,"width":width,"height":height,"hasAudio":false})).unwrap()}
fn setup()->(Composition,StillAssets){
 let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;let mut c=Composition::new(canvas);let raw=source("raw",2,2);c.sources.insert(raw.id.clone(),raw.clone());
 c.tracks=serde_json::from_value(json!([{"kind":"image","id":"image","name":"Image","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{}}]}]}])).unwrap();
 let snapshot=serde_json::from_value(json!({"sourceId":"raw","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":SELFIE_MODEL_HASH,"runtimeHash":"d".repeat(64),"correction":{"custom":false,"strokes":[],"feather":0}})).unwrap();
 (c,StillAssets{original:raw,copy:source("copy",2,2),mask:source("mask",256,256),regions:vec![],snapshot})
}
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use tower::ServiceExt;
use video_kadr_backend::{library::MediaEntry,services::local_person,model::JobStatus};
fn request(method:&str,path:&str,token:&str,data:serde_json::Value)->Request<Body>{Request::builder().method(method).uri(path).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(data.to_string())).unwrap()}
async fn value(response:axum::response::Response)->serde_json::Value{serde_json::from_slice(&to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap()}
#[tokio::test]
#[ignore="requires verified native mask correction helper"]
async fn saved_rectangular_rvm_mask_http_settings_job_and_asset(){
 use sha2::{Digest,Sha256};use video_kadr_backend::domain::cutout::PERSON_MODEL_HASH;
 let(state,_dir)=support::make_state(true,false).await;
 let state=state.with_person_config(Some(local_person::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:"/unused-person-model".into(),runtime_library:"/unused-ort".into()}));
 let owner=state.db.register_auth_user("rvm-owner","public synthetic password").await.unwrap().unwrap();let outsider=state.db.register_auth_user("rvm-outsider","public synthetic password").await.unwrap().unwrap();
 let(c,mut assets)=setup();let mut c=c;assets.original.width=4;assets.copy.width=4;assets.mask.width=512;assets.mask.height=256;assets.snapshot.model_hash=PERSON_MODEL_HASH.into();assets.snapshot.subject=Some(video_kadr_backend::domain::concat_cutout_import::Subject::Person);c.sources.insert(assets.original.id.clone(),assets.original.clone());
 let raw=state.sources_dir().join("raw.png");image::RgbaImage::from_pixel(4,2,image::Rgba([13,21,34,255])).save(&raw).unwrap();
 let mask=state.sources_dir().join("mask.png");let probabilities:Vec<u8>=(0..256).flat_map(|_|(0..512).map(|x|if x<256{255}else{0})).collect();image::GrayImage::from_raw(512,256,probabilities.clone()).unwrap().save(&mask).unwrap();
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&mask,std::fs::Permissions::from_mode(0o600)).unwrap();}
 image::RgbaImage::from_fn(4,2,|x,_|image::Rgba([13,21,34,if x<2{255}else{0}])).save(state.sources_dir().join("copy.png")).unwrap();
 let hash=|bytes:&[u8]|format!("{:x}",Sha256::digest(bytes));assets.snapshot.source_hash=hash(&std::fs::read(&raw).unwrap());assets.snapshot.mask_hash=hash(&probabilities);assets.snapshot.mask_file_hash=hash(&std::fs::read(&mask).unwrap());
 let p=plan(&json!({"subject":"person","mode":"auto","feather":0})).unwrap();let clip=CompositionClipId::parse("clip").unwrap();let bound=bind_still(&c,&clip,&p,&assets,&assets.snapshot.source_hash,PERSON_MODEL_HASH,&assets.snapshot.runtime_hash).unwrap();
 let ids=vec!["copy".into(),"mask".into(),"raw".into()];for id in &ids{let name=format!("{id}.png");assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":name,"url":format!("/files/sources/{name}")}))).await);}
 let space=state.db.create_space(&owner.user.username,"RVM masks").await.unwrap();let project=state.db.create_space_composition_project("RVM",&serde_json::to_value(bound).unwrap(),&ids,&owner.user.username,&space.id).await.unwrap();
 let app=support::router(state.clone());let settings=format!("/api/composition-projects/{}/cutout/clip",project.id);let endpoint=format!("/api/composition-projects/{}/cutout/corrections",project.id);
 assert_eq!(app.clone().oneshot(request("GET",&settings,&outsider.token,json!(null))).await.unwrap().status(),StatusCode::FORBIDDEN);
 let response=app.clone().oneshot(request("GET",&settings,&owner.token,json!(null))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(value(response).await["snapshot"]["modelHash"],PERSON_MODEL_HASH);
 let recipe=json!({"clipId":"clip","expectedAttachment":assets.snapshot,"correction":{"custom":true,"strokes":[{"tool":"brush","size":0.5,"points":[[0.5,0.5]],"at":null}],"feather":0.01}});
 assert_eq!(app.clone().oneshot(request("POST",&endpoint,&outsider.token,recipe.clone())).await.unwrap().status(),StatusCode::FORBIDDEN);
 let response=app.clone().oneshot(request("POST",&endpoint,&owner.token,recipe)).await.unwrap();assert_eq!(response.status(),StatusCode::ACCEPTED);let id=value(response).await["jobId"].as_str().unwrap().to_owned();
 let job=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{let job=state.get_job(&id).await.unwrap().unwrap();if matches!(job.status,JobStatus::Done|JobStatus::Error|JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.unwrap();assert_eq!(job.status,JobStatus::Done,"{:?}",job.error);
 assert_eq!(job.result.as_ref().unwrap()["maskHeight"],256);
 let response=app.oneshot(request("POST",&format!("/api/cutout/jobs/{id}/asset"),&owner.token,json!({"projectId":project.id}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let accepted=value(response).await;assert_eq!(accepted["cutout"]["snapshot"]["modelHash"],PERSON_MODEL_HASH);assert_eq!(accepted["cutout"]["mask"]["height"],256);assert_eq!(accepted["cutout"]["snapshot"]["subject"],"person");
}
