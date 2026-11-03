mod support;
use video_kadr_backend::{services::{local_person,local_person_still},process_control::ProcessRuntime};
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn invalid_identity_or_unavailable_engine_cleans_one_frame_staging(){
 use sha2::{Digest,Sha256};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("input.png");image::RgbaImage::new(2,2).save(&source).unwrap();
 let hash=format!("{:x}",Sha256::digest(std::fs::read(&source).unwrap()));
 let config=local_person::Config{executable:dir.path().join("missing-executable"),model:dir.path().join("missing-model"),runtime_library:dir.path().join("missing-ort")};
 let cancel=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 for digest in [&hash,&"a".repeat(64)]{
  assert!(local_person_still::analyse(&ProcessRuntime::local_default(),&config,&source,digest,dir.path(),&cancel,std::time::Duration::from_secs(10),&tx).await.is_err());
  assert!(std::fs::read_dir(dir.path()).unwrap().all(|e|!e.unwrap().file_type().unwrap().is_dir()));
 }
 cancel.cancel();assert!(local_person_still::analyse(&ProcessRuntime::local_default(),&config,&source,&hash,dir.path(),&cancel,std::time::Duration::from_secs(10),&tx).await.is_err());
}

#[tokio::test]
async fn person_inference_failure_in_common_runner_does_not_silently_use_selfie(){
 use video_kadr_backend::{services::{concat_cutout_still::{self,Engine},concat_cutout_selection::Model},domain::concat_cutout_import::Subject};
 use sha2::{Digest,Sha256};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("input.png");image::RgbaImage::new(2,2).save(&source).unwrap();let hash=format!("{:x}",Sha256::digest(std::fs::read(&source).unwrap()));
 let config=local_person::Config{executable:dir.path().join("missing-executable"),model:dir.path().join("missing-model"),runtime_library:dir.path().join("missing-ort")};
 let mut requested=vec![];let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 let result=concat_cutout_still::analyse(&ProcessRuntime::local_default(),Subject::Auto,None,&source,&hash,dir.path(),|model|{requested.push(model);std::future::ready(Ok(Engine::Person(config.clone())))},&CancellationToken::new(),std::time::Duration::from_secs(10),&tx).await;
 assert!(result.is_err());assert_eq!(requested,[Model::Person]);
 assert!(std::fs::read_dir(dir.path()).unwrap().all(|entry|!entry.unwrap().file_type().unwrap().is_dir()));
}

