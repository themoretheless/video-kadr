use sha2::{Digest,Sha256};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{services::{concat_cutout_asset::{load_still,ExpectedIdentity},local_cutout::{Metadata,read_verified_mask}},domain::{cutout::SELFIE_MODEL_HASH,concat_cutout_import::plan,concat_cutout_resolve::resolve,concat_cutout_mask::{apply,MaskPlane},concat_picture::Preparation,group_surface::GroupSurface}};
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn fixture()->(tempfile::TempDir,std::path::PathBuf,Metadata,Vec<u8>){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("verified.png");
 let pixels:Vec<u8>=(0..256).flat_map(|_|(0..256).map(|x|if x<128{0}else{255})).collect();image::GrayImage::from_raw(256,256,pixels.clone()).unwrap().save(&path).unwrap();
 let metadata=Metadata{schema_version:6,width:2,height:1,mask_width:256,mask_height:256,blank_mask:false,mask_hash:hash(&pixels),source_hash:hash(b"source-fixture"),model_hash:SELFIE_MODEL_HASH.into(),runtime_hash:hash(b"runtime-fixture"),output_hash:hash(b"alpha-output"),mask_file_hash:Some(hash(&std::fs::read(&path).unwrap()))};(dir,path,metadata,pixels)
}
fn expected(m:&Metadata)->ExpectedIdentity<'_>{ExpectedIdentity{source_hash:&m.source_hash,model_hash:&m.model_hash,runtime_hash:&m.runtime_hash}}
#[tokio::test]
async fn verified_png_probabilities_enter_correction_and_source_alpha_pipeline(){
 let(_dir,path,metadata,pixels)=fixture();let cancel=CancellationToken::new();let automatic=load_still(&path,&metadata,expected(&metadata),&cancel).await.unwrap();assert_eq!(automatic.probabilities,pixels);
 let encoded=read_verified_mask(&path,&metadata).await.unwrap();assert_eq!(encoded,std::fs::read(&path).unwrap());
 let p=plan(&serde_json::json!({"mode":"auto","feather":0})).unwrap();let corrected=resolve(&p,&automatic,&[],2.,&std::sync::atomic::AtomicBool::new(false)).unwrap().unwrap();
 let mut frame=GroupSurface{width:2,height:1,rgba:[99,88,77,255].repeat(2)};
 apply(&mut frame,MaskPlane{width:corrected.width,height:corrected.height,probabilities:&corrected.probabilities},&Preparation{crop:[0.;4],canvas:[2,1],flip_h:false,flip_v:false},&mut||Ok(())).unwrap();assert_eq!(frame.rgba,[99,88,77,0,99,88,77,255]);
}
#[tokio::test]
async fn identity_raw_digest_blank_flag_and_encoded_tampering_refuse(){
 let(_dir,path,metadata,_)=fixture();let cancel=CancellationToken::new();
 for field in [0,1,2]{let mut wrong=metadata.clone();match field{0=>wrong.source_hash=hash(b"other-source"),1=>wrong.model_hash=hash(b"other-model"),_=>wrong.runtime_hash=hash(b"other-runtime")};assert!(load_still(&path,&metadata,expected(&wrong),&cancel).await.is_err());}
 let mut wrong=metadata.clone();wrong.mask_hash=hash(b"other-probabilities");assert!(load_still(&path,&wrong,expected(&metadata),&cancel).await.is_err());
 let mut wrong=metadata.clone();wrong.blank_mask=true;assert!(load_still(&path,&wrong,expected(&metadata),&cancel).await.is_err());
 cancel.cancel();assert!(load_still(&path,&metadata,expected(&metadata),&cancel).await.is_err());
 std::fs::write(&path,b"tampered").unwrap();assert!(load_still(&path,&metadata,expected(&metadata),&CancellationToken::new()).await.is_err());
}

