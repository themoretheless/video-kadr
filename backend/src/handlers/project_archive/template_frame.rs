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

pub(super) async fn receive(mut multipart: Multipart, max_bytes: usize) -> AppResult<Vec<u8>> {
    run_with_timeout(
        async {
            let field = multipart
                .next_field()
                .await
                .map_err(multipart_error)?
                .filter(|field| field.name() == Some("poster"))
                .ok_or_else(|| AppError::bad_request("Выберите файл обложки"))?;
            let bytes = field.bytes().await.map_err(multipart_error)?;
            if bytes.len() > max_bytes {
                return Err(AppError::payload_too_large(
                    "Обложка превышает допустимый размер",
                ));
            }
            if multipart
                .next_field()
                .await
                .map_err(multipart_error)?
                .is_some()
            {
                return Err(AppError::bad_request("Выберите один файл обложки"));
            }
            Ok(bytes.to_vec())
        },
        Duration::from_secs(30),
        "Загрузка обложки превысила лимит времени",
    )
    .await?
}

pub(super) fn encode_cover(bytes: Vec<u8>, output: image::ImageFormat) -> AppResult<Vec<u8>> {
    let invalid = || AppError::bad_request("Обложка должна быть корректным PNG или JPEG");
    let format = image::guess_format(&bytes).map_err(|_| invalid())?;
    match format {
        image::ImageFormat::Png => {}
        image::ImageFormat::Jpeg if bytes.ends_with(&[0xff, 0xd9]) => {}
        _ => return Err(invalid()),
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| invalid())?;
    let thumbnail = if image.width() > 320 || image.height() > 180 {
        image.thumbnail(320, 180)
    } else {
        image
    };
    let thumbnail = if output == image::ImageFormat::Jpeg {
        image::DynamicImage::ImageRgb8(thumbnail.into_rgb8())
    } else {
        thumbnail
    };
    let mut encoded = std::io::Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut encoded, output)
        .map_err(|e| AppError::internal("encode project cover", e))?;
    Ok(encoded.into_inner())
}

pub async fn normalize_concat_cover(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiMultipart(multipart): ApiMultipart,
) -> AppResult<Json<Value>> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    archive_actor(&state, &headers, false).await?;
    let permit = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let bytes = receive(multipart, crate::template_bundle::MAX_CONCAT_POSTER_BYTES).await?;
    let cover = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        encode_cover(bytes, image::ImageFormat::Jpeg).map(|jpeg| STANDARD.encode(jpeg))
    })
    .await
    .map_err(|e| AppError::internal("join Concat cover normalizer", e))??;
    Ok(Json(serde_json::json!({ "coverJpeg": cover })))
}

/// Concat looks for a bright source frame before rendering the edited composition.
pub(super) async fn automatic_concat_cover(
    state: &AppState,
    headers: &HeaderMap,
    saved: &crate::db::CompositionProject,
    project: &NestedCompositionProject,
    composition: &str,
    format: image::ImageFormat,
) -> AppResult<Option<Vec<u8>>> {
    use crate::domain::composition::CompositionTrack;
    use crate::services::filmcraft_video_preview::SourceVideoRevision;

    let content = &project
        .compositions
        .get(composition)
        .ok_or_else(|| AppError::bad_request("Таймлайн обложки не найден"))?
        .content;
    let ticks_per_second = f64::from(content.time_base);
    let mut clips = Vec::new();
    for track in &content.tracks {
        match track {
            CompositionTrack::Image { clips: items, .. } => {
                clips.extend(
                    items
                        .iter()
                        .map(|clip| (clip.timeline_start_tick, &clip.source_id, 0.0, 0.0)),
                );
            }
            CompositionTrack::Video { clips: items, .. } => {
                clips.extend(items.iter().filter_map(|clip| {
                    let placement = &clip.placement;
                    let duration =
                        placement.timeline_duration_ticks().ok()? as f64 / ticks_per_second;
                    Some((
                        placement.timeline_start_tick,
                        &clip.source_id,
                        placement.source_in_tick as f64 / ticks_per_second,
                        duration * placement.speed.max(0.0),
                    ))
                }));
            }
            _ => {}
        }
    }
    clips.sort_by_key(|clip| clip.0);
    let mut moments = Vec::new();
    for (_, source, head, span) in clips {
        let mut times = vec![head, head + span.min(4.0) * 0.25, head + span * 0.5];
        times.dedup_by(|left, right| (*left - *right).abs() < 0.001);
        moments.extend(
            times
                .into_iter()
                .map(|time| (project.library_source_id(source), time)),
        );
        if moments.len() >= 12 {
            break;
        }
    }
    let cancel = tokio_util::sync::CancellationToken::new();
    let _cancel_on_drop = cancel.clone().drop_guard();
    for (source, time) in moments.into_iter().take(12) {
        let source = source.as_str();
        if !saved.source_ids.iter().any(|id| id == source)
            || crate::handlers::library::authorize_source_access(state, source, headers)
                .await
                .is_err()
        {
            continue;
        }
        let Some(entry) = state
            .library
            .get(source)
            .await
            .filter(|entry| entry.kind == "source")
        else {
            continue;
        };
        let Ok(path) = validated_source_path(state, &entry).await else {
            continue;
        };
        let Ok(revision) = SourceVideoRevision::read(&path).await else {
            continue;
        };
        let mut command = tokio::process::Command::new("ffmpeg");
        command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-protocol_whitelist",
                crate::tools::OFFLINE_PROTOCOLS,
                "-ss",
            ])
            .arg(format!("{time:.9}"))
            .arg("-i")
            .arg(&path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "scale=480:1080:force_original_aspect_ratio=decrease",
                "-threads",
                "1",
                "-pix_fmt",
                "rgba",
                "-f",
                "image2pipe",
                "-c:v",
                "png",
                "pipe:1",
            ]);
        let mut policy = state.process_runtime.probe_policy(&path);
        policy.output.capture_bytes = 3 * 1024 * 1024;
        let Ok(output) = crate::process_control::capture_output_cancellable(
            &state.process_runtime,
            command,
            policy,
            Duration::from_secs(3),
            &cancel,
        )
        .await
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let candidate =
            tokio::task::spawn_blocking(move || automatic_cover_bytes(&output.stdout, format))
                .await
                .map_err(|e| AppError::internal("join Concat poster encoder", e))?;
        if candidate.is_some()
            && crate::handlers::library::authorize_source_access(state, source, headers)
                .await
                .is_ok()
            && state.library.get(source).await.is_some_and(|current| {
                serde_json::to_value(&current).ok() == serde_json::to_value(&entry).ok()
            })
            && SourceVideoRevision::read(&path).await.ok().as_ref() == Some(&revision)
        {
            return Ok(candidate);
        }
    }
    Ok(None)
}

fn automatic_cover_bytes(png: &[u8], format: image::ImageFormat) -> Option<Vec<u8>> {
    let frame = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let mut brightness = 0_u64;
    let mut count = 0_u64;
    for pixel in frame.pixels().step_by(4) {
        brightness += u64::from(pixel[0].max(pixel[1]).max(pixel[2]));
        count += 1;
    }
    if count == 0 || brightness / count < 10 {
        return None;
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    let image = image::DynamicImage::ImageRgba8(frame);
    let image = if format == image::ImageFormat::Png {
        image.thumbnail(320, 180)
    } else {
        image::DynamicImage::ImageRgb8(image.into_rgb8())
    };
    image.write_to(&mut encoded, format).ok()?;
    Some(encoded.into_inner())
}
