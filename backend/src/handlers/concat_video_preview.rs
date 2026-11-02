//! Solo saved-video cutout preview using verified project masks and native effects.
use anyhow::{ensure, Result};
use axum::{extract::{Path, State}, http::HeaderMap};
use serde::Deserialize;
use crate::{state::AppState, error::{ApiJson, AppError, AppResult}, domain::composition::{Composition, CompositionTrack, CompositionClipId}};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request { #[serde(default)] source_original:bool, #[serde(default)] source_space:bool,region_tracks:Option<Vec<crate::domain::concat_video_binding::RegionTrack>>,correction:Option<crate::domain::cutout::Correction>,feather:Option<f64>,composition_id: Option<String>, clip_id: String, expected_revision: u64, time_us: u64 }
struct Cancel {
    token: tokio_util::sync::CancellationToken,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Cancel { fn drop(&mut self) { self.token.cancel(); self.flag.store(true, std::sync::atomic::Ordering::Relaxed); } }
pub async fn handler(State(state): State<AppState>, Path(project): Path<String>, headers: HeaderMap, ApiJson(request): ApiJson<Request>) -> AppResult<([(&'static str, String);4], Vec<u8>)> {
    if request.feather.is_some_and(|value|!value.is_finite()||!(0.0..=0.1).contains(&value)){return Err(AppError::bad_request("Неверная растушёвка видео"))}
    if request.source_original&&!request.source_space{return Err(AppError::bad_request("Original preview requires source space"))}
    let actor = super::publish::authenticated_actor(&state, &headers).await?;
    super::cutout_assets::authorize(&state, &project, &actor).await?;
    let saved = state.db.get_composition_project_for(&project, &actor).await.map_err(|e| AppError::internal("read video preview target", e))?.ok_or_else(|| AppError::not_found("Проект не найден"))?;
    if saved.revision != request.expected_revision { return Err(AppError::conflict("Проект изменился")); }
    let content = if let Some(nodes) = saved.document.get("compositions") {
        &nodes[request.composition_id.as_deref().ok_or_else(|| AppError::bad_request("Нужен таймлайн"))?]["content"]
    } else {
        if request.composition_id.is_some() { return Err(AppError::bad_request("Лишний таймлайн")); }
        &saved.document
    };
    let mut composition: Composition = serde_json::from_value(content.clone()).map_err(|_| AppError::bad_request("Неверная композиция"))?;
    composition.validate().map_err(|e| AppError::bad_request(e.to_string()))?;
    let id = CompositionClipId::parse(request.clip_id).map_err(|e| AppError::bad_request(e.to_string()))?;
    let selected = composition.tracks.iter().find_map(|track| match track {
        CompositionTrack::Video { locked: false, clips, .. } => clips.iter().find(|clip| clip.id == id && clip.enabled).cloned(), _ => None,
    }).ok_or_else(|| AppError::bad_request("Нужно незаблокированное видео"))?;
    if composition.metadata.as_ref().is_some_and(|m| m.media_versions.contains_key(&id)) { return Err(AppError::conflict("Клип содержит обработанную версию")); }
    if crate::domain::video_clock::source_tick(&selected.placement, selected.playback_mode, request.time_us).map_err(|e| AppError::bad_request(e.to_string()))?.is_none() { return Err(AppError::bad_request("Кадр вне клипа")); }
    let mut correction=request.correction.unwrap_or_else(||selected.concat_cutout.as_ref().map(|b|b.settings()).unwrap_or(crate::domain::cutout::Correction{custom:false,strokes:vec![],feather:0.}));
    if let Some(feather)=request.feather{correction.feather=feather;}
    correction.validate().map_err(|e|AppError::bad_request(e.to_string()))?;
    let source_time=crate::domain::video_clock::source_tick(&selected.placement,selected.playback_mode,request.time_us).map_err(|e|AppError::bad_request(e.to_string()))?.ok_or_else(||AppError::bad_request("Frame outside clip"))?;
    let source_dimensions=composition.sources.get(&selected.source_id).map(|s|(s.width as usize,s.height as usize)).ok_or_else(||AppError::bad_request("Missing source dimensions"))?;
    let region_tracks=request.region_tracks.unwrap_or_else(||selected.concat_cutout.as_ref().map(|b|b.region_tracks.clone()).unwrap_or_default());
    let source_id = selected.source_id.clone();
    composition.tracks.retain(|track| matches!(track, CompositionTrack::Video{clips,..} if clips.iter().any(|c| c.id == id)));
    if let CompositionTrack::Video { clips, hidden, transitions, .. } = &mut composition.tracks[0] { *clips = vec![selected]; *hidden = false; transitions.clear(); }
    composition.caption_tracks.clear();
    if let Some(metadata) = composition.metadata.as_mut() { metadata.clip_links.clear(); metadata.media_versions.clear(); metadata.clip_names.retain(|clip,_| *clip == id); }
    let token = tokio_util::sync::CancellationToken::new();
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _cancel = Cancel { token: token.clone(), flag: flag.clone() };
    let work = tokio::spawn(async move { let result: Result<crate::domain::group_surface::GroupSurface> = async {
        let _permit = super::concat_cutout_analyse::acquire_analysis(&state, &token).await?;
        let choice=super::concat_video_cache::choice(&state,&actor,&project,source_id.as_str(),request.expected_revision,&token).await?.ok_or_else(||anyhow::anyhow!("Video masks have not been analysed"))?;
        let mut cache=super::concat_video_cache::restore_for_model(&state,&actor,&project,source_id.as_str(),request.expected_revision,&choice.runtime_hash,&choice.model_hash,&token).await?.ok_or_else(||anyhow::anyhow!("Video masks have not been analysed"))?;
        crate::domain::concat_video_binding::validate_tracks(&correction,&region_tracks,cache.video_hash())?;
        let mut region_leases=vec![];let mut region_bytes=0u64;
        for track in &region_tracks{
            ensure!(track.snapshot.source_id==source_id.as_str(),"video region belongs to another source");
            region_bytes=region_bytes.checked_add(track.snapshot.frames*256*256).ok_or_else(||anyhow::anyhow!("video region budget overflow"))?;ensure!(region_bytes<=64*1024*1024*1024,"video region preview budget exceeded");
            region_leases.push((track.stroke,super::concat_video_region_cache::load(&state,&actor,&project,request.expected_revision,&track.snapshot,&token).await?));
        }
        let mut sources = std::collections::BTreeMap::new();
        for source in composition.sources.keys() { sources.insert(source.clone(), super::cutout_jobs::source(&state, source.as_str(), &actor).await?); }
        let original_path = sources.get(&source_id).ok_or_else(|| anyhow::anyhow!("missing video source"))?.clone();
        let before = crate::project_archive::fingerprint_file_with_limit(&original_path, 64*1024*1024*1024).await?;
        let codecs = crate::services::local_enhance_video::resolve_tools().await?;
        let luts = serde_json::from_value(saved.document.get("luts").cloned().unwrap_or_else(|| serde_json::json!([])))?;
        let project_document = crate::domain::nested_composition::NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![], schema_version: 1, root: "solo".into(), luts,
            compositions: std::collections::BTreeMap::from([("solo".into(), crate::domain::nested_composition::NestedCompositionDocument { duration_us: None, content: composition, instances: vec![] })]),
        };
        let decode_path=original_path.clone();
        let render_token = token.clone();
        let frame = tokio::task::spawn_blocking(move || {
            let mut resolve=|source_us:u64,frame:&mut crate::domain::group_surface::GroupSurface|->Result<()>{
                let regions=if region_leases.is_empty(){None}else{let mut planes=vec![None;correction.strokes.len()];for(index,lease)in &region_leases{planes[*index]=Some(lease.reader.read(cache.video_hash(),source_us,&render_token)?);}Some(crate::services::concat_video_analysis::SourceRegions{video_hash:cache.video_hash().into(),source_us,planes})};
                cache.apply_decoded_with_regions(frame,source_us,&correction,regions.as_ref(),&render_token)?;Ok(())
            };
            if request.source_space {
                let mut frame=crate::domain::group_media::decode_frame_policy(&codecs.ffmpeg,&decode_path,source_time,source_dimensions.0,source_dimensions.1,std::time::Duration::from_secs(30),&flag,true)?;
                if !request.source_original{resolve(source_time,&mut frame)?;}
                return Ok(frame)
            }
            let mut filter = |source: &crate::domain::composition::SourceId, source_us, frame: &mut crate::domain::group_surface::GroupSurface| -> Result<()> {
                if *source == source_id { resolve(source_us,frame)?; } Ok(())
            };
            crate::domain::group_media::render_project_frame_sources_with_filter(&project_document,"solo",&codecs.ffmpeg,&Default::default(),&sources,request.time_us,std::time::Duration::from_secs(30),&flag,None,Some(&mut filter))
        }).await??;
        super::cutout_assets::authorize(&state, &project, &actor).await.map_err(|_| anyhow::anyhow!("project editor access revoked"))?;
        let current = state.db.get_composition_project_for(&project,&actor).await?.ok_or_else(|| anyhow::anyhow!("project unavailable"))?;
        ensure!(current.revision == request.expected_revision && current.document == saved.document && !token.is_cancelled(), "video preview target changed");
        ensure!(crate::project_archive::fingerprint_file_with_limit(&original_path,64*1024*1024*1024).await? == before, "video preview source changed");
        Ok(frame)
    }.await; result });
    let frame = work.await.map_err(|e| AppError::internal("join video cutout preview",e))?.map_err(|e| AppError::conflict(e.to_string()))?;
    Ok(([ ("content-type","application/octet-stream".into()), ("x-frame-width",frame.width.to_string()), ("x-frame-height",frame.height.to_string()), ("cache-control","no-store".into()) ],frame.rgba))
}
