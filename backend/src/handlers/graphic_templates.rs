use crate::{
    domain::graphic_templates::{builtin_templates, GraphicTemplate},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditRequest {
    pub template: GraphicTemplate,
    pub control: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub clock: Option<EditClock>,
}
pub async fn catalog() -> AppResult<Json<Vec<GraphicTemplate>>> {
    Ok(Json(builtin_templates().map_err(|error| {
        AppError::bad_request(error.to_string())
    })?))
}
pub async fn edit(ApiJson(request): ApiJson<EditRequest>) -> AppResult<Json<GraphicTemplate>> {
    let local = local_time(&request.template, request.clock)?;
    Ok(Json(
        request
            .template
            .set_control_at(&request.control, request.value, local)
            .map_err(|error| AppError::bad_request(error.to_string()))?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileReadRequest {
    pub contents: String,
}
pub async fn read_file(
    ApiJson(request): ApiJson<FileReadRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let file = crate::domain::graphic_template_file::GraphicTemplateFile::read(
        request.contents.as_bytes(),
    )
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(file.value().clone()))
}
pub async fn write_file(ApiJson(document): ApiJson<serde_json::Value>) -> AppResult<Json<String>> {
    let bytes =
        serde_json::to_vec(&document).map_err(|error| AppError::bad_request(error.to_string()))?;
    let file = crate::domain::graphic_template_file::GraphicTemplateFile::read(&bytes)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let bytes = file
        .write()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(String::from_utf8(bytes).map_err(|error| {
        AppError::bad_request(error.to_string())
    })?))
}

pub async fn instantiate_file(
    ApiJson(document): ApiJson<serde_json::Value>,
) -> AppResult<Json<GraphicTemplate>> {
    let template = crate::domain::graphic_templates::from_original(&document)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    crate::domain::graphic_template_fonts::register(&template.resources);
    Ok(Json(template))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditClock {
    pub relative_us: i64,
    pub duration_us: u64,
    #[serde(default)]
    pub animation_offset_us: u64,
}
fn local_time(template: &GraphicTemplate, clock: Option<EditClock>) -> AppResult<i128> {
    let Some(clock) = clock else { return Ok(0) };
    let duration = i64::try_from(clock.duration_us)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    if duration <= 0
        || clock.duration_us > 9_007_199_254_740_991
        || clock.animation_offset_us > 9_007_199_254_740_991
    {
        return Err(AppError::bad_request("invalid template clock"));
    }
    let relative = clock.relative_us.clamp(0, duration - 1);
    if let Some(meta) = template.responsive_time.filter(|meta| meta.active()) {
        return meta
            .remap(relative, duration)
            .map(i128::from)
            .map_err(|error| AppError::bad_request(error.to_string()));
    }
    Ok(i128::from(relative) + i128::from(clock.animation_offset_us))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValuesRequest {
    pub template: GraphicTemplate,
    #[serde(default)]
    pub clock: Option<EditClock>,
}
pub async fn values(
    ApiJson(request): ApiJson<ValuesRequest>,
) -> AppResult<Json<serde_json::Map<String, serde_json::Value>>> {
    let local = local_time(&request.template, request.clock)?;
    Ok(Json(request.template.control_values_at(local).map_err(
        |error| AppError::bad_request(error.to_string()),
    )?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportRequest {
    pub template: GraphicTemplate,
    #[serde(default)]
    pub animation_offset_us: u64,
    #[serde(default)]
    pub embed_fonts:bool,
    #[serde(default="default_font_license")]
    pub font_license:String,
    #[serde(default="default_auto_text_controls")]
    pub auto_text_controls:bool,
    #[serde(default)]
    pub controls:Option<Vec<serde_json::Value>>,
}
fn default_auto_text_controls()->bool {true}
fn default_font_license()->String {"UNKNOWN".into()}
pub async fn export_file(ApiJson(request): ApiJson<ExportRequest>) -> AppResult<Json<String>> {
    request
        .template
        .graphic
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    crate::domain::graphic_templates::GraphicTemplateMetadata::from(&request.template)
        .validate()
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let template=if request.embed_fonts {
        request.template.with_embedded_fonts(&request.font_license).map_err(|error|AppError::bad_request(error.to_string()))?
    }else{request.template};
    let native = serde_json::to_value(template)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    let mut document =
        crate::domain::graphic_template_export::export_native_with_control_defaults(&native, request.animation_offset_us,request.auto_text_controls)
            .map_err(|error| AppError::bad_request(error.to_string()))?;
    if let Some(descriptors)=request.controls {
        let source=crate::domain::graphic_template_export::export_native_with_control_defaults(&native,0,false)
            .map_err(|error|AppError::bad_request(error.to_string()))?;
        document["controls"]=serde_json::json!(crate::domain::graphic_template_control_authoring::resolve(&source,&descriptors)
            .map_err(|error|AppError::bad_request(error.to_string()))?);
    }
    let file = crate::domain::graphic_template_file::GraphicTemplateFile::read(
        &serde_json::to_vec(&document).map_err(|error| AppError::bad_request(error.to_string()))?,
    )
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(
        String::from_utf8(
            file.write()
                .map_err(|error| AppError::bad_request(error.to_string()))?,
        )
        .map_err(|error| AppError::bad_request(error.to_string()))?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchEditRequest {
    pub template: GraphicTemplate,
    pub values: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub clock: Option<EditClock>,
}
pub async fn edit_batch(ApiJson(request): ApiJson<BatchEditRequest>) -> AppResult<Json<GraphicTemplate>> {
    let local = local_time(&request.template, request.clock)?;
    Ok(Json(request.template.set_controls_at(&request.values, local)
        .map_err(|error| AppError::bad_request(error.to_string()))?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailRequest {
    #[serde(default)]
    pub template:Option<GraphicTemplate>,
    #[serde(default)]
    pub document:Option<serde_json::Value>,
    #[serde(default="default_thumbnail_width")]
    pub width:u32,
}
fn default_thumbnail_width()->u32 {320}
pub async fn thumbnail(ApiJson(request):ApiJson<ThumbnailRequest>)->AppResult<Json<crate::domain::graphic_template_thumbnail::Thumbnail>> {
    let result=tokio::task::spawn_blocking(move||->anyhow::Result<_>{
        match (request.template,request.document) {
            (Some(template),None)=>crate::domain::graphic_template_thumbnail::png(&template,request.width),
            (None,Some(document))=>crate::domain::graphic_template_document_thumbnail::png(&document,request.width),
            _=>anyhow::bail!("provide exactly one template or document"),
        }
    }).await.map_err(|error|AppError::internal("template thumbnail worker",error))?
        .map_err(|error|AppError::bad_request(error.to_string()))?;
    Ok(Json(result))
}

/// Inspect all authored font dependencies without changing the project.
pub async fn fonts_used(
    ApiJson(composition): ApiJson<crate::domain::composition::Composition>,
) -> AppResult<Json<Vec<crate::domain::font_inventory_status::FontStatus>>> {
    Ok(Json(crate::domain::font_inventory_status::composition_fonts(&composition)
        .map_err(|error| AppError::bad_request(error.to_string()))?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceFontsRequest {
    pub composition: crate::domain::composition::Composition,
    pub from: crate::domain::composition_font_replace::FontSelection,
    pub to: crate::domain::composition_font_replace::FontSelection,
}
pub async fn replace_fonts(ApiJson(request): ApiJson<ReplaceFontsRequest>)
 -> AppResult<Json<crate::domain::composition_font_replace::Replacement>> {
 Ok(Json(crate::domain::composition_font_replace::replace_fonts(&request.composition,&request.from,&request.to)
  .map_err(|error|AppError::bad_request(error.to_string()))?))
}

pub async fn project_fonts_used(ApiJson(project): ApiJson<crate::domain::nested_composition::NestedCompositionProject>)
 -> AppResult<Json<Vec<crate::domain::font_inventory_status::FontStatus>>> {
 Ok(Json(crate::domain::font_inventory_status::project_fonts(&project).map_err(|error|AppError::bad_request(error.to_string()))?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceProjectFontsRequest {
 pub project: crate::domain::nested_composition::NestedCompositionProject,
 pub from: crate::domain::composition_font_replace::FontSelection,
 pub to: crate::domain::composition_font_replace::FontSelection,
}
pub async fn replace_project_fonts(ApiJson(request): ApiJson<ReplaceProjectFontsRequest>)
 -> AppResult<Json<crate::domain::project_fonts::Replacement>> {
 Ok(Json(crate::domain::project_fonts::replace_fonts(&request.project,&request.from,&request.to)
  .map_err(|error|AppError::bad_request(error.to_string()))?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFontEditRequest {
 pub project:crate::domain::nested_composition::NestedCompositionProject,
 pub operation:crate::domain::concat_project_fonts::Edit,
}
pub async fn edit_project_font(ApiJson(request):ApiJson<ProjectFontEditRequest>)->AppResult<Json<crate::domain::nested_composition::NestedCompositionProject>>{
 Ok(Json(crate::domain::concat_project_fonts::edit(&request.project,request.operation).map_err(|error|AppError::bad_request(error.to_string()))?))
}
