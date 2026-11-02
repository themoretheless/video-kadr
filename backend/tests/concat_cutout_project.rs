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
