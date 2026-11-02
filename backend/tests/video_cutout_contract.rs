mod support;
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::*,
    metadata_burnin::{CompositionMetadata, MediaVersions},
    video_cutout::Attachment,
};
fn attachment() -> Value {
    json!({"sourceId":"original","maskSourceId":"automatic-masks","sourceHash":"a".repeat(64),"maskHash":"b".repeat(64),"maskFileHash":"c".repeat(64),"width":37,"height":23,"maskWidth":512,"maskHeight":320,"frames":4,"frameRate":{"numerator":4,"denominator":1},"corrections":[]})
}
fn composition() -> Composition {
    let mut value = Composition::new(CanvasSpec::default());
    for (id, width, height) in [
        ("original", 37, 23),
        ("copy", 37, 23),
        ("automatic-masks", 512, 320),
    ] {
        let id = SourceId::parse(id).unwrap();
        value.sources.insert(
            id.clone(),
            CompositionSource {
                id,
                kind: SourceKind::Video,
                duration_ticks: 1_000_000,
                width,
                height,
                has_audio: false,
                audio_channels: None,
            },
        );
    }
    let clip = CompositionClipId::parse("clip").unwrap();
    value.tracks.push(CompositionTrack::Video {
        id: TrackId::parse("video").unwrap(),
        name: "Video".into(),
        hidden: false,
        muted: false,
        locked: false,
        transitions: vec![],
        clips: vec![VideoClip {concat_cutout:None,audio_stream:None,concat_gain_keys:Vec::new(),dsp_effects:Vec::new(),independent_audio:false,audio_fade_in_ticks:0,audio_fade_out_ticks:0,
            id: clip.clone(),
            source_id: SourceId::parse("copy").unwrap(),
            placement: ClipPlacement {
                concat_source_eof: false,
timeline_start_tick: 0,
                source_in_tick: 0,
                source_out_tick: 1_000_000,
                speed: 1.,
                timeline_duration_tick: None,
                source_timeline_offset_tick: None,
                source_head_timeline_tick: None, concat_retime: None,
                speed_ramp: None,
            },
            effect_timing: None,
            filmcraft_motion: None,
            playback_mode: PlaybackMode::Forward,
            stabilization: StabilizationSpec::Disabled,
            frame_interpolation: FrameInterpolation::Duplicate,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(1.),
            blend_mode: BlendMode::Normal,
            effects: vec![],
            source_audio_enabled: false,
            audio_gain: AnimatableValue::constant(1.),
            audio_pan: AnimatableValue::constant(0.),
            enabled: true,
        }],
    });
    let mut metadata = CompositionMetadata::default();
    metadata.media_versions.insert(
        clip,
        MediaVersions {
            original_source_id: SourceId::parse("original").unwrap(),
            copy_source_id: SourceId::parse("copy").unwrap(),
            cutout: None,
            video_cutout: Some(serde_json::from_value(attachment()).unwrap()),
        },
    );
    value.metadata = Some(metadata);
    value
}
#[test]
fn portable_video_mask_contract_roundtrip_and_resource_bindings() {
    let value = composition();
    value.validate().unwrap();
    let encoded = serde_json::to_value(&value).unwrap();
    let restored: Composition = serde_json::from_value(encoded.clone()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, value);
    for mode in [
        "missing-mask",
        "wrong-kind",
        "audio-mask",
        "wrong-shape",
        "wrong-original",
    ] {
        let mut broken = value.clone();
        let id = SourceId::parse("automatic-masks").unwrap();
        match mode {
            "missing-mask" => {
                broken.sources.remove(&id);
            }
            "wrong-kind" => broken.sources.get_mut(&id).unwrap().kind = SourceKind::Image,
            "audio-mask" => broken.sources.get_mut(&id).unwrap().has_audio = true,
            "wrong-shape" => broken.sources.get_mut(&id).unwrap().width = 511,
            _ => {
                broken
                    .sources
                    .get_mut(&SourceId::parse("original").unwrap())
                    .unwrap()
                    .width = 38
            }
        };
        assert!(broken.validate().is_err(), "{mode}");
    }
    for mode in [
        "count",
        "shape",
        "clock",
        "hash",
        "alias",
        "unknown",
        "duplicate",
    ] {
        let mut v = attachment();
        match mode {
            "count" => v["frames"] = json!(0),
            "shape" => v["maskWidth"] = json!(511),
            "clock" => v["frameRate"]["denominator"] = json!(0),
            "hash" => v["maskHash"] = json!("x".repeat(64)),
            "alias" => v["maskSourceId"] = json!("original"),
            "unknown" => v["path"] = json!("/unsafe"),
            _ => {
                let edit =
                    json!({"frame":0,"correction":{"custom":false,"strokes":[],"feather":0}});
                v["corrections"] = json!([edit, edit]);
            }
        }
        assert!(serde_json::from_value::<Attachment>(v).is_err(), "{mode}");
    }
}
#[tokio::test]
#[ignore = "requires host FFmpeg/FFprobe, native correction executable and retained original RVM sequence oracle"]
async fn actual_lossless_mask_video_native_project_archive_roundtrip() {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;
    let (state, _dir) = support::make_state(true, false).await;
    let state =
        state.with_person_config(Some(video_kadr_backend::services::local_person::Config {
            executable: std::env::var("VIDEO_KADR_TEST_PERSON_BIN").unwrap().into(),
            model: _dir.path().join("model-absent.onnx"),
            runtime_library: _dir.path().join("ort-absent.dylib"),
        }));
    let oracle =
        std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_ORACLE_DIR").unwrap());
    let tool = std::env::var("VIDEO_KADR_TEST_FFMPEG").unwrap();
    let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let mut content = composition();
    for (id, input, width, height, format, codec, extension) in [
        ("original", "input.rgba", 37, 23, "rgba", "qtrle", "mov"),
        ("copy", "reference.rgba", 37, 23, "rgba", "qtrle", "mov"),
        (
            "automatic-masks",
            "reference.masks",
            512,
            320,
            "gray",
            "ffv1",
            "mkv",
        ),
    ] {
        let filename = format!("{id}.{extension}");
        let path = state.sources_dir().join(&filename);
        let output = std::process::Command::new(&tool)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "rawvideo",
                "-pixel_format",
                format,
                "-video_size",
                &format!("{width}x{height}"),
                "-framerate",
                "4",
                "-i",
            ])
            .arg(oracle.join(input))
            .args([
                "-c:v",
                codec,
                "-pix_fmt",
                if format == "gray" { "gray" } else { "argb" },
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let bytes = std::fs::read(&path).unwrap();
        let pair = content
            .metadata
            .as_mut()
            .unwrap()
            .media_versions
            .get_mut(&CompositionClipId::parse("clip").unwrap())
            .unwrap();
        let snapshot = pair.video_cutout.as_mut().unwrap();
        if id == "original" {
            snapshot.source_hash = hash(&bytes);
        }
        if id == "automatic-masks" {
            snapshot.mask_file_hash = hash(&bytes);
            snapshot.mask_hash = hash(&std::fs::read(oracle.join(input)).unwrap());
        }
        assert!(state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":width,"height":height,"duration":1.,"fps":4.,"vcodec":codec,"sizeBytes":bytes.len()}))).await);
    }
    let owner = state
        .db
        .register_auth_user("video-mask-portable", "public archive test password")
        .await
        .unwrap()
        .unwrap();
    let app = support::router(state.clone());
    let project = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}});
    let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/composition-projects").header("authorization",format!("Bearer {}",owner.token)).header("content-type","application/json").body(Body::from(json!({"schemaVersion":2,"mode":"composition","name":"Video masks","document":project}).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let data = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&data)
    );
    let saved: Value = serde_json::from_slice(&data).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/composition-projects/{}/archive",
                    saved["id"].as_str().unwrap()
                ))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Imported video masks")
        .await
        .unwrap();
    let mut multipart=b"--VIDEO_MASK\r\nContent-Disposition: form-data; name=\"file\"; filename=\"video.veproj\"\r\nContent-Type: application/vnd.video-editor.project\r\n\r\n".to_vec();
    multipart.extend_from_slice(&archive);
    multipart.extend_from_slice(b"\r\n--VIDEO_MASK--\r\n");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/composition-projects/import")
                .header("authorization", format!("Bearer {}", owner.token))
                .header("x-space-id", &space.id)
                .header("content-type", "multipart/form-data; boundary=VIDEO_MASK")
                .body(Body::from(multipart))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .unwrap();
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let restored: Value = serde_json::from_slice(&bytes).unwrap();
    let restored_content: Composition = serde_json::from_value(
        restored["project"]["document"]["compositions"]["root"]["content"].clone(),
    )
    .unwrap();
    restored_content.validate().unwrap();
    let snapshot = restored_content
        .metadata
        .as_ref()
        .unwrap()
        .media_versions
        .get(&CompositionClipId::parse("clip").unwrap())
        .unwrap()
        .video_cutout
        .as_ref()
        .unwrap();
    assert_eq!(
        snapshot.source_id,
        restored["sourceMapping"]["original"].as_str().unwrap()
    );
    assert_eq!(
        snapshot.mask_source_id,
        restored["sourceMapping"]["automatic-masks"]
            .as_str()
            .unwrap()
    );
    assert_ne!(snapshot.source_id, "original");
    assert_ne!(snapshot.mask_source_id, "automatic-masks");
    let original = content
        .metadata
        .as_ref()
        .unwrap()
        .media_versions
        .get(&CompositionClipId::parse("clip").unwrap())
        .unwrap()
        .video_cutout
        .as_ref()
        .unwrap();
    assert_eq!(snapshot.source_hash, original.source_hash);
    assert_eq!(snapshot.mask_hash, original.mask_hash);
    assert_eq!(snapshot.mask_file_hash, original.mask_file_hash);
    assert_eq!(snapshot.corrections, original.corrections);
    for old in ["original", "copy", "automatic-masks"] {
        let new = restored["sourceMapping"][old].as_str().unwrap();
        let entry = state.library.get(new).await.unwrap();
        let path = state.library.resolve_media_path(&entry).await.unwrap();
        let original = state.library.get(old).await.unwrap();
        let old_path = state.library.resolve_media_path(&original).await.unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            std::fs::read(&old_path).unwrap()
        );
        if old == "automatic-masks" {
            let decoded = std::process::Command::new(&tool)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&path)
                .args([
                    "-map",
                    "0:v:0",
                    "-an",
                    "-fps_mode",
                    "passthrough",
                    "-pix_fmt",
                    "gray",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(decoded.status.success());
            assert_eq!(hash(&decoded.stdout), snapshot.mask_hash);
            let restored_raw = _dir.path().join("restored-project-masks.raw");
            let (progress, _updates) = tokio::sync::mpsc::unbounded_channel();
            video_kadr_backend::services::local_person_mask_video::decode(
                &state.process_runtime,
                video_kadr_backend::services::local_person_mask_video::Tools {
                    ffmpeg: std::path::Path::new(&tool),
                    ffprobe: std::path::Path::new(
                        &std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap(),
                    ),
                },
                &path,
                &video_kadr_backend::services::local_person_mask_video::Metadata {
                    width: snapshot.mask_width,
                    height: snapshot.mask_height,
                    frames: snapshot.frames,
                    frame_rate: snapshot.frame_rate,
                    mask_hash: snapshot.mask_hash.clone(),
                    file_hash: snapshot.mask_file_hash.clone(),
                    size_bytes: std::fs::metadata(&path).unwrap().len(),
                },
                &restored_raw,
                64 * 1024 * 1024 * 1024,
                &tokio_util::sync::CancellationToken::new(),
                std::time::Duration::from_secs(30),
                &progress,
            )
            .await
            .unwrap();
            assert_eq!(std::fs::read(&restored_raw).unwrap(), decoded.stdout);
            let source_entry = state.library.get(&snapshot.source_id).await.unwrap();
            let imported_source = state
                .library
                .resolve_media_path(&source_entry)
                .await
                .unwrap();
            let corrected = _dir.path().join("replayed-project.mov");
            let corrected_masks = _dir.path().join("replayed-project.raw");
            let native =
                std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_PERSON_BIN").unwrap());
            let codecs =
                std::path::PathBuf::from(std::env::var("VIDEO_KADR_TEST_FFPROBE").unwrap());
            let replay = video_kadr_backend::services::local_person_video::correct(
                &state.process_runtime,
                video_kadr_backend::services::local_person_video::Tools {
                    ffmpeg: std::path::Path::new(&tool),
                    ffprobe: &codecs,
                },
                &native,
                video_kadr_backend::services::local_person_video::CorrectionInput {
                    video: video_kadr_backend::services::local_person_video::Input {
                        source: &imported_source,
                        source_hash: &snapshot.source_hash,
                        mask_destination: Some(&corrected_masks),
                        staging_bytes: 64 * 1024 * 1024 * 1024,
                    },
                    masks: &restored_raw,
                    mask_hash: &snapshot.mask_hash,
                    corrections: &snapshot.corrections,
                },
                &corrected,
                &tokio_util::sync::CancellationToken::new(),
                std::time::Duration::from_secs(30),
                &progress,
            )
            .await
            .unwrap();
            assert_eq!(
                replay.correction.unwrap().source_mask_hash,
                snapshot.mask_hash
            );
            assert_eq!(std::fs::read(&corrected_masks).unwrap(), decoded.stdout);
            let replayed = std::process::Command::new(&tool)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&corrected)
                .args([
                    "-map",
                    "0:v:0",
                    "-an",
                    "-fps_mode",
                    "passthrough",
                    "-pix_fmt",
                    "rgba",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(replayed.status.success());
            assert_eq!(
                replayed.stdout,
                std::fs::read(oracle.join("reference.rgba")).unwrap()
            );
            let points: Vec<[f64; 2]> = (0..9)
                .flat_map(|y| (0..9).map(move |x| [x as f64 / 8., y as f64 / 8.]))
                .collect();
            let edits:Vec<video_kadr_backend::domain::video_cutout::FrameCorrection>=serde_json::from_value(json!([{"frame":1,"correction":{"custom":true,"strokes":[{"tool":"eraser","size":0.5,"points":points}],"feather":0}}])).unwrap();
            let authored = _dir.path().join("authored-project.mov");
            let authored_masks = _dir.path().join("authored-project.raw");
            video_kadr_backend::services::local_person_video::correct(
                &state.process_runtime,
                video_kadr_backend::services::local_person_video::Tools {
                    ffmpeg: std::path::Path::new(&tool),
                    ffprobe: &codecs,
                },
                &native,
                video_kadr_backend::services::local_person_video::CorrectionInput {
                    video: video_kadr_backend::services::local_person_video::Input {
                        source: &imported_source,
                        source_hash: &snapshot.source_hash,
                        mask_destination: Some(&authored_masks),
                        staging_bytes: 64 * 1024 * 1024 * 1024,
                    },
                    masks: &restored_raw,
                    mask_hash: &snapshot.mask_hash,
                    corrections: &edits,
                },
                &authored,
                &tokio_util::sync::CancellationToken::new(),
                std::time::Duration::from_secs(30),
                &progress,
            )
            .await
            .unwrap();
            let authored = std::process::Command::new(&tool)
                .args(["-nostdin", "-v", "error", "-i"])
                .arg(&authored)
                .args([
                    "-map",
                    "0:v:0",
                    "-an",
                    "-fps_mode",
                    "passthrough",
                    "-pix_fmt",
                    "rgba",
                    "-f",
                    "rawvideo",
                    "pipe:1",
                ])
                .output()
                .unwrap();
            assert!(authored.status.success());
            let frame_bytes = 37 * 23 * 4;
            assert_eq!(authored.stdout.len(), 4 * frame_bytes);
            for frame in 0..4 {
                let start = frame * frame_bytes;
                let end = start + frame_bytes;
                if frame == 1 {
                    assert!(authored.stdout[start..end]
                        .chunks_exact(4)
                        .all(|pixel| pixel[3] == 0));
                } else {
                    assert_eq!(authored.stdout[start..end], replayed.stdout[start..end]);
                }
            }
            let authored_planes = std::fs::read(authored_masks).unwrap();
            let plane = 512 * 320;
            for frame in 0..4 {
                let start = frame * plane;
                let end = start + plane;
                if frame == 1 {
                    assert!(authored_planes[start..end].iter().all(|v| *v == 0));
                } else {
                    assert_eq!(authored_planes[start..end], decoded.stdout[start..end]);
                }
            }
        }
    }
    // Actual project-origin queue with deliberately absent model and ORT.
    let imported_id = restored["project"]["id"].as_str().unwrap();
    let endpoint =
        format!("/api/composition-projects/{imported_id}/video-cutout/clip?compositionId=root");
    let settings = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&endpoint)
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(settings.status(), StatusCode::OK);
    let settings: Value =
        serde_json::from_slice(&to_bytes(settings.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(
        settings["snapshot"],
        serde_json::to_value(snapshot).unwrap()
    );
    let outsider = state
        .db
        .register_auth_user("video-mask-outsider", "public archive outsider password")
        .await
        .unwrap()
        .unwrap();
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&endpoint)
                .header("authorization", format!("Bearer {}", outsider.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let endpoint = format!("/api/composition-projects/{imported_id}/video-cutout/corrections");
    let points: Vec<[f64; 2]> = (0..9)
        .flat_map(|y| (0..9).map(move |x| [x as f64 / 8., y as f64 / 8.]))
        .collect();
    let edits = json!([{"frame":1,"correction":{"custom":true,"strokes":[{"tool":"eraser","size":0.5,"points":points}],"feather":0}}]);
    let edits = serde_json::to_value(
        serde_json::from_value::<Vec<video_kadr_backend::domain::video_cutout::FrameCorrection>>(
            edits,
        )
        .unwrap(),
    )
    .unwrap();
    let submit = |token: &str, attachment: Value| {
        Request::builder().method("POST").uri(&endpoint).header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(Body::from(json!({"compositionId":"root","clipId":"clip","expectedAttachment":attachment,"corrections":edits}).to_string())).unwrap()
    };
    let mut stale = serde_json::to_value(snapshot).unwrap();
    stale["sourceHash"] = json!("0".repeat(64));
    assert_eq!(
        app.clone()
            .oneshot(submit(&owner.token, stale))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        app.clone()
            .oneshot(submit(
                &outsider.token,
                serde_json::to_value(snapshot).unwrap()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let admitted = app
        .clone()
        .oneshot(submit(
            &owner.token,
            serde_json::to_value(snapshot).unwrap(),
        ))
        .await
        .unwrap();
    let status = admitted.status();
    let body = to_bytes(admitted.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let job: Value = serde_json::from_slice(&body).unwrap();
    let job_id = job["jobId"].as_str().unwrap();
    let done = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let job = state.get_job(job_id).await.unwrap().unwrap();
            if matches!(
                job.status,
                video_kadr_backend::model::JobStatus::Done
                    | video_kadr_backend::model::JobStatus::Error
                    | video_kadr_backend::model::JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        done.status,
        video_kadr_backend::model::JobStatus::Done,
        "{:?}",
        done
    );
    let recipe = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/person/video-jobs/{job_id}/corrections"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = recipe.status();
    let bytes = to_bytes(recipe.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let recipe: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(recipe["projectOrigin"]["projectId"], imported_id);
    assert_eq!(recipe["corrections"], edits);
    for frame in [0usize, 1, 3] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/person/video-jobs/{job_id}/source/{frame}"))
                    .header("authorization", format!("Bearer {}", owner.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let png = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let image = image::load_from_memory(&png).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (37, 23));
        let original = std::fs::read(oracle.join("input.rgba")).unwrap();
        let size = 37 * 23 * 4;
        assert_eq!(image.into_raw(), original[frame * size..(frame + 1) * size]);
    }
    for (frame, token, expected) in [
        (0, &outsider.token, StatusCode::FORBIDDEN),
        (4, &owner.token, StatusCode::BAD_REQUEST),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/person/video-jobs/{job_id}/source/{frame}"))
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    let original_entry = state.library.get(&snapshot.source_id).await.unwrap();
    let original_path = state
        .library
        .resolve_media_path(&original_entry)
        .await
        .unwrap();
    let original_bytes = std::fs::read(&original_path).unwrap();
    std::fs::write(&original_path, b"changed private original movie").unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/person/video-jobs/{job_id}/source/0"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    std::fs::write(&original_path, original_bytes).unwrap();
    let automatic = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/person/video-jobs/{job_id}/masks/1"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(automatic.status(), StatusCode::OK);
    let png = to_bytes(automatic.into_body(), 1024 * 1024).await.unwrap();
    let pixels = image::load_from_memory(&png)
        .unwrap()
        .into_luma8()
        .into_raw();
    let oracle_masks = std::fs::read(oracle.join("reference.masks")).unwrap();
    assert_eq!(pixels, oracle_masks[512 * 320..2 * 512 * 320]);
    let authored = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/person/video-jobs/{job_id}/masks"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authored.status(), StatusCode::OK);
    let bytes = to_bytes(authored.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    assert!(bytes[512 * 320..2 * 512 * 320].iter().all(|v| *v == 0));
    let accept = || {
        Request::builder()
            .method("POST")
            .uri(format!("/api/person/video-jobs/{job_id}/asset"))
            .header("authorization", format!("Bearer {}", owner.token))
            .header("content-type", "application/json")
            .body(Body::from(json!({"projectId":imported_id}).to_string()))
            .unwrap()
    };
    let accepted = app.clone().oneshot(accept()).await.unwrap();
    let status = accepted.status();
    let bytes = to_bytes(accepted.into_body(), 1024 * 1024).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let accepted: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(accepted["videoCutout"]["snapshot"]["corrections"], edits);
    assert_eq!(
        accepted["videoCutout"]["snapshot"]["maskHash"],
        snapshot.mask_hash
    );
    let repeated = app.clone().oneshot(accept()).await.unwrap();
    assert_eq!(repeated.status(), StatusCode::OK);
    let repeated: Value =
        serde_json::from_slice(&to_bytes(repeated.into_body(), 1024 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(repeated["id"], accepted["id"]);
    assert_eq!(repeated["videoCutout"], accepted["videoCutout"]);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
        )
        .await
        .unwrap();
    let original_document = restored["project"]["document"].clone();
    let source_ids: Vec<String> = restored_content
        .sources
        .keys()
        .map(|id| id.as_str().to_owned())
        .collect();
    let mask_entry = state.library.get(&snapshot.mask_source_id).await.unwrap();
    let mask_path = state.library.resolve_media_path(&mask_entry).await.unwrap();
    let original_mask_file = std::fs::read(&mask_path).unwrap();
    for mode in ["snapshot", "source", "revoked"] {
        let mut held = vec![];
        for _ in 0..state
            .resource_limit(video_kadr_backend::config::resource_classes::ResourceClass::Analysis)
        {
            held.push(
                state
                    .acquire_job_slot(
                        video_kadr_backend::config::resource_classes::ResourceClass::Analysis,
                    )
                    .await
                    .unwrap(),
            );
        }
        let admitted = app
            .clone()
            .oneshot(submit(
                &owner.token,
                serde_json::to_value(snapshot).unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(admitted.status(), StatusCode::ACCEPTED);
        let body: Value =
            serde_json::from_slice(&to_bytes(admitted.into_body(), 1024 * 1024).await.unwrap())
                .unwrap();
        let failed = body["jobId"].as_str().unwrap();
        match mode {
            "snapshot" => {
                let mut changed = original_document.clone();
                changed["compositions"]["root"]["content"]["metadata"]["mediaVersions"]["clip"]
                    ["videoCutout"]["corrections"] = edits.clone();
                state
                    .db
                    .update_composition_project(
                        imported_id,
                        "Changed recipe",
                        &changed,
                        &source_ids,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            "source" => std::fs::write(&mask_path, b"changed imported video mask").unwrap(),
            "revoked" => {
                sqlx::query(
                    "UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor=?",
                )
                .bind(imported_id)
                .bind(&owner.user.username)
                .execute(&pool)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        drop(held);
        let failed_job = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let job = state.get_job(failed).await.unwrap().unwrap();
                if matches!(
                    job.status,
                    video_kadr_backend::model::JobStatus::Error
                        | video_kadr_backend::model::JobStatus::Done
                        | video_kadr_backend::model::JobStatus::Cancelled
                ) {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            failed_job.status,
            video_kadr_backend::model::JobStatus::Error,
            "{mode}"
        );
        assert_eq!(
            failed_job.error_kind.as_deref(),
            Some(if mode == "revoked" {
                "security"
            } else {
                "validation"
            }),
            "{mode}: {:?}",
            failed_job
        );
        for extension in ["mov", "masks", "automatic.mkv", "original.masks"] {
            assert!(
                !state
                    .storage
                    .join("person-video-jobs")
                    .join(format!("{failed}.{extension}"))
                    .exists(),
                "{mode}/{extension}"
            );
        }
        match mode {
            "snapshot" => {
                state
                    .db
                    .update_composition_project(
                        imported_id,
                        "Video masks",
                        &original_document,
                        &source_ids,
                    )
                    .await
                    .unwrap()
                    .unwrap();
            }
            "source" => std::fs::write(&mask_path, &original_mask_file).unwrap(),
            "revoked" => {
                sqlx::query("UPDATE composition_project_members SET role='owner' WHERE project_id=? AND actor=?").bind(imported_id).bind(&owner.user.username).execute(&pool).await.unwrap();
            }
            _ => unreachable!(),
        }
    }
    let reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/person/video-jobs/{job_id}/corrections"))
                .header("authorization", format!("Bearer {}", owner.token))
                .header("content-type", "application/json")
                .body(Body::from(json!({"corrections":[]}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::ACCEPTED);
    let reset: Value =
        serde_json::from_slice(&to_bytes(reset.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    let reset_id = reset["jobId"].as_str().unwrap();
    let reset = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let job = state.get_job(reset_id).await.unwrap().unwrap();
            if matches!(
                job.status,
                video_kadr_backend::model::JobStatus::Done
                    | video_kadr_backend::model::JobStatus::Error
                    | video_kadr_backend::model::JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        reset.status,
        video_kadr_backend::model::JobStatus::Done,
        "{:?}",
        reset
    );
    assert_eq!(reset.result.unwrap()["maskHash"], snapshot.mask_hash);
    pool.close().await;
    let mut held = vec![];
    for _ in 0..state
        .resource_limit(video_kadr_backend::config::resource_classes::ResourceClass::Analysis)
    {
        held.push(
            state
                .acquire_job_slot(
                    video_kadr_backend::config::resource_classes::ResourceClass::Analysis,
                )
                .await
                .unwrap(),
        );
    }
    let queued = app
        .clone()
        .oneshot(submit(
            &owner.token,
            serde_json::to_value(snapshot).unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(queued.status(), StatusCode::ACCEPTED);
    let queued: Value =
        serde_json::from_slice(&to_bytes(queued.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    let queued_id = queued["jobId"].as_str().unwrap();
    state.begin_shutdown();
    drop(held);
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
    let recovered = video_kadr_backend::state::AppState::new(
        state.storage.clone(),
        2,
        (*state.tools).clone(),
        video_kadr_backend::library::Library::load(state.storage.clone()).await,
        video_kadr_backend::db::Db::open(&state.storage)
            .await
            .unwrap(),
    )
    .with_person_config((*state.person).clone());
    recovered.recover_jobs().await;
    video_kadr_backend::handlers::resume_pending_jobs(&recovered).await;
    let done = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let job = recovered.get_job(queued_id).await.unwrap().unwrap();
            if matches!(
                job.status,
                video_kadr_backend::model::JobStatus::Done
                    | video_kadr_backend::model::JobStatus::Error
                    | video_kadr_backend::model::JobStatus::Cancelled
            ) {
                break job;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        done.status,
        video_kadr_backend::model::JobStatus::Done,
        "{:?}",
        done
    );
    let cold_app = support::router(recovered.clone());
    let recipe = cold_app
        .oneshot(
            Request::builder()
                .uri(format!("/api/person/video-jobs/{queued_id}/corrections"))
                .header("authorization", format!("Bearer {}", owner.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(recipe.status(), StatusCode::OK);
    let recipe: Value =
        serde_json::from_slice(&to_bytes(recipe.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(recipe["corrections"], edits);
    assert_eq!(recipe["projectOrigin"]["projectId"], imported_id);
    recovered.begin_shutdown();
    assert!(
        recovered
            .wait_for_tasks(std::time::Duration::from_secs(10))
            .await
    );
}
