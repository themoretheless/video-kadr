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
        let physical=physical_id(state,actor,project,source_id,revision).await?;
        let mut transaction=state.db.pool().begin().await?;
        let changed = sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) SELECT ?,p.id,?,?,?,?,?,? FROM composition_projects p JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id WHERE p.id=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') AND s.source_id=?")
            .bind(&id).bind(source_id).bind(&receipt.video_hash).bind(&receipt.model_hash).bind(&receipt.runtime_hash).bind(&receipt.manifest_hash).bind(generation)
            .bind(project).bind(i64::try_from(revision)?).bind(actor).bind(&physical).execute(&mut *transaction).await?.rows_affected();
        ensure!(changed == 1, "project changed before video cache registration");
        retain_original(&mut transaction,project,source_id,&receipt.video_hash,&physical).await?;
        ensure!(!cancel.is_cancelled(),"video publication cancelled");transaction.commit().await?;
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
   retain_original(&mut transaction,&self.project,&self.source_id,&receipt.video_hash,&physical).await?;
   let changed=sqlx::query("INSERT INTO concat_video_model_choices(project_id,source_id,video_hash,model_hash,runtime_hash) SELECT project_id,source_id,video_hash,model_hash,runtime_hash FROM concat_video_project_caches WHERE id=? ON CONFLICT(project_id,source_id,video_hash) DO UPDATE SET model_hash=excluded.model_hash,runtime_hash=excluded.runtime_hash").bind(&id).execute(&mut *transaction).await?.rows_affected();ensure!(changed==1&&!cancel.is_cancelled(),"video prefix choice registration cancelled");transaction.commit().await?;Ok(())
  }.await;
  if result.is_err(){let _=tokio::fs::remove_dir_all(&publication.receipt.directory).await;}else{cache.accept_publication(publication)?;}result
 }
 pub async fn finish(&self,state:&AppState,cache:&RunCache,cancel:&CancellationToken)->Result<()>{self.check(state,cache,cancel).await?;ensure!(crate::project_archive::fingerprint_file_with_limit(&self.path,CACHE_BUDGET).await?.1==self.video_hash&&!cancel.is_cancelled(),"video source changed during progressive analysis");Ok(())}
}

