//! Import cutouts by saved-project references, never client mask assertions.
use crate::{
    domain::{
        composition::{CompositionClipId, SourceId},
        concat_project_import::{ImportReport, ImportRequest, ReadyStillBinding, StillBindings},
    },
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use anyhow::{ensure, Context, Result};
use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedReference {
    timeline_id: String,
    clip_id: String,
    project_id: String,
    composition_id: Option<String>,
    source_clip_id: String,
    expected_revision: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    request: ImportRequest,
    #[serde(default)]
    prepared_stills: Vec<SavedReference>,
    #[serde(default)]
    prepared_videos: Vec<SavedReference>,
}
fn settings(
    document: &Value,
    timeline: &str,
    clip: &str,
    kind: &str,
) -> Result<crate::domain::concat_cutout_import::Plan> {
    let timeline = if let Some(timelines) = document
        .get("timelines")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
    {
        timelines
            .iter()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(timeline))
            .context("destination timeline missing")?
    } else {
        ensure!(timeline == "TL1", "destination timeline missing");
        document
    };
    let clip = timeline
        .get("clips")
        .and_then(Value::as_array)
        .and_then(|clips| {
            clips
                .iter()
                .find(|c| c.get("id").and_then(Value::as_str) == Some(clip))
        })
        .context("destination clip missing")?;
    ensure!(
        clip.get("kind").and_then(Value::as_str) == Some(kind),
        "prepared cutout destination kind mismatch"
    );
    crate::domain::concat_cutout_import::plan(
        clip.get("cutout")
            .context("destination cutout settings missing")?,
    )
}
pub async fn handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(input): ApiJson<Request>,
) -> AppResult<([(&'static str, &'static str); 1], Json<ImportReport>)> {
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    if input.prepared_stills.len()+input.prepared_videos.len()==0 || input.prepared_stills.len()+input.prepared_videos.len() > 16 * 512 {
        return Err(AppError::bad_request(
            "Expected 1..8192 saved cutout references",
        ));
    }
    // Preserve explicit authorization errors before admitting expensive decoding.
    for reference in input.prepared_stills.iter().chain(&input.prepared_videos) {
        super::cutout_assets::authorize(&state, &reference.project_id, &actor).await?;
    }
    for source in input.request.media_bindings.values() {
        super::library::authorize_source_access(&state, source.id.as_str(), &headers).await?;
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    let _guard = cancel.clone().drop_guard();
    let worker = tokio::spawn(async move {
        let result: Result<ImportReport> = async {
            let _permit = super::concat_cutout_analyse::acquire_analysis(&state, &cancel).await?;
            let mut media_paths = BTreeMap::new();
            for source in input.request.media_bindings.values() {
                media_paths.insert(
                    source.id.clone(),
                    super::cutout_jobs::source(&state, source.id.as_str(), &actor).await?,
                );
            }
            let mut ready = StillBindings::new();
            let mut checks = vec![];
            for reference in input.prepared_stills {
                ensure!(!cancel.is_cancelled(), "ready import cancelled");
                ensure!(
                    !ready
                        .get(&reference.timeline_id)
                        .is_some_and(|clips| clips.contains_key(&reference.clip_id)),
                    "duplicate prepared destination"
                );
                let plan = settings(
                    &input.request.document,
                    &reference.timeline_id,
                    &reference.clip_id,
                    "image",
                )?;
                let clip = CompositionClipId::parse(&reference.source_clip_id)?;
                let saved = state
                    .db
                    .get_composition_project_for(&reference.project_id, &actor)
                    .await?
                    .context("saved source project unavailable")?;
                ensure!(
                    saved.revision == reference.expected_revision,
                    "stale saved cutout revision"
                );
                let descriptors = crate::domain::concat_cutout_binding::saved_still(
                    &saved.document,
                    reference.composition_id.as_deref(),
                    &clip,
                )?;
                for source in [&descriptors.original, &descriptors.copy, &descriptors.mask]
                    .into_iter()
                    .chain(descriptors.regions.iter())
                {
                    let physical = super::concat_cutout_access::library_id(
                        &saved.document,
                        source.id.as_str(),
                    )?;
                    super::library::authorize_source_access(&state, &physical, &headers)
                        .await
                        .map_err(|_| anyhow::anyhow!("saved cutout file access denied"))?;
                }
                let candidate = super::concat_cutout_access::verify_saved_still(
                    &state,
                    &actor,
                    &reference.project_id,
                    reference.composition_id.as_deref(),
                    &clip,
                    reference.expected_revision,
                    &plan,
                    &cancel,
                )
                .await?;
                let mut assets = candidate.assets.clone();
                let physical =
                    |id: &str| super::concat_cutout_access::library_id(&saved.document, id);
                let original = physical(assets.original.id.as_str())?;
                assets.snapshot.source_id = original.clone();
                assets.snapshot.mask_source_id = physical(&assets.snapshot.mask_source_id)?;
                for stroke in &mut assets.snapshot.correction.strokes {
                    if let Some(region) = &mut stroke.region {
                        region.source_id = original.clone();
                        region.mask_source_id = physical(&region.mask_source_id)?;
                    }
                }
                for source in [&mut assets.original, &mut assets.copy, &mut assets.mask]
                    .into_iter()
                    .chain(assets.regions.iter_mut())
                {
                    source.id = SourceId::parse(physical(source.id.as_str())?)?;
                }
                let snapshot = &assets.snapshot;
                let binding = ReadyStillBinding {
                    source_settings: plan.clone(),
                    source_hash: snapshot.source_hash.clone(),
                    model_hash: snapshot.model_hash.clone(),
                    runtime_hash: snapshot.runtime_hash.clone(),
                    assets,
                };
                ready
                    .entry(reference.timeline_id.clone())
                    .or_insert_with(BTreeMap::new)
                    .insert(reference.clip_id.clone(), binding);
                checks.push((reference, clip, plan, candidate));
            }
            let mut videos=crate::domain::concat_project_import::VideoBindings::new();
            let mut video_checks=vec![];
            for reference in input.prepared_videos {
                ensure!(!ready.get(&reference.timeline_id).is_some_and(|clips|clips.contains_key(&reference.clip_id))&&!videos.get(&reference.timeline_id).is_some_and(|clips|clips.contains_key(&reference.clip_id)),"duplicate prepared destination");
                let plan=settings(&input.request.document,&reference.timeline_id,&reference.clip_id,"video")?;
                let (binding,identity)=verify_video(&state,&actor,&headers,&reference,&plan,&cancel).await?;
                videos.entry(reference.timeline_id.clone()).or_insert_with(BTreeMap::new).insert(reference.clip_id.clone(),binding);
                video_checks.push((reference,plan,identity));
            }
            let report = crate::domain::concat_project_import::import_with_ready_bindings(
                input.request,
                &ready,
                &videos,
            )?;
            for(reference,plan,before)in video_checks {
                let (_,current)=verify_video(&state,&actor,&headers,&reference,&plan,&cancel).await?;
                ensure!(before==current,"saved video cutout changed during import");
            }
            // Every referenced project/file is checked again after conversion; no
            // library records, assets or saved documents are created by this route.
            for (reference, clip, plan, before) in checks {
                let current = super::concat_cutout_access::verify_saved_still(
                    &state,
                    &actor,
                    &reference.project_id,
                    reference.composition_id.as_deref(),
                    &clip,
                    reference.expected_revision,
                    &plan,
                    &cancel,
                )
                .await?;
                ensure!(
                    current.paths == before.paths
                        && current.assets.snapshot == before.assets.snapshot
                        && current.assets.original == before.assets.original
                        && current.assets.copy == before.assets.copy
                        && current.assets.mask == before.assets.mask
                        && current.assets.regions == before.assets.regions,
                    "saved cutout changed during import"
                );
            }
            for (id, path) in media_paths {
                super::library::authorize_source_access(&state, id.as_str(), &headers)
                    .await
                    .map_err(|_| anyhow::anyhow!("import media access revoked"))?;
                ensure!(
                    super::cutout_jobs::source(&state, id.as_str(), &actor).await? == path,
                    "import media path changed"
                );
            }
            ensure!(!cancel.is_cancelled(), "ready import cancelled");
            Ok(report)
        }
        .await;
        result
    });
    let report = worker
        .await
        .map_err(|e| AppError::internal("join ready cutout import", e))?
        .map_err(|e| AppError::conflict(e.to_string()))?;
    Ok(([("cache-control", "private, no-store")], Json(report)))
}

/// Build a server-owned binding from the selected saved clip and verified stores.
async fn verify_video(state:&AppState,actor:&str,headers:&HeaderMap,reference:&SavedReference,plan:&crate::domain::concat_cutout_import::Plan,cancel:&tokio_util::sync::CancellationToken)->Result<(crate::domain::concat_project_import::ReadyVideoBinding,String)> {
    use crate::domain::{composition::{Composition,CompositionTrack},concat_video_binding::CacheOrigin,concat_project_import::ReadyVideoBinding};
    super::cutout_assets::authorize(state,&reference.project_id,actor).await.map_err(|_|anyhow::anyhow!("saved video editor access required"))?;
    let saved=state.db.get_composition_project_for(&reference.project_id,actor).await?.context("saved video project unavailable")?;
    ensure!(saved.revision==reference.expected_revision,"stale saved video revision");
    let content=if let Some(nodes)=saved.document.get("compositions") {
        nodes.get(reference.composition_id.as_deref().context("source composition required")?).and_then(|n|n.get("content")).context("source composition missing")?
    }else{ensure!(reference.composition_id.is_none(),"unexpected source composition");&saved.document};
    let composition:Composition=serde_json::from_value(content.clone())?;composition.validate_native_surface()?;
    let clip_id=CompositionClipId::parse(&reference.source_clip_id)?;
    let clip=composition.tracks.iter().find_map(|track|match track{CompositionTrack::Video{clips,..}=>clips.iter().find(|c|c.id==clip_id),_=>None}).context("saved video clip missing")?;
    let mut binding=clip.concat_cutout.clone().context("saved video cutout missing")?;binding.validate()?;
    ensure!(binding.origin.is_none(),"save video cache ownership before reimporting a referenced draft");
    ensure!(binding.settings()==plan.correction,"saved video correction differs from destination");
    let subject=match plan.subject{crate::domain::concat_cutout_import::Subject::Auto=>"auto",crate::domain::concat_cutout_import::Subject::Person=>"person",crate::domain::concat_cutout_import::Subject::Object=>"object"};
    ensure!(binding.subject==subject,"saved video subject differs from destination");
    let physical=|id:&str|super::concat_cutout_access::library_id(&saved.document,id);
    let source_id=physical(clip.source_id.as_str())?;
    super::library::authorize_source_access(state,&source_id,headers).await.map_err(|_|anyhow::anyhow!("saved video file access denied"))?;
    let cache=super::concat_video_cache::restore_for_model(state,actor,&reference.project_id,clip.source_id.as_str(),reference.expected_revision,&binding.runtime_hash,&binding.model_hash,cancel).await?.context("saved video mask cache missing")?;
    ensure!(!cache.is_empty()&&cache.video_hash()==binding.source_hash,"saved video store differs from binding");
    let mut source=composition.sources.get(&clip.source_id).context("saved video descriptor missing")?.clone();source.id=SourceId::parse(&source_id)?;
    let mut regions=BTreeMap::new();let mut region_sources=BTreeMap::new();let mut staged=0u64;
    for track in &mut binding.region_tracks {
        ensure!(track.snapshot.source_id==clip.source_id.as_str(),"saved region original differs from clip");
        let old=track.snapshot.mask_source_id.clone();
        let lease=super::concat_video_region_cache::load(state,actor,&reference.project_id,reference.expected_revision,&track.snapshot,cancel).await?;
        super::library::authorize_source_access(state,&lease.library_source_id,headers).await.map_err(|_|anyhow::anyhow!("saved region file access denied"))?;
        ensure!(physical(&old)?==lease.library_source_id,"saved region physical binding changed");
        if !regions.contains_key(&old) {
            staged=staged.checked_add(track.snapshot.frames.checked_mul(256*256).context("region size overflow")?).context("region budget overflow")?;
            ensure!(staged<=64*1024*1024*1024,"video region import budget exceeded");
            let mut descriptor=composition.sources.get(&SourceId::parse(&old)?).context("saved region descriptor missing")?.clone();descriptor.id=SourceId::parse(&lease.library_source_id)?;
            regions.insert(old.clone(),descriptor);
        }
        let physical_mask=regions[&old].id.as_str().to_owned();
        region_sources.entry(physical_mask.clone()).or_insert(old.clone());
        track.snapshot.source_id=source_id.clone();track.snapshot.mask_source_id=physical_mask;
    }
    binding.origin=Some(CacheOrigin{project_id:reference.project_id.clone(),expected_revision:reference.expected_revision,source_id:clip.source_id.as_str().into(),region_sources});binding.validate()?;
    let mut unique=BTreeMap::new();for descriptor in regions.into_values(){if let Some(previous)=unique.insert(descriptor.id.clone(),descriptor.clone()){ensure!(previous==descriptor,"conflicting saved video region descriptors");}}
    let ready=ReadyVideoBinding{source,source_settings:plan.clone(),binding,regions:unique.into_values().collect()};
    let identity=serde_json::to_string(&(&ready.source,&ready.binding,&ready.regions,cache.identity()?))?;
    super::concat_video_bound::recheck_authorities(state,actor,&[super::concat_video_bound::SavedProject{project_id:reference.project_id.clone(),expected_revision:reference.expected_revision}]).await?;
    ensure!(!cancel.is_cancelled(),"ready video import cancelled");
    Ok((ready,identity))
}
