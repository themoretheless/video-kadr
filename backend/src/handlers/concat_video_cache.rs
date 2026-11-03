//! Internal project cache bridge. Authenticated actor/media IDs only; no client paths.
use anyhow::{ensure, Result};
use tokio_util::sync::CancellationToken;
use crate::{state::AppState, services::concat_video_analysis::{PublishedCache, RunCache}};
const CACHE_BUDGET: u64 = 64 * 1024 * 1024 * 1024;

async fn physical_id(state: &AppState, actor: &str, project: &str, source_id: &str, revision: u64) -> Result<String> {
    super::cutout_assets::authorize(state, project, actor).await.map_err(|_| anyhow::anyhow!("project editor access required"))?;
    let saved = state.db.get_composition_project_for(project, actor).await?.ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
    let physical = super::concat_cutout_access::library_id(&saved.document, source_id)?;
    ensure!(saved.revision == revision && saved.source_ids.iter().any(|id| id == &physical), "stale project or unrelated video source");
    Ok(physical)
}
async fn source(state: &AppState, actor: &str, project: &str, source_id: &str, revision: u64) -> Result<std::path::PathBuf> {
    let physical = physical_id(state, actor, project, source_id, revision).await?;
    super::cutout_jobs::source(state, &physical, actor).await
}
/// Publish first, then atomically register only if editor membership, revision and
/// source reference still hold. Failed registration removes only this new generation.
pub async fn publish(state: &AppState, actor: &str, project: &str, source_id: &str, revision: u64, cache: &mut RunCache, cancel: &CancellationToken) -> Result<String> {
    let path = source(state, actor, project, source_id, revision).await?;
    let before = crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await?;
    let root = state.storage.join("concat-video-caches");
    tokio::fs::create_dir_all(&root).await?;
    let publication=cache.prepare_incremental(&root,CACHE_BUDGET,cancel).await?;
    let receipt=&publication.receipt;
    let result: Result<String> = async {
        ensure!(receipt.video_hash == before.1, "video cache source identity mismatch");
        ensure!(source(state, actor, project, source_id, revision).await? == path && crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await? == before && !cancel.is_cancelled(), "video cache source/access changed");
        let generation = receipt.directory.file_name().and_then(|s| s.to_str()).ok_or_else(|| anyhow::anyhow!("invalid cache generation"))?;
        let id = uuid::Uuid::new_v4().to_string();
        let changed = sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) SELECT ?,p.id,?,?,?,?,?,? FROM composition_projects p JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id WHERE p.id=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') AND s.source_id=?")
            .bind(&id).bind(source_id).bind(&receipt.video_hash).bind(&receipt.model_hash).bind(&receipt.runtime_hash).bind(&receipt.manifest_hash).bind(generation)
            .bind(project).bind(i64::try_from(revision)?).bind(actor).bind(physical_id(state,actor,project,source_id,revision).await?).execute(state.db.pool()).await?.rows_affected();
        ensure!(changed == 1, "project changed before video cache registration");
        Ok(id)
    }.await;
    if result.is_err(){let _=tokio::fs::remove_dir_all(&receipt.directory).await;}else{cache.accept_publication(publication)?;}
    result
}
/// Restore the newest registered generation matching the current source and host runtime.
/// A changed source/runtime is a miss; corrupted matching data is an explicit error.
pub async fn restore(state: &AppState, actor: &str, project: &str, source_id: &str, revision: u64, runtime_hash: &str, cancel: &CancellationToken) -> Result<Option<RunCache>> {
    restore_for_model(state,actor,project,source_id,revision,runtime_hash,crate::services::local_person::MODEL_HASH,cancel).await
}
/// Model-specific lookup; legacy callers continue to resolve Person/RVM only.
pub async fn restore_for_model(state:&AppState,actor:&str,project:&str,source_id:&str,revision:u64,runtime_hash:&str,model_hash:&str,cancel:&CancellationToken)->Result<Option<RunCache>>{
    let path = source(state, actor, project, source_id, revision).await?;
    ensure!(!cancel.is_cancelled(), "video cache restore cancelled");
    let before = crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await?;
    let row: Option<(String, String)> = sqlx::query_as("SELECT c.generation,c.manifest_hash FROM concat_video_project_caches c JOIN composition_projects p ON p.id=c.project_id JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id AND s.source_id=? WHERE c.project_id=? AND c.source_id=? AND c.video_hash=? AND c.model_hash=? AND c.runtime_hash=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') ORDER BY c.rowid DESC LIMIT 1")
        .bind(physical_id(state,actor,project,source_id,revision).await?).bind(project).bind(source_id).bind(&before.1).bind(model_hash).bind(runtime_hash).bind(i64::try_from(revision)?).bind(actor).fetch_optional(state.db.pool()).await?;
    let Some((generation, manifest_hash)) = row else { return Ok(None); };
    let suffix = generation.strip_prefix("concat-video-cache-").ok_or_else(|| anyhow::anyhow!("invalid stored cache generation"))?;
    ensure!(uuid::Uuid::parse_str(suffix)?.to_string() == suffix, "invalid stored cache generation");
    let receipt = PublishedCache { model_hash:model_hash.into(), directory: state.storage.join("concat-video-caches").join(generation), manifest_hash, video_hash: before.1.clone(), runtime_hash: runtime_hash.into() };
    let cache = RunCache::restore(&receipt, CACHE_BUDGET, cancel).await?;
    ensure!(source(state, actor, project, source_id, revision).await? == path && crate::project_archive::fingerprint_file_with_limit(&path, CACHE_BUDGET).await? == before && !cancel.is_cancelled(), "video cache source/access changed");
    Ok(Some(cache))
}

