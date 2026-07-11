use std::future::Future;
use std::path::Path;
use std::time::Duration;

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;
use uuid::Uuid;

use crate::library::MediaEntry;
use crate::state::AppState;
use crate::tools::{self, ProbeInfo};

const UPLOAD_RECEIVE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
struct ReceivedUpload {
    original_name: Option<String>,
    total_bytes: u64,
}

/// Accept a local media file. The client filename is display-only: the stored
/// extension is derived from ffprobe so an HTML/SVG filename cannot control the
/// response MIME type under `/files/sources`.
pub async fn upload_handler(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _upload_slot = state.try_acquire_upload_slot().ok_or_else(|| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            "слишком много одновременных загрузок, повторите позже".into(),
        )
    })?;
    let video_id = Uuid::new_v4().to_string();
    let sources = state.sources_dir();
    let temporary_path = sources.join(format!("{video_id}.upload"));

    let received = receive_with_timeout(
        receive_upload(multipart, &temporary_path),
        &temporary_path,
        UPLOAD_RECEIVE_TIMEOUT,
    )
    .await?;

    if received.total_bytes == 0 {
        remove_quietly(&temporary_path).await;
        return Err((StatusCode::BAD_REQUEST, "пустой файл".into()));
    }

    let info = match tools::probe_video(&temporary_path).await {
        Ok(info) if info.width > 0 || info.duration > 0.0 => info,
        Err(error) => {
            remove_quietly(&temporary_path).await;
            return Err(probe_error_response(&error));
        }
        _ => {
            remove_quietly(&temporary_path).await;
            return Err((
                StatusCode::BAD_REQUEST,
                "не удалось распознать видео в файле".into(),
            ));
        }
    };
    let Some(extension) = safe_upload_extension(&info) else {
        remove_quietly(&temporary_path).await;
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "формат файла не поддерживается".into(),
        ));
    };

    let filename = format!("{video_id}.{extension}");
    let path = sources.join(&filename);
    if let Err(error) = tokio::fs::rename(&temporary_path, &path).await {
        remove_quietly(&temporary_path).await;
        return Err((StatusCode::INTERNAL_SERVER_ERROR, error.to_string()));
    }

    let size = tokio::fs::metadata(&path).await.map(|meta| meta.len()).ok();
    let title = received.original_name.as_deref().map(display_title);
    let body = json!({
        "id": video_id,
        "url": format!("/files/sources/{filename}"),
        "filename": filename,
        "duration": info.duration,
        "width": info.width,
        "height": info.height,
        "title": title,
        "fps": info.fps,
        "vcodec": info.vcodec,
        "acodec": info.acodec,
        "sizeBytes": size,
    });
    state
        .library
        .add(MediaEntry::from_result("source", &body))
        .await;
    Ok(Json(body))
}

fn probe_error_response(error: &anyhow::Error) -> (StatusCode, String) {
    if tools::is_tool_timeout(error) {
        (
            StatusCode::GATEWAY_TIMEOUT,
            "анализ файла превысил лимит времени".into(),
        )
    } else {
        (
            StatusCode::BAD_REQUEST,
            "не удалось распознать видео в файле".into(),
        )
    }
}

async fn receive_with_timeout<F>(
    receive: F,
    temporary_path: &Path,
    limit: Duration,
) -> Result<ReceivedUpload, (StatusCode, String)>
where
    F: Future<Output = Result<ReceivedUpload, (StatusCode, String)>>,
{
    match timeout(limit, receive).await {
        Ok(Ok(received)) => Ok(received),
        Ok(Err(error)) => {
            remove_quietly(temporary_path).await;
            Err(error)
        }
        Err(_) => {
            remove_quietly(temporary_path).await;
            Err((
                StatusCode::REQUEST_TIMEOUT,
                "загрузка файла превысила лимит времени".into(),
            ))
        }
    }
}

async fn receive_upload(
    mut multipart: Multipart,
    temporary_path: &Path,
) -> Result<ReceivedUpload, (StatusCode, String)> {
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?
    {
        let original = field.file_name().map(str::to_owned);
        if field.name() != Some("file") && original.is_none() {
            continue;
        }

        let total_bytes = stream_field_to_file(&mut field, temporary_path).await?;
        return Ok(ReceivedUpload {
            original_name: original,
            total_bytes,
        });
    }

    Err((StatusCode::BAD_REQUEST, "файл не найден в запросе".into()))
}

