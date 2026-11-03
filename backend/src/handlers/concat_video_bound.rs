//! Project-authorized saved per-clip providers shared by preview and export.
use std::collections::BTreeMap;
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::{state::AppState,domain::{composition::{CompositionClipId,CompositionTrack,SourceId},nested_composition::NestedCompositionProject},services::concat_video_analysis::RunCache};
#[derive(Debug,Clone,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SavedProject {pub project_id:String,pub expected_revision:u64}
type CacheKey=(SourceId,String,String,String);
struct Owner{cache:CacheKey,correction:crate::domain::cutout::Correction,tracks:Vec<(usize,String)>}
pub struct BoundMasks {owners:BTreeMap<(String,CompositionClipId),Owner>,regions:BTreeMap<String,super::concat_video_region_cache::Lease>,caches:BTreeMap<CacheKey,RunCache>,cancel:CancellationToken}
impl BoundMasks {
 pub fn is_empty(&self)->bool{self.owners.is_empty()}
 pub fn identities(&self)->Result<BTreeMap<SourceId,String>>{
  use sha2::{Digest,Sha256};
  let mut grouped:BTreeMap<SourceId,Vec<String>>=BTreeMap::new();
  for((source,_,_,_),cache)in &self.caches{grouped.entry(source.clone()).or_default().push(cache.identity()?);}
  // Keep the existing single-family job identity stable. Multiple families of
  // one source contribute every identity in deterministic cache-key order.
  grouped.into_iter().map(|(source,values)|{let identity=if values.len()==1{values[0].clone()}else{format!("{:x}",Sha256::digest(serde_json::to_vec(&values)?))};Ok((source,identity))}).collect()
 }
 pub async fn load(state:&AppState,actor:&str,context:Option<&SavedProject>,requested:&NestedCompositionProject,cancel:&CancellationToken)->Result<Self>{
  let wanted:Vec<_>=requested.compositions.iter().flat_map(|(node,document)|document.content.tracks.iter().flat_map(move|track|match track{CompositionTrack::Video{clips,..}=>clips.iter().filter(|c|c.enabled).filter_map(move|clip|clip.concat_cutout.as_ref().map(|binding|(node.clone(),clip.id.clone(),clip.source_id.clone(),binding.clone()))).collect::<Vec<_>>(),_=>vec![]})).collect();
  let mut result=Self{owners:Default::default(),regions:Default::default(),caches:Default::default(),cancel:cancel.clone()};
  if wanted.is_empty(){return Ok(result)}
  let context=context.ok_or_else(||anyhow::anyhow!("saved project identity required for enabled video cutout"))?;
  super::cutout_assets::authorize(state,&context.project_id,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
  let saved=state.db.get_composition_project_for(&context.project_id,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;ensure!(saved.revision==context.expected_revision,"stale saved video project");
  // Undo/reapply and draft split/copy may reference a previously registered
  // family no longer active in the latest document. The project-owned verified
  // source/model/runtime cache is the authority; current document tuples are not.
  for(node,id,source,binding)in wanted{
   binding.validate()?;
   let physical=super::concat_cutout_access::library_id(&saved.document,source.as_str())?;
   ensure!(requested.library_source_id(&source).as_str()==physical,"requested video library binding differs from saved cache owner");
   ensure!(saved.source_ids.iter().any(|id|id==&physical),"requested video source is not authorized by the saved project");
   let key=(source.clone(),binding.model_hash.clone(),binding.runtime_hash.clone(),binding.source_hash.clone());
   if !result.caches.contains_key(&key){
    let cache=super::concat_video_cache::restore_for_model(state,actor,&context.project_id,source.as_str(),context.expected_revision,&binding.runtime_hash,&binding.model_hash,cancel).await?.ok_or_else(||anyhow::anyhow!("saved video mask cache missing"))?;
    ensure!(cache.video_hash()==binding.source_hash,"saved video source changed");result.caches.insert(key.clone(),cache);
   }
   ensure!(result.caches[&key].video_hash()==binding.source_hash,"inconsistent saved video source identities");
   let mut tracks=vec![];
   for track in &binding.region_tracks{
    use sha2::{Digest,Sha256};ensure!(track.snapshot.source_id==source.as_str(),"region video source changed");
    let identity=format!("{:x}",Sha256::digest(serde_json::to_vec(&track.snapshot)?));
    if !result.regions.contains_key(&identity){
     let used:u64=result.regions.values().map(|r|r.reader.snapshot().frames*256*256).sum();ensure!(used.checked_add(track.snapshot.frames*256*256).is_some_and(|n|n<=64*1024*1024*1024),"video region provider staging budget exceeded");
     let lease=super::concat_video_region_cache::load(state,actor,&context.project_id,context.expected_revision,&track.snapshot,cancel).await?;ensure!(requested.library_source_id(&SourceId::parse(&track.snapshot.mask_source_id)?).as_str()==lease.library_source_id,"requested region library binding differs from reserved asset");result.regions.insert(identity.clone(),lease);
    }
    tracks.push((track.stroke,identity));
   }
   result.owners.insert((node,id),Owner{cache:key,correction:binding.settings(),tracks});
  }
  ensure!(!cancel.is_cancelled(),"saved video provider cancelled");Ok(result)
 }
 pub fn apply(&mut self,node:&str,clip:&CompositionClipId,source:&SourceId,time:u64,frame:&mut crate::domain::group_surface::GroupSurface)->Result<()>{
  ensure!(!self.cancel.is_cancelled(),"saved video provider cancelled");
  let Some(owner)=self.owners.get(&(node.to_owned(),clip.clone()))else{return Ok(())};let expected=&owner.cache;ensure!(&expected.0==source,"saved video clip source changed");
  let regions=if owner.tracks.is_empty(){None}else{
   let mut planes=vec![None;owner.correction.strokes.len()];for(index,key)in &owner.tracks{planes[*index]=Some(self.regions.get(key).ok_or_else(||anyhow::anyhow!("video region provider missing"))?.reader.read(&expected.3,time,&self.cancel)?);}
   Some(crate::services::concat_video_analysis::SourceRegions{video_hash:expected.3.clone(),source_us:time,planes})
  };
  self.caches.get_mut(expected).ok_or_else(||anyhow::anyhow!("saved video mask provider missing"))?.apply_decoded_with_regions(frame,time,&owner.correction,regions.as_ref(),&self.cancel)?;Ok(())
 }
}