/// Source-level family decision, as in Concat's MaskStore metadata. Subject
/// compatibility is checked by the shared policy, not by creating separate stores.
pub struct Choice {pub video_hash:String,pub model:crate::services::concat_cutout_selection::Model,pub model_hash:String,pub runtime_hash:String}
pub async fn remember_choice(state:&AppState,actor:&str,project:&str,source_id:&str,revision:u64,cache:&RunCache,cancel:&CancellationToken)->Result<()>{
    ensure!(!cache.is_empty()&&!cancel.is_cancelled(),"cannot choose an empty/cancelled video cache");
    crate::services::concat_video_selection::model_from_hash(cache.model_hash())?;
    let path=source(state,actor,project,source_id,revision).await?;
    ensure!(crate::project_archive::fingerprint_file_with_limit(&path,CACHE_BUDGET).await?.1==cache.video_hash(),"video choice source changed");
    ensure!(!cancel.is_cancelled(),"video model choice cancelled");
    let changed=sqlx::query("INSERT INTO concat_video_model_choices(project_id,source_id,video_hash,model_hash,runtime_hash) SELECT p.id,c.source_id,c.video_hash,c.model_hash,c.runtime_hash FROM concat_video_project_caches c JOIN composition_projects p ON p.id=c.project_id JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id AND s.source_id=? WHERE c.project_id=? AND c.source_id=? AND c.video_hash=? AND c.model_hash=? AND c.runtime_hash=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') ORDER BY c.rowid DESC LIMIT 1 ON CONFLICT(project_id,source_id,video_hash) DO UPDATE SET model_hash=excluded.model_hash,runtime_hash=excluded.runtime_hash")
        .bind(physical_id(state,actor,project,source_id,revision).await?).bind(project).bind(source_id).bind(cache.video_hash()).bind(cache.model_hash()).bind(cache.runtime_hash()).bind(i64::try_from(revision)?).bind(actor).execute(state.db.pool()).await?.rows_affected();
    ensure!(changed==1,"published video cache required before choosing its model");Ok(())
}
pub async fn choice(state:&AppState,actor:&str,project:&str,source_id:&str,revision:u64,cancel:&CancellationToken)->Result<Option<Choice>>{
    let path=source(state,actor,project,source_id,revision).await?;ensure!(!cancel.is_cancelled(),"video choice lookup cancelled");
    let before=crate::project_archive::fingerprint_file_with_limit(&path,CACHE_BUDGET).await?;
    let stored:Option<(String,String)>=sqlx::query_as("SELECT model_hash,runtime_hash FROM concat_video_model_choices WHERE project_id=? AND source_id=? AND video_hash=?").bind(project).bind(source_id).bind(&before.1).fetch_optional(state.db.pool()).await?;
    // Old RVM registries predate explicit choice rows. Their newest completed
    // generation is already a recorded model decision, not undecided Auto.
    let stored=if stored.is_some(){stored}else{sqlx::query_as("SELECT model_hash,runtime_hash FROM concat_video_project_caches WHERE project_id=? AND source_id=? AND video_hash=? ORDER BY rowid DESC LIMIT 1").bind(project).bind(source_id).bind(&before.1).fetch_optional(state.db.pool()).await?};
    ensure!(source(state,actor,project,source_id,revision).await?==path&&crate::project_archive::fingerprint_file_with_limit(&path,CACHE_BUDGET).await?==before&&!cancel.is_cancelled(),"video choice source/access changed");
    stored.map(|(model_hash,runtime_hash)|Ok(Choice{video_hash:before.1.clone(),model:crate::services::concat_video_selection::model_from_hash(&model_hash)?,model_hash,runtime_hash})).transpose()
}

