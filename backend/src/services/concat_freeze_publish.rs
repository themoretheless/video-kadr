//! Revision-bound still preparation/publication. HTTP and timeline splice remain caller-owned.
use super::concat_freeze_capture::{self, CapturedStill, Tools};
use crate::{
    db::CompositionProject,
    domain::{
        composition::{Composition, CompositionTrack, PlaybackMode},
        concat_retime_edit::Clock,
    },
    library::MediaEntry,
    state::AppState,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{io::AsyncWriteExt, time::Instant};
use tokio_util::sync::CancellationToken;
pub struct Target {
    pub project: String,
    pub revision: u64,
    pub composition: Option<String>,
    pub clip: String,
    pub time: f64,
}
pub struct PreparedFreeze {
    target: Target,
    actor: String,
    saved: CompositionProject,
    source: String,
    path: PathBuf,
    hash: String,
    frame: CapturedStill,
}
pub struct PublishedFreeze {
    pub media: MediaEntry,
    pub project: CompositionProject,
}
async fn authorize(state: &AppState, project: &str, actor: &str) -> Result<()> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM composition_project_members WHERE project_id=? AND actor=?",
    )
    .bind(project)
    .bind(actor)
    .fetch_optional(state.db.pool())
    .await?;
    ensure!(
        matches!(role.as_deref(), Some("owner" | "editor")),
        "freeze requires project editor access"
    );
    Ok(())
}
async fn source(state: &AppState, project: &str, actor: &str, id: &str) -> Result<PathBuf> {
    authorize(state, project, actor).await?;
    if let Some(space) = state.db.source_space_id(id).await? {
        ensure!(
            state.db.is_space_member(&space, actor).await?,
            "freeze source is private"
        );
    }
    let entry = state
        .library
        .get(id)
        .await
        .filter(|e| e.kind == "source")
        .context("freeze original source missing")?;
    Ok(state.library.resolve_media_path(&entry).await?)
}
async fn identity(path: &Path, deadline: Instant, cancel: &CancellationToken) -> Result<String> {
    ensure!(
        tokio::fs::symlink_metadata(path)
            .await?
            .file_type()
            .is_file(),
        "freeze original must be a regular file"
    );
    tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("freeze cancelled"),result=tokio::time::timeout_at(deadline,crate::project_archive::fingerprint_file_with_limit(path,64*1024*1024*1024))=>Ok(result??.1)}
}
/// Captures only a server-resolved referenced video. The opaque result cannot be
/// constructed from client JPEG/path JSON or transferred to a different actor.
pub async fn prepare(
    state: &AppState,
    actor: &str,
    target: Target,
    tools: Tools<'_>,
    limit: Duration,
    cancel: &CancellationToken,
) -> Result<Option<PreparedFreeze>> {
    ensure!(
        !cancel.is_cancelled() && !limit.is_zero() && limit <= Duration::from_secs(120),
        "invalid freeze preparation deadline"
    );
    let deadline = Instant::now() + limit;
    authorize(state, &target.project, actor).await?;
    let saved = state
        .db
        .get_composition_project_for(&target.project, actor)
        .await?
        .context("freeze project missing")?;
    ensure!(
        saved.revision == target.revision,
        "stale freeze project revision"
    );
    let content = if saved.document.get("compositions").is_some() {
        let id = target
            .composition
            .as_ref()
            .context("freeze composition required")?;
        saved.document["compositions"][id]["content"].clone()
    } else {
        ensure!(
            target.composition.is_none(),
            "unexpected freeze composition"
        );
        saved.document.clone()
    };
    let composition: Composition = serde_json::from_value(content)?;
    let (clip, locked) = composition
        .tracks
        .iter()
        .find_map(|track| {
            if let CompositionTrack::Video { clips, locked, .. } = track {
                clips
                    .iter()
                    .find(|c| c.id.as_str() == target.clip)
                    .map(|clip| (clip, *locked))
            } else {
                None
            }
        })
        .context("freeze video clip missing")?;
    ensure!(!locked, "freeze video track is locked");
    ensure!(
        clip.playback_mode == PlaybackMode::Forward,
        "native reverse/freeze capture needs its own clock qualification"
    );
    let p = &clip.placement;
    let clock = if let Some(retained) = &p.concat_retime {
        retained.validate_placement(p)?;
        retained
            .document_clock
            .clone()
            .context("freeze requires original document clock")?
    } else {
        ensure!(
            p.speed_ramp.is_none()
                && p.source_timeline_offset_tick.unwrap_or(0) == 0
                && p.source_head_timeline_tick.unwrap_or(0) == 0,
            "freeze requires an intact scalar source clock"
        );
        Clock {
            start: p.timeline_start_tick as f64 / 1e6,
            duration: p.timeline_duration_ticks()? as f64 / 1e6,
            source_start: p.source_in_tick as f64 / 1e6,
            speed: p.speed,
            speed_curve: None,
        }
    };
    if clock.freeze(target.time, Some(1.))?.is_none() {
        return Ok(None);
    }
    let capture = clock.still_capture(target.time)?;
    let id = clip.source_id.as_str().to_owned();
    ensure!(
        saved.source_ids.contains(&id),
        "freeze source not referenced by saved project"
    );
    let path = source(state, &target.project, actor, &id).await?;
    let hash = identity(&path, deadline, cancel).await?;
    let frame = concat_freeze_capture::capture(
        &state.process_runtime,
        tools,
        &path,
        &capture,
        deadline.saturating_duration_since(Instant::now()),
        cancel,
    )
    .await?;
    ensure!(
        identity(&path, deadline, cancel).await? == hash,
        "freeze original changed during capture"
    );
    let current = state
        .db
        .get_composition_project_for(&target.project, actor)
        .await?
        .context("freeze project unavailable")?;
    ensure!(
        current.revision == target.revision,
        "freeze project changed during capture"
    );
    Ok(Some(PreparedFreeze {
        target,
        actor: actor.into(),
        saved,
        source: id,
        path,
        hash,
        frame,
    }))
}
struct Staged(PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
/// The builder owns the timeline edit; this function validates it before CAS.
/// Ordinary failure cleanup removes only the fresh media/file/claim created here.
pub async fn publish<F, Fut>(
    state: &AppState,
    prepared: PreparedFreeze,
    cancel: &CancellationToken,
    build: F,
) -> Result<PublishedFreeze>
where
    F: FnOnce(Value, MediaEntry) -> Fut,
    Fut: Future<Output = Result<Value>>,
{
    let PreparedFreeze {
        target,
        actor,
        saved,
        source: source_id,
        path,
        hash,
        frame,
    } = prepared;
    authorize(state, &target.project, &actor).await?;
    ensure!(!cancel.is_cancelled(), "freeze publication cancelled");
    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.jpg");
    ensure!(
        state.library.get(&id).await.is_none(),
        "generated freeze ID collision"
    );
    let space = state
        .db
        .claim_project_freeze_media(&target.project, &actor, &id, target.revision)
        .await?;
    let mut linked: Option<PathBuf> = None;
    let mut added = false;
    let result=async{
        let root=super::tts_model_download::directory(&state.sources_dir(),&Path::new("spaces").join(&space)).await?;
        let staged=Staged(state.staging_dir().join(format!("freeze-{id}.jpg.tmp")));
        let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{options.mode(0o600);}
        let mut output=options.open(&staged.0).await?;
        output.write_all(&frame.jpeg).await?;output.sync_all().await?;drop(output);
        let dest=root.join(&filename);tokio::fs::hard_link(&staged.0,&dest).await?;linked=Some(dest.clone());
        let key=dest.strip_prefix(state.sources_dir())?.to_str().context("invalid freeze storage key")?.to_owned();
        let media=MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"storageKey":key,"url":format!("/files/sources/{filename}"),"mediaType":"image","title":"Стоп-кадр","width":frame.width,"height":frame.height,"sizeBytes":frame.jpeg.len()}));
        ensure!(!cancel.is_cancelled(),"freeze publication cancelled");
        added=state.library.add(media.clone()).await;ensure!(added,"freeze library persistence failed");
        let document=tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("freeze publication cancelled"),result=tokio::time::timeout(Duration::from_secs(30),build(saved.document.clone(),media.clone()))=>result??};
        let contents:Vec<_>=if let Some(nodes)=document["compositions"].as_object(){nodes.values().filter_map(|n|n.get("content")).collect()}else{vec![&document]};
        let mut found=false;
        for content in contents{if let Some(source)=content["sources"].get(&id){found=true;ensure!(source["id"]==id&&source["kind"]=="image"&&source["width"]==frame.width&&source["height"]==frame.height&&source["hasAudio"]==false&&source["durationTicks"]==0,"freeze still metadata changed in edit");}}
        ensure!(found,"freeze edit did not register the new still");
        if document.get("compositions").is_some(){let native:crate::domain::nested_composition::NestedCompositionProject=serde_json::from_value(document.clone())?;native.dependency_order()?;}else{let native:Composition=serde_json::from_value(document.clone())?;native.validate_native_surface()?;}
        let sources=crate::db::extract_document_source_ids(&document)?;ensure!(sources.contains(&id),"freeze edit did not reference the new still");
        let actual=source(state,&target.project,&actor,&source_id).await?;ensure!(actual==path,"freeze original path changed");
        ensure!(identity(&actual,Instant::now()+Duration::from_secs(30),cancel).await?==hash,"freeze original changed before commit");
        ensure!(identity(&dest,Instant::now()+Duration::from_secs(30),cancel).await?==format!("{:x}",Sha256::digest(&frame.jpeg)),"freeze JPEG changed before commit");
        ensure!(!cancel.is_cancelled(),"freeze publication cancelled");
        let project=state.db.update_composition_project_for(&target.project,&actor,&saved.name,&document,&sources,target.revision).await?.context("freeze project changed before commit")?;
        Ok::<_,anyhow::Error>(PublishedFreeze{media,project})
    }.await;
    match result {
        Ok(value) => {
            finish_journal(state, &id).await?;
            state.index_media(&value.media).await;
            Ok(value)
        }
        Err(error) => {
            // A concurrent committed editor may already reference the fresh ID.
            // Never delete media that has become part of somebody's saved work.
            if !state.db.retire_unused_freeze_media(&id).await? {
                finish_journal(state, &id).await?;
                return Err(error.context(format!(
                    "freeze still {id} retained because a committed project references it"
                )));
            }
            if added {
                let removed = state.library.remove(&id).await;
                ensure!(
                    removed,
                    "freeze rollback needs library recovery after error: {error}"
                );
            }
            if state.library.get(&id).await.is_none() {
                if let Some(path) = linked {
                    remove_if_present(&path).await?;
                }
                state.db.release_space_source_claim(&id).await?;
                finish_journal(state, &id).await?;
            }
            Err(error)
        }
    }
}