#[tokio::test]
#[ignore="requires native executable, pinned RVM model and ORT"]
async fn actual_person_still_enters_common_runner_with_rectangular_verified_mask(){
 use video_kadr_backend::{services::{concat_cutout_still::{self,Engine},concat_cutout_selection::Model,concat_cutout_asset::{load_still,ExpectedIdentity}},domain::{concat_cutout_import::Subject,cutout::PERSON_MODEL_HASH}};
 use sha2::{Digest,Sha256};
 let config=local_person::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap().into(),runtime_library:std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into()};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("input.png");image::RgbaImage::from_fn(37,23,|x,y|{let n=y*37+x;image::Rgba([(n%151)as u8,((n*17)%180)as u8,((n*31)%199)as u8,(n%256)as u8])}).save(&source).unwrap();let hash=format!("{:x}",Sha256::digest(std::fs::read(&source).unwrap()));
 let cancel=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();let mut requested=vec![];
 let result=concat_cutout_still::analyse(&ProcessRuntime::local_default(),Subject::Person,None,&source,&hash,dir.path(),|model|{requested.push(model);std::future::ready(Ok(Engine::Person(config.clone())))},&cancel,std::time::Duration::from_secs(60),&tx).await.unwrap();
 assert_eq!(requested,[Model::Person]);assert_eq!(result.model,Model::Person);assert_eq!(result.metadata.schema_version,8);assert_eq!(result.metadata.model_hash,PERSON_MODEL_HASH);assert_eq!(result.metadata.source_hash,hash);assert_eq!((result.metadata.mask_width,result.metadata.mask_height),(512,320));
 let mask=load_still(&result.mask,&result.metadata,ExpectedIdentity{source_hash:&hash,model_hash:PERSON_MODEL_HASH,runtime_hash:&result.metadata.runtime_hash},&cancel).await.unwrap();assert_eq!(mask.probabilities.len(),512*320);

 let correction=video_kadr_backend::domain::concat_cutout_import::plan(&serde_json::json!({"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"brush","size":0.2,"points":[[0.5,0.5]]}]})).unwrap();
 let corrected_path=dir.path().join("corrected.png");
 video_kadr_backend::services::local_mask_edit::correct(&ProcessRuntime::local_default(),&config.executable,video_kadr_backend::services::local_mask_edit::Input{source:&source,mask:&result.mask,original:&result.metadata,correction:&correction.correction},&corrected_path,&cancel,std::time::Duration::from_secs(60),&tx).await.unwrap();
 let automatic=load_still(&result.mask,&result.metadata,ExpectedIdentity{source_hash:&hash,model_hash:PERSON_MODEL_HASH,runtime_hash:&result.metadata.runtime_hash},&cancel).await.unwrap();
 let resolved=video_kadr_backend::domain::concat_cutout_resolve::resolve(&correction,&automatic,&[],37./23.,&std::sync::atomic::AtomicBool::new(false)).unwrap();
 let original=video_kadr_backend::services::concat_cutout_asset::load_verified_image(&source,Some(&hash),&cancel).await.unwrap();
 let corrected=video_kadr_backend::services::concat_cutout_asset::load_verified_image(&corrected_path,None,&cancel).await.unwrap();
 video_kadr_backend::services::concat_cutout_asset::verify_copy_pixels(&original,&corrected,resolved.as_deref(),&cancel).unwrap();

 let descriptor=|id:&str,w:u32,h:u32|->video_kadr_backend::domain::composition::CompositionSource{serde_json::from_value(serde_json::json!({"id":id,"kind":"image","durationTicks":1000000,"width":w,"height":h,"hasAudio":false})).unwrap()};
 let raw=descriptor("raw",37,23);
 let prepared=result.prepare_binding(&ProcessRuntime::local_default(),&source,correction.clone(),raw.clone(),descriptor("copy",37,23),descriptor("mask",512,320),&cancel,std::time::Duration::from_secs(60),&tx).await.unwrap();
 use video_kadr_backend::domain::concat_project_import::{ImportRequest,StillBindings,import_with_still_bindings};
 let doc=serde_json::json!({"version":1,"media":[{"id":"media","kind":"image","name":"portrait.png"}],"activeTimelineId":"a","timelines":[{"id":"a","video":{"width":40,"height":24,"rateNum":24,"rateDen":1},"tracks":[{"id":"track"}],"clips":[{"id":"clip","trackId":"track","mediaId":"media","kind":"image","duration":1,"cutout":{"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"brush","size":0.2,"points":[[0.5,0.5]]}]}}]}]});
 let request:ImportRequest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"document":doc,"mediaBindings":{"media":raw}})).unwrap();
 let bindings:StillBindings=std::collections::BTreeMap::from([("a".into(),std::collections::BTreeMap::from([("clip".into(),prepared.binding.clone())]))]);
 let report=import_with_still_bindings(request,&bindings).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);
 let project=report.project.unwrap();let c=&project.compositions["concat-timeline-0"].content;c.validate().unwrap();
 let snapshot=&c.metadata.as_ref().unwrap().media_versions[&video_kadr_backend::domain::composition::CompositionClipId::parse("clip").unwrap()].cutout;
 assert_eq!(snapshot.as_ref().unwrap().model_hash,PERSON_MODEL_HASH);assert_eq!(snapshot.as_ref().unwrap().correction,correction.correction);
 // Rebind the actual generated bundle into stable Concat slots, including a
 // collision between a source slot and the physical processed-copy ID.
 let mut logical_doc=report.source_document.clone();logical_doc["media"].as_array_mut().unwrap().push(serde_json::json!({"id":"copy","kind":"image"}));
 let logical_request:ImportRequest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"preserveMediaIds":true,"document":logical_doc,"mediaBindings":{"media":descriptor("raw",37,23),"copy":descriptor("unrelated-copy",37,23)}})).unwrap();
 let logical_report=import_with_still_bindings(logical_request,&bindings).unwrap();assert_eq!(logical_report.status,"converted","{:?}",logical_report.diagnostics);
 let logical=logical_report.project.unwrap();logical.dependency_order().unwrap();
 let assets=video_kadr_backend::domain::concat_cutout_binding::saved_still(&serde_json::to_value(&logical).unwrap(),Some(&logical.root),&video_kadr_backend::domain::composition::CompositionClipId::parse("clip").unwrap()).unwrap();
 assert_eq!(assets.original.id.as_str(),"media");assert_eq!(assets.snapshot.source_id,"media");assert_ne!(assets.copy.id.as_str(),"copy");assert_eq!(logical.library_source_id(&assets.copy.id).as_str(),"copy");assert_eq!(logical.library_source_id(&assets.original.id).as_str(),"raw");
 video_kadr_backend::services::concat_cutout_asset::verify_still_bundle(&source,&prepared.copy,&prepared.mask,&assets,&correction,ExpectedIdentity{source_hash:&hash,model_hash:PERSON_MODEL_HASH,runtime_hash:&prepared.binding.runtime_hash},&cancel).await.unwrap();
 let temporary_copy=prepared.copy.clone();let temporary_mask=prepared.mask.clone();

 let(state,_state_dir)=support::make_state(true,false).await;
 let state=state.with_person_config(Some(config.clone()));
 let registered_source=state.sources_dir().join("raw.png");std::fs::copy(&source,&registered_source).unwrap();
 assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&serde_json::json!({"id":"raw","filename":"raw.png","url":"/files/sources/raw.png","mediaType":"image","width":37,"height":23}))).await);
 let space=state.db.create_space("owner","Imported cutout").await.unwrap();
 let mut canvas=video_kadr_backend::domain::composition::CanvasSpec::default();canvas.width=40;canvas.height=24;
 let mut base=video_kadr_backend::domain::composition::Composition::new(canvas);let raw=prepared.binding.assets.original.clone();base.sources.insert(raw.id.clone(),raw);
 base.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let project=state.db.create_space_composition_project("Import",&serde_json::to_value(base).unwrap(),&["raw".into()],"owner",&space.id).await.unwrap();
 let registered=video_kadr_backend::handlers::concat_cutout_publish::register(&state,"owner",&project.id,project.revision,prepared,&cancel).await.unwrap();
 assert!(!temporary_copy.exists()&&!temporary_mask.exists());
 let copy_path=state.library.resolve_media_path(&registered.copy).await.unwrap();let mask_path=state.library.resolve_media_path(&registered.mask).await.unwrap();
 assert!(copy_path.is_file()&&mask_path.is_file());
 assert_eq!(state.db.source_space_id(&registered.copy.id).await.unwrap().as_deref(),Some(space.id.as_str()));assert_eq!(state.db.source_space_id(&registered.mask.id).await.unwrap().as_deref(),Some(space.id.as_str()));
 assert!(!state.db.accessible_space_source_ids("outsider").await.unwrap().contains(&registered.copy.id));
 assert_eq!(registered.binding.source_hash,hash);assert_eq!(format!("{:x}",Sha256::digest(std::fs::read(&mask_path).unwrap())),registered.binding.assets.snapshot.mask_file_hash);
 let copied=video_kadr_backend::services::concat_cutout_asset::load_verified_image(&copy_path,None,&cancel).await.unwrap();assert_eq!((copied.width,copied.height),(37,23));

 let clip=video_kadr_backend::domain::composition::CompositionClipId::parse("clip").unwrap();
 use video_kadr_backend::handlers::concat_cutout_publish::apply_registered;
 assert!(apply_registered(&state,"outsider",&project.id,None,&clip,project.revision,&registered,&cancel).await.is_err());
 assert!(apply_registered(&state,"owner",&project.id,None,&clip,project.revision+1,&registered,&cancel).await.is_err());
 let other=state.db.create_space_composition_project("Other cutout target",&project.document,&project.source_ids,"owner",&space.id).await.unwrap();
 assert!(apply_registered(&state,"owner",&other.id,None,&clip,other.revision,&registered,&cancel).await.is_err());
 assert_eq!(state.db.get_composition_project_for(&other.id,"owner").await.unwrap().unwrap().document,other.document);
 let applied=apply_registered(&state,"owner",&project.id,None,&clip,project.revision,&registered,&cancel).await.unwrap();assert_eq!(applied.revision,project.revision+1);
 assert_eq!(applied.document["metadata"]["mediaVersions"]["clip"]["copySourceId"],registered.copy.id);
 assert_eq!(applied.document["metadata"]["mediaVersions"]["clip"]["cutout"]["modelHash"],PERSON_MODEL_HASH);
 video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,"owner",&project.id,None,&clip,applied.revision,&correction,&cancel).await.unwrap();
 assert!(apply_registered(&state,"owner",&project.id,None,&clip,project.revision,&registered,&cancel).await.is_err());

 use tower::ServiceExt;
 let owner=state.db.register_auth_user("owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("outsider","public synthetic password").await.unwrap().unwrap();
 let captured=std::env::var("CONCAT_CUTOUT_UI_REQUEST_INPUT").ok().map(|path|serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap());
 let http_document=if let Some(body)=&captured{let id=body["compositionId"].as_str().unwrap();serde_json::json!({"schemaVersion":1,"root":id,"compositions":{id:{"content":project.document,"instances":[]}}})}else{project.document.clone()};
 let http_project=state.db.create_space_composition_project("HTTP analysis",&http_document,&["raw".into()],"owner",&space.id).await.unwrap();
 let app=support::router(state.clone());let endpoint=format!("/api/composition-projects/{}/concat-cutout",http_project.id);
 let body=serde_json::json!({"clipId":"clip","expectedRevision":http_project.revision,"cutout":{"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"brush","size":0.2,"points":[[0.5,0.5]]}]}});
 let body=captured.unwrap_or(body);assert_eq!(body["expectedRevision"],http_project.revision);
 let request=|token:&str,body:&serde_json::Value|axum::http::Request::builder().method("POST").uri(&endpoint).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(axum::body::Body::from(body.to_string())).unwrap();
 assert_eq!(app.clone().oneshot(request(&outsider.token,&body)).await.unwrap().status(),axum::http::StatusCode::FORBIDDEN);
 let mut stale=body.clone();stale["expectedRevision"]=serde_json::json!(http_project.revision+1);assert_eq!(app.clone().oneshot(request(&owner.token,&stale)).await.unwrap().status(),axum::http::StatusCode::CONFLICT);
 let response=app.clone().oneshot(request(&owner.token,&body)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,axum::http::StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
 let updated:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(updated["revision"],http_project.revision+1);
 let content=if let Some(id)=body["compositionId"].as_str(){&updated["document"]["compositions"][id]["content"]}else{&updated["document"]};
 assert_eq!(content["metadata"]["mediaVersions"]["clip"]["cutout"]["modelHash"],PERSON_MODEL_HASH);
 assert_eq!(content["metadata"]["mediaVersions"]["clip"]["cutout"]["subject"],"person");
 let captured_plan=video_kadr_backend::domain::concat_cutout_import::plan(&body["cutout"]).unwrap();
 video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,"owner",&http_project.id,body["compositionId"].as_str(),&clip,http_project.revision+1,&captured_plan,&cancel).await.unwrap();

 let old_copy=content["metadata"]["mediaVersions"]["clip"]["copySourceId"].as_str().unwrap().to_owned();
 let old_mask=content["metadata"]["mediaVersions"]["clip"]["cutout"]["maskSourceId"].as_str().unwrap().to_owned();
 let mut repeat=body.clone();repeat["expectedRevision"]=serde_json::json!(http_project.revision+1);repeat["cutout"]["feather"]=serde_json::json!(0.03);
 let response=app.clone().oneshot(request(&owner.token,&repeat)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,axum::http::StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
 let repeated:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let content=if let Some(id)=repeat["compositionId"].as_str(){&repeated["document"]["compositions"][id]["content"]}else{&repeated["document"]};
 let pair=&content["metadata"]["mediaVersions"]["clip"];assert_eq!(pair["originalSourceId"],"raw");assert_ne!(pair["copySourceId"],old_copy);assert_eq!(pair["cutout"]["sourceHash"],hash);assert_eq!(pair["cutout"]["correction"]["feather"],0.03);
 assert!(content["sources"].get(&old_copy).is_some()&&content["sources"].get(&old_mask).is_some());
 for id in [&old_copy,&old_mask]{let entry=state.library.get(id).await.unwrap();assert!(state.library.resolve_media_path(&entry).await.unwrap().is_file());}
 let repeated_plan=video_kadr_backend::domain::concat_cutout_import::plan(&repeat["cutout"]).unwrap();video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,"owner",&http_project.id,repeat["compositionId"].as_str(),&clip,http_project.revision+2,&repeated_plan,&cancel).await.unwrap();
 assert_eq!(app.oneshot(request(&owner.token,&body)).await.unwrap().status(),axum::http::StatusCode::CONFLICT);
 drop(registered);assert!(copy_path.is_file()&&mask_path.is_file());

}

#[tokio::test]
#[ignore="requires native RVM/helper/ORT; region plane is synthetic"]
async fn actual_person_prepares_binding_with_verified_retained_smart_region(){
 use sha2::{Digest,Sha256};use video_kadr_backend::{services::{concat_cutout_still::{self,Engine,SavedRegions},concat_cutout_asset::{verify_still_bundle_with_regions,ExpectedIdentity}},domain::{composition::CompositionSource,concat_cutout_import::{plan,Subject},cutout::RegionAttachment}};
 let hash=|bytes:&[u8]|format!("{:x}",Sha256::digest(bytes));
 let config=local_person::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap().into(),runtime_library:std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into()};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.png");image::RgbaImage::from_fn(37,23,|x,y|image::Rgba([(x*3)as u8,(y*7)as u8,33,255])).save(&source).unwrap();let source_hash=hash(&std::fs::read(&source).unwrap());
 let token=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 let result=concat_cutout_still::analyse(&ProcessRuntime::local_default(),Subject::Person,None,&source,&source_hash,dir.path(),|_|std::future::ready(Ok(Engine::Person(config.clone()))),&token,std::time::Duration::from_secs(60),&tx).await.unwrap();
 let plan=plan(&serde_json::json!({"subject":"person","mode":"custom","feather":0.01,"strokes":[{"tool":"smartBrush","size":0.2,"points":[[0.25,0.5]],"at":0}]})).unwrap();let points=plan.correction.strokes[0].points.clone();
 let pixels:Vec<u8>=(0..256).flat_map(|_|(0..256).map(|x|if x<128{255}else{0})).collect();let path=dir.path().join("region.png");image::GrayImage::from_raw(256,256,pixels.clone()).unwrap().save(&path).unwrap();
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();}
 let region=RegionAttachment{source_id:"raw".into(),mask_source_id:"region".into(),source_hash:source_hash.clone(),mask_hash:hash(&pixels),mask_file_hash:hash(&std::fs::read(&path).unwrap()),encoder_hash:hash(b"synthetic region encoder"),decoder_hash:hash(b"synthetic region decoder"),runtime_hash:hash(b"synthetic region runtime"),prompt_hash:hash(&serde_json::to_vec(&points).unwrap()),points};
 let mut correction=plan.correction.clone();correction.strokes[0].region=Some(region.clone());
 let descriptor=|id:&str,w:u32,h:u32|->CompositionSource{serde_json::from_value(serde_json::json!({"id":id,"kind":"image","durationTicks":1000000,"width":w,"height":h,"hasAudio":false})).unwrap()};
 let paths=std::collections::BTreeMap::from([("region".into(),path)]);
 let regions=SavedRegions{correction:Some(correction),sources:vec![descriptor("region",256,256)],paths:paths.clone()};
 let prepared=result.prepare_binding_with_regions(&ProcessRuntime::local_default(),&source,plan.clone(),descriptor("raw",37,23),descriptor("copy",37,23),descriptor("mask",512,320),regions,&token,std::time::Duration::from_secs(60),&tx).await.unwrap();
 assert_eq!(prepared.binding.assets.snapshot.correction.strokes[0].region,Some(region));
 let b=&prepared.binding;verify_still_bundle_with_regions(&source,&prepared.copy,&prepared.mask,&b.assets,&plan,ExpectedIdentity{source_hash:&b.source_hash,model_hash:&b.model_hash,runtime_hash:&b.runtime_hash},&paths,&token).await.unwrap();

 let copy=prepared.copy.clone();
 let(state,_state_dir)=support::make_state(true,false).await;let state=state.with_person_config(Some(config.clone()));
 let owner=state.db.register_auth_user("smart-owner","public synthetic password").await.unwrap().unwrap();
 std::fs::copy(&source,state.sources_dir().join("raw.png")).unwrap();std::fs::copy(&paths["region"],state.sources_dir().join("region.png")).unwrap();
 for(id,w,h)in [("raw",37,23),("region",256,256)]{assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&serde_json::json!({"id":id,"filename":format!("{id}.png"),"url":format!("/files/sources/{id}.png"),"mediaType":"image","width":w,"height":h}))).await);}
 let mut canvas=video_kadr_backend::domain::composition::CanvasSpec::default();canvas.width=40;canvas.height=24;let mut base=video_kadr_backend::domain::composition::Composition::new(canvas);
 for source in [descriptor("raw",37,23),descriptor("region",256,256)]{base.sources.insert(source.id.clone(),source);}
 base.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let space=state.db.create_space(&owner.user.username,"Smart region replay").await.unwrap();
 let project=state.db.create_space_composition_project("Smart",&serde_json::to_value(base).unwrap(),&["raw".into(),"region".into()],&owner.user.username,&space.id).await.unwrap();
 let registered=video_kadr_backend::handlers::concat_cutout_publish::register(&state,&owner.user.username,&project.id,project.revision,prepared,&token).await.unwrap();assert!(!copy.exists());
 let clip=video_kadr_backend::domain::composition::CompositionClipId::parse("clip").unwrap();
 let saved=video_kadr_backend::handlers::concat_cutout_publish::apply_registered(&state,&owner.user.username,&project.id,None,&clip,project.revision,&registered,&token).await.unwrap();
 use tower::ServiceExt;let app=support::router(state.clone());let endpoint=format!("/api/composition-projects/{}/concat-cutout",project.id);
 let body=serde_json::json!({"clipId":"clip","expectedRevision":saved.revision,"cutout":{"subject":"person","mode":"custom","feather":0.03,"strokes":[{"tool":"smartBrush","size":0.2,"points":[[0.25,0.5]],"at":0}]}});
 let request=|body:&serde_json::Value|axum::http::Request::builder().method("POST").uri(&endpoint).header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(axum::body::Body::from(body.to_string())).unwrap();
 let response=app.clone().oneshot(request(&body)).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,axum::http::StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
 let updated:serde_json::Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(updated["document"]["metadata"]["mediaVersions"]["clip"]["cutout"]["correction"]["strokes"][0]["region"]["maskSourceId"],"region");
 let expected=video_kadr_backend::domain::concat_cutout_import::plan(&body["cutout"]).unwrap();
 video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,&owner.user.username,&project.id,None,&clip,saved.revision+1,&expected,&token).await.unwrap();
 if std::env::var_os("CONCAT_CUTOUT_EXPORT").is_some(){assert_saved_cutout_lossless_export(&state,&owner.token,&updated["document"]).await;}

 let mut changed=body.clone();changed["expectedRevision"]=serde_json::json!(saved.revision+1);changed["cutout"]["strokes"][0]["points"][0][0]=serde_json::json!(0.75);assert_eq!(app.clone().oneshot(request(&changed)).await.unwrap().status(),axum::http::StatusCode::CONFLICT);
 let region_entry=state.library.get("region").await.unwrap();let region_path=state.library.resolve_media_path(&region_entry).await.unwrap();std::fs::write(region_path,b"changed region").unwrap();
 let mut tampered=body.clone();tampered["expectedRevision"]=serde_json::json!(saved.revision+1);assert_eq!(app.oneshot(request(&tampered)).await.unwrap().status(),axum::http::StatusCode::CONFLICT);
 assert_eq!(state.db.get_composition_project_for(&project.id,&owner.user.username).await.unwrap().unwrap().revision,saved.revision+1);

}