/// Trusted project/source context; no client paths or publication receipts.
#[derive(Clone,Copy)]
pub struct ProjectSource<'a>{pub project:&'a str,pub source_id:&'a str,pub revision:u64}
/// Give another saved logical slot independent authority over a verified store.
/// Immutable generation files are shared; registry and family choice commit together.
/// Clip settings are a separate orchestration step. This wrapper transfers no regions.
pub async fn transfer_for_model(state:&AppState,actor:&str,from:ProjectSource<'_>,to:ProjectSource<'_>,runtime_hash:&str,model_hash:&str,cancel:&CancellationToken)->Result<String>{
    Ok(transfer_with_regions(state,actor,from,to,runtime_hash,model_hash,&[],cancel).await?.cache_id)
}
pub struct RegionTransfer{pub snapshot:crate::domain::video_region::Attachment,pub target_mask_source_id:String}
pub struct TransferredStore{pub cache_id:String,pub regions:Vec<crate::domain::video_region::Attachment>}
/// Atomically retain verified temporal region ownership alongside the mask store.
pub fn transfer_with_regions<'a>(state:&'a AppState,actor:&'a str,from:ProjectSource<'a>,to:ProjectSource<'a>,runtime_hash:&'a str,model_hash:&'a str,regions:&'a [RegionTransfer],cancel:&'a CancellationToken)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<TransferredStore>>+Send+'a>>{
 Box::pin(async move {
  let target=state.db.get_composition_project_for(to.project,actor).await?.ok_or_else(||anyhow::anyhow!("target project unavailable"))?;
  ensure!(target.revision==to.revision,"stale target region revision");
  let prepared=prepare_transfer(state,actor,from,to.source_id,&target.document,runtime_hash,model_hash,regions,cancel).await?;
  let mut transaction=state.db.pool().begin_with("BEGIN IMMEDIATE").await?;
  let cache_id=prepared.register(&mut transaction,to.project,to.revision).await?;
  transaction.commit().await?;
  Ok(TransferredStore{cache_id,regions:prepared.planned.into_iter().map(|(_,_,snapshot)|snapshot).collect()})
 })
}
/// Opaque verified input for registration inside a caller-owned save transaction.
/// No authority is granted until `register` rechecks both saved endpoints.
pub(crate) struct PreparedTransfer {
 actor:String,source_project:Option<String>,source_id:String,source_revision:u64,target_source:String,
 physical:String,target_document_hash:String,cache:RunCache,planned:Vec<(String,String,crate::domain::video_region::Attachment)>,cancel:CancellationToken,
}
pub(crate) fn prepare_transfer<'a>(state:&'a AppState,actor:&'a str,from:ProjectSource<'a>,target_source:&'a str,target_document:&'a serde_json::Value,runtime_hash:&'a str,model_hash:&'a str,regions:&'a [RegionTransfer],cancel:&'a CancellationToken)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<PreparedTransfer>>+Send+'a>>{
 Box::pin(async move {
    crate::services::concat_video_selection::model_from_hash(model_hash)?;
    let physical=physical_id(state,actor,from.project,from.source_id,from.revision).await?;
    ensure!(super::concat_cutout_access::library_id(target_document,target_source)?==physical,"video cache transfer requires the same registered physical source");
    let cache=restore_for_model(state,actor,from.project,from.source_id,from.revision,runtime_hash,model_hash,cancel).await?.ok_or_else(||anyhow::anyhow!("source project has no verified video cache"))?;
    ensure!(!cache.is_empty(),"cannot transfer an empty video cache");
    ensure!(regions.len()<=1024,"too many transferred video regions");
    let mut leases=std::collections::BTreeMap::new();let mut total=0u64;let mut planned=vec![];
    for region in regions{
        ensure!(region.snapshot.source_id==from.source_id&&region.snapshot.source_hash==cache.video_hash(),"transferred region belongs to another original");
        let key=serde_json::to_string(&region.snapshot)?;
        if !leases.contains_key(&key){
            total=total.checked_add(region.snapshot.frames.checked_mul(256*256).ok_or_else(||anyhow::anyhow!("region byte budget overflow"))?).ok_or_else(||anyhow::anyhow!("region byte budget overflow"))?;ensure!(total<=CACHE_BUDGET,"transferred region byte budget exceeded");
            leases.insert(key.clone(),super::concat_video_region_cache::load(state,actor,from.project,from.revision,&region.snapshot,cancel).await?);
        }
        let library=leases[&key].library_source_id.clone();
        let mask_id=crate::domain::composition::SourceId::parse(&region.target_mask_source_id)?;
        if super::concat_video_region_cache::declares_source(target_document,mask_id.as_str()){ensure!(super::concat_cutout_access::library_id(target_document,mask_id.as_str())?==library,"target region ID is bound to another library file");}
        let mut snapshot=region.snapshot.clone();snapshot.source_id=target_source.into();snapshot.mask_source_id=mask_id.as_str().into();snapshot.validate()?;
        planned.push((region.snapshot.mask_source_id.clone(),library,snapshot));
    }
    cache.published_receipt()?;
    let path=source(state,actor,from.project,from.source_id,from.revision).await?;
    ensure!(crate::project_archive::fingerprint_file_with_limit(&path,CACHE_BUDGET).await?.1==cache.video_hash()&&!cancel.is_cancelled(),"video cache transfer source/access changed");
    Ok(PreparedTransfer{actor:actor.into(),source_project:Some(from.project.into()),source_id:from.source_id.into(),source_revision:from.revision,target_source:target_source.into(),physical,target_document_hash:document_identity(target_document)?,cache,planned,cancel:cancel.clone()})
    })
}
impl PreparedTransfer {
 pub(crate) fn video_hash(&self)->&str{self.cache.video_hash()}
 pub(crate) fn register<'a>(&'a self,connection:&'a mut sqlx::SqliteConnection,target_project:&'a str,target_revision:u64)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<String>>+Send+'a>>{
  Box::pin(async move {
    let stored:String=sqlx::query_scalar("SELECT document_json FROM composition_projects WHERE id=? AND revision=?").bind(target_project).bind(i64::try_from(target_revision)?).fetch_optional(&mut *connection).await?.ok_or_else(||anyhow::anyhow!("target project changed before video registration"))?;
    ensure!(document_identity(&serde_json::from_str(&stored)?)?==self.target_document_hash,"target video document differs from verified transfer plan");
    let actor=self.actor.as_str();let physical=&self.physical;let cache=&self.cache;let planned=&self.planned;let cancel=&self.cancel;
    let from=self.source_project.as_deref().map(|project|ProjectSource{project,source_id:&self.source_id,revision:if project==target_project&&self.source_revision.checked_add(1)==Some(target_revision){target_revision}else{self.source_revision}});
    let to=ProjectSource{project:target_project,source_id:&self.target_source,revision:target_revision};
    let runtime_hash=cache.runtime_hash();let model_hash=cache.model_hash();
    let receipt=cache.published_receipt()?;
    let generation=receipt.directory.file_name().and_then(|s|s.to_str()).ok_or_else(||anyhow::anyhow!("invalid video cache generation"))?;
    ensure!(!cancel.is_cancelled(),"video cache transfer cancelled");
    for endpoint in from.into_iter().chain(std::iter::once(to)){
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_projects p JOIN composition_project_members m ON m.project_id=p.id JOIN composition_project_sources s ON s.project_id=p.id WHERE p.id=? AND p.revision=? AND m.actor=? AND m.role IN ('owner','editor') AND s.source_id=?)").bind(endpoint.project).bind(i64::try_from(endpoint.revision)?).bind(actor).bind(physical).fetch_one(&mut *connection).await?;
        ensure!(allowed,"video transfer project/source authority changed");
    }
    let source_row=if let Some(from)=from {
        let source_row:Option<String>=sqlx::query_scalar("SELECT id FROM concat_video_project_caches WHERE project_id=? AND source_id=? AND video_hash=? AND model_hash=? AND runtime_hash=? AND generation=? AND manifest_hash=? ORDER BY rowid DESC LIMIT 1").bind(from.project).bind(from.source_id).bind(cache.video_hash()).bind(model_hash).bind(runtime_hash).bind(generation).bind(&receipt.manifest_hash).fetch_optional(&mut *connection).await?;
        Some(source_row.ok_or_else(||anyhow::anyhow!("verified source cache registration disappeared"))?)
    }else{None};
    let existing:Option<String>=sqlx::query_scalar("SELECT id FROM concat_video_project_caches WHERE project_id=? AND source_id=? AND video_hash=? AND model_hash=? AND runtime_hash=? AND generation=? AND manifest_hash=? ORDER BY rowid LIMIT 1").bind(to.project).bind(to.source_id).bind(cache.video_hash()).bind(model_hash).bind(runtime_hash).bind(generation).bind(&receipt.manifest_hash).fetch_optional(&mut *connection).await?;
    let id=if let Some(id)=existing{id}else{
        let id=uuid::Uuid::new_v4().to_string();
        let changed=if let Some(source_row)=&source_row {sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) SELECT ?,?,?,video_hash,model_hash,runtime_hash,manifest_hash,generation FROM concat_video_project_caches WHERE id=?").bind(&id).bind(to.project).bind(to.source_id).bind(&source_row).execute(&mut *connection).await?.rows_affected()}else{
            sqlx::query("INSERT INTO concat_video_project_caches(id,project_id,source_id,video_hash,model_hash,runtime_hash,manifest_hash,generation) VALUES(?,?,?,?,?,?,?,?)").bind(&id).bind(to.project).bind(to.source_id).bind(cache.video_hash()).bind(model_hash).bind(runtime_hash).bind(&receipt.manifest_hash).bind(generation).execute(&mut *connection).await?.rows_affected()
        };ensure!(changed==1,"source cache disappeared during transfer");id
    };
    retain_original(connection,to.project,to.source_id,cache.video_hash(),physical).await?;
    for(source_mask,library,snapshot)in planned{
        let allowed:bool=if let Some(from)=from {sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_project_sources WHERE project_id=? AND source_id=?) OR EXISTS(SELECT 1 FROM person_video_project_resources WHERE project_id=? AND role='sam-video-regions' AND source_id=? AND file_hash=?) OR EXISTS(SELECT 1 FROM concat_video_project_region_refs WHERE project_id=? AND source_id=? AND video_hash=? AND mask_source_id=? AND library_source_id=? AND file_hash=?)").bind(from.project).bind(library).bind(from.project).bind(library).bind(&snapshot.mask_file_hash).bind(from.project).bind(from.source_id).bind(cache.video_hash()).bind(source_mask).bind(library).bind(&snapshot.mask_file_hash).fetch_one(&mut *connection).await?}else{sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM composition_project_sources WHERE project_id=? AND source_id=?)").bind(to.project).bind(library).fetch_one(&mut *connection).await?};ensure!(allowed,"source region authority changed");
        let existing:Option<String>=sqlx::query_scalar("SELECT library_source_id FROM concat_video_project_region_refs WHERE project_id=? AND source_id=? AND video_hash=? AND mask_source_id=? AND file_hash=?").bind(to.project).bind(to.source_id).bind(cache.video_hash()).bind(&snapshot.mask_source_id).bind(&snapshot.mask_file_hash).fetch_optional(&mut *connection).await?;ensure!(existing.as_ref().is_none_or(|old|old==library),"target region reservation changed physical file");
        sqlx::query("INSERT INTO concat_video_project_region_refs(project_id,source_id,video_hash,mask_source_id,library_source_id,file_hash) VALUES(?,?,?,?,?,?) ON CONFLICT DO NOTHING").bind(to.project).bind(to.source_id).bind(cache.video_hash()).bind(&snapshot.mask_source_id).bind(library).bind(&snapshot.mask_file_hash).execute(&mut *connection).await?;
    }
    let changed=sqlx::query("INSERT INTO concat_video_model_choices(project_id,source_id,video_hash,model_hash,runtime_hash) SELECT project_id,source_id,video_hash,model_hash,runtime_hash FROM concat_video_project_caches WHERE id=? ON CONFLICT(project_id,source_id,video_hash) DO UPDATE SET model_hash=excluded.model_hash,runtime_hash=excluded.runtime_hash").bind(&id).execute(&mut *connection).await?.rows_affected();
    ensure!(changed==1&&!cancel.is_cancelled(),"video cache transfer cancelled");Ok(id)
  })
 }
}

