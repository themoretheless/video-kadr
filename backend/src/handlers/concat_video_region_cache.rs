//! Project-authorized model-free restoration of portable temporal region movies.
use anyhow::{ensure,Result};
use std::{path::PathBuf,time::Duration};
use tokio_util::sync::CancellationToken;
use crate::{state::AppState,domain::video_region::Attachment,services::concat_video_regions::VerifiedRegions};
pub struct Lease {pub reader:VerifiedRegions,pub(crate) library_source_id:String,stage:PathBuf}
impl Drop for Lease{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.stage);}}
pub(crate) fn declares_source(document:&serde_json::Value,id:&str)->bool{
 document.get("projectMedia").and_then(|v|v.get(id)).is_some()||document.get("sources").and_then(|v|v.get(id)).is_some()||document.get("compositions").and_then(serde_json::Value::as_object).is_some_and(|nodes|nodes.values().any(|node|node.get("content").and_then(|v|v.get("sources")).and_then(|v|v.get(id)).is_some()))
}
pub async fn load(state:&AppState,actor:&str,project:&str,revision:u64,snapshot:&Attachment,cancel:&CancellationToken)->Result<Lease>{
 snapshot.validate()?;
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("region editor access required"))?;
 let saved=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("region project unavailable"))?;
 let physical=super::concat_cutout_access::library_id(&saved.document,&snapshot.source_id)?;
 ensure!(saved.revision==revision&&saved.source_ids.contains(&physical),"stale project or unrelated region original");
 // Undo followed by Save can remove the current mask reference. A verified
 // project-owned immutable SAM reservation remains authority for later Redo.
 // Actor ownership or source-space readability alone must not grant this.
 let shared:Option<String>=sqlx::query_scalar("SELECT library_source_id FROM concat_video_project_region_refs WHERE project_id=? AND source_id=? AND video_hash=? AND mask_source_id=? AND file_hash=?").bind(project).bind(&snapshot.source_id).bind(&snapshot.source_hash).bind(&snapshot.mask_source_id).bind(&snapshot.mask_file_hash).fetch_optional(state.db.pool()).await?;
 let live=super::concat_cutout_access::library_id(&saved.document,&snapshot.mask_source_id)?;
 // Snapshot-specific ownership survives a later rebinding of the live bin slot.
 // Full render also checks the requested snapshot's explicit library mapping.
 let library_source_id=shared.clone().unwrap_or(live);
 let historical:bool=if saved.source_ids.contains(&library_source_id)||shared.as_ref()==Some(&library_source_id){true}else{
  sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_video_project_resources WHERE project_id=? AND source_id=? AND role='sam-video-regions' AND file_hash=?)").bind(project).bind(&library_source_id).bind(&snapshot.mask_file_hash).fetch_one(state.db.pool()).await?
 };
 ensure!(historical,"unrelated region source: no current reference or exact project reservation");
 let movie=super::cutout_jobs::source(state,&library_source_id,actor).await?;
 let lease=verify_movie(state,&movie,snapshot,&library_source_id,cancel).await?;
  super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("region editor access revoked"))?;
  let current=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("region project unavailable"))?;
  ensure!(current.revision==revision&&current.document==saved.document&&!cancel.is_cancelled(),"region project changed");ensure!(super::cutout_jobs::source(state,&library_source_id,actor).await?==movie,"region file access/path changed"); Ok(lease)

}

/// File verification only; callers separately own project/upload authorization.
pub(crate) async fn verify_movie(state:&AppState,movie:&std::path::Path,snapshot:&Attachment,library_source_id:&str,cancel:&CancellationToken)->Result<Lease>{
 snapshot.validate()?;
 let metadata=crate::services::local_person_mask_video::Metadata{width:256,height:256,frames:snapshot.frames,frame_rate:snapshot.frame_rate,mask_hash:snapshot.mask_hash.clone(),file_hash:snapshot.mask_file_hash.clone(),size_bytes:tokio::fs::metadata(movie).await?.len()};
 let root=state.storage.join("concat-video-region-stages");tokio::fs::create_dir_all(&root).await?;let stage=root.join(uuid::Uuid::new_v4().to_string());tokio::fs::create_dir(&stage).await?;
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&stage,std::fs::Permissions::from_mode(0o700)).await?;}
 let result=async{
  let codecs=crate::services::local_enhance_video::resolve_tools().await?;let raw=stage.join("regions.raw");let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
  crate::services::local_person_mask_video::decode(&state.process_runtime,crate::services::local_person_mask_video::Tools{ffmpeg:&codecs.ffmpeg,ffprobe:&codecs.ffprobe},movie,&metadata,&raw,64*1024*1024*1024,cancel,Duration::from_secs(3600),&tx).await?;
  let reader=crate::services::concat_video_regions::open(&raw,snapshot,cancel).await?;
Ok::<_,anyhow::Error>(reader)
 }.await;
 match result{Ok(reader)=>Ok(Lease{reader,library_source_id:library_source_id.into(),stage}),Err(error)=>{let _=tokio::fs::remove_dir_all(stage).await;Err(error)}}
}
