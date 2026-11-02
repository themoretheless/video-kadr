//! Verified source/model binding for an already generated still mask asset.
//! Caller owns source/mask ACL and Analysis admission; no client paths or models
//! are resolved here, and subject-model selection remains with the analysis job.
use std::{path::Path,sync::Arc};
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::domain::concat_cutout_clock::ResolvedMask;
#[derive(Clone,Copy)]
pub struct ExpectedIdentity<'a>{pub source_hash:&'a str,pub model_hash:&'a str,pub runtime_hash:&'a str}
pub async fn load_still(path:&Path,metadata:&super::local_cutout::Metadata,expected:ExpectedIdentity<'_>,cancel:&CancellationToken)->Result<Arc<ResolvedMask>>{
 ensure!(!cancel.is_cancelled(),"source cutout load cancelled");
 ensure!(path.is_absolute(),"mask asset must be server-resolved absolute path");
 for hash in [expected.source_hash,expected.model_hash,expected.runtime_hash]{ensure!(hash.len()==64&&hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"invalid expected cutout identity");}
 ensure!(metadata.source_hash==expected.source_hash&&metadata.model_hash==expected.model_hash&&metadata.runtime_hash==expected.runtime_hash,"source/model/runtime cutout binding mismatch");
 // Reuse encoded and raw digests, bounded grayscale decode and blank validation.
 // Await completion even on cancellation, rather than abandoning a decode task.
 let probabilities=super::local_cutout::read_verified_probabilities(path,metadata).await?;
 ensure!(!cancel.is_cancelled(),"source cutout load cancelled");
 Ok(Arc::new(ResolvedMask{width:metadata.mask_width as usize,height:metadata.mask_height as usize,probabilities}))
}

/// Compare decoded copy pixels with the source and the fully resolved correction.
/// `None` is the original automatic blank-mask fallback, retaining the source.
/// The caller must verify file identities and resolve the saved recipe first.
pub fn verify_copy_pixels(
 original:&crate::domain::group_surface::GroupSurface,
 copy:&crate::domain::group_surface::GroupSurface,
 corrected:Option<&ResolvedMask>,
 cancel:&CancellationToken,
)->Result<()> {
 use crate::domain::{concat_cutout_mask::{apply,MaskPlane},concat_picture::Preparation};
 original.validate()?;copy.validate()?;
 ensure!((original.width,original.height)==(copy.width,copy.height),"cutout copy dimensions changed");
 let mut checkpoint=||{ensure!(!cancel.is_cancelled(),"cutout copy verification cancelled");Ok(())};
 checkpoint()?;
 let mut expected=original.clone();
 if let Some(mask)=corrected {
  apply(&mut expected,MaskPlane{width:mask.width,height:mask.height,probabilities:&mask.probabilities},&Preparation{crop:[0.;4],canvas:[original.width,original.height],flip_h:false,flip_v:false},&mut checkpoint)?;
 }
 // Compare all channels, including hidden RGB, rather than opaque preview pixels.
 for (actual,expected) in copy.rgba.chunks(64*1024).zip(expected.rgba.chunks(64*1024)){
  checkpoint()?;ensure!(actual==expected,"saved cutout copy does not match source/mask recipe");
 }
 checkpoint()?;Ok(())
}

/// Decode one immutable bounded byte snapshot and verify its encoded identity.
/// This accepts server-resolved paths only; it does not authorize them.
pub async fn load_verified_image(path:&Path,expected_hash:Option<&str>,cancel:&CancellationToken)->Result<crate::domain::group_surface::GroupSurface>{
 use tokio::io::AsyncReadExt;
 use sha2::{Digest,Sha256};
 ensure!(path.is_absolute()&&!cancel.is_cancelled(),"invalid/cancelled image load");
 let metadata=tokio::fs::symlink_metadata(path).await?;
 ensure!(metadata.file_type().is_file()&&metadata.len()<=256*1024*1024,"invalid image file/size");
 let file=tokio::fs::File::open(path).await?;
 let mut bytes=Vec::new();file.take(256*1024*1024+1).read_to_end(&mut bytes).await?;
 ensure!(bytes.len()<=256*1024*1024&&!cancel.is_cancelled(),"image load exceeded limit/cancelled");
 if let Some(expected)=expected_hash{ensure!(format!("{:x}",Sha256::digest(&bytes))==expected,"saved image identity changed");}
 let token=cancel.clone();
 let result=tokio::task::spawn_blocking(move||->Result<_>{
  use image::ImageDecoder;
  ensure!(!token.is_cancelled(),"image decode cancelled");
  let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
  let mut limits=image::Limits::default();limits.max_alloc=Some(128*1024*1024);reader.limits(limits);
  let mut decoder=reader.into_decoder()?;let(w,h)=decoder.dimensions();
  ensure!(w>0&&h>0&&u64::from(w)*u64::from(h)<=3840*2160,"saved image dimensions exceed render limit");
  let orientation=decoder.orientation()?;
  let mut image=image::DynamicImage::from_decoder(decoder)?;image.apply_orientation(orientation);
  let rgba=image.into_rgba8();
  let frame=crate::domain::group_surface::GroupSurface{width:rgba.width()as usize,height:rgba.height()as usize,rgba:rgba.into_raw()};
  frame.validate()?;ensure!(!token.is_cancelled(),"image decode cancelled");Ok(frame)
 }).await??;
 ensure!(!cancel.is_cancelled(),"image decode cancelled");Ok(result)
}