/// A verified encoded-source snapshot. Published rows stay keyed to this hash,
/// so a changed source cannot resolve old masks even during a cancelled request.
/// Session append checks current ownership/revision/path, without hashing the
/// entire encoded video again for every individual frame. Finish rehashes once.
pub struct PublicationSession{storage:std::path::PathBuf,path:std::path::PathBuf,actor:String,project:String,source_id:String,revision:u64,video_hash:String,model_hash:String,runtime_hash:String}
impl PublicationSession{
 pub async fn begin(state:&AppState,actor:&str,project:&str,source_id:&str,revision:u64,cache:&RunCache,cancel:&CancellationToken)->Result<Self>{
  ensure!(!cancel.is_cancelled(),"video publication cancelled");crate::services::concat_video_selection::model_from_hash(cache.model_hash())?;let path=source(state,actor,project,source_id,revision).await?;ensure!(crate::project_archive::fingerprint_file_with_limit(&path,CACHE_BUDGET).await?.1==cache.video_hash()&&!cancel.is_cancelled(),"video publication snapshot changed");
  Ok(Self{storage:state.storage.clone(),path,actor:actor.into(),project:project.into(),source_id:source_id.into(),revision,video_hash:cache.video_hash().into(),model_hash:cache.model_hash().into(),runtime_hash:cache.runtime_hash().into()})
 }
 async fn check(&self,state:&AppState,cache:&RunCache,cancel:&CancellationToken)->Result<()>{
  ensure!(!cancel.is_cancelled()&&state.storage==self.storage&&cache.video_hash()==self.video_hash&&cache.model_hash()==self.model_hash&&cache.runtime_hash()==self.runtime_hash,"video publication context changed/cancelled");
  ensure!(source(state,&self.actor,&self.project,&self.source_id,self.revision).await?==self.path,"video publication source path changed");Ok(())
 }
 pub async fn append(&self,state:&AppState,cache:&mut RunCache,cancel:&CancellationToken)->Result<()>{
  self.check(state,cache,cancel).await?;ensure!(!cache.is_empty(),"cannot publish an empty video prefix");let root=state.storage.join("concat-video-caches");tokio::fs::create_dir_all(&root).await?;let publication=cache.prepare_incremental(&root,CACHE_BUDGET,cancel).await?;
  let result:Result<()>=async{
   self.check(state,cache,cancel).await?;let receipt=&publication.receipt;let generation=receipt.directory.file_name().and_then(|s|s.to_str()).ok_or_else(||anyhow::anyhow!("invalid video generation"))?;let id=uuid::Uuid::new_v4().to_string();let physical=physical_id(state,&self.actor,&self.project,&self.source_id,self.revision).await?;let mut transaction=state.db.pool().begin().await?;
   let changed=sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) SELECT ?,p.id,?,?,?,?,?,? FROM composition_projects p JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id WHERE p.id=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') AND s.source_id=?").bind(&id).bind(&self.source_id).bind(&receipt.video_hash).bind(&receipt.model_hash).bind(&receipt.runtime_hash).bind(&receipt.manifest_hash).bind(generation).bind(&self.project).bind(i64::try_from(self.revision)?).bind(&self.actor).bind(&physical).execute(&mut *transaction).await?.rows_affected();ensure!(changed==1,"project changed before prefix registration");
   let changed=sqlx::query("INSERT INTO concat_video_model_choices(project_id,source_id,video_hash,model_hash,runtime_hash) SELECT project_id,source_id,video_hash,model_hash,runtime_hash FROM concat_video_project_caches WHERE id=? ON CONFLICT(project_id,source_id,video_hash) DO UPDATE SET model_hash=excluded.model_hash,runtime_hash=excluded.runtime_hash").bind(&id).execute(&mut *transaction).await?.rows_affected();ensure!(changed==1&&!cancel.is_cancelled(),"video prefix choice registration cancelled");transaction.commit().await?;Ok(())
  }.await;
  if result.is_err(){let _=tokio::fs::remove_dir_all(&publication.receipt.directory).await;}else{cache.accept_publication(publication)?;}result
 }
 pub async fn finish(&self,state:&AppState,cache:&RunCache,cancel:&CancellationToken)->Result<()>{self.check(state,cache,cancel).await?;ensure!(crate::project_archive::fingerprint_file_with_limit(&self.path,CACHE_BUDGET).await?.1==self.video_hash&&!cancel.is_cancelled(),"video source changed during progressive analysis");Ok(())}
}
