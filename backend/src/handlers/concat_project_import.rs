//! Pure project conversion: no source paths are opened and no project is saved.
use crate::{
    domain::concat_project_import::{ImportReport, ImportRequest},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
pub async fn import_handler(
    ApiJson(request): ApiJson<ImportRequest>,
) -> AppResult<Json<ImportReport>> {
    crate::domain::concat_project_import::import(request)
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

#[derive(serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ExportRequest {project:crate::domain::nested_composition::NestedCompositionProject,#[serde(default)] name:Option<String>,#[serde(default)] media_paths:std::collections::BTreeMap<String,String>}
pub async fn export_handler(ApiJson(request):ApiJson<ExportRequest>)->AppResult<Json<serde_json::Value>>{
 let mut document=crate::domain::concat_project_export::export_with_paths(&request.project,&request.media_paths).map_err(|error|AppError::bad_request(format!("{error:#}")))?;
 if let Some(name)=request.name{if name.chars().count()>256||name.contains('\0'){return Err(AppError::bad_request("Invalid project name"))}document["name"]=serde_json::json!(name);}
 if serde_json::to_vec(&document).map_err(|error|AppError::bad_request(error.to_string()))?.len()>4*1024*1024{return Err(AppError::bad_request("Concat JSON exceeds the import budget"))}
 Ok(Json(serde_json::json!({"schemaVersion":1,"document":document})))
}

#[derive(serde::Deserialize)]
#[serde(tag="operation",rename_all="camelCase",deny_unknown_fields)]
pub enum MediaSlotEdit {
    SetPlaceholder { document:serde_json::Value, #[serde(rename="mediaId")] media_id:String, placeholder:bool },
    FillSlot { document:serde_json::Value, #[serde(rename="mediaId")] media_id:String, replacement:serde_json::Value },
}
/// Source-document operation only: no library mutation, persistence or file I/O.
pub async fn media_slot_handler(ApiJson(request):ApiJson<MediaSlotEdit>)->AppResult<Json<serde_json::Value>> {
    let edited=match request {
        MediaSlotEdit::SetPlaceholder{document,media_id,placeholder}=>crate::domain::concat_media_slots::set_placeholder(&document,&media_id,placeholder),
        MediaSlotEdit::FillSlot{document,media_id,replacement}=>crate::domain::concat_media_slots::fill(&document,&media_id,&replacement).map(|document|(document,true)),
    }.map_err(|error|AppError::bad_request(error.to_string()))?;
    Ok(Json(serde_json::json!({"schemaVersion":1,"document":edited.0,"applied":edited.1})))
}
