use super::*;
use crate::{
    domain::{
        concat_project_export::export_with_paths, concat_project_fonts,
        nested_composition::NestedCompositionProject,
    },
    project_archive::MAX_ARCHIVE_TOTAL_MEDIA_BYTES,
    services::filmcraft_video_preview::SourceVideoRevision,
    template_bundle,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    #[serde(default)]
    use_saved_cover: bool,
    poster_time_us: Option<u64>,
    poster_composition_id: Option<String>,
}

pub async fn export(
    State(state): State<AppState>,
    AxPath(id): AxPath<String>,
    headers: HeaderMap,
    axum::extract::Query(options): axum::extract::Query<Options>,
) -> AppResult<Response> {
    let actor = archive_actor(&state, &headers, true).await?;
    let slot = state.try_acquire_upload_slot().ok_or_else(|| {
        AppError::too_many_requests("слишком много операций с локальными файлами")
    })?;
    let project = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|error| AppError::internal("load Concat template project", error))?
        .ok_or_else(|| AppError::not_found("Проект не найден"))?;
    let native: NestedCompositionProject = serde_json::from_value(project.document.clone())
        .map_err(|_| AppError::bad_request("Шаблоны требуют нативный проект с таймлайнами"))?;
    let plan =
        template_bundle::plan(&native).map_err(|error| AppError::bad_request(error.to_string()))?;
    let poster = if options.use_saved_cover {
        if options.poster_time_us.is_some() || options.poster_composition_id.is_some() {
            return Err(AppError::bad_request("Выберите один источник обложки"));
        }
        let data = native
            .cover_jpeg
            .as_deref()
            .ok_or_else(|| AppError::bad_request("У сохранённого проекта нет обложки"))?;
        let bytes = STANDARD
            .decode(data)
            .map_err(|_| AppError::bad_request("Некорректная обложка проекта"))?;
        if !bytes.ends_with(&[0xff, 0xd9])
            || image::ImageReader::with_format(
                std::io::Cursor::new(&bytes),
                image::ImageFormat::Jpeg,
            )
            .into_dimensions()
            .is_err()
        {
            return Err(AppError::bad_request("Некорректная обложка проекта"));
        }
        Some(bytes)
    } else if let Some(time) = options.poster_time_us {
        let composition = options
            .poster_composition_id
            .unwrap_or_else(|| native.root.clone());
        Some(
            Box::pin(super::template_frame::render(
                state.clone(),
                headers.clone(),
                native,
                crate::handlers::concat_video_bound::SavedProject {
                    project_id: id.clone(),
                    expected_revision: project.revision,
                },
                composition,
                time,
                image::ImageFormat::Jpeg,
            ))
            .await?,
        )
    } else {
        if options.poster_composition_id.is_some() {
            return Err(AppError::bad_request("Укажите время кадра обложки"));
        }
        None
    };
    let mut originals = Vec::new();
    let mut names = BTreeMap::new();
    let mut total = 0_u64;
    for (source, key) in &plan.assets {
        if !project.source_ids.iter().any(|id| id == source.as_str()) {
            return Err(AppError::conflict(
                "Ресурс отсутствует в сохранённом проекте",
            ));
        }
        crate::handlers::library::authorize_source_access(&state, source.as_str(), &headers)
            .await?;
        let entry = state
            .library
            .get(source.as_str())
            .await
            .filter(|entry| entry.kind == "source")
            .ok_or_else(|| AppError::conflict("Ресурс шаблона отсутствует"))?;
        let path = validated_source_path(&state, &entry).await?;
        let revision = SourceVideoRevision::read(&path)
            .await
            .map_err(|_| AppError::conflict("Ресурс шаблона отсутствует"))?;
        let length = tokio::fs::metadata(&path)
            .await
            .map_err(|error| AppError::internal("stat Concat template asset", error))?
            .len();
        total = total.saturating_add(length);
        if length == 0 || total > MAX_ARCHIVE_TOTAL_MEDIA_BYTES {
            return Err(AppError::payload_too_large(
                "Некорректный размер ресурсов шаблона",
            ));
        }
        let extension = Path::new(&entry.filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .filter(|ext| {
                !ext.is_empty()
                    && ext.len() <= 12
                    && ext.bytes().all(|byte| byte.is_ascii_alphanumeric())
            });
        let name = match extension {
            Some(ext) => format!("assets/{}.{ext}", key.as_str()),
            None => format!("assets/{}", key.as_str()),
        };
        names.insert(key.clone(), name.clone());
        originals.push((source.clone(), entry, path, revision, name, length));
    }
    let inputs: Vec<_> = originals
        .iter()
        .map(|(_, _, path, _, name, length)| (path.clone(), name.clone(), *length))
        .collect();
    let paths = plan
        .project
        .project_media
        .iter()
        .map(|(id, item)| {
            let path = item
                .library_source_id
                .as_ref()
                .map(|key| names[key].clone())
                .unwrap_or_default();
            (id.as_str().to_owned(), path)
        })
        .collect();
    let output = state
        .staging_dir()
        .join(format!("{}.concat-template", Uuid::new_v4()));
    let title = project.name.clone();
    let archive = tokio::task::spawn_blocking(move || -> AppResult<TemporaryFile> {
        let _slot = slot;
        let mut native = plan.project;
        let fonts = std::mem::take(&mut native.fonts);
        let _font_scope = concat_project_fonts::Scope::enter(&fonts)
            .map_err(|error| AppError::bad_request(format!("{error:#}")))?;
        let subtitles = native.compositions.get_mut(&native.root)
            .and_then(|root| root.content.metadata.as_mut())
            .map(|metadata| std::mem::take(&mut metadata.subtitle_documents))
            .unwrap_or_default();
        if !crate::domain::metadata_burnin::validate_subtitle_documents(&subtitles) {
            return Err(AppError::bad_request("Некорректный пул субтитров"));
        }
        let subtitle_sidecar = if subtitles.is_empty() {
            None
        } else {
            Some(serde_json::to_vec(&subtitles)
                .map_err(|error| AppError::internal("encode template subtitle documents", error))?)
        };
        let mut document = export_with_paths(&native, &paths)
            .map_err(|error| AppError::bad_request(format!("{error:#}")))?;
        document["name"] = Value::String(title);
        let mut font_files = Vec::new();
        let mut font_links = Vec::new();
        for (index, font) in fonts.into_iter().enumerate() {
            let bytes = STANDARD
                .decode(font.data)
                .map_err(|error| AppError::bad_request(error.to_string()))?;
            let extension = match bytes.get(..4) {
                Some(b"OTTO") => "otf",
                Some(b"ttcf") => "ttc",
                _ => "ttf",
            };
            let path = format!("assets/font-{index:04}.{extension}");
            font_links.push(serde_json::json!({ "family": font.family, "path": path }));
            font_files.push((path, bytes));
        }
        document["fonts"] = Value::Array(font_links);
        let manifest = serde_json::to_vec(&document)
            .map_err(|error| AppError::internal("encode Concat template", error))?;
        if manifest.len() > 4 * 1024 * 1024 {
            return Err(AppError::payload_too_large("Файл Concat превышает 4 МиБ"));
        }
        let mut exported_clips = std::collections::BTreeMap::new();
        for timeline in document["timelines"].as_array().into_iter().flatten() {
            let timeline_id = timeline["id"].as_str().expect("exported timeline identity");
            for clip in timeline["clips"].as_array().into_iter().flatten() {
                let clip_id = clip["id"].as_str().expect("exported clip identity");
                exported_clips.insert((timeline_id, clip_id), (clip, &timeline["video"]));
            }
        }
        let mut authored_captions = Vec::new();
        for (timeline_id, node) in &native.compositions {
            let Some(metadata) = &node.content.metadata else { continue };
            for track in &node.content.tracks {
                if let crate::domain::composition::CompositionTrack::Text { id, clips, .. } = track {
                    if metadata.caption_text_tracks.iter().any(|role| role == id.as_str()) {
                        for clip in clips {
                            let (baseline, video) = exported_clips
                                .get(&(timeline_id.as_str(), clip.id.as_str()))
                                .ok_or_else(|| AppError::bad_request("Клип субтитров отсутствует в экспорте Concat"))?;
                            authored_captions.push((timeline_id, clip, serde_json::json!({
                                "clip": baseline, "video": video,
                            })));
                        }
                    }
                }
            }
        }
        let captions_sidecar = if authored_captions.is_empty() {
            None
        } else {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "manifestSha256": format!("{:x}", Sha256::digest(&manifest)),
                "clips": authored_captions,
            })).map_err(|error| AppError::internal("encode template captions", error))?;
            if bytes.len() > 8 * 1024 * 1024 {
                return Err(AppError::payload_too_large("Дорожки субтитров шаблона превышают 8 МиБ"));
            }
            Some(bytes)
        };
        let archive = TemporaryFile::new(output);
        (|| -> anyhow::Result<()> {
            let file = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(archive.path())?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file("template.json", options)?;
            zip.write_all(&manifest)?;
            if let Some(bytes) = subtitle_sidecar {
                zip.start_file("video-kadr-subtitles.json", options)?;
                zip.write_all(&bytes)?;
            }
            if let Some(bytes) = captions_sidecar {
                zip.start_file("video-kadr-caption-clips.json", options)?;
                zip.write_all(&bytes)?;
            }
            if let Some(bytes) = poster {
                zip.start_file("poster.jpg", options)?;
                zip.write_all(&bytes)?;
            }
            for (path, bytes) in font_files {
                zip.start_file(path, options)?;
                zip.write_all(&bytes)?;
            }
            for (path, name, length) in inputs {
                zip.start_file(name, options)?;
                let mut source = std::fs::File::open(path)?.take(length + 1);
                anyhow::ensure!(
                    std::io::copy(&mut source, &mut zip)? == length,
                    "Template asset size changed"
                );
            }
            zip.finish()?.sync_all()?;
            Ok(())
        })()
        .map_err(|error| AppError::conflict(error.to_string()))?;
        Ok(archive)
    })
    .await
    .map_err(|error| AppError::internal("join Concat template export", error))??;
    for (source, entry, path, revision, _, _) in originals {
        crate::handlers::library::authorize_source_access(&state, source.as_str(), &headers)
            .await?;
        let current = state
            .library
            .get(source.as_str())
            .await
            .ok_or_else(|| AppError::conflict("Ресурс шаблона изменился"))?;
        if serde_json::to_value(&current).ok() != serde_json::to_value(&entry).ok()
            || validated_source_path(&state, &current).await? != path
            || SourceVideoRevision::read(&path).await.ok().as_ref() != Some(&revision)
        {
            return Err(AppError::conflict("Ресурс шаблона изменился"));
        }
    }
    archive_actor(&state, &headers, true).await?;
    let current = state
        .db
        .get_composition_project_for(&id, &actor)
        .await
        .map_err(|error| AppError::internal("recheck Concat template project", error))?
        .ok_or_else(|| AppError::conflict("Доступ к проекту отозван"))?;
    if current.revision != project.revision
        || current.document != project.document
        || current.source_ids != project.source_ids
    {
        return Err(AppError::conflict(
            "Проект изменился во время упаковки шаблона",
        ));
    }
    let file = tokio::fs::File::open(archive.path())
        .await
        .map_err(|error| AppError::internal("open Concat template", error))?;
    let length = file
        .metadata()
        .await
        .map_err(|error| AppError::internal("stat Concat template", error))?
        .len();
    let base = archive_download_filename(&project.name);
    let filename = format!("{}-concat.zip", base.trim_end_matches(".veproj"));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from_stream(ReaderStream::new(
            archive.into_reader(file),
        )))
        .map_err(|error| AppError::internal("Concat template response", error))
}
