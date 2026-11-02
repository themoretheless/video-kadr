use crate::{
    domain::color_lut::ColorLut,
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Serialize;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutInfo {
    pub title: String,
    pub shaper_size: usize,
    pub cube_size: usize,
}
pub async fn color_lut_handler(ApiJson(lut): ApiJson<ColorLut>) -> AppResult<Json<LutInfo>> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let slot = SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::too_many_requests("слишком много проверок LUT"))?;
    let info = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let table = lut.table()?;
        Ok::<_, anyhow::Error>(LutInfo {
            title: table.title.clone(),
            shaper_size: table.shaper.as_ref().map_or(0, |s| s.size()),
            cube_size: table.cube.as_ref().map_or(0, |c| c.size),
        })
    })
    .await
    .map_err(|e| AppError::internal("join native LUT validation", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(info))
}

pub async fn builtin_luts_handler() -> AppResult<Json<Vec<crate::domain::builtin_luts::BuiltinInfo>>>
{
    let entries = crate::domain::builtin_luts::catalog()
        .map_err(|e| AppError::internal("builtin LUT catalog", e))?;
    Ok(Json(entries))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportProjectLut {
    pub project: crate::domain::nested_composition::NestedCompositionProject,
    pub lut: ColorLut,
    pub name: String,
    pub source_path: Option<String>,
    pub composition: String,
    pub track: String,
    pub clip: String,
    pub effect_index: usize,
    pub input: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedProjectLut {
    pub project: crate::domain::nested_composition::NestedCompositionProject,
    pub id: String,
    pub reused: bool,
}
pub async fn import_project_lut_handler(
    ApiJson(mut request): ApiJson<ImportProjectLut>,
) -> AppResult<Json<ImportedProjectLut>> {
    static SLOTS: Semaphore = Semaphore::const_new(2);
    let permit = SLOTS
        .try_acquire()
        .map_err(|_| AppError::too_many_requests("слишком много импортов LUT"))?;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<ImportedProjectLut> {
        let _permit = permit;
        request.project.dependency_order()?;
        let reserved: std::collections::BTreeSet<String> = request
            .project
            .compositions
            .values()
            .flat_map(|document| &document.content.tracks)
            .flat_map(|track| match track {
                crate::domain::composition::CompositionTrack::Video { clips, .. } => clips
                    .iter()
                    .flat_map(|clip| &clip.effects)
                    .collect::<Vec<_>>(),
                crate::domain::composition::CompositionTrack::Image { clips, .. } => clips
                    .iter()
                    .flat_map(|clip| &clip.effects)
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .filter_map(|effect| match effect {
                crate::domain::composition::VideoEffect::Lumetri { parameters } => Some(parameters),
                _ => None,
            })
            .flat_map(|p| [&p.input_lut_library, &p.look_lut_library])
            .flatten()
            .cloned()
            .collect();
        let (id, reused) = crate::domain::project_luts::import_reserving(
            &mut request.project.luts,
            request.lut,
            request.name,
            request.source_path,
            &reserved,
        )?;
        let content = &mut request
            .project
            .compositions
            .get_mut(&request.composition)
            .ok_or_else(|| anyhow::anyhow!("composition missing"))?
            .content;
        let track = content
            .tracks
            .iter_mut()
            .find(|track| track.id().as_str() == request.track)
            .ok_or_else(|| anyhow::anyhow!("track missing"))?;
        let effects = match track {
            crate::domain::composition::CompositionTrack::Video { clips, locked, .. } => {
                anyhow::ensure!(!*locked, "track locked");
                &mut clips
                    .iter_mut()
                    .find(|clip| clip.id.as_str() == request.clip)
                    .ok_or_else(|| anyhow::anyhow!("clip missing"))?
                    .effects
            }
            crate::domain::composition::CompositionTrack::Image { clips, locked, .. } => {
                anyhow::ensure!(!*locked, "track locked");
                &mut clips
                    .iter_mut()
                    .find(|clip| clip.id.as_str() == request.clip)
                    .ok_or_else(|| anyhow::anyhow!("clip missing"))?
                    .effects
            }
            _ => anyhow::bail!("visual track required"),
        };
        if let Some(crate::domain::composition::VideoEffect::Lumetri { parameters }) =
            effects.get_mut(request.effect_index)
        {
            if request.input {
                parameters.input_lut = None;
                parameters.input_lut_builtin = None;
                parameters.input_lut_library = Some(id.clone());
            } else {
                parameters.look_lut = None;
                parameters.look_lut_builtin = None;
                parameters.look_lut_library = Some(id.clone());
            }
        } else {
            anyhow::bail!("Lumetri effect missing");
        }
        request.project.dependency_order()?;
        Ok(ImportedProjectLut {
            project: request.project,
            id,
            reused,
        })
    })
    .await
    .map_err(|e| AppError::internal("join LUT import", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportLut {
    #[serde(default)]
    pub luts: Vec<crate::domain::project_luts::ProjectLut>,
    pub reference: Option<String>,
    pub lut: Option<ColorLut>,
    pub format: crate::domain::color_lut::TableFormat,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedLut {
    pub text: String,
    pub filename: String,
}
pub async fn export_lut_handler(
    ApiJson(request): ApiJson<ExportLut>,
) -> AppResult<Json<ExportedLut>> {
    static SLOTS: Semaphore = Semaphore::const_new(2);
    let permit = SLOTS
        .try_acquire()
        .map_err(|_| AppError::too_many_requests("слишком много экспортов LUT"))?;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<ExportedLut> {
        let _permit = permit;
        crate::domain::project_luts::validate(&request.luts)?;
        let table = match (request.reference, request.lut) {
            (Some(reference), None) => {
                crate::domain::project_luts::resolve_reference(&request.luts, &reference)?
                    .ok_or_else(|| anyhow::anyhow!("no LUT selected"))?
            }
            (None, Some(lut)) => lut.table()?,
            _ => anyhow::bail!("choose one LUT source"),
        };
        let (text, extension) = match request.format {
            crate::domain::color_lut::TableFormat::Cube => (table.to_cube(), "cube"),
            crate::domain::color_lut::TableFormat::ThreeDl => {
                (table.to_3dl().map_err(anyhow::Error::msg)?, "3dl")
            }
        };
        anyhow::ensure!(text.len() <= 32 * 1024 * 1024, "exported LUT exceeds 32MiB");
        let stem: String = table
            .title
            .chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
            .take(80)
            .collect();
        let filename = format!(
            "{}.{}",
            if stem.is_empty() { "LUT" } else { &stem },
            extension
        );
        Ok(ExportedLut { text, filename })
    })
    .await
    .map_err(|e| AppError::internal("join LUT export", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProjectLut {
    pub project: crate::domain::nested_composition::NestedCompositionProject,
    pub id: String,
    pub name: Option<String>,
    pub lut: Option<ColorLut>,
    pub source_path: Option<String>,
}
pub async fn update_project_lut_handler(
    ApiJson(mut request): ApiJson<UpdateProjectLut>,
) -> AppResult<Json<crate::domain::nested_composition::NestedCompositionProject>> {
    static SLOTS: Semaphore = Semaphore::const_new(2);
    let permit = SLOTS
        .try_acquire()
        .map_err(|_| AppError::too_many_requests("слишком много изменений LUT"))?;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _permit = permit;
        request.project.dependency_order()?;
        crate::domain::project_luts::update(
            &mut request.project.luts,
            &request.id,
            request.name,
            request.lut,
            request.source_path,
        )?;
        request.project.dependency_order()?;
        Ok(request.project)
    })
    .await
    .map_err(|e| AppError::internal("join LUT update", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveProjectLut {
    pub project: crate::domain::nested_composition::NestedCompositionProject,
    pub id: String,
}
pub async fn remove_project_lut_handler(
    ApiJson(mut request): ApiJson<RemoveProjectLut>,
) -> AppResult<Json<crate::domain::nested_composition::NestedCompositionProject>> {
    static SLOTS: Semaphore = Semaphore::const_new(2);
    let permit = SLOTS
        .try_acquire()
        .map_err(|_| AppError::too_many_requests("слишком много изменений LUT"))?;
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let _permit = permit;
        request.project.dependency_order()?;
        crate::domain::project_luts::remove(&mut request.project.luts, &request.id)?;
        request.project.dependency_order()?;
        Ok(request.project)
    })
    .await
    .map_err(|e| AppError::internal("join LUT removal", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(result))
}
