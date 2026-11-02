//! Server-side saved asset resolution. This is not a verified ready binding.
use std::{collections::BTreeMap,path::PathBuf};
use anyhow::{ensure,Result};
use crate::{state::AppState,domain::{composition::CompositionClipId,concat_cutout_binding::{saved_still,StillAssets}}};
/// Deliberately not deserializable: paths come exclusively from the library.
#[derive(Debug)]
pub struct SavedStillCandidate {
 pub assets:StillAssets,
 pub paths:BTreeMap<String,PathBuf>,
 pub revision:u64,
}
/// Caller supplies its authenticated actor, never an actor from import JSON.
pub async fn resolve_saved_still(state:&AppState,actor:&str,project:&str,timeline:Option<&str>,clip:&CompositionClipId,revision:u64)->Result<SavedStillCandidate>{
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
 let saved=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
 ensure!(saved.revision==revision,"stale saved project revision");
 let assets=saved_still(&saved.document,timeline,clip)?;
 let mut paths=BTreeMap::new();
 for descriptor in [&assets.original,&assets.copy,&assets.mask].into_iter().chain(assets.regions.iter()){
  let id=descriptor.id.as_str();
  paths.insert(id.to_owned(),super::cutout_jobs::source(state,id,actor).await?);
 }
 // Access and document can change while asynchronous library resolution runs.
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project editor access revoked"))?;
 let current=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
 ensure!(current.revision==revision&&current.document==saved.document,"saved project changed during resolution");
 for (id,path) in &paths{ensure!(super::cutout_jobs::source(state,id,actor).await?==*path,"source changed during resolution");}
 Ok(SavedStillCandidate{assets,paths,revision})
}

/// Resolve and verify a saved bundle, then recheck authorization and descriptors.
/// The return value proves saved pixels, not the originating subject/model policy.
pub async fn verify_saved_still(state:&AppState,actor:&str,project:&str,timeline:Option<&str>,clip:&CompositionClipId,revision:u64,plan:&crate::domain::concat_cutout_import::Plan,cancel:&tokio_util::sync::CancellationToken)->Result<SavedStillCandidate>{
 use crate::services::concat_cutout_asset::{verify_still_bundle_with_regions,ExpectedIdentity};
 let candidate=resolve_saved_still(state,actor,project,timeline,clip,revision).await?;
 let assets=&candidate.assets;let snapshot=&assets.snapshot;
 let path=|id:&str|candidate.paths.get(id).ok_or_else(||anyhow::anyhow!("saved asset path missing"));
 let regions=assets.regions.iter().map(|region|Ok((region.id.as_str().to_owned(),path(region.id.as_str())?.clone()))).collect::<Result<BTreeMap<_,_>>>()?;
 verify_still_bundle_with_regions(path(assets.original.id.as_str())?,path(assets.copy.id.as_str())?,path(assets.mask.id.as_str())?,assets,plan,ExpectedIdentity{source_hash:&snapshot.source_hash,model_hash:&snapshot.model_hash,runtime_hash:&snapshot.runtime_hash},&regions,cancel).await?;
 let current=resolve_saved_still(state,actor,project,timeline,clip,revision).await?;
 ensure!(current.paths==candidate.paths&&current.assets.snapshot==candidate.assets.snapshot&&current.assets.original==candidate.assets.original&&current.assets.copy==candidate.assets.copy&&current.assets.mask==candidate.assets.mask&&current.assets.regions==candidate.assets.regions,"saved cutout changed during verification");
 ensure!(!cancel.is_cancelled(),"saved cutout verification cancelled");
 Ok(candidate)
}
