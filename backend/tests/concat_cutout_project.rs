use serde_json::json;
use std::collections::BTreeMap;
use video_kadr_backend::domain::{concat_project_import::{ImportRequest,ReadyStillBinding,StillBindings,import,import_with_still_bindings},concat_cutout_binding::StillAssets,composition::{CompositionSource,CompositionTrack,CompositionClipId},cutout::SELFIE_MODEL_HASH};
fn source(id:&str,size:u32)->CompositionSource{serde_json::from_value(json!({"id":id,"kind":"image","durationTicks":1000000,"width":size,"height":size,"hasAudio":false})).unwrap()}
fn document()->serde_json::Value{
 let timeline=|id:&str|json!({"id":id,"video":{"width":2,"height":2,"rateNum":24,"rateDen":1},"tracks":[{"id":"track"}],"clips":[{"id":"same","trackId":"track","mediaId":"media","kind":"image","duration":1,"cutout":{"mode":"auto","feather":0}}]});
 json!({"version":1,"media":[{"id":"media","kind":"image","name":"original.png"}],"activeTimelineId":"a","timelines":[timeline("a"),timeline("b")]})
}
fn request(doc:serde_json::Value)->ImportRequest{serde_json::from_value(json!({"schemaVersion":1,"document":doc,"mediaBindings":{"media":source("raw",2)}})).unwrap()}
fn ready()->StillBindings{
 ["a","b"].into_iter().map(|timeline|{
 let snapshot=serde_json::from_value(json!({"sourceId":"raw","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":SELFIE_MODEL_HASH,"runtimeHash":"d".repeat(64),"correction":{"custom":false,"strokes":[],"feather":0}})).unwrap();
 (timeline.into(),BTreeMap::from([("same".into(),ReadyStillBinding{source_settings:video_kadr_backend::domain::concat_cutout_import::plan(&json!({"mode":"auto","feather":0})).unwrap(),assets:StillAssets{original:source("raw",2),copy:source(&format!("copy-{timeline}"),2),mask:source("mask",256),regions:vec![],snapshot},source_hash:"a".repeat(64),model_hash:SELFIE_MODEL_HASH.into(),runtime_hash:"d".repeat(64)})]))
 }).collect()
}
#[test]
fn ready_image_cutouts_convert_whole_project_with_timeline_scoped_bindings(){
 let original=document();let report=import_with_still_bindings(request(original.clone()),&ready()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);assert_eq!(report.source_document,original);
 let project=report.project.unwrap();for(index,name)in ["a","b"].into_iter().enumerate(){let c=&project.compositions[&format!("concat-timeline-{index}")].content;let CompositionTrack::Image{clips,..}=&c.tracks[0]else{panic!()};assert_eq!(clips[0].source_id.as_str(),format!("copy-{name}"));assert!(c.metadata.as_ref().unwrap().media_versions.contains_key(&CompositionClipId::parse("same").unwrap()));c.validate().unwrap();}
}
#[test]
fn unresolved_stale_or_mismatched_bindings_never_return_partial_projects(){
 let original=document();assert_eq!(import(request(original.clone())).unwrap().status,"needs_attention");
 for mode in 0..6{let mut bindings=ready();match mode{0=>{bindings.remove("b");},1=>bindings.get_mut("a").unwrap().get_mut("same").unwrap().assets.snapshot.correction.feather=0.01,2=>{bindings.insert("unknown".into(),bindings["a"].clone());},3=>bindings.get_mut("a").unwrap().get_mut("same").unwrap().source_hash="e".repeat(64),4=>bindings.get_mut("a").unwrap().get_mut("same").unwrap().source_settings.subject=video_kadr_backend::domain::concat_cutout_import::Subject::Object,_=>{bindings.insert("unknown".into(),BTreeMap::new());}}
  let report=import_with_still_bindings(request(original.clone()),&bindings).unwrap();assert_eq!(report.status,"needs_attention","mode{mode}");assert!(report.project.is_none());assert_eq!(report.source_document,original);
 }
}
#[test]
fn ready_cutouts_preserve_logical_originals_and_isolate_colliding_asset_ids(){
 let mut original=document();
 original["media"].as_array_mut().unwrap().extend([json!({"id":"copy-a","kind":"image"}),json!({"id":"mask","kind":"image"})]);
 let mut request=request(original.clone());request.preserve_media_ids=true;
 request.media_bindings.insert("copy-a".into(),source("unrelated-copy",2));request.media_bindings.insert("mask".into(),source("unrelated-mask",2));
 let report=import_with_still_bindings(request,&ready()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);assert_eq!(report.source_document,original);
 let project=report.project.unwrap();project.dependency_order().unwrap();
 let media=video_kadr_backend::domain::composition::SourceId::parse("media").unwrap();assert_eq!(project.project_media[&media].library_source_id.as_ref().unwrap().as_str(),"raw");
 for(index,name)in ["a","b"].into_iter().enumerate(){
  let c=&project.compositions[&format!("concat-timeline-{index}")].content;
  let pair=&c.metadata.as_ref().unwrap().media_versions[&CompositionClipId::parse("same").unwrap()];let snapshot=pair.cutout.as_ref().unwrap();
  assert_eq!(pair.original_source_id,media);assert_eq!(snapshot.source_id,"media");assert_eq!(snapshot.source_hash,"a".repeat(64));
  assert_ne!(snapshot.mask_source_id,"mask");assert_eq!(project.library_source_id(&video_kadr_backend::domain::composition::SourceId::parse(&snapshot.mask_source_id).unwrap()).as_str(),"mask");
  assert_eq!(project.library_source_id(&pair.copy_source_id).as_str(),format!("copy-{name}"));
  assert_eq!(project.library_source_id(&video_kadr_backend::domain::composition::SourceId::parse("mask").unwrap()).as_str(),"unrelated-mask");
 }
 let sources=project.compositions.values().flat_map(|node|node.content.sources.keys()).chain(project.project_media.keys()).map(|id|project.library_source_id(id).as_str().to_owned()).collect::<std::collections::BTreeSet<_>>();
 for id in ["raw","copy-a","copy-b","mask","unrelated-copy","unrelated-mask"]{assert!(sources.iter().any(|s|s==id),"missing physical {id}");}
 assert!(!sources.iter().any(|s|s=="media"||s.starts_with("concat-cutout-")));
}
#[test]
fn logical_ready_import_refuses_an_original_from_another_library_file(){
 let mut request=request(document());request.preserve_media_ids=true;request.media_bindings.insert("media".into(),source("different",2));
 let report=import_with_still_bindings(request,&ready()).unwrap();assert_eq!(report.status,"needs_attention");assert!(report.project.is_none());
}
#[test]
fn logical_ready_import_rebinds_nested_source_owned_regions_and_their_collisions(){
 use sha2::Digest;let prompt=format!("{:x}",sha2::Sha256::digest(serde_json::to_vec(&vec![[0.5f64,0.5]]).unwrap()));
 let mut doc=document();let mut bindings=ready();
 let cutout=json!({"mode":"custom","feather":0,"strokes":[{"tool":"smartBrush","size":0.1,"points":[[0.5,0.5]],"at":0}]});
 for timeline in doc["timelines"].as_array_mut().unwrap(){timeline["clips"][0]["cutout"]=cutout.clone();}
 for clips in bindings.values_mut(){let binding=clips.get_mut("same").unwrap();binding.source_settings=video_kadr_backend::domain::concat_cutout_import::plan(&cutout).unwrap();binding.assets.snapshot.correction=binding.source_settings.correction.clone();
  binding.assets.snapshot.correction.strokes[0].region=Some(serde_json::from_value(json!({"sourceId":"raw","maskSourceId":"region","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"encoderHash":"d".repeat(64),"decoderHash":"e".repeat(64),"runtimeHash":"f".repeat(64),"promptHash":prompt,"points":[[0.5,0.5]]})).unwrap());binding.assets.regions=vec![source("region",256)];
 }
 doc["media"].as_array_mut().unwrap().push(json!({"id":"region","kind":"image"}));
 let mut request=request(doc);request.preserve_media_ids=true;request.media_bindings.insert("region".into(),source("unrelated-region",2));
 let report=import_with_still_bindings(request,&bindings).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);let project=report.project.unwrap();project.dependency_order().unwrap();
 for node in project.compositions.values(){let pair=&node.content.metadata.as_ref().unwrap().media_versions[&CompositionClipId::parse("same").unwrap()];let snapshot=pair.cutout.as_ref().unwrap();let region=snapshot.correction.strokes[0].region.as_ref().unwrap();assert_eq!(region.source_id,"media");assert_ne!(region.mask_source_id,"region");assert_eq!(project.library_source_id(&video_kadr_backend::domain::composition::SourceId::parse(&region.mask_source_id).unwrap()).as_str(),"region");}
}
#[test]
fn logical_ready_import_keeps_physical_asset_identity_checks_before_rebinding(){
 for mode in 0..3{let mut bindings=ready();let binding=bindings.get_mut("a").unwrap().get_mut("same").unwrap();match mode{0=>binding.assets.snapshot.source_id="other".into(),1=>binding.assets.copy=binding.assets.original.clone(),_=>binding.assets.snapshot.mask_source_id="other".into()};let mut request=request(document());request.preserve_media_ids=true;assert_eq!(import_with_still_bindings(request,&bindings).unwrap().status,"needs_attention");}
}
