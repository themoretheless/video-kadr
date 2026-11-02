//! Register owned, verified analysis results as private project media.
use anyhow::{ensure,Result};
use crate::{state::AppState,library::MediaEntry,services::concat_cutout_still::PreparedStill,domain::{composition::SourceId,concat_project_import::ReadyStillBinding}};
use tokio_util::sync::CancellationToken;
pub struct RegisteredStill {pub binding:ReadyStillBinding,pub copy:MediaEntry,pub mask:MediaEntry}
pub async fn register(state:&AppState,actor:&str,project:&str,mut prepared:PreparedStill,cancel:&CancellationToken)->Result<RegisteredStill>{
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
 let source=super::cutout_jobs::source(state,prepared.binding.assets.original.id.as_str(),actor).await?;
 for region in &prepared.binding.assets.regions{let id=region.id.as_str();prepared.region_paths.insert(id.into(),super::cutout_jobs::source(state,id,actor).await?);}

 let copy_id=uuid::Uuid::new_v4().to_string();let mask_id=uuid::Uuid::new_v4().to_string();
 prepared.binding.assets.copy.id=SourceId::parse(copy_id.clone())?;prepared.binding.assets.mask.id=SourceId::parse(mask_id.clone())?;prepared.binding.assets.snapshot.mask_source_id=mask_id.clone();
 ensure!(state.library.get(&copy_id).await.is_none()&&state.library.get(&mask_id).await.is_none(),"generated cutout ID collision");
 let space=state.db.claim_project_narration(project,actor,&copy_id).await?;
 let other=match state.db.claim_project_narration(project,actor,&mask_id).await{Ok(space)=>space,Err(error)=>{let _=state.db.release_space_source_claim(&copy_id).await;return Err(error)}};
 if space!=other{let _=state.db.release_space_source_claim(&copy_id).await;let _=state.db.release_space_source_claim(&mask_id).await;anyhow::bail!("cutout project space changed");}
 let result=async{
  let root=crate::services::tts_model_download::directory(&state.sources_dir(),&std::path::Path::new("spaces").join(&space)).await?;
  let published=prepared.publish_files(&root,&source,cancel).await?;
  let build=|id:&str,path:&std::path::Path,width:u32,height:u32|->Result<MediaEntry>{
   let key=path.strip_prefix(state.sources_dir())?.to_str().ok_or_else(||anyhow::anyhow!("invalid cutout storage key"))?.to_owned();
   Ok(MediaEntry::from_result("source",&serde_json::json!({"id":id,"filename":format!("{id}.png"),"url":format!("/files/sources/{id}.png"),"storageKey":key,"mediaType":"image","width":width,"height":height})))
  };
  let copy_path=root.join(format!("{copy_id}.png"));let mask_path=root.join(format!("{mask_id}.png"));
  let(mut copy_linked,mut mask_linked,mut copy_added,mut mask_added)=(false,false,false,false);
  let registration=async{
   // The library's scoped storage contract is exactly spaces/<space>/<filename>.
   // Fresh IDs and no-replace hard links preserve that contract.
   tokio::fs::hard_link(&published.copy,&copy_path).await?;copy_linked=true;
   tokio::fs::hard_link(&published.mask,&mask_path).await?;mask_linked=true;
   let copy=build(&copy_id,&copy_path,published.binding.assets.copy.width,published.binding.assets.copy.height)?;
   let mask=build(&mask_id,&mask_path,published.binding.assets.mask.width,published.binding.assets.mask.height)?;
   super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project access revoked"))?;
   ensure!(super::cutout_jobs::source(state,published.binding.assets.original.id.as_str(),actor).await?==source,"original source path changed");
   ensure!(!cancel.is_cancelled(),"cutout registration cancelled");
   copy_added=state.library.add(copy.clone()).await;ensure!(copy_added,"copy library persistence failed");
   mask_added=state.library.add(mask.clone()).await;ensure!(mask_added,"mask library persistence failed");
   super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project access revoked"))?;
   ensure!(!cancel.is_cancelled(),"cutout registration cancelled");Ok::<_,anyhow::Error>((copy,mask))
  }.await;
  let (copy,mask)=match registration{Ok(entries)=>entries,Err(error)=>{if copy_added{let _=state.library.remove(&copy_id).await;}if mask_added{let _=state.library.remove(&mask_id).await;}if copy_linked&&state.library.get(&copy_id).await.is_none(){let _=tokio::fs::remove_file(&copy_path).await;}if mask_linked&&state.library.get(&mask_id).await.is_none(){let _=tokio::fs::remove_file(&mask_path).await;}if let Some(dir)=published.copy.parent(){let _=tokio::fs::remove_dir_all(dir).await;}return Err(error)}};
  if let Some(dir)=published.copy.parent(){let _=tokio::fs::remove_dir_all(dir).await;}
  state.index_media(&copy).await;state.index_media(&mask).await;
  Ok(RegisteredStill{binding:published.binding,copy,mask})
 }.await;
 if result.is_err(){if state.library.get(&copy_id).await.is_none(){let _=state.db.release_space_source_claim(&copy_id).await;}if state.library.get(&mask_id).await.is_none(){let _=state.db.release_space_source_claim(&mask_id).await;}}
 result
}

