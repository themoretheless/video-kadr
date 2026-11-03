//! Project-authorized saved per-clip providers shared by preview and export.
use std::collections::BTreeMap;
use anyhow::{ensure,Result};
use tokio_util::sync::CancellationToken;
use crate::{state::AppState,domain::{composition::{CompositionClipId,CompositionTrack,SourceId},nested_composition::NestedCompositionProject},services::concat_video_analysis::RunCache};
#[derive(Debug,Clone,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SavedProject {pub project_id:String,pub expected_revision:u64}
type CacheKey=(SourceId,String,String,String,String,String,u64);
struct Owner{cache:CacheKey,correction:crate::domain::cutout::Correction,tracks:Vec<(usize,String)>}
pub struct BoundMasks {authorities:BTreeMap<String,u64>,owners:BTreeMap<(String,CompositionClipId),Owner>,regions:BTreeMap<String,super::concat_video_region_cache::Lease>,caches:BTreeMap<CacheKey,RunCache>,cancel:CancellationToken}
impl BoundMasks {
 pub(crate) fn archive_caches(&mut self)->impl Iterator<Item=(&SourceId,&mut RunCache)>{self.caches.iter_mut().map(|(key,cache)|(&key.0,cache))}
 pub fn authorities(&self)->Vec<SavedProject>{self.authorities.iter().map(|(id,revision)|SavedProject{project_id:id.clone(),expected_revision:*revision}).collect()}
 pub fn is_empty(&self)->bool{self.owners.is_empty()}
 pub fn identities(&self)->Result<BTreeMap<SourceId,String>>{
  use sha2::{Digest,Sha256};
  let mut grouped:BTreeMap<SourceId,Vec<String>>=BTreeMap::new();
  for((source,_,_,_,_,_,_),cache)in &self.caches{grouped.entry(source.clone()).or_default().push(cache.identity()?);}
  // Keep the existing single-family job identity stable. Multiple families of
  // one source contribute every identity in deterministic cache-key order.
  grouped.into_iter().map(|(source,values)|{let identity=if values.len()==1{values[0].clone()}else{format!("{:x}",Sha256::digest(serde_json::to_vec(&values)?))};Ok((source,identity))}).collect()
 }
 pub fn load<'a>(state:&'a AppState,actor:&'a str,context:Option<&'a SavedProject>,requested:&'a NestedCompositionProject,cancel:&'a CancellationToken)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<Self>>+Send+'a>>{
  Box::pin(async move {
  let wanted:Vec<_>=requested.compositions.iter().flat_map(|(node,document)|document.content.tracks.iter().flat_map(move|track|match track{CompositionTrack::Video{clips,..}=>clips.iter().filter(|c|c.enabled).filter_map(move|clip|clip.concat_cutout.as_ref().map(|binding|(node.clone(),clip.id.clone(),clip.source_id.clone(),binding.clone()))).collect::<Vec<_>>(),_=>vec![]})).collect();
  let mut result=Self{authorities:Default::default(),owners:Default::default(),regions:Default::default(),caches:Default::default(),cancel:cancel.clone()};
  if wanted.is_empty(){return Ok(result)}
  // Undo/reapply and draft split/copy may reference a previously registered
  // family no longer active in the latest document. The project-owned verified
  // source/model/runtime cache is the authority; current document tuples are not.
  for(node,id,source,mut binding)in wanted{
   binding.validate()?;
   if binding.origin.is_some(){
    let masks=binding.region_tracks.iter().map(|track|Ok((track.snapshot.mask_source_id.clone(),requested.library_source_id(&SourceId::parse(&track.snapshot.mask_source_id)?).as_str().to_owned()))).collect::<Result<BTreeMap<_,_>>>()?;
    if super::concat_video_ownership::use_owned(state,actor,context,source.as_str(),requested.library_source_id(&source).as_str(),&masks,&binding).await?{binding.origin=None;}
   }
   let owned;
   let context=if let Some(origin)=&binding.origin {owned=SavedProject{project_id:origin.project_id.clone(),expected_revision:origin.expected_revision};&owned}else{context.ok_or_else(||anyhow::anyhow!("saved project identity required for enabled video cutout"))?};
   let owner_source=binding.origin.as_ref().map(|o|o.source_id.as_str()).unwrap_or(source.as_str());
   if let Some(revision)=result.authorities.insert(context.project_id.clone(),context.expected_revision){ensure!(revision==context.expected_revision,"conflicting saved owner revisions");}
   super::cutout_assets::authorize(state,&context.project_id,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
   let saved=state.db.get_composition_project_for(&context.project_id,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
   ensure!(saved.revision==context.expected_revision,"stale saved video project");
   let physical=requested.library_source_id(&source).as_str().to_owned();
   let key=(source.clone(),binding.model_hash.clone(),binding.runtime_hash.clone(),binding.source_hash.clone(),context.project_id.clone(),owner_source.to_owned(),context.expected_revision);
   if !result.caches.contains_key(&key){
    let cache=super::concat_video_cache::restore_requested(state,actor,&context.project_id,owner_source,context.expected_revision,&physical,&binding.source_hash,&binding.runtime_hash,&binding.model_hash,cancel).await?.ok_or_else(||anyhow::anyhow!("saved video mask cache missing"))?;
    ensure!(cache.video_hash()==binding.source_hash,"saved video source changed");result.caches.insert(key.clone(),cache);
   }
   ensure!(result.caches[&key].video_hash()==binding.source_hash,"inconsistent saved video source identities");
   let mut tracks=vec![];
   for track in &binding.region_tracks{
    use sha2::{Digest,Sha256};ensure!(track.snapshot.source_id==source.as_str(),"region video source changed");
    let mut snapshot=track.snapshot.clone();snapshot.source_id=owner_source.into();
    if let Some(mapped)=binding.origin.as_ref().and_then(|o|o.region_sources.get(&snapshot.mask_source_id)){snapshot.mask_source_id=mapped.clone();}
    let identity=format!("{:x}",Sha256::digest(serde_json::to_vec(&(&context.project_id,context.expected_revision,&snapshot))?));
    if !result.regions.contains_key(&identity){
     let used:u64=result.regions.values().map(|r|r.reader.snapshot().frames*256*256).sum();ensure!(used.checked_add(track.snapshot.frames*256*256).is_some_and(|n|n<=64*1024*1024*1024),"video region provider staging budget exceeded");
     let lease=super::concat_video_region_cache::load_for_original(state,actor,&context.project_id,context.expected_revision,&snapshot,&physical,cancel).await?;result.regions.insert(identity.clone(),lease);
    }
    ensure!(requested.library_source_id(&SourceId::parse(&track.snapshot.mask_source_id)?).as_str()==result.regions[&identity].library_source_id,"requested region library binding differs from reserved asset");
    tracks.push((track.stroke,identity));
   }
   result.owners.insert((node,id),Owner{cache:key,correction:binding.settings(),tracks});
  }
  recheck_authorities(state,actor,&result.authorities()).await?;
  ensure!(!cancel.is_cancelled(),"saved video provider cancelled");Ok(result)
  })
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

pub async fn recheck_authorities(state:&AppState,actor:&str,owners:&[SavedProject])->Result<()> {
 for owner in owners {
  super::cutout_assets::authorize(state,&owner.project_id,actor).await.map_err(|_|anyhow::anyhow!("saved project access revoked"))?;
  let saved=state.db.get_composition_project_for(&owner.project_id,actor).await?.ok_or_else(||anyhow::anyhow!("saved project unavailable"))?;
  ensure!(saved.revision==owner.expected_revision,"saved project changed during rendering");
 }
 Ok(())
}
