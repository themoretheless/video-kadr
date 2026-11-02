//! Pure binding of already authorized/verified image cutout assets to a clip.
//! No paths, inference or ACL decisions are performed by this schema operation.
use anyhow::{ensure,Result};
use super::{composition::{Composition,CompositionClipId,CompositionSource,CompositionTrack,SourceKind},cutout::Attachment,concat_cutout_import::Plan,metadata_burnin::MediaVersions};
#[derive(Debug,Clone)]
pub struct StillAssets{
 pub original:CompositionSource,pub copy:CompositionSource,pub mask:CompositionSource,
 pub regions:Vec<CompositionSource>,pub snapshot:Attachment,
}
pub fn bind_still(composition:&Composition,clip_id:&CompositionClipId,plan:&Plan,assets:&StillAssets,expected_source_hash:&str,expected_model_hash:&str,expected_runtime_hash:&str)->Result<Composition>{
 assets.snapshot.validate()?;
 ensure!(assets.snapshot.subject.is_none_or(|subject|subject==plan.subject),"cutout subject differs from captured analysis");
 ensure!(assets.snapshot.source_hash==expected_source_hash&&assets.snapshot.model_hash==expected_model_hash&&assets.snapshot.runtime_hash==expected_runtime_hash,"source/model/runtime cutout binding mismatch");
 ensure!(assets.original.id.as_str()==assets.snapshot.source_id&&assets.mask.id.as_str()==assets.snapshot.mask_source_id&&assets.copy.id!=assets.original.id&&assets.copy.id!=assets.mask.id,"cutout source IDs mismatch");
 ensure!([&assets.original,&assets.copy,&assets.mask].iter().all(|source|source.kind==SourceKind::Image&&!source.has_audio)&&(assets.copy.width,assets.copy.height)==(assets.original.width,assets.original.height),"cutout image shape/kind mismatch");
 let mut correction=assets.snapshot.correction.clone();for stroke in &mut correction.strokes{stroke.region=None;}
 ensure!(correction==plan.correction,"cutout output recipe differs from original source settings");
 let mut changed=composition.clone();
 for source in std::iter::once(&assets.original).chain(std::iter::once(&assets.copy)).chain(std::iter::once(&assets.mask)).chain(assets.regions.iter()){
  if let Some(existing)=changed.sources.get(&source.id){ensure!(existing==source,"cutout asset ID collision");}else{changed.sources.insert(source.id.clone(),source.clone());}
 }
 let pair=MediaVersions{original_source_id:assets.original.id.clone(),copy_source_id:assets.copy.id.clone(),cutout:Some(assets.snapshot.clone()),video_cutout:None};
 let already_bound=composition.metadata.as_ref().and_then(|metadata|metadata.media_versions.get(clip_id))==Some(&pair);
 let mut found=false;
 for track in &mut changed.tracks{if let CompositionTrack::Image{locked,clips,..}=track{
  for clip in clips{if &clip.id==clip_id{ensure!(!*locked,"cutout track is locked");ensure!(clip.source_id==assets.original.id||(already_bound&&clip.source_id==assets.copy.id),"cutout clip belongs to another source");clip.source_id=assets.copy.id.clone();found=true;}}
 }}
 ensure!(found,"cutout image clip not found");

 let metadata=changed.metadata.get_or_insert_with(Default::default);
 if let Some(existing)=metadata.media_versions.get(clip_id){ensure!(existing==&pair,"cutout clip already has another media version");}
 if let Some(name)=metadata.source_names.get(&assets.original.id).cloned(){metadata.source_names.entry(assets.copy.id.clone()).or_insert(name);}
 metadata.media_versions.insert(clip_id.clone(),pair);changed.validate()?;Ok(changed)
}

