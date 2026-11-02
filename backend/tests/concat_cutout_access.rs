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
#[tokio::test]
async fn saved_assets_require_editor_current_revision_and_accessible_library_sources(){
 use video_kadr_backend::{handlers::concat_cutout_access::{resolve_saved_still,verify_saved_still},library::MediaEntry,db::SpaceRole};
 let(state,_dir)=support::make_state(true,false).await;
 let(c,mut assets)=setup();let id=CompositionClipId::parse("clip").unwrap();
 let p=plan(&json!({"mode":"auto","feather":0})).unwrap();
 let space=state.db.create_space("owner","Saved cutout").await.unwrap();
 let ids=vec!["copy".to_owned(),"mask".to_owned(),"raw".to_owned()];
 for source in &ids{
  let filename=format!("{source}.png");
  image::RgbaImage::new(2,2).save(state.sources_dir().join(&filename)).unwrap();
  assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":source,"filename":filename,"url":format!("/files/sources/{filename}")}))).await);
 }
 use sha2::{Digest,Sha256};
 let hash=|bytes:&[u8]|format!("{:x}",Sha256::digest(bytes));
 image::GrayImage::new(256,256).save(state.sources_dir().join("mask.png")).unwrap();
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(state.sources_dir().join("mask.png"),std::fs::Permissions::from_mode(0o600)).unwrap();}
 assets.snapshot.source_hash=hash(&std::fs::read(state.sources_dir().join("raw.png")).unwrap());
 assets.snapshot.mask_hash=hash(&vec![0;256*256]);
 assets.snapshot.mask_file_hash=hash(&std::fs::read(state.sources_dir().join("mask.png")).unwrap());
 let bound=bind_still(&c,&id,&p,&assets,&assets.snapshot.source_hash,SELFIE_MODEL_HASH,&assets.snapshot.runtime_hash).unwrap();
 let document=serde_json::to_value(bound).unwrap();
 let project=state.db.create_space_composition_project("Saved",&document,&ids,"owner",&space.id).await.unwrap();
 for (actor,role) in [("editor",SpaceRole::Editor),("viewer",SpaceRole::Viewer)]{
  state.db.set_space_member(&space.id,"owner",actor,role).await.unwrap().unwrap();
 }
 for actor in ["owner","editor"]{
  let candidate=resolve_saved_still(&state,actor,&project.id,None,&id,project.revision).await.unwrap();
  assert_eq!(candidate.paths.len(),3);assert_eq!(candidate.revision,project.revision);
  assert_eq!(candidate.assets.snapshot,assets.snapshot);
  verify_saved_still(&state,actor,&project.id,None,&id,project.revision,&p,&tokio_util::sync::CancellationToken::new()).await.unwrap();
 }
 for actor in ["viewer","outsider"]{assert!(resolve_saved_still(&state,actor,&project.id,None,&id,project.revision).await.is_err());}
 assert!(resolve_saved_still(&state,"owner",&project.id,None,&id,project.revision+1).await.is_err());
 state.db.remove_space_member(&space.id,"owner","editor").await.unwrap();
 assert!(resolve_saved_still(&state,"editor",&project.id,None,&id,project.revision).await.is_err());
 // A registered copy whose pixels change must be refused by the combined path.
 image::RgbaImage::from_pixel(2,2,image::Rgba([1,0,0,0])).save(state.sources_dir().join("copy.png")).unwrap();
 assert!(verify_saved_still(&state,"owner",&project.id,None,&id,project.revision,&p,&tokio_util::sync::CancellationToken::new()).await.is_err());
 // File resolution is mandatory even when the saved native descriptor is valid.
 std::fs::remove_file(state.sources_dir().join("mask.png")).unwrap();
 assert!(resolve_saved_still(&state,"owner",&project.id,None,&id,project.revision).await.is_err());
}
