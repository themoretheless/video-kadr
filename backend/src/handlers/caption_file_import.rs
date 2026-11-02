//! Prepare an exact-clock caption import before the authored-document transaction.
use super::caption_files::FileDocument;
use crate::{
    domain::{caption_file_timing::prepare_file_import, graphic_template_fonts::base64_decode},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use filmcraft_captions::{CaptionFormat, Format};
use filmcraft_time::FrameRate;
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub data_base64: String,
    pub path: String,
    pub format: Option<String>,
    pub name: Option<String>,
    pub rate: FrameRate,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub name: String,
    pub format: &'static str,
    pub language: String,
    pub enabled: bool,
    pub locked: bool,
    pub sync_lock: bool,
    pub background_color: [u8; 4],
    pub document: FileDocument,
}
fn prepare(request: Request) -> anyhow::Result<Response> {
    let bytes = base64_decode(&request.data_base64)
        .ok_or_else(|| anyhow::anyhow!("invalid caption base64"))?;
    let ext = std::path::Path::new(&request.path)
        .extension()
        .and_then(|ext| ext.to_str());
    let format = match request.format {
        Some(name) => Format::from_name(&name),
        None => filmcraft_captions::detect(&bytes, ext),
    }
    .ok_or_else(|| anyhow::anyhow!("unsupported caption format"))?;
    let prepared = prepare_file_import(
        &bytes,
        format,
        &request.path,
        request.name.as_deref(),
        request.rate,
    )?;
    let format = match prepared.format {
        CaptionFormat::Subtitle => "Subtitle",
        CaptionFormat::Cea608 => "CEA-608",
        CaptionFormat::Cea708 => "CEA-708",
        CaptionFormat::Teletext => "Teletext",
    };
    Ok(Response {
        name: prepared.name,
        format,
        language: prepared.language,
        enabled: prepared.enabled,
        locked: prepared.locked,
        sync_lock: prepared.sync_lock,
        background_color: prepared.background_color,
        document: prepared.document.into(),
    })
}
pub async fn handle(ApiJson(request): ApiJson<Request>) -> AppResult<Json<Response>> {
    prepare(request)
        .map(Json)
        .map_err(|e| AppError::bad_request(e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graphic_template_fonts::base64_encode;
    #[tokio::test]
    async fn caption_file_import_http_preserves_submicrosecond_frames_and_metadata() {
        use axum::{
            body::Body,
            http::{Request as HttpRequest, StatusCode},
            routing::post,
            Router,
        };
        use http_body_util::BodyExt;
        use serde_json::{json, Value};
        use tower::ServiceExt;
        let vtt="WEBVTT\n\nNOTE retained\n\ncue-1\n00:00:00.540 --> 00:00:02.000 align:start\n<v Ann>Hello\n";
        let request = json!({"dataBase64":base64_encode(vtt.as_bytes()),"path":"/captions/talk.vtt","rate":{"num":24000,"den":1001}});
        let response = Router::new()
            .route("/import", post(handle))
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/import")
                    .header("content-type", "application/json")
                    .body(Body::from(request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(value["name"], "talk");
        assert_eq!(value["format"], "Subtitle");
        assert_eq!(value["syncLock"], true);
        assert_eq!(value["document"]["ticksPerSecond"], "254016000000");
        let cue = &value["document"]["cues"][0];
        assert_eq!(cue["startTick"], "137729592000");
        assert_eq!(cue["id"], "cue-1");
        assert_eq!(cue["speaker"], "Ann");
        assert_eq!(value["document"]["blocks"][0], "NOTE retained");
        let tick = cue["startTick"].as_str().unwrap().parse::<i64>().unwrap();
        assert_ne!(tick % 254016, 0);
        let request =
            json!({"format":"vtt","document":value["document"],"offsetTick":"254016000000"});
        let response = Router::new()
            .route("/write", post(crate::handlers::caption_files::write))
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/write")
                    .header("content-type", "application/json")
                    .body(Body::from(request.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let written: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let bytes = base64_decode(written["dataBase64"].as_str().unwrap()).unwrap();
        let exported = filmcraft_captions::parse(&bytes, Format::WebVtt).unwrap();
        assert_eq!(exported.cues[0].start.0, 0);
        assert_eq!(exported.cues[0].end.0, 254524032000);
        assert_eq!(exported.cues[0].speaker.as_deref(), Some("Ann"));
        assert_eq!(exported.blocks, vec!["NOTE retained"]);
    }
}