#[tokio::test]
#[ignore="requires actual pinned RVM and SAM models, helper and ORT"]
async fn actual_new_smart_region_http_generates_reuses_and_replaces_regions(){
 use video_kadr_backend::{services::{local_sam,concat_cutout_asset},domain::{composition::{CanvasSpec,Composition,TransformSpec,CompositionClipId},concat_cutout_import::plan}};use sha2::{Digest,Sha256};use tower::ServiceExt;
 let executable:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into();let ort:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into();
 let person=local_person::Config{executable:executable.clone(),model:std::env::var("VIDEO_KADR_TEST_PERSON_MODEL").unwrap().into(),runtime_library:ort.clone()};
 let sam=local_sam::Config{executable,encoder:std::env::var("VIDEO_KADR_TEST_SAM_ENCODER_MODEL").unwrap().into(),decoder:std::env::var("VIDEO_KADR_TEST_SAM_DECODER_MODEL").unwrap().into(),runtime_library:ort};
 let(state,_dir)=support::make_state(true,false).await;let state=state.with_person_config(Some(person)).with_sam_config(Some(sam));
 let owner=state.db.register_auth_user("new-smart-owner","public synthetic password").await.unwrap().unwrap();
 let source=state.sources_dir().join("raw.png");image::RgbaImage::from_fn(37,23,|x,y|image::Rgba([(x*3)as u8,(y*7)as u8,33,255])).save(&source).unwrap();let source_hash=format!("{:x}",Sha256::digest(std::fs::read(&source).unwrap()));
 assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&serde_json::json!({"id":"raw","filename":"raw.png","url":"/files/sources/raw.png","mediaType":"image","width":37,"height":23}))).await);
 let mut canvas=CanvasSpec::default();canvas.width=40;canvas.height=24;let mut composition=Composition::new(canvas);let raw:video_kadr_backend::domain::composition::CompositionSource=serde_json::from_value(serde_json::json!({"id":"raw","kind":"image","durationTicks":1000000,"width":37,"height":23,"hasAudio":false})).unwrap();composition.sources.insert(raw.id.clone(),raw);
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let space=state.db.create_space(&owner.user.username,"Learned regions").await.unwrap();let project=state.db.create_space_composition_project("Smart",&serde_json::to_value(composition).unwrap(),&["raw".into()],&owner.user.username,&space.id).await.unwrap();
 let app=support::router(state.clone());let endpoint=format!("/api/composition-projects/{}/concat-cutout",project.id);let cancel=CancellationToken::new();let clip=CompositionClipId::parse("clip").unwrap();let mut last_region=String::new();
 for pass in 0..3{
  let points=if pass==2{vec![[0.75,0.5]]}else{vec![[0.25,0.5]]};let revision=project.revision+pass;
  let body=serde_json::json!({"clipId":"clip","expectedRevision":revision,"cutout":{"subject":"person","mode":"custom","feather":0.01+pass as f64*0.01,"strokes":[{"tool":"smartBrush","size":0.2,"points":points,"at":0}]}});
  let request=axum::http::Request::builder().method("POST").uri(&endpoint).header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(axum::body::Body::from(body.to_string())).unwrap();
  let response=app.clone().oneshot(request).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,axum::http::StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
  let saved:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let region=&saved["document"]["metadata"]["mediaVersions"]["clip"]["cutout"]["correction"]["strokes"][0]["region"];let id=region["maskSourceId"].as_str().unwrap().to_owned();
  assert_eq!(region["encoderHash"],local_sam::ENCODER_HASH);assert_eq!(region["decoderHash"],local_sam::DECODER_HASH);assert_eq!(region["sourceHash"],source_hash);assert_eq!(region["promptHash"],format!("{:x}",Sha256::digest(serde_json::to_vec(&points).unwrap())));
  if pass==1{assert_eq!(id,last_region);}if pass==2{assert_ne!(id,last_region);assert!(state.library.get(&last_region).await.is_some());}last_region=id.clone();
  assert_eq!(state.db.source_space_id(&id).await.unwrap().as_deref(),Some(space.id.as_str()));
  let expected=plan(&body["cutout"]).unwrap();let candidate=video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,&owner.user.username,&project.id,None,&clip,revision+1,&expected,&cancel).await.unwrap();assert_eq!(candidate.assets.regions.len(),1);
  if pass==1{
   use video_kadr_backend::domain::concat_project_import::{ImportRequest,ReadyStillBinding,StillBindings,import_with_still_bindings};
   let mut other=candidate.assets.original.clone();other.id=video_kadr_backend::domain::composition::SourceId::parse("unrelated-region").unwrap();
   image::RgbaImage::new(37,23).save(state.sources_dir().join("unrelated-region.png")).unwrap();assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&serde_json::json!({"id":"unrelated-region","filename":"unrelated-region.png","url":"/files/sources/unrelated-region.png","mediaType":"image","width":37,"height":23}))).await);
   let doc=serde_json::json!({"version":1,"media":[{"id":"slot","kind":"image"},{"id":id,"kind":"image"}],"timelines":[{"id":"main","video":{"width":40,"height":24,"rateNum":24,"rateDen":1},"tracks":[{"id":"images"}],"clips":[{"id":"clip","trackId":"images","mediaId":"slot","kind":"image","duration":1,"cutout":body["cutout"]}]}]});
   let request:ImportRequest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"preserveMediaIds":true,"document":doc,"mediaBindings":{"slot":candidate.assets.original,id.clone():other}})).unwrap();
   let snapshot=&candidate.assets.snapshot;let binding=ReadyStillBinding{assets:candidate.assets.clone(),source_settings:expected.clone(),source_hash:snapshot.source_hash.clone(),model_hash:snapshot.model_hash.clone(),runtime_hash:snapshot.runtime_hash.clone()};
   let bindings:StillBindings=std::collections::BTreeMap::from([("main".into(),std::collections::BTreeMap::from([("clip".into(),binding)]))]);
   let report=import_with_still_bindings(request,&bindings).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);let project=report.project.unwrap();project.dependency_order().unwrap();
   let assets=video_kadr_backend::domain::concat_cutout_binding::saved_still(&serde_json::to_value(&project).unwrap(),Some(&project.root),&clip).unwrap();let region=assets.snapshot.correction.strokes[0].region.as_ref().unwrap();assert_eq!(assets.snapshot.source_id,"slot");assert_eq!(region.source_id,"slot");assert_ne!(region.mask_source_id,id);
   let path=|id:&video_kadr_backend::domain::composition::SourceId|candidate.paths[project.library_source_id(id).as_str()].clone();
   let regions=assets.regions.iter().map(|s|(s.id.as_str().to_owned(),path(&s.id))).collect();
   concat_cutout_asset::verify_still_bundle_with_regions(&path(&assets.original.id),&path(&assets.copy.id),&path(&assets.mask.id),&assets,&expected,concat_cutout_asset::ExpectedIdentity{source_hash:&snapshot.source_hash,model_hash:&snapshot.model_hash,runtime_hash:&snapshot.runtime_hash},&regions,&cancel).await.unwrap();
   let physical=project.compositions.values().flat_map(|n|n.content.sources.keys()).chain(project.project_media.keys()).map(|id|project.library_source_id(id).as_str().to_owned()).collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>();
   let imported=state.db.create_space_composition_project("Rebound learned region",&serde_json::to_value(&project).unwrap(),&physical,&owner.user.username,&space.id).await.unwrap();
   let reopened=video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,&owner.user.username,&imported.id,Some(&project.root),&clip,imported.revision,&expected,&cancel).await.unwrap();assert_eq!(reopened.assets.snapshot.source_id,"slot");assert_ne!(reopened.assets.snapshot.correction.strokes[0].region.as_ref().unwrap().mask_source_id,id);

  }
  let copy_path=state.library.resolve_media_path(&state.library.get(candidate.assets.copy.id.as_str()).await.unwrap()).await.unwrap();assert_eq!(concat_cutout_asset::load_verified_image(&copy_path,None,&cancel).await.unwrap().width,37);
 }
}

