use serde_json::json;
use video_kadr_backend::domain::{concat_cutout_binding::{bind_still,StillAssets},concat_cutout_import::plan,composition::{Composition,CanvasSpec,TransformSpec,CompositionClipId,CompositionSource},cutout::SELFIE_MODEL_HASH,flat_surface::render_media_layers,group_surface::GroupSurface};
fn source(id:&str,width:u32,height:u32)->CompositionSource{serde_json::from_value(json!({"id":id,"kind":"image","durationTicks":1000000,"width":width,"height":height,"hasAudio":false})).unwrap()}
fn setup()->(Composition,StillAssets){
 let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;let mut c=Composition::new(canvas);let raw=source("raw",2,2);c.sources.insert(raw.id.clone(),raw.clone());
 c.tracks=serde_json::from_value(json!([{"kind":"image","id":"image","name":"Image","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{}}]}]}])).unwrap();
 let snapshot=serde_json::from_value(json!({"sourceId":"raw","maskSourceId":"mask","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"modelHash":SELFIE_MODEL_HASH,"runtimeHash":"d".repeat(64),"correction":{"custom":false,"strokes":[],"feather":0}})).unwrap();
 (c,StillAssets{original:raw,copy:source("copy",2,2),mask:source("mask",256,256),regions:vec![],snapshot})
}
#[test]
fn binds_alpha_copy_before_effects_and_preserves_original_recipe_and_source(){
 let(mut c,assets)=setup();c.metadata.get_or_insert_with(Default::default).source_names.insert(assets.original.id.clone(),"portrait.png".into());let before=c.clone();let p=plan(&json!({"mode":"auto","feather":0})).unwrap();let id=CompositionClipId::parse("clip").unwrap();
 let changed=bind_still(&c,&id,&p,&assets,&"a".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).unwrap();assert_eq!(c,before);assert_eq!(changed.metadata.as_ref().unwrap().media_versions[&id].original_source_id,assets.original.id);
 let mut requested=vec![];let mut decode=|source:&video_kadr_backend::domain::composition::SourceId|{requested.push(source.as_str().to_string());Ok(GroupSurface{width:2,height:2,rgba:[255,0,0,255].repeat(4)})};
 let frame=render_media_layers(&changed,0,&mut||Ok(()),Some(&mut decode),None).unwrap().unwrap();assert_eq!(requested,["copy"]);assert_eq!(frame.rgba,[0,255,255,255].repeat(4));
 assert_eq!(changed.metadata.as_ref().unwrap().source_names[&assets.copy.id],"portrait.png");
 assert_eq!(bind_still(&changed,&id,&p,&assets,&"a".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).unwrap(),changed);
 let _:Composition=serde_json::from_value(serde_json::to_value(&changed).unwrap()).unwrap();
}
#[test]
fn rejects_stale_identity_recipe_and_source_collisions_transactionally(){
 let(c,assets)=setup();let p=plan(&json!({"mode":"auto","feather":0})).unwrap();let id=CompositionClipId::parse("clip").unwrap();let before=c.clone();
 assert!(bind_still(&c,&id,&p,&assets,&"e".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).is_err());
 let other=plan(&json!({"mode":"auto","feather":0.01})).unwrap();assert!(bind_still(&c,&id,&other,&assets,&"a".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).is_err());
 let mut collision=c.clone();let mut wrong=assets.copy.clone();wrong.width=3;collision.sources.insert(wrong.id.clone(),wrong);assert!(bind_still(&collision,&id,&p,&assets,&"a".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).is_err());assert_eq!(c,before);
}
#[test]
fn saved_pair_extraction_requires_matching_timeline_clip_and_native_document(){
 use video_kadr_backend::domain::concat_cutout_binding::saved_still;
 let(c,assets)=setup();let p=plan(&json!({"mode":"auto","feather":0})).unwrap();let id=CompositionClipId::parse("clip").unwrap();
 let bound=bind_still(&c,&id,&p,&assets,&"a".repeat(64),SELFIE_MODEL_HASH,&"d".repeat(64)).unwrap();
 let plain=serde_json::to_value(&bound).unwrap();
 assert_eq!(saved_still(&plain,None,&id).unwrap().snapshot,assets.snapshot);
 assert!(saved_still(&plain,Some("a"),&id).is_err());
 let nested=json!({"compositions":{"a":{"content":plain}}});
 assert!(saved_still(&nested,None,&id).is_err());
 assert!(saved_still(&nested,Some("missing"),&id).is_err());
 assert!(saved_still(&nested,Some("a"),&CompositionClipId::parse("other").unwrap()).is_err());
 assert_eq!(saved_still(&nested,Some("a"),&id).unwrap().copy.id,assets.copy.id);
 let mut stale=nested.clone();stale["compositions"]["a"]["content"]["metadata"]["mediaVersions"]["clip"]["cutout"]["sourceId"]=json!("copy");
 assert!(saved_still(&stale,Some("a"),&id).is_err());
}

#[test]
fn rvm_saved_binding_preserves_rectangular_grid_and_refuses_square_substitution(){
 use video_kadr_backend::domain::cutout::{PERSON_MODEL_HASH,mask_dimensions};
 assert_eq!(mask_dimensions(PERSON_MODEL_HASH,37,23).unwrap(),(512,320));
 assert_eq!(mask_dimensions(PERSON_MODEL_HASH,23,37).unwrap(),(320,512));
 let(mut c,mut assets)=setup();
 assets.original.width=4;assets.copy.width=4;assets.mask.width=512;assets.mask.height=256;assets.snapshot.model_hash=PERSON_MODEL_HASH.into();
 c.sources.insert(assets.original.id.clone(),assets.original.clone());
 let p=plan(&json!({"mode":"auto","feather":0})).unwrap();let id=CompositionClipId::parse("clip").unwrap();
 let bound=bind_still(&c,&id,&p,&assets,&assets.snapshot.source_hash,PERSON_MODEL_HASH,&assets.snapshot.runtime_hash).unwrap();
 assert_eq!(bound.sources[&assets.mask.id].height,256);
 let document=serde_json::to_value(bound).unwrap();assert_eq!(video_kadr_backend::domain::concat_cutout_binding::saved_still(&document,None,&id).unwrap().mask.height,256);
 assets.mask.height=512;assert!(bind_still(&c,&id,&p,&assets,&assets.snapshot.source_hash,PERSON_MODEL_HASH,&assets.snapshot.runtime_hash).is_err());
}

#[test]
fn captured_subject_roundtrips_and_prevents_using_another_subject_plan(){
 use video_kadr_backend::domain::concat_cutout_import::Subject;
 let(c,mut assets)=setup();assets.snapshot.subject=Some(Subject::Person);
 let plain=plan(&json!({"mode":"auto","feather":0})).unwrap();let person=plan(&json!({"subject":"person","mode":"auto","feather":0})).unwrap();let id=CompositionClipId::parse("clip").unwrap();
 assert!(bind_still(&c,&id,&plain,&assets,&assets.snapshot.source_hash,SELFIE_MODEL_HASH,&assets.snapshot.runtime_hash).is_err());
 let changed=bind_still(&c,&id,&person,&assets,&assets.snapshot.source_hash,SELFIE_MODEL_HASH,&assets.snapshot.runtime_hash).unwrap();
 let document=serde_json::to_value(&changed).unwrap();assert_eq!(document["metadata"]["mediaVersions"]["clip"]["cutout"]["subject"],"person");
 assert_eq!(video_kadr_backend::domain::concat_cutout_binding::saved_still(&document,None,&id).unwrap().snapshot.subject,Some(Subject::Person));
 let mut bad=serde_json::to_value(&assets.snapshot).unwrap();bad["subject"]=json!("unknown");assert!(serde_json::from_value::<video_kadr_backend::domain::cutout::Attachment>(bad).is_err());
}

#[test]
fn reanalysis_uses_original_and_keeps_old_copy_and_mask_without_mutating_input(){
 use video_kadr_backend::domain::{concat_cutout_binding::rebind_still,concat_cutout_import::Subject,composition::SourceId};
 let(c,first)=setup();let id=CompositionClipId::parse("clip").unwrap();let p=plan(&json!({"mode":"auto","feather":0})).unwrap();
 let old=bind_still(&c,&id,&p,&first,&first.snapshot.source_hash,SELFIE_MODEL_HASH,&first.snapshot.runtime_hash).unwrap();let before=old.clone();
 let mut next=first.clone();next.copy.id=SourceId::parse("new-copy").unwrap();next.mask.id=SourceId::parse("new-mask").unwrap();next.snapshot.mask_source_id="new-mask".into();next.snapshot.subject=Some(Subject::Person);
 let person=plan(&json!({"subject":"person","mode":"auto","feather":0})).unwrap();
 let changed=rebind_still(&old,&id,&person,&next,&next.snapshot.source_hash,SELFIE_MODEL_HASH,&next.snapshot.runtime_hash).unwrap();
 assert_eq!(old,before);assert!(changed.sources.contains_key(&first.copy.id)&&changed.sources.contains_key(&first.mask.id));assert_eq!(changed.metadata.as_ref().unwrap().media_versions[&id].copy_source_id,next.copy.id);
 let mut wrong=next.clone();wrong.original.id=SourceId::parse("other").unwrap();assert!(rebind_still(&old,&id,&person,&wrong,&wrong.snapshot.source_hash,SELFIE_MODEL_HASH,&wrong.snapshot.runtime_hash).is_err());
}

#[test]
fn saved_regions_require_exact_stroke_match_and_keep_legacy_confidence_fallback(){
 use sha2::{Digest,Sha256};use video_kadr_backend::domain::{concat_cutout_binding::saved_region_correction,cutout::RegionAttachment};
 let(_,mut assets)=setup();let p=plan(&json!({"mode":"custom","strokes":[{"tool":"smartBrush","size":0.2,"points":[[0.25,0.5]],"at":0.25}]})).unwrap();
 let points=p.correction.strokes[0].points.clone();let region=RegionAttachment{source_id:"raw".into(),mask_source_id:"region".into(),source_hash:assets.snapshot.source_hash.clone(),mask_hash:"b".repeat(64),mask_file_hash:"c".repeat(64),encoder_hash:"d".repeat(64),decoder_hash:"e".repeat(64),runtime_hash:"f".repeat(64),prompt_hash:format!("{:x}",Sha256::digest(serde_json::to_vec(&points).unwrap())),points};
 assets.snapshot.correction=p.correction.clone();assets.snapshot.correction.strokes[0].region=Some(region.clone());
 assert_eq!(saved_region_correction(Some(&assets.snapshot),&p).unwrap().strokes[0].region,Some(region));
 let mut changed=p.clone();changed.correction.strokes[0].points[0][0]=0.75;assert!(saved_region_correction(Some(&assets.snapshot),&changed).is_err());assert!(saved_region_correction(None,&p).is_err());
 let legacy=plan(&json!({"mode":"custom","strokes":[{"tool":"smartBrush","size":0.2,"points":[[0.25,0.5]]}]})).unwrap();assert!(saved_region_correction(None,&legacy).unwrap().strokes[0].region.is_none());
}
