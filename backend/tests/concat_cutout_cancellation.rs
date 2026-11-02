mod support;
use video_kadr_backend::{config::resource_classes::ResourceClass,handlers::concat_cutout_analyse::acquire_analysis};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn cancellation_leaves_resource_queue_without_waiting_for_occupied_permits(){
 let(state,_dir)=support::make_state(true,false).await;let limit=state.resource_limit(ResourceClass::Analysis);let mut occupied=vec![];
 for _ in 0..limit{occupied.push(state.acquire_job_slot(ResourceClass::Analysis).await.unwrap());}
 assert_eq!(state.resource_available(ResourceClass::Analysis),0);
 let cancel=CancellationToken::new();let worker_state=state.clone();let token=cancel.clone();let task=tokio::spawn(async move{acquire_analysis(&worker_state,&token).await});
 tokio::task::yield_now().await;cancel.cancel();
 let result=tokio::time::timeout(std::time::Duration::from_secs(5),task).await.expect("cancelled admission must exit while all permits remain occupied").unwrap();assert!(result.is_err());assert_eq!(state.resource_available(ResourceClass::Analysis),0);
 drop(occupied);assert_eq!(state.resource_available(ResourceClass::Analysis),limit);
}
#[tokio::test]
async fn pre_cancelled_admission_does_not_consume_a_free_permit(){
 let(state,_dir)=support::make_state(true,false).await;let before=state.resource_available(ResourceClass::Analysis);let cancel=CancellationToken::new();cancel.cancel();
 assert!(acquire_analysis(&state,&cancel).await.is_err());assert_eq!(state.resource_available(ResourceClass::Analysis),before);
}

#[cfg(unix)]
#[tokio::test]
#[ignore="requires pinned SAM/RVM inventory and native helper; Person process is a controlled sleep shim"]
async fn dropping_http_reaps_person_process_and_removes_new_sam_region(){
 use std::os::unix::fs::PermissionsExt;use tower::ServiceExt;
 use video_kadr_backend::{services::{local_person,local_sam},domain::composition::{CanvasSpec,Composition,CompositionSource,TransformSpec},library::MediaEntry};
 let(state,_dir)=support::make_state(true,false).await;
 let marker=state.storage.join("person.pid");let shim=state.storage.join("person-wait.sh");let quoted=format!("'{}'",marker.to_str().unwrap().replace('\'',"'\\''"));std::fs::write(&shim,format!("#!/bin/sh\nprintf '%s' \"$$\" > {quoted}\nexec /bin/sleep 60\n")).unwrap();std::fs::set_permissions(&shim,std::fs::Permissions::from_mode(0o700)).unwrap();
 let executable:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into();let ort:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into();
 let person=local_person::Config{executable:shim,model:std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap().into(),runtime_library:ort.clone()};let sam=local_sam::Config{executable,encoder:std::env::var("VIDEO_KADR_TEST_SAM_ENCODER_MODEL").unwrap().into(),decoder:std::env::var("VIDEO_KADR_TEST_SAM_DECODER_MODEL").unwrap().into(),runtime_library:ort};let state=state.with_person_config(Some(person)).with_sam_config(Some(sam));
 let owner=state.db.register_auth_user("cancel-owner","public synthetic password").await.unwrap().unwrap();image::RgbaImage::from_pixel(2,2,image::Rgba([20,30,40,255])).save(state.sources_dir().join("raw.png")).unwrap();assert!(state.library.add(MediaEntry::from_result("source",&serde_json::json!({"id":"raw","filename":"raw.png","url":"/files/sources/raw.png","mediaType":"image","width":2,"height":2}))).await);
 let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;let mut document=Composition::new(canvas);let source:CompositionSource=serde_json::from_value(serde_json::json!({"id":"raw","kind":"image","durationTicks":1000000,"width":2,"height":2,"hasAudio":false})).unwrap();document.sources.insert(source.id.clone(),source);document.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let space=state.db.create_space(&owner.user.username,"Cancel analysis").await.unwrap();let project=state.db.create_space_composition_project("Cancel",&serde_json::to_value(document).unwrap(),&["raw".into()],&owner.user.username,&space.id).await.unwrap();let limit=state.resource_limit(ResourceClass::Analysis);
 let request=axum::http::Request::builder().method("POST").uri(format!("/api/composition-projects/{}/concat-cutout",project.id)).header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(axum::body::Body::from(serde_json::json!({"clipId":"clip","expectedRevision":project.revision,"cutout":{"subject":"person","mode":"custom","strokes":[{"tool":"smartBrush","size":0.2,"points":[[0.5,0.5]],"at":0}]}}).to_string())).unwrap();
 let app=support::router(state.clone());let http=tokio::spawn(app.oneshot(request));
 tokio::time::timeout(std::time::Duration::from_secs(30),async{while !marker.is_file(){assert!(!http.is_finished(),"HTTP ended before controlled Person process started");tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.unwrap();
 let pid:i32=std::fs::read_to_string(&marker).unwrap().parse().unwrap();assert_eq!(unsafe{libc::kill(pid,0)},0);assert!(state.db.space_source_ids(&space.id).await.unwrap().len()>1,"real SAM region must exist before cancellation");
 http.abort();let _=http.await;
 tokio::time::timeout(std::time::Duration::from_secs(10),async{loop{if state.resource_available(ResourceClass::Analysis)==limit&&state.db.space_source_ids(&space.id).await.unwrap().len()==1{break}tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.unwrap();
 assert_eq!(unsafe{libc::kill(pid,0)},-1,"controlled child must be gone after permit is released");
 let saved=state.db.get_composition_project_for(&project.id,&owner.user.username).await.unwrap().unwrap();assert_eq!(saved.revision,project.revision);assert_eq!(saved.document,project.document);
 let root=state.storage.join("concat-analysis");assert!(std::fs::read_dir(root).unwrap().next().is_none());assert_eq!(state.library.list().await.len(),1);
}