#[tokio::test]
#[ignore="requires actual Object/Selfie models, helper and ORT"]
async fn actual_object_and_auto_fallback_reach_verified_saved_http_projects(){
 use video_kadr_backend::{services::{local_cutout,local_selfie,concat_cutout_still::{self,Engine},concat_cutout_selection::Model},domain::{concat_cutout_import::{Subject,plan},composition::{CompositionSource,CanvasSpec,Composition,TransformSpec,CompositionClipId}}};use sha2::{Digest,Sha256};use tower::ServiceExt;
 const OBJECT_HASH:&str="60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a";
 let executable:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into();let ort:std::path::PathBuf=std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into();
 let object=local_cutout::Config{executable:executable.clone(),model:std::env::var("VIDEO_KADR_TEST_OBJECT_MODEL").unwrap().into(),runtime_library:ort.clone()};let selfie=local_selfie::Config{executable,model:std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL").unwrap().into(),runtime_library:ort};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("raw.png");image::RgbaImage::from_fn(37,23,|x,y|{let n=y*37+x;image::Rgba([(n%151)as u8,((n*17)%180)as u8,((n*31)%199)as u8,(n%256)as u8])}).save(&source).unwrap();let hash=format!("{:x}",Sha256::digest(std::fs::read(&source).unwrap()));
 let descriptor=|id:&str,w:u32,h:u32|->CompositionSource{serde_json::from_value(serde_json::json!({"id":id,"kind":"image","durationTicks":1000000,"width":w,"height":h,"hasAudio":false})).unwrap()};let token=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 for subject in [Subject::Object,Subject::Auto]{
  let mut requested=vec![];let result=concat_cutout_still::analyse(&ProcessRuntime::local_default(),subject,None,&source,&hash,dir.path(),|model|{requested.push(model);std::future::ready(match model{Model::Person=>Err(anyhow::anyhow!("Person not configured")),Model::Selfie=>Ok(Engine::Selfie(selfie.clone())),Model::Object=>Ok(Engine::Object(object.clone()))})},&token,std::time::Duration::from_secs(120),&tx).await.unwrap();
  assert_eq!(result.model,Model::Object);assert_eq!(result.metadata.model_hash,OBJECT_HASH);assert_eq!((result.metadata.mask_width,result.metadata.mask_height),(512,512));assert_eq!(requested,if subject==Subject::Object{vec![Model::Object]}else{vec![Model::Person,Model::Selfie,Model::Object]});
  let settings=plan(&serde_json::json!({"mode":"auto","subject":subject,"feather":0.01})).unwrap();let prepared=result.prepare_binding(&ProcessRuntime::local_default(),&source,settings,descriptor("raw",37,23),descriptor("copy",37,23),descriptor("mask",512,512),&token,std::time::Duration::from_secs(60),&tx).await.unwrap();assert_eq!(prepared.binding.assets.snapshot.subject,Some(subject));drop(prepared);
 }
 let(state,_state_dir)=support::make_state(true,false).await;let state=state.with_cutout_config(Some(object)).with_selfie_config(Some(selfie));
 let owner=state.db.register_auth_user("object-owner","public synthetic password").await.unwrap().unwrap();std::fs::copy(&source,state.sources_dir().join("raw.png")).unwrap();assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&serde_json::json!({"id":"raw","filename":"raw.png","url":"/files/sources/raw.png","mediaType":"image","width":37,"height":23}))).await);
 let mut canvas=CanvasSpec::default();canvas.width=40;canvas.height=24;let mut composition=Composition::new(canvas);let raw=descriptor("raw",37,23);composition.sources.insert(raw.id.clone(),raw);composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let space=state.db.create_space(&owner.user.username,"Object policy").await.unwrap();let project=state.db.create_space_composition_project("Object",&serde_json::to_value(composition).unwrap(),&["raw".into()],&owner.user.username,&space.id).await.unwrap();let app=support::router(state.clone());let clip=CompositionClipId::parse("clip").unwrap();
 for(index,subject)in [Subject::Object,Subject::Auto].into_iter().enumerate(){
  let revision=project.revision+index as u64;let body=serde_json::json!({"clipId":"clip","expectedRevision":revision,"cutout":{"mode":"auto","subject":subject,"feather":0.01}});
  let request=axum::http::Request::builder().method("POST").uri(format!("/api/composition-projects/{}/concat-cutout",project.id)).header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(axum::body::Body::from(body.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();let status=response.status();let bytes=axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap();assert_eq!(status,axum::http::StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
  let saved:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let snapshot=&saved["document"]["metadata"]["mediaVersions"]["clip"]["cutout"];assert_eq!(snapshot["modelHash"],OBJECT_HASH);assert_eq!(snapshot["subject"],serde_json::to_value(subject).unwrap());assert_eq!(snapshot["sourceHash"],hash);
  video_kadr_backend::handlers::concat_cutout_access::verify_saved_still(&state,&owner.user.username,&project.id,None,&clip,revision+1,&plan(&body["cutout"]).unwrap(),&token).await.unwrap();
 }
}

async fn assert_saved_cutout_lossless_export(state:&video_kadr_backend::state::AppState,token:&str,content:&serde_json::Value){
 use video_kadr_backend::domain::{composition::Composition,flat_surface::render_media_layers,group_surface::GroupSurface};
 let mut composition:Composition=serde_json::from_value(content.clone()).unwrap();
 for track in &mut composition.tracks{if let video_kadr_backend::domain::composition::CompositionTrack::Image{clips,..}=track{for clip in clips{clip.effects=serde_json::from_value(serde_json::json!([{"kind":"color_invert","parameters":{}}])).unwrap();}}}
 let copy_id=composition.metadata.as_ref().unwrap().media_versions.values().next().unwrap().copy_source_id.clone();
 let entry=state.library.get(copy_id.as_str()).await.unwrap();let copy=state.library.resolve_media_path(&entry).await.unwrap();let rgba=image::open(copy).unwrap().to_rgba8();let source_frame=GroupSurface{width:rgba.width()as usize,height:rgba.height()as usize,rgba:rgba.into_raw()};
 let mut requested=vec![];let mut decoder=|id:&video_kadr_backend::domain::composition::SourceId|{requested.push(id.clone());assert_eq!(id,&copy_id);Ok(source_frame.clone())};
 let expected=render_media_layers(&composition,0,&mut||Ok(()),Some(&mut decoder),None).unwrap().unwrap();assert_eq!(requested,[copy_id]);
 let raw_id=&composition.metadata.as_ref().unwrap().media_versions.values().next().unwrap().original_source_id;let raw_path=state.library.resolve_media_path(&state.library.get(raw_id.as_str()).await.unwrap()).await.unwrap();let raw=image::open(raw_path).unwrap().to_rgba8();let raw_frame=GroupSurface{width:raw.width()as usize,height:raw.height()as usize,rgba:raw.into_raw()};
 let mut wrong_decoder=|_:&video_kadr_backend::domain::composition::SourceId|Ok(raw_frame.clone());let uncut=render_media_layers(&composition,0,&mut||Ok(()),Some(&mut wrong_decoder),None).unwrap().unwrap();assert_ne!(uncut.rgba,expected.rgba,"the control must detect accidental original-source export");

 let mut export_state=state.clone();let caps=std::sync::Arc::make_mut(&mut export_state.tools);caps.ffmpeg_encoders.push("ffv1".into());caps.ffmpeg_muxers.push("matroska".into());
 let app=support::router(export_state);let project=serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":composition,"instances":[]}}});
 let request=axum::http::Request::builder().method("POST").uri("/api/compositions/nested-export").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(axum::body::Body::from(serde_json::json!({"schemaVersion":1,"mode":"video_only","project":project,"compositionId":"root","startUs":0,"endUs":100000}).to_string())).unwrap();
 let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,axum::http::StatusCode::ACCEPTED,"{accepted}");let id=accepted["jobId"].as_str().unwrap();
 let job=tokio::time::timeout(std::time::Duration::from_secs(120),async{loop{let job=state.get_job(id).await.unwrap().unwrap();if matches!(job.status,video_kadr_backend::model::JobStatus::Done|video_kadr_backend::model::JobStatus::Error|video_kadr_backend::model::JobStatus::Cancelled){break job}tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.unwrap();assert_eq!(job.status,video_kadr_backend::model::JobStatus::Done,"{:?}",job.error);
 let path=state.outputs_dir().join(job.result.as_ref().unwrap()["filename"].as_str().unwrap());
 let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(path).args(["-map","0:v:0","-frames:v","1","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));assert_eq!(decoded.stdout,expected.rgba,"encoded cutout/effect frame differs from checked native composition");
}
