//! Stage cutout assets for a new Concat project before publishing any media.
use super::*;
use crate::{
    domain::{composition::CompositionSource, concat_cutout_import::Plan, cutout::Tool},
    handlers::{concat_cutout_analyse, concat_smart_region, sam_models},
    services::concat_cutout_still::{PreparedStill, SavedRegions},
};
use anyhow::Result;
use tokio_util::sync::CancellationToken;

pub(super) async fn prepare(
    state: &AppState,
    actor: &str,
    original: &CompositionSource,
    source: &Path,
    settings: Plan,
    region_stage: &Path,
    sources: &mut Vec<PreparedSource>,
    cancel: &CancellationToken,
) -> Result<PreparedStill> {
    let (_, hash) =
        crate::project_archive::fingerprint_file_with_limit(source, 256 * 1024 * 1024).await?;
    let mut regions = SavedRegions::default();
    if !settings.region_source_seconds.is_empty() {
        let (config, _) = sam_models::config_for_owner(state, actor)
            .await
            .map_err(|_| anyhow::anyhow!("SAM model unavailable"))?;
        crate::services::local_sam::identity(&config).await?;
        let mut correction = settings.correction.clone();
        for stroke in &mut correction.strokes {
            if stroke.at.is_none() || !matches!(stroke.tool, Tool::SmartBrush | Tool::SmartEraser) {
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let path = region_stage.join(format!("{id}.png"));
            let (attachment, descriptor) = concat_smart_region::stage(
                state,
                &config,
                original,
                source,
                &hash,
                &stroke.points,
                &path,
                &id,
                cancel,
                &mut None,
            )
            .await?;
            sources.push(stage_image(&descriptor, &path, "Область SAM").await?);
            regions.paths.insert(id, path);
            regions.sources.push(descriptor);
            stroke.region = Some(attachment);
        }
        regions.correction = Some(correction);
    }
    let prepared = Box::pin(concat_cutout_analyse::prepare_still(
        state,
        actor,
        source,
        &hash,
        settings,
        original.clone(),
        regions,
        cancel,
    ))
    .await?;
    sources.push(
        stage_image(
            &prepared.binding.assets.copy,
            &prepared.copy,
            "Вырезанное изображение",
        )
        .await?,
    );
    sources.push(
        stage_image(
            &prepared.binding.assets.mask,
            &prepared.mask,
            "Маска изображения",
        )
        .await?,
    );
    Ok(prepared)
}

async fn stage_image(
    descriptor: &CompositionSource,
    path: &Path,
    title: &str,
) -> Result<PreparedSource> {
    let id = descriptor.id.as_str();
    let filename = format!("{id}.png");
    let (byte_length, sha256) = fingerprint_file(path).await?;
    let archived = ArchivedSource {
        source_id: id.into(),
        filename: filename.clone(),
        media_type: "image".into(),
        title: Some(title.into()),
        original_filename: None,
        duration: None,
        width: Some(descriptor.width),
        height: Some(descriptor.height),
        favorite: false,
        tags: vec![],
        byte_length,
        sha256,
    };
    let entry = MediaEntry::from_result(
        "source",
        &serde_json::json!({
            "id":id, "filename":filename, "url":format!("/files/sources/{filename}"),
            "mediaType":"image", "title":title, "width":descriptor.width, "height":descriptor.height,
            "sizeBytes":byte_length,
        }),
    );
    Ok(PreparedSource {
        archived,
        staged: crate::project_archive::StagedArchiveMedia::from_staged_file(
            id.into(),
            filename.clone(),
            path.to_owned(),
        ),
        new_id: id.into(),
        destination_filename: filename,
        entry,
    })
}