#[tokio::test]
#[ignore="requires the verified native Selfie model, executable and ONNX Runtime"]
async fn actual_selfie_mask_enters_verified_correction_and_alpha_pipeline(){
 use video_kadr_backend::{process_control::ProcessRuntime,services::{local_selfie,local_speech::fingerprint_model}};
 use std::time::Duration;
 let config=local_selfie::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL").unwrap().into(),runtime_library:std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into()};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.png");let mask=dir.path().join("automatic.png");let output=dir.path().join("cut.png");
 let image=image::RgbaImage::from_fn(37,23,|x,y|{let n=y*37+x;image::Rgba([(n%151)as u8,((n*17)%180)as u8,((n*31)%199)as u8,(n%256)as u8])});image.save(&source).unwrap();
 let source_hash=fingerprint_model(&source).await.unwrap().1;let cancel=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
 let produced=local_selfie::cutout(&ProcessRuntime::local_default(),&config,local_selfie::Input{source:&source,source_hash:&source_hash,mask_destination:Some(&mask)},&output,&cancel,Duration::from_secs(60),&tx).await.unwrap();
 let metadata:Metadata=serde_json::from_value(serde_json::to_value(&produced).unwrap()).unwrap();assert_eq!(metadata.model_hash,SELFIE_MODEL_HASH);assert_eq!(metadata.source_hash,source_hash);assert!(metadata.blank_mask);
 let automatic=load_still(&mask,&metadata,expected(&metadata),&cancel).await.unwrap();
 let auto=plan(&serde_json::json!({"mode":"auto","feather":0})).unwrap();assert!(resolve(&auto,&automatic,&[],37./23.,&std::sync::atomic::AtomicBool::new(false)).unwrap().is_none());
 let custom=plan(&serde_json::json!({"mode":"custom","feather":0,"strokes":[{"tool":"brush","size":0.5,"points":[[0.5,0.5]]}]})).unwrap();
 let corrected=resolve(&custom,&automatic,&[],37./23.,&std::sync::atomic::AtomicBool::new(false)).unwrap().unwrap();assert!(corrected.probabilities.contains(&255));
 let original=image.into_raw();let mut frame=GroupSurface{width:37,height:23,rgba:original.clone()};
 apply(&mut frame,MaskPlane{width:corrected.width,height:corrected.height,probabilities:&corrected.probabilities},&Preparation{crop:[0.;4],canvas:[37,23],flip_h:false,flip_v:false},&mut||Ok(())).unwrap();
 assert!(frame.rgba.chunks_exact(4).zip(original.chunks_exact(4)).all(|(a,b)|a[..3]==b[..3]&&a[3]<=b[3]));
 assert!(frame.rgba.chunks_exact(4).zip(original.chunks_exact(4)).any(|(a,b)|b[3]>0&&a[3]<b[3]));
 assert!(frame.rgba.chunks_exact(4).zip(original.chunks_exact(4)).any(|(a,b)|b[3]>0&&a[3]==b[3]));
 // Produce the same alpha copy through the existing real correction process,
 // then bind its registered descriptors into the native image renderer.
 use video_kadr_backend::{services::local_mask_edit,domain::{concat_cutout_binding::{StillAssets,bind_still},composition::{CanvasSpec,Composition,CompositionSource,CompositionClipId,TransformSpec,SourceId},flat_surface::render_media_layers}};
 let corrected_path=dir.path().join("corrected.png");
 local_mask_edit::correct(&ProcessRuntime::local_default(),&config.executable,local_mask_edit::Input{source:&source,mask:&mask,original:&metadata,correction:&custom.correction},&corrected_path,&cancel,Duration::from_secs(60),&tx).await.unwrap();
 let copied=image::open(&corrected_path).unwrap().into_rgba8().into_raw();assert_eq!(copied,frame.rgba);
 video_kadr_backend::services::concat_cutout_asset::verify_copy_pixels(&GroupSurface{width:37,height:23,rgba:original.clone()},&GroupSurface{width:37,height:23,rgba:copied.clone()},Some(&corrected),&cancel).unwrap();
 let descriptor=|id:&str,width:u32,height:u32|->CompositionSource{serde_json::from_value(serde_json::json!({"id":id,"kind":"image","durationTicks":1000000,"width":width,"height":height,"hasAudio":false})).unwrap()};
 let raw=descriptor("raw",37,23);let copy=descriptor("copy",37,23);let mask_source=descriptor("mask",256,256);
 let snapshot=video_kadr_backend::domain::cutout::Attachment{subject:None,source_id:"raw".into(),mask_source_id:"mask".into(),source_hash:metadata.source_hash.clone(),mask_hash:metadata.mask_hash.clone(),mask_file_hash:metadata.mask_file_hash.clone().unwrap(),model_hash:metadata.model_hash.clone(),runtime_hash:metadata.runtime_hash.clone(),correction:custom.correction.clone()};
 let assets=StillAssets{original:raw.clone(),copy,mask:mask_source,regions:vec![],snapshot};
 video_kadr_backend::services::concat_cutout_asset::verify_still_bundle(&source,&corrected_path,&mask,&assets,&custom,expected(&metadata),&cancel).await.unwrap();
 let good_copy=std::fs::read(&corrected_path).unwrap();
 let mut tampered=image::open(&corrected_path).unwrap().into_rgba8();tampered.get_pixel_mut(0,0).0[0]^=1;tampered.save(&corrected_path).unwrap();
 assert!(video_kadr_backend::services::concat_cutout_asset::verify_still_bundle(&source,&corrected_path,&mask,&assets,&custom,expected(&metadata),&cancel).await.is_err());
 std::fs::write(&corrected_path,good_copy).unwrap();
 let mut wrong_recipe=custom.clone();wrong_recipe.correction.feather=0.01;
 assert!(video_kadr_backend::services::concat_cutout_asset::verify_still_bundle(&source,&corrected_path,&mask,&assets,&wrong_recipe,expected(&metadata),&cancel).await.is_err());

 let mut canvas=CanvasSpec::default();canvas.width=40;canvas.height=24;let mut composition=Composition::new(canvas);composition.sources.insert(raw.id.clone(),raw);
 let mut transform=TransformSpec::default();transform.concat_preparation=Some(Preparation{crop:[0.;4],canvas:[40,24],flip_h:false,flip_v:false});
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"image","name":"Image","clips":[{"id":"clip","sourceId":"raw","timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}])).unwrap();
 let bound=bind_still(&composition,&CompositionClipId::parse("clip").unwrap(),&custom,&assets,&metadata.source_hash,&metadata.model_hash,&metadata.runtime_hash).unwrap();
 let mut requested=vec![];let mut decoder=|source:&SourceId|{requested.push(source.as_str().to_string());assert_eq!(source.as_str(),"copy");Ok(GroupSurface{width:37,height:23,rgba:copied.clone()})};
 let rendered=render_media_layers(&bound,0,&mut||Ok(()),Some(&mut decoder),None).unwrap().unwrap();assert_eq!(requested,["copy"]);assert_eq!(&rendered.rgba[4..8],&[0,0,0,255]);assert!(rendered.rgba[(12*40+20)*4..(12*40+20)*4+3].iter().any(|value|*value>0));

}

#[test]
fn saved_copy_pixel_verification_rejects_changed_alpha_rgb_size_and_cancel(){
 use video_kadr_backend::{services::concat_cutout_asset::verify_copy_pixels,domain::concat_cutout_clock::ResolvedMask};
 let original=GroupSurface{width:2,height:2,rgba:vec![13,21,34,255,55,89,144,200,9,8,7,0,1,2,3,100]};
 let mask=ResolvedMask{width:2,height:2,probabilities:vec![0,255,128,64]};
 let mut copy=original.clone();let cancel=CancellationToken::new();
 apply(&mut copy,MaskPlane{width:2,height:2,probabilities:&mask.probabilities},&Preparation{crop:[0.;4],canvas:[2,2],flip_h:false,flip_v:false},&mut||Ok(())).unwrap();
 verify_copy_pixels(&original,&copy,Some(&mask),&cancel).unwrap();
 verify_copy_pixels(&original,&original,None,&cancel).unwrap();
 assert!(verify_copy_pixels(&original,&copy,None,&cancel).is_err());
 for channel in [0,3,8]{let mut wrong=copy.clone();wrong.rgba[channel]^=1;assert!(verify_copy_pixels(&original,&wrong,Some(&mask),&cancel).is_err());}
 let mut wrong=copy.clone();wrong.width=1;wrong.height=4;assert!(verify_copy_pixels(&original,&wrong,Some(&mask),&cancel).is_err());
 cancel.cancel();assert!(verify_copy_pixels(&original,&copy,Some(&mask),&cancel).is_err());
}

#[tokio::test]
async fn verified_image_loader_uses_encoded_identity_and_preserves_hidden_rgb(){
 use video_kadr_backend::services::concat_cutout_asset::load_verified_image;
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("source.png");
 let pixels=vec![13,21,34,0,55,89,144,200];
 image::RgbaImage::from_raw(2,1,pixels.clone()).unwrap().save(&path).unwrap();
 let digest=hash(&std::fs::read(&path).unwrap());let cancel=CancellationToken::new();
 let frame=load_verified_image(&path,Some(&digest),&cancel).await.unwrap();assert_eq!(frame.rgba,pixels);
 assert!(load_verified_image(&path,Some(&hash(b"wrong")),&cancel).await.is_err());
 assert!(load_verified_image(std::path::Path::new("source.png"),None,&cancel).await.is_err());
 std::fs::write(&path,b"not an image").unwrap();assert!(load_verified_image(&path,None,&cancel).await.is_err());
 cancel.cancel();assert!(load_verified_image(&path,None,&cancel).await.is_err());
}

#[tokio::test]
async fn saved_smart_region_bundle_verifies_mask_identity_and_refuses_missing_or_changed_region(){
 use video_kadr_backend::{services::concat_cutout_asset::verify_still_bundle_with_regions,domain::{concat_cutout_binding::StillAssets,composition::CompositionSource,cutout::RegionAttachment,concat_cutout_clock::ResolvedMask}};
 let(dir,mask,mut metadata,pixels)=fixture();let source=dir.path().join("raw.png");let copy=dir.path().join("copy.png");let region_path=dir.path().join("region.png");
 image::RgbaImage::from_raw(2,1,[13,21,34,255].repeat(2)).unwrap().save(&source).unwrap();metadata.source_hash=hash(&std::fs::read(&source).unwrap());
 let region_pixels:Vec<u8>=(0..256).flat_map(|_|(0..256).map(|x|if x<128{255}else{0})).collect();image::GrayImage::from_raw(256,256,region_pixels.clone()).unwrap().save(&region_path).unwrap();
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&region_path,std::fs::Permissions::from_mode(0o600)).unwrap();}
 let p=plan(&serde_json::json!({"mode":"custom","feather":0,"strokes":[{"tool":"smartBrush","size":0.1,"points":[[0.25,0.5]],"at":0}]})).unwrap();
 let points=p.correction.strokes[0].points.clone();
 let region=RegionAttachment{source_id:"raw".into(),mask_source_id:"region".into(),source_hash:metadata.source_hash.clone(),mask_hash:hash(&region_pixels),mask_file_hash:hash(&std::fs::read(&region_path).unwrap()),encoder_hash:hash(b"synthetic encoder identity"),decoder_hash:hash(b"synthetic decoder identity"),runtime_hash:metadata.runtime_hash.clone(),prompt_hash:hash(&serde_json::to_vec(&points).unwrap()),points};region.validate().unwrap();
 let mut recipe=p.clone();recipe.correction.strokes[0].region=Some(region);
 let resolved=resolve(&recipe,&ResolvedMask{width:256,height:256,probabilities:pixels},&[Some(std::sync::Arc::new(ResolvedMask{width:256,height:256,probabilities:region_pixels}))],2.,&std::sync::atomic::AtomicBool::new(false)).unwrap().unwrap();
 let mut frame=GroupSurface{width:2,height:1,rgba:[13,21,34,255].repeat(2)};apply(&mut frame,MaskPlane{width:256,height:256,probabilities:&resolved.probabilities},&Preparation{crop:[0.;4],canvas:[2,1],flip_h:false,flip_v:false},&mut||Ok(())).unwrap();image::RgbaImage::from_raw(2,1,frame.rgba).unwrap().save(&copy).unwrap();
 let descriptor=|id:&str,w:u32,h:u32|->CompositionSource{serde_json::from_value(serde_json::json!({"id":id,"kind":"image","durationTicks":1000000,"width":w,"height":h,"hasAudio":false})).unwrap()};
 let assets=StillAssets{original:descriptor("raw",2,1),copy:descriptor("copy",2,1),mask:descriptor("mask",256,256),regions:vec![descriptor("region",256,256)],snapshot:video_kadr_backend::domain::cutout::Attachment{subject:None,source_id:"raw".into(),mask_source_id:"mask".into(),source_hash:metadata.source_hash.clone(),mask_hash:metadata.mask_hash.clone(),mask_file_hash:metadata.mask_file_hash.clone().unwrap(),model_hash:metadata.model_hash.clone(),runtime_hash:metadata.runtime_hash.clone(),correction:recipe.correction}};
 let mut paths=std::collections::BTreeMap::from([("region".into(),region_path.clone())]);let cancel=CancellationToken::new();
 verify_still_bundle_with_regions(&source,&copy,&mask,&assets,&p,expected(&metadata),&paths,&cancel).await.unwrap();
 paths.insert("extra".into(),region_path.clone());assert!(verify_still_bundle_with_regions(&source,&copy,&mask,&assets,&p,expected(&metadata),&paths,&cancel).await.is_err());paths.remove("extra");
 assert!(verify_still_bundle_with_regions(&source,&copy,&mask,&assets,&p,expected(&metadata),&std::collections::BTreeMap::new(),&cancel).await.is_err());
 std::fs::write(&region_path,b"changed region").unwrap();assert!(verify_still_bundle_with_regions(&source,&copy,&mask,&assets,&p,expected(&metadata),&paths,&cancel).await.is_err());
}