async fn stream_field_to_file(
    field: &mut axum::extract::multipart::Field<'_>,
    path: &Path,
) -> Result<u64, (StatusCode, String)> {
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(internal_error)?;
    let mut total = 0_u64;
    while let Some(chunk) = field.chunk().await.map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("не удалось прочитать файл: {error}"),
        )
    })? {
        file.write_all(&chunk).await.map_err(internal_error)?;
        total += chunk.len() as u64;
    }
    file.flush().await.map_err(internal_error)?;
    Ok(total)
}

fn internal_error(error: std::io::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

async fn remove_quietly(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
}

fn display_title(original: &str) -> String {
    Path::new(original)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(original)
        .to_owned()
}

fn safe_upload_extension(info: &ProbeInfo) -> Option<&'static str> {
    let formats = info.format_name.as_deref().unwrap_or_default();
    let has = |expected: &str| formats.split(',').any(|format| format == expected);

    if has("mov") || has("mp4") || has("m4a") || has("3gp") || has("3g2") || has("mj2") {
        Some("mp4")
    } else if has("matroska") || has("webm") {
        let webm_video = matches!(info.vcodec.as_deref(), Some("vp8" | "vp9" | "av1"));
        let webm_audio = matches!(info.acodec.as_deref(), None | Some("opus" | "vorbis"));
        Some(if webm_video && webm_audio {
            "webm"
        } else {
            "mkv"
        })
    } else if has("avi") {
        Some("avi")
    } else if has("flv") {
        Some("flv")
    } else if has("mpegts") {
        Some("ts")
    } else if has("mpeg") {
        Some("mpg")
    } else if has("ogg") {
        Some(if info.width > 0 { "ogv" } else { "ogg" })
    } else if has("gif") {
        Some("gif")
    } else if has("mp3") {
        Some("mp3")
    } else if has("wav") {
        Some("wav")
    } else if has("flac") {
        Some("flac")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(format_name: &str, vcodec: Option<&str>, acodec: Option<&str>) -> ProbeInfo {
        ProbeInfo {
            duration: 1.0,
            width: u32::from(vcodec.is_some()),
            height: u32::from(vcodec.is_some()),
            fps: None,
            vcodec: vcodec.map(str::to_owned),
            acodec: acodec.map(str::to_owned),
            format_name: Some(format_name.to_owned()),
        }
    }

    #[test]
    fn upload_extension_comes_from_probed_container() {
        assert_eq!(
            safe_upload_extension(&probe("mov,mp4,m4a,3gp,3g2,mj2", Some("h264"), None)),
            Some("mp4")
        );
        assert_eq!(
            safe_upload_extension(&probe("matroska,webm", Some("vp9"), Some("opus"))),
            Some("webm")
        );
        assert_eq!(
            safe_upload_extension(&probe("matroska,webm", Some("h264"), Some("aac"))),
            Some("mkv")
        );
        assert_eq!(safe_upload_extension(&probe("html", None, None)), None);
    }

    #[tokio::test]
    async fn receive_timeout_removes_the_temporary_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pending.upload");
        tokio::fs::write(&path, b"partial").await.unwrap();

        let error = receive_with_timeout(std::future::pending(), &path, Duration::from_millis(10))
            .await
            .unwrap_err();

        assert_eq!(error.0, StatusCode::REQUEST_TIMEOUT);
        assert!(tokio::fs::metadata(path).await.is_err());
    }

    #[test]
    fn probe_timeout_maps_to_gateway_timeout() {
        let error = anyhow::Error::new(crate::tools::ToolTimeout);
        let response = probe_error_response(&error);

        assert_eq!(response.0, StatusCode::GATEWAY_TIMEOUT);
        assert!(response.1.contains("превысил лимит времени"));
        assert_eq!(
            probe_error_response(&anyhow::anyhow!("invalid media")).0,
            StatusCode::BAD_REQUEST
        );
    }
}
