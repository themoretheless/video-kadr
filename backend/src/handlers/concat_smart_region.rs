//! Generate a new source-owned region using private server-selected SAM inventory.
use anyhow::{ensure,Result};
use crate::{state::AppState,domain::{composition::{CompositionSource,SourceId},cutout::RegionAttachment},library::MediaEntry};
use tokio_util::sync::CancellationToken;
pub async fn generate(state:&AppState,actor:&str,project:&str,revision:u64,original:&CompositionSource,source:&std::path::Path,source_hash:&str,points:&[[f64;2]],cancel:&CancellationToken)->Result<(RegionAttachment,CompositionSource,std::path::PathBuf)>{
 super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project editor access required"))?;
 let saved=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
 ensure!(saved.revision==revision,"stale smart region project revision");
 ensure!(super::concat_cutout_access::source_for_document(state,actor,&saved.document,original.id.as_str()).await?==source,"smart region source changed");
 let _models=tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("smart region request cancelled"),guard=state.sam_model_ops.lock()=>guard};
 let(config,_)=super::sam_models::config_for_owner(state,actor).await.map_err(|_|anyhow::anyhow!("SAM model unavailable"))?;
 crate::services::local_sam::identity(&config).await?;
 let id=uuid::Uuid::new_v4().to_string();let space=state.db.claim_project_narration(project,actor,&id).await?;
 let mut owned_path=None;
 let result=async{
  let root=crate::services::tts_model_download::directory(&state.sources_dir(),&std::path::Path::new("spaces").join(&space)).await?;
  let filename=format!("{id}.png");let path=root.join(&filename);
  ensure!(!path.try_exists()?,"smart region output collision");
  let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();
  let metadata=crate::services::local_sam::region(&state.process_runtime,&config,crate::services::local_sam::Input{source,source_hash,points},&path,cancel,std::time::Duration::from_secs(1800),&tx).await?;
  owned_path=Some(path.clone());
  let attachment=RegionAttachment{source_id:original.id.as_str().into(),mask_source_id:id.clone(),source_hash:metadata.source_hash,mask_hash:metadata.mask_hash,mask_file_hash:metadata.output_hash,encoder_hash:metadata.encoder_hash,decoder_hash:metadata.decoder_hash,runtime_hash:metadata.runtime_hash,prompt_hash:metadata.prompt_hash,points:points.to_vec()};attachment.validate()?;
  super::cutout_assets::authorize(state,project,actor).await.map_err(|_|anyhow::anyhow!("project access revoked"))?;
  let current=state.db.get_composition_project_for(project,actor).await?.ok_or_else(||anyhow::anyhow!("project unavailable"))?;
  ensure!(current.revision==revision&&current.document==saved.document,"smart region project/binding changed");
  ensure!(super::concat_cutout_access::source_for_document(state,actor,&current.document,original.id.as_str()).await?==source&&!cancel.is_cancelled(),"smart region access changed/cancelled");
  let entry=MediaEntry::from_result("source",&serde_json::json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"storageKey":format!("spaces/{space}/{filename}"),"mediaType":"image","width":256,"height":256}));
  ensure!(state.library.get(&id).await.is_none(),"smart region library collision");ensure!(state.library.add(entry.clone()).await,"smart region library persistence failed");
  state.index_media(&entry).await;
  let mut descriptor=original.clone();descriptor.id=SourceId::parse(id.clone())?;descriptor.width=256;descriptor.height=256;
  Ok((attachment,descriptor,path))
 }.await;
 if result.is_err()&&state.library.get(&id).await.is_none(){if let Some(path)=owned_path{let _=tokio::fs::remove_file(path).await;}let _=state.db.release_space_source_claim(&id).await;}
 result
}