#[tokio::test]
#[ignore="requires verified native Selfie executable, model and ORT"]
async fn actual_subject_policy_uses_selfie_fallback_and_requests_object_after_blank(){
 use video_kadr_backend::{services::{concat_cutout_selection::{analyse,Model},local_selfie},domain::concat_cutout_import::Subject,process_control::ProcessRuntime};
 let config=local_selfie::Config{executable:std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),model:std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL").unwrap().into(),runtime_library:std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY").unwrap().into()};
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.png");
 image::RgbaImage::from_fn(37,23,|x,y|{let n=y*37+x;image::Rgba([(n%151)as u8,((n*17)%180)as u8,((n*31)%199)as u8,(n%256)as u8])}).save(&source).unwrap();
 let source_hash=hash(&std::fs::read(&source).unwrap());
 for subject in [Subject::Person,Subject::Auto]{
  let cancel=CancellationToken::new();let mut requests=vec![];
  let output=dir.path().join(format!("{subject:?}.png"));let mask_path=dir.path().join(format!("{subject:?}-mask.png"));
  let result=analyse(subject,None,|model|{
   requests.push(model);
   std::future::ready(match model{Model::Selfie=>Ok(config.clone()),Model::Person=>Err(anyhow::anyhow!("Person not configured for this still run")),Model::Object=>Err(anyhow::anyhow!("Object not configured for this test"))})
  },|config|{
   let source=source.clone();let source_hash=source_hash.clone();let output=output.clone();let mask_path=mask_path.clone();let token=cancel.clone();
   async move{
    let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
    let produced=local_selfie::cutout(&ProcessRuntime::local_default(),&config,local_selfie::Input{source:&source,source_hash:&source_hash,mask_destination:Some(&mask_path)},&output,&token,std::time::Duration::from_secs(60),&tx).await?;
    let metadata:Metadata=serde_json::from_value(serde_json::to_value(produced)?)?;
    let verified=load_still(&mask_path,&metadata,expected(&metadata),&token).await?;
    Ok(video_kadr_backend::domain::concat_cutout_clock::ResolvedMask{width:verified.width,height:verified.height,probabilities:verified.probabilities.clone()})
   }
  },&cancel).await;
  if subject==Subject::Person{
   let(model,mask)=result.unwrap();assert_eq!(model,Model::Selfie);assert!(mask.probabilities.iter().all(|v|*v<10));assert_eq!(requests,[Model::Person,Model::Selfie]);
  }else{
   assert!(result.err().expect("automatic blank must request missing Object").to_string().contains("Object not configured"));assert_eq!(requests,[Model::Person,Model::Selfie,Model::Object]);
  }
  // The production still runner owns staging and verifies native outputs itself.
  use video_kadr_backend::services::concat_cutout_still::{self,Engine};
  let native=concat_cutout_still::analyse(&ProcessRuntime::local_default(),subject,None,&source,&source_hash,dir.path(),|model|std::future::ready(match model{Model::Selfie=>Ok(Engine::Selfie(config.clone())),_=>Err(anyhow::anyhow!("model unavailable"))}),&cancel,std::time::Duration::from_secs(60),&tokio::sync::mpsc::unbounded_channel().0).await;
  if subject==Subject::Person{let result=native.unwrap();assert_eq!(result.model,Model::Selfie);assert_eq!(result.subject,subject);assert_eq!(result.metadata.source_hash,source_hash);let copy=result.copy.clone();let mask=result.mask.clone();assert!(copy.is_file()&&mask.is_file());drop(result);assert!(!copy.exists()&&!mask.exists());}else{assert!(native.is_err());}
  assert!(std::fs::read_dir(dir.path()).unwrap().all(|entry|!entry.unwrap().file_type().unwrap().is_dir()));
 }
}

#[tokio::test]
async fn rvm_rectangular_png_verification_requires_model_grid_schema_and_digests(){
 use video_kadr_backend::domain::cutout::PERSON_MODEL_HASH;
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("rvm.png");let pixels=vec![128;512*256];
 image::GrayImage::from_raw(512,256,pixels.clone()).unwrap().save(&path).unwrap();
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600)).unwrap();}
 let metadata=Metadata{schema_version:8,width:2,height:1,mask_width:512,mask_height:256,blank_mask:false,mask_hash:hash(&pixels),source_hash:hash(b"source"),model_hash:PERSON_MODEL_HASH.into(),runtime_hash:hash(b"runtime"),output_hash:String::new(),mask_file_hash:Some(hash(&std::fs::read(&path).unwrap()))};
 let cancel=CancellationToken::new();let verified=load_still(&path,&metadata,expected(&metadata),&cancel).await.unwrap();assert_eq!((verified.width,verified.height),(512,256));
 for field in 0..3{let mut wrong=metadata.clone();match field{0=>wrong.schema_version=3,1=>wrong.width=1,_=>wrong.model_hash=SELFIE_MODEL_HASH.into()};assert!(load_still(&path,&wrong,expected(&wrong),&cancel).await.is_err());}
}