/// Verify a saved non-region still bundle against its encoded input and recipe.
/// ACL, admission and captured subject/model policy must be established by caller.
pub async fn verify_still_bundle(
 source:&Path,copy:&Path,mask:&Path,
 assets:&crate::domain::concat_cutout_binding::StillAssets,
 plan:&crate::domain::concat_cutout_import::Plan,
 expected:ExpectedIdentity<'_>,cancel:&CancellationToken,
)->Result<()> {
 verify_still_bundle_with_regions(source,copy,mask,assets,plan,expected,&std::collections::BTreeMap::new(),cancel).await
}
/// Region paths must be resolved and authorized from saved source IDs by caller.
pub async fn verify_still_bundle_with_regions(
 source:&Path,copy:&Path,mask:&Path,
 assets:&crate::domain::concat_cutout_binding::StillAssets,
 plan:&crate::domain::concat_cutout_import::Plan,
 expected:ExpectedIdentity<'_>,region_paths:&std::collections::BTreeMap<String,std::path::PathBuf>,cancel:&CancellationToken,
)->Result<()> {
 use crate::domain::{cutout,concat_cutout_resolve};
 assets.snapshot.validate()?;
 ensure!(assets.snapshot.subject.is_none_or(|subject|subject==plan.subject),"saved cutout subject changed");
 let mut saved_recipe=assets.snapshot.correction.clone();for stroke in &mut saved_recipe.strokes{stroke.region=None;}
 ensure!(saved_recipe==plan.correction,"saved cutout recipe changed");
 let mut saved_plan=plan.clone();saved_plan.correction=assets.snapshot.correction.clone();
 let mut regions=Vec::with_capacity(saved_plan.correction.strokes.len());
 let mut used=std::collections::BTreeSet::new();
 for stroke in &saved_plan.correction.strokes{
  ensure!(!cancel.is_cancelled(),"saved region verification cancelled");
  if let Some(region)=&stroke.region{
   let path=region_paths.get(&region.mask_source_id).ok_or_else(||anyhow::anyhow!("saved region path missing"))?;
   ensure!(path.is_absolute(),"region path must be server-resolved");
   let bytes=super::local_sam::read_verified_probabilities(path.clone(),super::local_sam::Metadata{schema_version:5,width:assets.original.width,height:assets.original.height,mask_width:256,mask_height:256,source_hash:region.source_hash.clone(),encoder_hash:region.encoder_hash.clone(),decoder_hash:region.decoder_hash.clone(),runtime_hash:region.runtime_hash.clone(),prompt_hash:region.prompt_hash.clone(),mask_hash:region.mask_hash.clone(),output_hash:region.mask_file_hash.clone()}).await?;
   regions.push(Some(Arc::new(ResolvedMask{width:256,height:256,probabilities:bytes})));used.insert(region.mask_source_id.clone());
  }else{
   ensure!(!(saved_plan.correction.custom&&matches!(stroke.tool,cutout::Tool::SmartBrush|cutout::Tool::SmartEraser)&&stroke.at.is_some()),"timed smart stroke requires saved region");
   regions.push(None);
  }
 }
 ensure!(used.len()==region_paths.len(),"unused saved region paths");
 ensure!(assets.snapshot.source_hash==expected.source_hash&&assets.snapshot.model_hash==expected.model_hash&&assets.snapshot.runtime_hash==expected.runtime_hash,"saved cutout identity changed");
 let original=load_verified_image(source,Some(expected.source_hash),cancel).await?;
 ensure!((original.width as u32,original.height as u32)==(assets.original.width,assets.original.height),"saved original dimensions changed");
 let (mask_width,mask_height)=cutout::mask_dimensions(expected.model_hash,assets.original.width,assets.original.height)?;
 let mut metadata=super::local_cutout::Metadata{schema_version:cutout::still_mask_schema(expected.model_hash),width:assets.original.width,height:assets.original.height,mask_width,mask_height,blank_mask:false,mask_hash:assets.snapshot.mask_hash.clone(),source_hash:assets.snapshot.source_hash.clone(),model_hash:assets.snapshot.model_hash.clone(),runtime_hash:assets.snapshot.runtime_hash.clone(),output_hash:String::new(),mask_file_hash:Some(assets.snapshot.mask_file_hash.clone())};
 let automatic=match load_still(mask,&metadata,expected,cancel).await{
  Ok(mask)=>mask,
  Err(first)=>{ensure!(!cancel.is_cancelled(),"saved cutout verification cancelled");metadata.blank_mask=true;load_still(mask,&metadata,expected,cancel).await.map_err(|second|anyhow::anyhow!("saved mask verification failed: {first}; {second}"))?}
 };
 let corrected=concat_cutout_resolve::resolve(&saved_plan,&automatic,&regions,original.width as f32/original.height as f32,&std::sync::atomic::AtomicBool::new(cancel.is_cancelled()))?;
 let actual=load_verified_image(copy,None,cancel).await?;
 ensure!((actual.width as u32,actual.height as u32)==(assets.copy.width,assets.copy.height),"saved copy dimensions changed");
 verify_copy_pixels(&original,&actual,corrected.as_deref(),cancel)
}
