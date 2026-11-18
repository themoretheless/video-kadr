//! Prepare newly analysed mask caches and region movies for project publication.
use super::{video_import::Imported, AppState, ArchivedSource, PreparedSource, Value};
use crate::{
    domain::composition::SourceId,
    handlers::concat_video_replace::{regions::Regions, CACHE_BUDGET},
    services::concat_video_analysis::RunCache,
};
use anyhow::Result;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

pub(super) struct Generated {
    pub media_id: SourceId,
    pub original: PathBuf,
    pub cache: RunCache,
    pub regions: Regions,
}

pub(super) async fn prepare(
    state: &AppState,
    actor: &str,
    document: &Value,
    generated: &mut [Generated],
    prepared: &mut Vec<PreparedSource>,
    video: &mut Imported,
    cancel: &CancellationToken,
) -> Result<()> {
    if !generated.is_empty() {
        let root = state.storage.join("concat-video-caches");
        tokio::fs::create_dir_all(&root).await?;
        for Generated {
            media_id,
            original,
            cache,
            regions,
        } in generated
        {
            let receipt = cache.publish(&root, CACHE_BUDGET, cancel).await?;
            video.track_directory(receipt.directory.clone());
            let identity = cache.portable_identity()?;
            let registration = crate::handlers::concat_video_cache::prepare_archive(
                state,
                actor,
                media_id.as_str(),
                document,
                original,
                &receipt,
                &identity,
                regions.files(),
                cancel,
            )
            .await?;
            video.registrations.push(registration);
            for (entry, (snapshot, _, path)) in regions.entries().iter().zip(regions.files()) {
                prepared.push(PreparedSource {
                    archived: ArchivedSource {
                        source_id: entry.id.clone(),
                        filename: entry.filename.clone(),
                        media_type: "video".into(),
                        title: entry.title.clone(),
                        original_filename: None,
                        duration: entry.duration,
                        width: entry.width,
                        height: entry.height,
                        favorite: false,
                        tags: vec![],
                        byte_length: entry.size_bytes.unwrap_or_default(),
                        sha256: snapshot.mask_file_hash.clone(),
                    },
                    staged: crate::project_archive::StagedArchiveMedia::from_staged_file(
                        entry.id.clone(),
                        entry.filename.clone(),
                        path.clone(),
                    ),
                    new_id: entry.id.clone(),
                    destination_filename: entry.filename.clone(),
                    entry: entry.clone(),
                });
            }
        }
    }
    Ok(())
}