/// Extract saved descriptors only; physical files and access remain unverified.
pub fn saved_still(document:&serde_json::Value,timeline:Option<&str>,clip:&CompositionClipId)->Result<StillAssets>{
 let content=if let Some(nodes)=document.get("compositions"){
  let id=timeline.ok_or_else(||anyhow::anyhow!("timeline required"))?;
  nodes.get(id).and_then(|node|node.get("content")).ok_or_else(||anyhow::anyhow!("timeline missing"))?
 }else{ensure!(timeline.is_none(),"unexpected timeline");document};
 let composition:Composition=serde_json::from_value(content.clone())?;composition.validate()?;
 let pair=composition.metadata.as_ref().and_then(|m|m.media_versions.get(clip)).ok_or_else(||anyhow::anyhow!("saved media pair missing"))?;
 let snapshot=pair.cutout.clone().ok_or_else(||anyhow::anyhow!("saved image cutout missing"))?;
 ensure!(snapshot.source_id==pair.original_source_id.as_str(),"saved cutout original mismatch");
 ensure!(composition.tracks.iter().any(|track|matches!(track,CompositionTrack::Image{clips,..} if clips.iter().any(|c|&c.id==clip&&(c.source_id==pair.original_source_id||c.source_id==pair.copy_source_id)))),"saved image clip missing");
 let source=|id:&str|->Result<CompositionSource>{composition.sources.values().find(|s|s.id.as_str()==id).cloned().ok_or_else(||anyhow::anyhow!("saved source missing"))};
 let regions=snapshot.correction.strokes.iter().filter_map(|s|s.region.as_ref()).map(|r|source(&r.mask_source_id)).collect::<Result<Vec<_>>>()?;
 Ok(StillAssets{original:source(pair.original_source_id.as_str())?,copy:source(pair.copy_source_id.as_str())?,mask:source(&snapshot.mask_source_id)?,regions,snapshot})
}

/// Explicit reanalysis keeps old sources available and replaces only this pair.
pub fn rebind_still(composition:&Composition,clip:&CompositionClipId,plan:&Plan,assets:&StillAssets,source_hash:&str,model_hash:&str,runtime_hash:&str)->Result<Composition>{
 composition.validate()?;
 let pair=composition.metadata.as_ref().and_then(|m|m.media_versions.get(clip)).ok_or_else(||anyhow::anyhow!("saved media pair missing"))?;
 ensure!(pair.cutout.is_some()&&pair.video_cutout.is_none()&&pair.original_source_id==assets.original.id,"reanalysis belongs to another original");
 ensure!(composition.sources.get(&pair.original_source_id)==Some(&assets.original),"reanalysis original metadata changed");
 let mut staged=composition.clone();let mut found=false;
 for track in &mut staged.tracks{if let CompositionTrack::Image{locked,clips,..}=track{for image in clips{if &image.id==clip{ensure!(!*locked,"cutout track is locked");ensure!(image.source_id==pair.original_source_id||image.source_id==pair.copy_source_id,"stale reanalysis clip");image.source_id=pair.original_source_id.clone();found=true;}}}}
 ensure!(found,"reanalysis image missing");
 staged.metadata.as_mut().unwrap().media_versions.remove(clip);
 bind_still(&staged,clip,plan,assets,source_hash,model_hash,runtime_hash)
}

/// Reuse only the exact saved stroke's source-owned region; never accept a client region.
pub fn saved_region_correction(saved:Option<&Attachment>,plan:&Plan)->Result<super::cutout::Correction>{
 let result=match_saved_region_correction(saved,plan)?;
 for stroke in &result.strokes{ensure!(!(result.custom&&matches!(stroke.tool,super::cutout::Tool::SmartBrush|super::cutout::Tool::SmartEraser)&&stroke.at.is_some()&&stroke.region.is_none()),"timed smart stroke requires a matching saved region");}
 Ok(result)
}
pub fn match_saved_region_correction(saved:Option<&Attachment>,plan:&Plan)->Result<super::cutout::Correction>{
 let mut result=plan.correction.clone();
 for(index,stroke)in result.strokes.iter_mut().enumerate(){
  if let Some(old)=saved.and_then(|s|s.correction.strokes.get(index)){
   if old.tool==stroke.tool&&old.size==stroke.size&&old.points==stroke.points&&old.at==stroke.at{stroke.region=old.region.clone();}
  }
 }
 result.validate()?;Ok(result)
}