fn document_identity(document:&serde_json::Value)->Result<String>{
 use sha2::{Digest,Sha256};Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(document)?)))
}

/// Archived bytes are uploaded data, not authority borrowed from another project.
/// Verify all data before the save transaction grants this target independent ownership.
pub(crate) fn prepare_archive<'a>(state:&'a AppState,actor:&'a str,target_source:&'a str,target_document:&'a serde_json::Value,original:&'a std::path::Path,receipt:&'a PublishedCache,identity:&'a crate::services::concat_video_analysis::portable::Identity,regions:&'a [(crate::domain::video_region::Attachment,String,std::path::PathBuf)],cancel:&'a CancellationToken)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<PreparedTransfer>>+Send+'a>>{
 Box::pin(async move {
    ensure!(receipt.directory.parent()==Some(state.storage.join("concat-video-caches").as_path()),"archive generation is outside the owned cache root");
    let cache=RunCache::restore(receipt,CACHE_BUDGET,cancel).await?;
    ensure!(!cache.is_empty()&&cache.portable_identity()?==*identity,"archived cache identity mismatch");
    ensure!(crate::project_archive::fingerprint_file(original).await?.1==cache.video_hash(),"archived original hash differs from cache");
    let physical=super::concat_cutout_access::library_id(target_document,target_source)?;
    let mut planned=vec![];let mut seen=std::collections::BTreeSet::new();let mut total=0u64;
    ensure!(regions.len()<=1024,"too many archived temporal regions");
    for(snapshot,library,movie)in regions {
        ensure!(snapshot.source_id==target_source&&snapshot.source_hash==cache.video_hash()&&super::concat_cutout_access::library_id(target_document,&snapshot.mask_source_id)?==*library,"archived temporal region scope mismatch");
        let key=serde_json::to_string(snapshot)?;if seen.insert(key){
            total=total.checked_add(snapshot.frames.checked_mul(256*256).ok_or_else(||anyhow::anyhow!("archive region size overflow"))?).ok_or_else(||anyhow::anyhow!("archive region size overflow"))?;ensure!(total<=CACHE_BUDGET,"archive region budget exceeded");
            drop(super::concat_video_region_cache::verify_movie(state,movie,snapshot,library,cancel).await?);
        }
        planned.push((snapshot.mask_source_id.clone(),library.clone(),snapshot.clone()));
    }
    ensure!(!cancel.is_cancelled(),"archive video verification cancelled");
    Ok(PreparedTransfer{actor:actor.into(),source_project:None,source_id:target_source.into(),source_revision:0,target_source:target_source.into(),physical,target_document_hash:document_identity(target_document)?,cache,planned,cancel:cancel.clone()})
 })
}

async fn retain_original(connection:&mut sqlx::SqliteConnection,project:&str,source:&str,hash:&str,physical:&str)->Result<()>{
 sqlx::query("INSERT INTO concat_video_project_source_refs(project_id,source_id,video_hash,library_source_id) VALUES(?,?,?,?) ON CONFLICT DO NOTHING").bind(project).bind(source).bind(hash).bind(physical).execute(connection).await?;Ok(())
}