/// Apply only an internally registered binding, with optimistic project revision.
/// Public import JSON cannot supply RegisteredStill or server-resolved file paths.
pub async fn apply_registered(state:&AppState,actor:&str,project:&str,timeline:Option<&str>,clip:&crate::domain::composition::CompositionClipId,revision:u64,registered:&RegisteredStill,cancel:&CancellationToken)->Result<crate::db::CompositionProject>{
 use crate::domain::{composition::Composition,concat_cutout_binding::{bind_still,rebind_still}};
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
 let saved=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
 ensure!(saved.revision==revision,"stale project revision");
 let binding=&registered.binding;let assets=&binding.assets;
 ensure!(registered.copy.id==assets.copy.id.as_str()&&registered.mask.id==assets.mask.id.as_str(),"registered cutout IDs changed");
 let source=super::cutout_jobs::source(state,assets.original.id.as_str(),actor).await?;
 let copy=super::cutout_jobs::source(state,assets.copy.id.as_str(),actor).await?;
 let mask=super::cutout_jobs::source(state,assets.mask.id.as_str(),actor).await?;
 let mut regions=std::collections::BTreeMap::new();for region in &assets.regions{regions.insert(region.id.as_str().to_owned(),super::cutout_jobs::source(state,region.id.as_str(),actor).await?);}
 crate::services::concat_cutout_asset::verify_still_bundle_with_regions(&source,&copy,&mask,assets,&binding.source_settings,crate::services::concat_cutout_asset::ExpectedIdentity{source_hash:&binding.source_hash,model_hash:&binding.model_hash,runtime_hash:&binding.runtime_hash},&regions,cancel).await?;
 let mut document=saved.document.clone();
 let content=if document.get("compositions").is_some(){
  let id=timeline.ok_or_else(||anyhow::anyhow!("timeline required"))?;
  document.get_mut("compositions").and_then(|nodes|nodes.get_mut(id)).and_then(|node|node.get_mut("content")).ok_or_else(||anyhow::anyhow!("timeline missing"))?
 }else{ensure!(timeline.is_none(),"unexpected timeline");&mut document};
 let composition:Composition=serde_json::from_value(content.clone())?;
 let bind=if composition.metadata.as_ref().is_some_and(|m|m.media_versions.contains_key(clip)){rebind_still}else{bind_still};
 let changed=bind(&composition,clip,&binding.source_settings,assets,&binding.source_hash,&binding.model_hash,&binding.runtime_hash)?;
 *content=serde_json::to_value(changed)?;
 let sources=crate::db::extract_document_source_ids(&document)?;
 ensure!(!cancel.is_cancelled(),"cutout application cancelled");
 state.db.update_composition_project_for(project,actor,&saved.name,&document,&sources,revision).await?.ok_or_else(||anyhow::anyhow!("project changed during cutout application"))
}
