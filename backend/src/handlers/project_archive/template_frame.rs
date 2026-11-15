//! Poster generation uses the same authorized Rust renderer as the editor.
use super::*;
use crate::domain::nested_composition::NestedCompositionProject;
pub(super) async fn render(
    state: AppState,
    mut headers: HeaderMap,
    project: NestedCompositionProject,
    saved: super::super::concat_video_bound::SavedProject,
    composition: String,
    time_us: u64,
    format: image::ImageFormat,
) -> AppResult<Vec<u8>> {
    if !headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| h.starts_with("Bearer "))
    {
        let token = headers
            .get(header::COOKIE)
            .and_then(|h| h.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .map(str::trim)
                    .find_map(|cookie| cookie.strip_prefix("video_kadr_session="))
            })
            .ok_or_else(|| AppError::unauthorized("Требуется сессия"))?;
        let bearer = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| AppError::unauthorized("Некорректная сессия"))?;
        headers.insert(header::AUTHORIZATION, bearer);
    }
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/octet-stream"),
    );
    let canvas = project
        .compositions
        .get(&composition)
        .ok_or_else(|| AppError::bad_request("Таймлайн обложки не найден"))?
        .content
        .canvas;
    let response = Box::pin(super::super::nested_composition::nested_preview_handler(
        State(state),
        headers,
        crate::error::ApiJson(super::super::nested_composition::NestedPreviewRequest {
            saved_project: Some(saved),
            project,
            time_us,
            rendered_outputs: Default::default(),
            composition_id: Some(composition),
        }),
    ))
    .await?;
    let bytes = axum::body::to_bytes(
        response.into_body(),
        canvas.width as usize * canvas.height as usize * 4,
    )
    .await
    .map_err(|e| AppError::internal("read native poster frame", e))?;
    tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let image = image::RgbaImage::from_raw(canvas.width, canvas.height, bytes.to_vec())
            .ok_or_else(|| anyhow::anyhow!("Native poster frame has invalid dimensions"))?;
        let thumbnail = image::DynamicImage::ImageRgba8(image).thumbnail(320, 180);
        let mut encoded = std::io::Cursor::new(Vec::new());
        if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(thumbnail.to_rgb8()).write_to(&mut encoded, format)?;
        } else {
            thumbnail.write_to(&mut encoded, format)?;
        }
        Ok(encoded.into_inner())
    })
    .await
    .map_err(|e| AppError::internal("join native poster encoder", e))?
    .map_err(|e| AppError::bad_request(e.to_string()))
}