async fn finish_journal(state: &AppState, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM composition_freeze_publications WHERE source_id=?")
        .bind(id).execute(state.db.pool()).await?;
    Ok(())
}
async fn remove_if_present(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
/// Startup-only reconciliation, before accepting requests or starting media workers.
/// Never run concurrently with a publisher: a pending row is not a lease expiry.
pub async fn recover_at_startup(state: &AppState) -> Result<usize> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT source_id,space_id FROM composition_freeze_publications ORDER BY source_id")
        .fetch_all(state.db.pool()).await?;
    let mut recovered = 0;
    for (id, space) in rows {
        ensure!(uuid::Uuid::parse_str(&id)?.to_string()==id && uuid::Uuid::parse_str(&space)?.to_string()==space,"invalid freeze recovery identity");
        let filename = format!("{id}.jpg");
        let key = format!("spaces/{space}/{filename}");
        let entry = state.library.get(&id).await;
        if let Some(entry) = &entry {
            ensure!(entry.kind=="source" && entry.filename==filename && entry.storage_key.as_deref()==Some(key.as_str()),"freeze recovery library identity mismatch");
        }
        if state.db.retire_unused_freeze_media(&id).await? {
            if entry.is_some() {
                ensure!(state.library.remove(&id).await,"freeze recovery library persistence failed");
            }
            let root=super::tts_model_download::directory(&state.sources_dir(), &Path::new("spaces").join(&space)).await?;
            remove_if_present(&root.join(&filename)).await?;
            state.db.release_space_source_claim(&id).await?;
        } else {
            let entry = entry.context("committed freeze missing from library; recovery requires repair")?;
            ensure!(state.library.resolve_media_path(&entry).await?.is_file(),"committed freeze file missing; recovery requires repair");
        }
        remove_if_present(&state.staging_dir().join(format!("freeze-{id}.jpg.tmp"))).await?;
        finish_journal(state, &id).await?;
        recovered += 1;
    }
    Ok(recovered)
}


/// Publish a captured video still using the native lossless lane splice, then
/// revision-CAS the saved flat/nested project. Unsupported controls fail closed.
pub async fn publish_native(state: &AppState, prepared: PreparedFreeze, cancel: &CancellationToken) -> Result<PublishedFreeze> {
    let clip = prepared.target.clip.clone();
    let time = prepared.target.time;
    let composition = prepared.target.composition.clone();
    publish(state, prepared, cancel, move |mut document, media| async move {
        let source: crate::domain::composition::CompositionSource = serde_json::from_value(json!({"id":media.id,"kind":"image","durationTicks":0,"width":media.width,"height":media.height,"hasAudio":false}))?;
        let tail_id = uuid::Uuid::new_v4().to_string();
        let still_id = uuid::Uuid::new_v4().to_string();
        let image_track_id = uuid::Uuid::new_v4().to_string();
        let edit = |content: Composition| crate::domain::concat_freeze_native::splice(&content, &clip, time, &source, &tail_id, &still_id, &image_track_id)?.context("prepared freeze became a no-op");
        if document.get("compositions").is_some() {
            let id = composition.context("nested freeze composition missing")?;
            let node = document["compositions"].get_mut(&id).context("nested freeze composition disappeared")?;
            let content = edit(serde_json::from_value(node["content"].clone())?)?;
            if let Some(duration) = node["durationUs"].as_u64() {
                node["durationUs"] = json!(duration.max(content.native_duration_ticks()?));
            }
            node["content"] = serde_json::to_value(content)?;
        } else {
            document = serde_json::to_value(edit(serde_json::from_value(document)?)?)?;
        }
        Ok(document)
    }).await
}
