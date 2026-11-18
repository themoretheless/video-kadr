//! Real HTTP analysis and saved mask delivery across logical/physical media IDs.
mod support;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use video_kadr_backend::{
    domain::{
        composition::SourceId,
        concat_project_import::{import, ImportRequest},
    },
    handlers::{
        concat_video_bound::{BoundMasks, SavedProject},
        concat_video_cache as cache,
    },
    library::MediaEntry,
};
fn request(url: &str, token: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(url)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}
#[tokio::test]
#[ignore = "requires pinned Selfie model, native vision worker, ORT and FFmpeg"]
async fn logical_video_http_masks_bind_preview_and_restore_without_engines() {
    let (state, _dir) = support::make_state(true, false).await;
    let config = video_kadr_backend::services::local_selfie::Config {
        executable: std::env::var("VIDEO_KADR_TEST_ENHANCE_BIN").unwrap().into(),
        model: std::env::var("VIDEO_KADR_TEST_SELFIE_MODEL")
            .unwrap()
            .into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let state = state
        .with_person_config(None)
        .with_selfie_config(Some(config));
    let owner = state
        .db
        .register_auth_user("owner", "synthetic video password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("outsider", "synthetic video password")
        .await
        .unwrap()
        .unwrap();
    for (id, color) in [("physical", "red"), ("slot", "blue")] {
        let path = state.sources_dir().join(format!("{id}.mkv"));
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:s=16x16:r=10:d=0.2"),
                "-c:v",
                "ffv1",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":format!("{id}.mkv"),"url":format!("/files/sources/{id}.mkv"),"mediaType":"video","width":16,"height":16}))).await);
    }
    let hash = format!(
        "{:x}",
        Sha256::digest(std::fs::read(state.sources_dir().join("physical.mkv")).unwrap())
    );
    let descriptor = json!({"id":"physical","kind":"video","durationTicks":200000,"width":16,"height":16,"hasAudio":false});
    let input:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":true,"document":{"version":1,"media":[{"id":"slot","name":"Selected","kind":"video","placeholder":true},{"id":"alias","name":"Other slot","kind":"video","placeholder":true}],"timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":10,"rateDen":1},"tracks":[{"id":"videos"}],"clips":[{"id":"clip","trackId":"videos","mediaId":"slot","kind":"video","duration":0.2}]},{"id":"other","video":{"width":16,"height":16,"rateNum":10,"rateDen":1},"tracks":[{"id":"videos"}],"clips":[{"id":"other","trackId":"videos","mediaId":"alias","kind":"video","duration":0.2}]}]},"mediaBindings":{"slot":descriptor,"alias":descriptor}})).unwrap();
    let report = import(input).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let mut native = report.project.unwrap();
    let alias = SourceId::parse("alias").unwrap();
    let alias_descriptor = native.project_media[&alias].source.clone();
    let node = native.compositions.get_mut(&native.root).unwrap();
    node.content.sources.insert(alias, alias_descriptor);
    node.content.tracks.push(serde_json::from_value(json!({"id":"native-eof","name":"Native EOF","kind":"video","clips":[{"id":"eof","sourceId":"alias","placement":{"timelineStartTick":2000000,"sourceInTick":0,"sourceOutTick":400000,"speed":1,"concatSourceEof":true},"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false}]})).unwrap());
    node.content.validate_native_surface().unwrap();
    assert!(node.content.validate().is_err());
    native.dependency_order().unwrap();
    let root = native.root.clone();
    let document = serde_json::to_value(&native).unwrap();
    let space = state
        .db
        .create_space("owner", "Logical video")
        .await
        .unwrap();
    let saved = state
        .db
        .create_space_composition_project(
            "Logical video",
            &document,
            &["physical".into()],
            "owner",
            &space.id,
        )
        .await
        .unwrap();
    let endpoint = |action: &str| {
        format!(
            "/api/composition-projects/{}/concat-video-cutout/{action}",
            saved.id
        )
    };
    let base = json!({"compositionId":root,"clipId":"clip","expectedRevision":saved.revision});
    let app = support::router(state.clone());
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut analyse = base.clone();
    analyse["subject"] = json!("person");
    assert_eq!(
        app.clone()
            .oneshot(request(
                &endpoint("analyse"),
                &outsider.token,
                analyse.clone()
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    for reused in [false, true] {
        let response = app
            .clone()
            .oneshot(request(&endpoint("analyse"), &owner.token, analyse.clone()))
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["sourceId"], "slot");
        assert_eq!(result["cacheReused"], reused);
    }
    let chosen = cache::choice(&state, "owner", &saved.id, "slot", saved.revision, &cancel)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(chosen.video_hash, hash);
    assert!(
        cache::choice(&state, "owner", &saved.id, "alias", saved.revision, &cancel)
            .await
            .unwrap()
            .is_none()
    );
    // A genuine replacement file is shorter than its slot. Analyse the file
    // and compare the last real source sample with a cursor in the held tail.
    let replacement = serde_json::from_value(json!({
        "id":"slot","kind":"video","durationTicks":200000,"width":16,"height":16,"hasAudio":false
    })).unwrap();
    let filled = video_kadr_backend::domain::project_media_fill::fill(
        &native, &SourceId::parse("alias").unwrap(),
        &json!({"name":"Blue replacement","kind":"video","path":"library:slot",
            "duration":0.2,"width":16,"height":16,"hasAudio":false}),
        &replacement,
    ).unwrap();
    let saved_replacement = state.db.create_space_composition_project(
        "Short replacement", &serde_json::to_value(filled).unwrap(),
        &["physical".into(), "slot".into()], "owner", &space.id,
    ).await.unwrap();
    let replacement_endpoint = |action| format!(
        "/api/composition-projects/{}/concat-video-cutout/{action}", saved_replacement.id,
    );
    let target = json!({"compositionId":root,"clipId":"eof","expectedRevision":saved_replacement.revision});
    let response = app.clone().oneshot(request(&replacement_endpoint("analyse"), &owner.token, target.clone())).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let analysed: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(analysed["analysedFrames"].as_u64().unwrap() > 0);
    let selected = cache::choice(&state, "owner", &saved_replacement.id, "alias", saved_replacement.revision, &cancel)
        .await.unwrap().unwrap();
    assert_ne!(selected.video_hash, hash);

    for original in [true, false] {
        let mut images = Vec::new();
        for time in [2_199_999, 2_399_999] {
            let mut input = target.clone();
            input["timeUs"] = json!(time);
            input["sourceSpace"] = json!(true);
            input["sourceOriginal"] = json!(original);
            let response = app.clone().oneshot(request(&replacement_endpoint("preview"), &owner.token, input)).await.unwrap();
            let status = response.status();
            let image = to_bytes(response.into_body(), 1 << 20).await.unwrap();
            assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&image));
            if original {
                assert!(image.chunks_exact(4).all(|pixel| pixel[2] > 200 && pixel[0] < 20));
            }
            images.push(image);
        }
        assert_eq!(images[0], images[1]);
    }
    Box::pin(qualify_masked_source_replacement(
        &state, &app, &saved_replacement, &root, &owner.token, &outsider.token, &hash,
    )).await;
    let mut expected_rgba = vec![];
    for (source_space, source_original) in [(true, true), (true, false), (false, false)] {
        let mut body = base.clone();
        body["timeUs"] = json!(100000);
        body["sourceSpace"] = json!(source_space);
        body["sourceOriginal"] = json!(source_original);
        let response = app
            .clone()
            .oneshot(request(&endpoint("preview"), &owner.token, body))
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
        assert_eq!(bytes.len(), 16 * 16 * 4);
        if source_original {
            assert!(bytes.chunks_exact(4).all(|p| p[0] > 200 && p[2] < 20));
            let mut expected = video_kadr_backend::domain::group_surface::GroupSurface {
                width: 16,
                height: 16,
                rgba: bytes.to_vec(),
            };
            let mut verified = cache::restore_for_model(
                &state,
                "owner",
                &saved.id,
                "slot",
                saved.revision,
                &chosen.runtime_hash,
                &chosen.model_hash,
                &cancel,
            )
            .await
            .unwrap()
            .unwrap();
            verified
                .apply_decoded(&mut expected, 100000, &cancel)
                .unwrap();
            expected_rgba = expected.rgba;
        } else {
            assert_eq!(bytes.as_ref(), expected_rgba);
        }
    }
    let mut body = base.clone();
    body["enabled"] = json!(true);
    body["subject"] = json!("person");
    let response = app
        .clone()
        .oneshot(request(&endpoint("binding"), &owner.token, body))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let bound = state
        .db
        .get_composition_project_for(&saved.id, "owner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(bound.source_ids, vec!["physical"]);
    assert_eq!(bound.document["projectMedia"], document["projectMedia"]);
    assert_eq!(
        bound.document["compositions"][&root]["content"]["tracks"][1],
        document["compositions"][&root]["content"]["tracks"][1]
    );
    for (id, node) in document["compositions"].as_object().unwrap() {
        if id != &root {
            assert_eq!(&bound.document["compositions"][id], node);
        }
    }
    assert_eq!(
        bound.document["compositions"][&root]["content"]["tracks"][0]["clips"][0]["concatCutout"]
            ["sourceHash"],
        hash
    );
    let native = serde_json::from_value(bound.document.clone()).unwrap();
    let context = SavedProject {
        project_id: saved.id.clone(),
        expected_revision: bound.revision,
    };
    let no_engines = state
        .clone()
        .with_person_config(None)
        .with_selfie_config(None)
        .with_cutout_config(None);
    let masks = BoundMasks::load(&no_engines, "owner", Some(&context), &native, &cancel)
        .await
        .unwrap();
    assert_eq!(
        masks
            .identities()
            .unwrap()
            .keys()
            .map(SourceId::as_str)
            .collect::<Vec<_>>(),
        vec!["slot"]
    );
    let mut forged = bound.document.clone();
    forged["projectMedia"]["slot"]["librarySourceId"] = json!("slot");
    let forged = serde_json::from_value(forged).unwrap();
    assert!(
        BoundMasks::load(&no_engines, "owner", Some(&context), &forged, &cancel)
            .await
            .is_err()
    );
    let mut delivery_state = no_engines;
    let mut capabilities = delivery_state.tools.as_ref().clone();
    capabilities
        .ffmpeg_encoders
        .extend(["ffv1".into(), "pcm_f32le".into()]);
    capabilities.ffmpeg_muxers.push("matroska".into());
    delivery_state.tools = std::sync::Arc::new(capabilities);
    let delivery = support::router(delivery_state.clone());
    let preview = json!({"project":native,"compositionId":root,"timeUs":100000,"renderedOutputs":{},"savedProject":context});
    let response = delivery
        .clone()
        .oneshot(request(
            "/api/compositions/nested-preview",
            &owner.token,
            preview,
        ))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(bytes.len(), 16 * 16 * 4);
    assert_eq!(bytes.as_ref(), expected_rgba);
    let mut export_draft=serde_json::to_value(&native).unwrap();
    export_draft["compositions"][&root]["content"]["tracks"][0]["clips"][0]["concatCutout"]["origin"]=json!({"projectId":context.project_id,"expectedRevision":context.expected_revision,"sourceId":"slot"});
    let body = json!({"schemaVersion":1,"mode":"video_audio","project":export_draft,"compositionId":root,"startUs":0,"endUs":100000,"renderedOutputs":{}});
    let response = delivery
        .clone()
        .oneshot(request(
            "/api/compositions/nested-export",
            &owner.token,
            body,
        ))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let accepted: Value = serde_json::from_slice(&bytes).unwrap();
    let id = accepted["jobId"].as_str().unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let job = loop {
        let job = delivery_state.get_job(id).await.unwrap().unwrap();
        if job.status.is_terminal() {
            break job;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "export timeout: {job:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    assert_eq!(
        job.status,
        video_kadr_backend::model::JobStatus::Done,
        "{job:?}"
    );
    let result = job.result.unwrap();
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(
            delivery_state
                .outputs_dir()
                .join(result["filename"].as_str().unwrap()),
        )
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, expected_rgba);
    let mut transferred_document = bound.document.clone();
    transferred_document["compositions"][&root]["content"]["tracks"][0]["clips"][0]["sourceId"] =
        json!("alias");
    let transferred = state
        .db
        .create_space_composition_project(
            "Transferred video masks",
            &transferred_document,
            &["physical".into()],
            "owner",
            &space.id,
        )
        .await
        .unwrap();
    let transferred_native = serde_json::from_value(transferred_document).unwrap();
    let transferred_context = SavedProject {
        project_id: transferred.id.clone(),
        expected_revision: transferred.revision,
    };
    assert!(BoundMasks::load(
        &delivery_state,
        "owner",
        Some(&transferred_context),
        &transferred_native,
        &cancel
    )
    .await
    .is_err());
    let from = cache::ProjectSource {
        project: &saved.id,
        source_id: "slot",
        revision: bound.revision,
    };
    let to = cache::ProjectSource {
        project: &transferred.id,
        source_id: "alias",
        revision: transferred.revision,
    };
    let cache_id = cache::transfer_for_model(
        &delivery_state,
        "owner",
        from,
        to,
        &chosen.runtime_hash,
        &chosen.model_hash,
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(
        cache::transfer_for_model(
            &delivery_state,
            "owner",
            from,
            to,
            &chosen.runtime_hash,
            &chosen.model_hash,
            &cancel
        )
        .await
        .unwrap(),
        cache_id
    );
    assert_eq!(
        BoundMasks::load(
            &delivery_state,
            "owner",
            Some(&transferred_context),
            &transferred_native,
            &cancel
        )
        .await
        .unwrap()
        .identities()
        .unwrap()
        .keys()
        .map(SourceId::as_str)
        .collect::<Vec<_>>(),
        vec!["alias"]
    );
    let response=delivery.clone().oneshot(request("/api/compositions/nested-preview",&owner.token,json!({"project":transferred_native,"compositionId":root,"timeUs":100000,"renderedOutputs":{},"savedProject":transferred_context}))).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(bytes.as_ref(), expected_rgba);
    // Trusted Concat conversion plus explicit target cache ownership, with no
    // model present during adoption/rendering. Unsaved provenance is separate.
    use video_kadr_backend::domain::concat_project_import::{
        import_with_ready_bindings, ReadyVideoBinding, StillBindings, VideoBindings,
    };
    let mut physical_source: video_kadr_backend::domain::composition::CompositionSource =
        serde_json::from_value(
            bound.document["compositions"][&root]["content"]["sources"]["slot"].clone(),
        )
        .unwrap();
    physical_source.id = SourceId::parse("physical").unwrap();
    let ready_binding: video_kadr_backend::domain::concat_video_binding::Binding =
        serde_json::from_value(
            bound.document["compositions"][&root]["content"]["tracks"][0]["clips"][0]
                ["concatCutout"]
                .clone(),
        )
        .unwrap();
    let settings = json!({"mode":"auto","subject":"person","feather":0});
    let prepared_input:ImportRequest=serde_json::from_value(json!({"schemaVersion":1,"preserveMediaIds":true,"mediaBindings":{"prepared-slot":physical_source},"document":{"version":1,"media":[{"id":"prepared-slot","kind":"video"}],"timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":10,"rateDen":1},"tracks":[{"id":"videos"}],"clips":[{"id":"prepared-clip","kind":"video","trackId":"videos","mediaId":"prepared-slot","duration":0.2,"cutout":settings}]}]}})).unwrap();
    let ready: VideoBindings = std::collections::BTreeMap::from([(
        "main".into(),
        std::collections::BTreeMap::from([(
            "prepared-clip".into(),
            ReadyVideoBinding {
                source: physical_source,
                source_settings: video_kadr_backend::domain::concat_cutout_import::plan(&settings)
                    .unwrap(),
                binding: ready_binding,
                regions: vec![],
            },
        )]),
    )]);
    let public_input=json!({"request":{"schemaVersion":1,"document":prepared_input.document,"mediaBindings":prepared_input.media_bindings,"preserveMediaIds":true},"preparedVideos":[{"timelineId":"main","clipId":"prepared-clip","projectId":context.project_id,"compositionId":root,"sourceClipId":"clip","expectedRevision":context.expected_revision}]});
    let report = import_with_ready_bindings(prepared_input, &StillBindings::new(), &ready).unwrap();
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    let prepared_native = report.project.unwrap();
    prepared_native.dependency_order().unwrap();
    let before_count=delivery_state.library.list().await.len();
    let response=delivery.clone().oneshot(request("/api/compositions/concat-project/import-ready",&owner.token,public_input.clone())).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),1<<20).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
    let public:Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(public["status"],"converted");
    if let Some(path)=std::env::var_os("CONCAT_VIDEO_IMPORT_UI_CAPTURE_OUTPUT"){
        let list=delivery.clone().oneshot(Request::builder().uri("/api/composition-projects").header("authorization",format!("Bearer {}",owner.token)).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(list.status(),StatusCode::OK);
        let projects:Value=serde_json::from_slice(&to_bytes(list.into_body(),4*1024*1024).await.unwrap()).unwrap();
        std::fs::write(path,serde_json::to_vec(&json!({"projects":projects,"request":public_input["request"],"references":public_input["preparedVideos"],"report":public})).unwrap()).unwrap();
    }

    let mut public_project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(public["project"].clone()).unwrap();
    let response=delivery.clone().oneshot(request("/api/compositions/nested-preview",&owner.token,json!({"project":public_project,"timeUs":100000,"renderedOutputs":{}}))).await.unwrap();assert_eq!(response.status(),StatusCode::OK);assert_eq!(to_bytes(response.into_body(),1<<20).await.unwrap().as_ref(),expected_rgba.as_slice());
    if let video_kadr_backend::domain::composition::CompositionTrack::Video{clips,..}=&mut public_project.compositions.get_mut("concat-timeline-0").unwrap().content.tracks[0]{assert_eq!(clips[0].concat_cutout.as_ref().unwrap().origin.as_ref().unwrap().source_id,"slot");clips[0].concat_cutout.as_mut().unwrap().origin=None;}
    assert_eq!(serde_json::to_value(public_project).unwrap(),serde_json::to_value(&prepared_native).unwrap());
    for mode in 0..3 {let mut wrong=public_input.clone();match mode{0=>wrong["preparedVideos"][0]["expectedRevision"]=json!(context.expected_revision+1),1=>wrong["preparedVideos"][0]["sourceClipId"]=json!("missing"),_=>{let reference=wrong["preparedVideos"][0].clone();wrong["preparedVideos"].as_array_mut().unwrap().push(reference);}}assert_eq!(delivery.clone().oneshot(request("/api/compositions/concat-project/import-ready",&owner.token,wrong)).await.unwrap().status(),StatusCode::CONFLICT);}
    assert_eq!(delivery_state.library.list().await.len(),before_count);

    // An unsaved imported draft can reference the authorized source store,
    // without claiming ownership or requiring a configured inference engine.
    let mut draft=prepared_native.clone();
    if let video_kadr_backend::domain::composition::CompositionTrack::Video{clips,..}=&mut draft.compositions.get_mut("concat-timeline-0").unwrap().content.tracks[0]{
        clips[0].concat_cutout.as_mut().unwrap().origin=Some(video_kadr_backend::domain::concat_video_binding::CacheOrigin{project_id:context.project_id.clone(),expected_revision:context.expected_revision,source_id:"slot".into(),region_sources:Default::default()});
    }else{panic!()}
    let response=delivery.clone().oneshot(request("/api/compositions/nested-preview",&owner.token,json!({"project":draft,"timeUs":100000,"renderedOutputs":{}}))).await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);
    let draft_pixels=to_bytes(response.into_body(),usize::MAX).await.unwrap();
    assert_eq!(draft_pixels.as_ref(),expected_rgba.as_slice());
    assert!(BoundMasks::load(&delivery_state,"stranger",None,&draft,&cancel).await.is_err());
    let mut forged_origin=serde_json::to_value(&draft).unwrap();
    forged_origin["projectMedia"]["prepared-slot"]["librarySourceId"]=json!("another-file");
    let forged_origin=serde_json::from_value(forged_origin).unwrap();
    assert!(BoundMasks::load(&delivery_state,"owner",None,&forged_origin,&cancel).await.is_err());
    let mut stale=draft.clone();
    if let video_kadr_backend::domain::composition::CompositionTrack::Video{clips,..}=&mut stale.compositions.get_mut("concat-timeline-0").unwrap().content.tracks[0]{clips[0].concat_cutout.as_mut().unwrap().origin.as_mut().unwrap().expected_revision+=1;}
    assert!(BoundMasks::load(&delivery_state,"owner",None,&stale,&cancel).await.is_err());
    Box::pin(qualify_atomic_save(&delivery_state,&owner.token,&space.id,&draft,&expected_rgba)).await;
    let prepared = state
        .db
        .create_space_composition_project(
            "Prepared Concat video",
            &serde_json::to_value(&prepared_native).unwrap(),
            &["physical".into()],
            "owner",
            &space.id,
        )
        .await
        .unwrap();
    let prepared_context = SavedProject {
        project_id: prepared.id.clone(),
        expected_revision: prepared.revision,
    };
    assert!(BoundMasks::load(&delivery_state,"owner",Some(&prepared_context),&draft,&cancel).await.is_ok(),"an unadopted saved slot must retain its authorized external provider");
    assert!(BoundMasks::load(
        &delivery_state,
        "owner",
        Some(&prepared_context),
        &prepared_native,
        &cancel
    )
    .await
    .is_err());
    cache::transfer_for_model(
        &delivery_state,
        "owner",
        cache::ProjectSource {
            project: &saved.id,
            source_id: "slot",
            revision: bound.revision,
        },
        cache::ProjectSource {
            project: &prepared.id,
            source_id: "prepared-slot",
            revision: prepared.revision,
        },
        &chosen.runtime_hash,
        &chosen.model_hash,
        &cancel,
    )
    .await
    .unwrap();
    let response=delivery.clone().oneshot(request("/api/compositions/nested-preview",&owner.token,json!({"project":prepared_native,"compositionId":prepared_native.root,"timeUs":100000,"renderedOutputs":{},"savedProject":prepared_context}))).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(bytes.as_ref(), expected_rgba);
    let decoy = state
        .library
        .resolve_media_path(&state.library.get("slot").await.unwrap())
        .await
        .unwrap();
    std::fs::write(decoy, b"changed logical-ID decoy").unwrap();
    assert!(
        cache::choice(&state, "owner", &saved.id, "slot", bound.revision, &cancel)
            .await
            .unwrap()
            .is_some()
    );
    let mut current = base.clone();
    current["expectedRevision"] = json!(bound.revision);
    current["timeUs"] = json!(100000);
    let response = app
        .clone()
        .oneshot(request(&endpoint("preview"), &owner.token, current.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .as_ref(),
        expected_rgba
    );
    let path = state
        .library
        .resolve_media_path(&state.library.get("physical").await.unwrap())
        .await
        .unwrap();
    let mut changed = std::fs::read(&path).unwrap();
    changed.push(0);
    std::fs::write(&path, changed).unwrap();
    assert!(
        cache::choice(&state, "owner", &saved.id, "slot", bound.revision, &cancel)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        app.clone()
            .oneshot(request(&endpoint("preview"), &owner.token, current))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let mut stale = base;
    stale["timeUs"] = json!(0);
    assert_eq!(
        app.oneshot(request(&endpoint("preview"), &owner.token, stale))
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
}

async fn qualify_masked_source_replacement(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    project: &video_kadr_backend::db::CompositionProject,
    root: &str,
    token: &str,
    outsider: &str,
    replacement_hash: &str,
) {
    let endpoint = |action| format!("/api/composition-projects/{}/concat-video-cutout/{action}", project.id);
    let response = app.clone().oneshot(request(&endpoint("binding"), token, json!({
        "compositionId":root,"clipId":"eof","expectedRevision":project.revision,
        "enabled":true,"subject":"person","feather":0.04,
        "correction":{"custom":true,"feather":0.04,"strokes":[
            {"tool":"brush","size":0.1,"points":[[0.25,0.5]],"at":0.1},
            {"tool":"eraser","size":0.06,"points":[[0.75,0.5]],"at":null}
        ]},
    }))).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let saved: Value = serde_json::from_slice(&body).unwrap();
    let revision = saved["revision"].as_u64().unwrap();
    Box::pin(qualify_masked_template_slot(
        state, app, project, root, token, &saved["document"], replacement_hash,
    )).await;
    let replacement = json!({"mediaId":"alias","sourceId":"physical","expectedRevision":revision});
    assert_eq!(app.clone().oneshot(request(&endpoint("replace-source"), outsider, replacement.clone())).await.unwrap().status(), StatusCode::FORBIDDEN);

    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
    ).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_replacement BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'forced replacement rollback'); END")
        .execute(&pool).await.unwrap();
    let root_cache = state.storage.join("concat-video-caches");
    let before = std::fs::read_dir(&root_cache).unwrap().count();
    let response = app.clone().oneshot(request(&endpoint("replace-source"), token, replacement.clone())).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{}", String::from_utf8_lossy(&body));
    assert!(String::from_utf8_lossy(&body).contains("forced replacement rollback"));
    let current = state.db.get_composition_project_for(&project.id, "owner").await.unwrap().unwrap();
    assert_eq!(current.revision, revision);
    assert_eq!(current.document, saved["document"]);
    assert_eq!(std::fs::read_dir(&root_cache).unwrap().count(), before);
    sqlx::query("DROP TRIGGER reject_replacement").execute(&pool).await.unwrap();

    let response = app.clone().oneshot(request(&endpoint("replace-source"), token, replacement)).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let result: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(result["document"]["projectMedia"]["alias"]["librarySourceId"], "physical");
    let binding = &result["document"]["compositions"][root]["content"]["tracks"][1]["clips"][0]["concatCutout"];
    assert_eq!(binding["feather"], 0.04);
    assert_eq!(
        binding["correction"],
        saved["document"]["compositions"][root]["content"]["tracks"][1]["clips"][0]["concatCutout"]["correction"],
    );
    assert_eq!(binding["sourceHash"], replacement_hash);
    let cancel = tokio_util::sync::CancellationToken::new();
    let new_cache = cache::choice(state, "owner", &project.id, "alias", revision + 1, &cancel)
        .await.unwrap().unwrap();
    assert_eq!(new_cache.video_hash, replacement_hash);
    assert_ne!(new_cache.video_hash, saved["document"]["compositions"][root]["content"]["tracks"][1]["clips"][0]["concatCutout"]["sourceHash"].as_str().unwrap());
    let context = SavedProject { project_id: project.id.clone(), expected_revision: revision + 1 };
    let mut identities = Vec::new();
    for snapshot in [&saved["document"], &result["document"]] {
        let draft = serde_json::from_value(snapshot.clone()).unwrap();
        let provider = BoundMasks::load(state, "owner", Some(&context), &draft, &cancel).await.unwrap();
        identities.push(provider.identities().unwrap());
    }
    assert_ne!(identities[0], identities[1]);
    let mut without_strokes = result["document"].clone();
    without_strokes["compositions"][root]["content"]["tracks"][1]["clips"][0]["concatCutout"]
        .as_object_mut().unwrap().remove("correction");
    let mut frames = Vec::new();
    for snapshot in [&result["document"], &without_strokes] {
        let draft = serde_json::from_value(snapshot.clone()).unwrap();
        let mut provider = BoundMasks::load(state, "owner", Some(&context), &draft, &cancel).await.unwrap();
        let mut frame = video_kadr_backend::domain::group_surface::GroupSurface {
            width: 16, height: 16, rgba: [240, 0, 0, 255].repeat(16 * 16),
        };
        provider.apply(
            root, &video_kadr_backend::domain::composition::CompositionClipId::parse("eof").unwrap(),
            &SourceId::parse("alias").unwrap(), 100000, &mut frame,
        ).unwrap();
        frames.push(frame.rgba);
    }
    assert_ne!(frames[0], frames[1], "preserved strokes should affect the new mask");

}

async fn qualify_masked_template_slot(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    project: &video_kadr_backend::db::CompositionProject,
    root: &str,
    token: &str,
    document: &Value,
    replacement_hash: &str,
) {
    use video_kadr_backend::{domain::project_media, template_bundle};

    let draft = serde_json::from_value(document.clone()).unwrap();
    let (draft, marked) = project_media::set_placeholder(
        &draft, &SourceId::parse("alias").unwrap(), true,
    ).unwrap();
    assert!(marked);
    let work = tempfile::tempdir().unwrap();
    let author = state.db.create_space_composition_project(
        "Masked video slot", &serde_json::to_value(draft).unwrap(),
        &project.source_ids, "owner", project.space_id.as_deref().unwrap(),
    ).await.unwrap();
    let response = app.clone().oneshot(Request::get(format!(
        "/api/composition-projects/{}/template", author.id,
    )).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let package = to_bytes(response.into_body(), 8 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&package));
    let parsed = template_bundle::read(std::io::Cursor::new(package.as_ref()), work.path()).unwrap();
    assert_eq!(parsed.manifest.slot_masks.len(), 1);
    assert!(parsed.manifest.video_caches.is_empty());
    let recipe = serde_json::to_value(&parsed.manifest.slot_masks[0].recipe).unwrap();
    assert!(recipe.get("sourceHash").is_none());
    assert_eq!(recipe["correction"]["strokes"].as_array().unwrap().len(), 2);

    let selections: Vec<_> = parsed.manifest.project.project_media.iter()
        .filter(|(_, item)| item.placeholder)
        .map(|(id, _)| json!({"mediaId":id,"sourceId":"physical"})).collect();
    let selections = serde_json::to_string(&selections).unwrap();
    let fills: String = url::form_urlencoded::byte_serialize(selections.as_bytes()).collect();
    let upload = |uri: &str| {
        let mut body = b"--MASKSLOT\r\nContent-Disposition: form-data; name=\"file\"; filename=\"slot.vktemplate\"\r\n\r\n".to_vec();
        body.extend_from_slice(&package);
        body.extend_from_slice(b"\r\n--MASKSLOT--\r\n");
        Request::post(uri)
            .header("content-type", "multipart/form-data; boundary=MASKSLOT")
            .header("x-space-id", project.space_id.as_deref().unwrap())
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body)).unwrap()
    };
    let uri = format!("/api/composition-projects/template-import?fills={fills}");
    let pool = sqlx::sqlite::SqlitePoolOptions::new().connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")),
    ).await.unwrap();
    let project_count = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM composition_projects")
            .fetch_one(&pool).await.unwrap()
    };
    let before_count = project_count().await;
    let cache_root = state.storage.join("concat-video-caches");
    let before_generations = std::fs::read_dir(&cache_root).unwrap().count();
    sqlx::query("CREATE TRIGGER reject_slot_template BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'forced slot template rollback'); END")
        .execute(&pool).await.unwrap();
    let rejected = app.clone().oneshot(upload(&uri)).await.unwrap();
    assert_eq!(rejected.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(project_count().await, before_count);
    assert_eq!(std::fs::read_dir(&cache_root).unwrap().count(), before_generations);
    sqlx::query("DROP TRIGGER reject_slot_template").execute(&pool).await.unwrap();
    let direct = app.clone().oneshot(upload(&uri)).await.unwrap();
    let stored = app.clone().oneshot(upload("/api/template-library")).await.unwrap();
    assert_eq!(stored.status(), StatusCode::CREATED);
    let stored: Value = serde_json::from_slice(&to_bytes(stored.into_body(), 1 << 20).await.unwrap()).unwrap();
    let library = app.clone().oneshot(Request::post(format!(
        "/api/template-library/{}/create?fills={fills}", stored["id"].as_str().unwrap(),
    )).header("authorization", format!("Bearer {token}"))
      .header("x-space-id", project.space_id.as_deref().unwrap())
      .body(Body::empty()).unwrap()).await.unwrap();
    for response in [direct, library] {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
        let result: Value = serde_json::from_slice(&bytes).unwrap();
        let saved = &result["project"];
        assert_ne!(saved["id"], project.id);
        assert_eq!(saved["document"]["projectMedia"]["alias"]["librarySourceId"], "physical");
        let binding = &saved["document"]["compositions"][root]["content"]["tracks"][1]["clips"][0]["concatCutout"];
        assert_eq!(binding["sourceHash"], replacement_hash);
        assert_eq!(binding["correction"], recipe["correction"]);
        assert!(binding.get("origin").is_none());
        let rendered = support::router(state.clone().with_selfie_config(None)).oneshot(
            request("/api/compositions/nested-preview", token, json!({
                "project":saved["document"],"compositionId":root,"timeUs":2100000,
                "renderedOutputs":{},"savedProject":{"projectId":saved["id"],"expectedRevision":saved["revision"]},
            })),
        ).await.unwrap();
        assert_eq!(rendered.status(), StatusCode::OK);
    }
    let still = create_saved_import_still(state, project.space_id.as_deref().unwrap()).await;
    let saved_space = project.space_id.as_deref().unwrap();
    let rebuild = || Request::post("/api/compositions/concat-project/import-rebuild")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .header("x-space-id", project.space_id.as_deref().unwrap())
        .body(Body::from(json!({
            "name":"Concat restored mask",
            "preparedStills":[{"timelineId":"main","clipId":"image","projectId":still.id,
                "sourceClipId":"still","expectedRevision":still.revision}],
            "subtitleDocuments":[{"id":"dialogue","name":"dialogue.srt","text":"1\n00:00:00,000 --> 00:00:01,000\nDialogue"}],
            "request": {
                "schemaVersion":1, "preserveMediaIds":true,
                "document":{"version":1,"media":[{"id":"cutout-slot","kind":"video","path":"never-open.mkv"},
                    {"id":"picture","kind":"image","path":"never-open.png"}],
                    "timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":10,"rateDen":1},
                        "tracks":[{"id":"image"},{"id":"video"}],"clips":[{"id":"image","trackId":"image","mediaId":"picture",
                            "kind":"image","duration":0.2,"cutout":{"mode":"auto","feather":0}},
                            {"id":"mask","trackId":"video","mediaId":"cutout-slot",
                            "kind":"video","duration":0.2,"cutout":{"mode":"custom","subject":"person","feather":0.04,
                                "strokes":[{"tool":"brush","size":0.1,"points":[[0.25,0.5]],"at":0.1}]}}]}]},
                "mediaBindings":{"cutout-slot":{"id":"physical","kind":"video","durationTicks":9000000,
                    "width":80,"height":80,"hasAudio":false},
                    "picture":{"id":"still-raw","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}}
            }
        }).to_string())).unwrap();
    let count = project_count().await;
    let generations = std::fs::read_dir(&cache_root).unwrap().count();
    sqlx::query("CREATE TRIGGER reject_concat_mask_import BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'forced Concat mask rollback'); END")
        .execute(&pool).await.unwrap();
    assert_eq!(app.clone().oneshot(rebuild()).await.unwrap().status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(project_count().await, count);
    assert_eq!(std::fs::read_dir(&cache_root).unwrap().count(), generations);
    sqlx::query("DROP TRIGGER reject_concat_mask_import").execute(&pool).await.unwrap();
    let response = app.clone().oneshot(rebuild()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let project = &result["project"];
    let document = &project["document"];
    assert_eq!(result["timelineIds"]["main"], "concat-timeline-0");
    assert_eq!(document["compositions"]["concat-timeline-0"]["content"]["metadata"]["subtitleDocuments"][0]["name"], "dialogue.srt");
    let content = &document["compositions"]["concat-timeline-0"]["content"];
    let video_clip = content["tracks"].as_array().unwrap().iter().find(|track| track["kind"] == "video").unwrap();
    let selection = &video_clip["clips"][0]["concatCutout"];
    assert_eq!(content["metadata"]["mediaVersions"]["image"]["cutout"]["modelHash"],
        video_kadr_backend::domain::cutout::SELFIE_MODEL_HASH);
    assert_eq!(selection["sourceHash"], replacement_hash);
    assert_eq!(selection["correction"]["strokes"][0]["at"], 0.1);
    assert_eq!(document["projectMedia"]["cutout-slot"]["source"]["width"], 16);
    assert_eq!(document["projectMedia"]["cutout-slot"]["librarySourceId"], "physical");
    let rendered = support::router(state.clone().with_selfie_config(None)).oneshot(
        request("/api/compositions/nested-preview", token, json!({
            "project":document,"timeUs":100000,"renderedOutputs":{},
            "savedProject":{"projectId":project["id"],"expectedRevision":project["revision"]},
        })),
    ).await.unwrap();
    assert_eq!(rendered.status(), StatusCode::OK);

    Box::pin(qualify_fixed_concat_zip_round_trip(
        state, app, token, saved_space, replacement_hash,
    )).await;

}

async fn qualify_fixed_concat_zip_round_trip(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    token: &str,
    space: &str,
    expected_hash: &str,
) {
    let create = |document: Value, source_id: &str| {
        Request::post("/api/compositions/concat-project/import-rebuild")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .header("x-space-id", space)
            .body(Body::from(json!({
                "request":{"schemaVersion":1,"preserveMediaIds":true,"document":document,
                    "mediaBindings":{"fixed":{"id":source_id,"kind":"video","durationTicks":200000,
                        "width":16,"height":16,"hasAudio":false}}}
            }).to_string())).unwrap()
    };
    let source = json!({"version":1,"media":[{"id":"fixed","kind":"video"}],
        "timelines":[{"id":"main","video":{"width":16,"height":16,"rateNum":10,"rateDen":1},
            "tracks":[{"id":"video"}],"clips":[{"id":"cut","trackId":"video","mediaId":"fixed",
                "kind":"video","sourceStart":0.1,"duration":0.1,
                "cutout":{"mode":"custom","subject":"person","feather":0.04,
                    "strokes":[{"tool":"brush","size":0.1,"points":[[0.25,0.5]],"at":0.1}]}}]}]});
    let creation = app.clone().oneshot(create(source, "physical")).await.unwrap();
    let status = creation.status();
    let bytes = to_bytes(creation.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    Box::pin(qualify_replaceable_concat_zip_round_trip(
        state, app, token, space, &original["project"], expected_hash,
    )).await;


    let export = app.clone().oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", original["project"]["id"].as_str().unwrap(),
    )).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
    let status = export.status();
    let bytes = to_bytes(export.into_body(), 8 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let manifest: Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    let mut video = Vec::new();
    std::io::Read::read_to_end(&mut zip.by_name(manifest["media"][0]["path"].as_str().unwrap()).unwrap(), &mut video).unwrap();
    assert_eq!(format!("{:x}", Sha256::digest(&video)), expected_hash);
    drop(zip);
    std::fs::write(state.sources_dir().join("zip-copy.mkv"), video).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source", &json!({
        "id":"zip-copy","filename":"zip-copy.mkv","url":"/files/sources/zip-copy.mkv",
        "mediaType":"video","width":16,"height":16,
    }))).await);
    let imported = app.clone().oneshot(create(manifest, "zip-copy")).await.unwrap();
    let status = imported.status();
    let bytes = to_bytes(imported.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let imported: Value = serde_json::from_slice(&bytes).unwrap();
    assert_ne!(original["project"]["id"], imported["project"]["id"]);
    assert_eq!(imported["project"]["document"]["projectMedia"]["fixed"]["librarySourceId"], "zip-copy");
    let render = support::router(state.clone().with_selfie_config(None));
    let mut frames = Vec::new();
    for result in [&original, &imported] {
        let project = &result["project"];
        let clip = &project["document"]["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0];
        assert_eq!(clip["placement"]["sourceInTick"], 100000);
        assert_eq!(clip["concatCutout"]["sourceHash"], expected_hash);
        assert_eq!(clip["concatCutout"]["correction"]["strokes"][0]["at"], 0.1);
        let frame = render.clone().oneshot(request("/api/compositions/nested-preview", token, json!({
            "project":project["document"],"timeUs":0,"renderedOutputs":{},
            "savedProject":{"projectId":project["id"],"expectedRevision":project["revision"]},
        }))).await.unwrap();
        let status = frame.status();
        let pixels = to_bytes(frame.into_body(), 1 << 20).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pixels));
        frames.push(pixels);
    }
    assert_eq!(frames[0], frames[1]);
    let saved = &imported["project"];
    let mut unmasked = saved["document"].clone();
    unmasked["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]
        .as_object_mut().unwrap().remove("concatCutout");
    let plain = render.oneshot(request("/api/compositions/nested-preview", token, json!({
        "project":unmasked,"timeUs":0,"renderedOutputs":{},
        "savedProject":{"projectId":saved["id"],"expectedRevision":saved["revision"]},
    }))).await.unwrap();
    let status = plain.status();
    let pixels = to_bytes(plain.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pixels));
    assert_ne!(frames[1], pixels, "fixture must visibly exercise the restored mask");
    let mut styled = saved["document"].clone();
    let content = &mut styled["compositions"]["concat-timeline-0"]["content"];
    content["metadata"]["captionTextTracks"] = json!(["captions"]);
    content["tracks"].as_array_mut().unwrap().push(json!({
        "id":"captions","kind":"text","name":"Captions","clips":[{
            "id":"caption","timelineStartTick":200000,"timelineEndTick":400000,"text":"Dialogue",
            "style":{"fontFamily":"Noto Sans","fontSize":4,"align":"center",
                "color":{"red":1,"green":1,"blue":1,"alpha":1},
                "background":{"red":0,"green":0,"blue":0,"alpha":0},
                "stroke":{"red":0,"green":0,"blue":0,"alpha":0},"strokeWidth":0,
                "shadow":{"red":0,"green":0,"blue":0,"alpha":0.5},"shadowX":2,"shadowY":-1},
            "transform":video_kadr_backend::domain::composition::TransformSpec::default(),
            "opacity":{"mode":"constant","value":1},"enabled":true,
        }]
    }));
    let updated = app.clone().oneshot(Request::put(format!("/api/composition-projects/{}", saved["id"].as_str().unwrap()))
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "schemaVersion":2,"mode":"composition","name":saved["name"],
            "baseRevision":saved["revision"],"document":styled,
        }).to_string())).unwrap()).await.unwrap();
    let status = updated.status();
    let bytes = to_bytes(updated.into_body(), 1 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let styled: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(styled["revision"].as_u64().unwrap(), saved["revision"].as_u64().unwrap() + 1);
    assert_eq!(styled["document"]["compositions"]["concat-timeline-0"]["content"]["tracks"][1]["clips"][0]["style"]["shadowX"], 2.0);
    let rendered = support::router(state.clone().with_selfie_config(None)).oneshot(
        request("/api/compositions/nested-preview", token, json!({
            "project":styled["document"],"timeUs":0,"renderedOutputs":{},
            "savedProject":{"projectId":styled["id"],"expectedRevision":styled["revision"]},
        })),
    ).await.unwrap();
    assert_eq!(rendered.status(), StatusCode::OK);
    assert_eq!(to_bytes(rendered.into_body(), 1 << 20).await.unwrap(), frames[1]);

}

async fn qualify_replaceable_concat_zip_round_trip(
    state: &video_kadr_backend::state::AppState,
    app: &axum::Router,
    token: &str,
    space: &str,
    original: &Value,
    old_hash: &str,
) {
    let mut document = original["document"].clone();
    document["projectMedia"]["fixed"]["placeholder"] = json!(true);
    let saved = state.db.create_space_composition_project(
        "Replaceable Concat video",
        &document,
        &["physical".into()],
        "owner",
        space,
    ).await.unwrap();
    let archive = app.clone().oneshot(Request::get(format!(
        "/api/composition-projects/{}/concat-template", saved.id,
    )).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()).await.unwrap();
    let status = archive.status();
    let bytes = to_bytes(archive.into_body(), 8 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 1, "export must omit the old source bytes");
    let manifest: Value = serde_json::from_reader(zip.by_name("template.json").unwrap()).unwrap();
    assert_eq!(manifest["media"][0]["placeholder"], true);
    assert_eq!(manifest["timelines"][0]["clips"][0]["cutout"]["strokes"][0]["at"], 0.0);
    drop(zip);
    let response = app.clone().oneshot(request(
        "/api/compositions/concat-project/fill-registered-slot",
        token,
        json!({"document":manifest,"mediaId":"fixed","sourceId":"slot"}),
    )).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 << 20).await.unwrap();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let filled: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(filled["document"]["timelines"][0]["clips"][0]["sourceStart"], 0.0);
    let response = app.clone().oneshot(Request::post("/api/compositions/concat-project/import-rebuild")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("x-space-id", space)
        .body(Body::from(json!({
            "request":{"schemaVersion":1,"preserveMediaIds":true,"document":filled["document"],
                "mediaBindings":{"fixed":filled["source"]}}
        }).to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 << 20).await.unwrap();
    assert_eq!(status, StatusCode::CREATED, "{}", String::from_utf8_lossy(&bytes));
    let result: Value = serde_json::from_slice(&bytes).unwrap();
    let project = &result["project"];
    let document = &project["document"];
    assert_eq!(document["projectMedia"]["fixed"]["librarySourceId"], "slot");
    let clip = &document["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0];
    assert_eq!(clip["placement"]["sourceInTick"], 0);
    let mask = &clip["concatCutout"];
    let entry = state.library.get("slot").await.unwrap();
    let path = state.library.resolve_media_path(&entry).await.unwrap();
    let hash = format!("{:x}", Sha256::digest(std::fs::read(path).unwrap()));
    assert_ne!(hash, old_hash);
    assert_eq!(mask["sourceHash"], hash);
    assert_eq!(mask["correction"]["strokes"][0]["at"], 0.0);
    let render = support::router(state.clone().with_selfie_config(None));
    let preview = |document: Value| request("/api/compositions/nested-preview", token, json!({
        "project":document,"timeUs":0,"renderedOutputs":{},
        "savedProject":{"projectId":project["id"],"expectedRevision":project["revision"]},
    }));
    let mut unmasked = document.clone();
    unmasked["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]
        .as_object_mut().unwrap().remove("concatCutout");
    let mut frames = Vec::new();
    for frame in [document.clone(), unmasked] {
        let rendered = render.clone().oneshot(preview(frame)).await.unwrap();
        let status = rendered.status();
        let pixels = to_bytes(rendered.into_body(), 1 << 20).await.unwrap();
        assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&pixels));
        frames.push(pixels);
    }
    assert_ne!(frames[0], frames[1], "restored mask must change the replacement frame");
    let viewer = state.db.register_auth_user("mask-space-viewer", "synthetic video password")
        .await.unwrap().unwrap();
    state.db.set_space_member(space, "owner", &viewer.user.username, video_kadr_backend::db::SpaceRole::Viewer)
        .await.unwrap().unwrap();
    let viewer_preview = |document: Value| request("/api/compositions/nested-preview", &viewer.token, json!({
        "project":document,"timeUs":0,"renderedOutputs":{},
        "savedProject":{"projectId":project["id"],"expectedRevision":project["revision"]},
    }));
    let rendered = render.clone().oneshot(viewer_preview(document.clone())).await.unwrap();
    assert_eq!(rendered.status(), StatusCode::OK);
    assert_eq!(to_bytes(rendered.into_body(), 1 << 20).await.unwrap(), frames[0]);
    let mut modified = document.clone();
    modified["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]
        ["concatCutout"]["feather"] = json!(0.08);
    let denied = render.oneshot(viewer_preview(modified)).await.unwrap();
    assert_eq!(denied.status(), StatusCode::BAD_REQUEST);

}

async fn create_saved_import_still(
    state: &video_kadr_backend::state::AppState,
    space: &str,
) -> video_kadr_backend::db::CompositionProject {
    use video_kadr_backend::domain::{
        composition::{CanvasSpec, Composition, CompositionClipId, CompositionSource, TransformSpec},
        concat_cutout_binding::{bind_still, StillAssets},
        concat_cutout_import::plan,
        cutout::SELFIE_MODEL_HASH,
    };
    let descriptor = |id, size| -> CompositionSource {
        serde_json::from_value(json!({"id":id,"kind":"image","durationTicks":0,
            "width":size,"height":size,"hasAudio":false})).unwrap()
    };
    let ids = ["still-raw", "still-copy", "still-mask"];
    for (id, size) in [("still-raw", 2), ("still-copy", 2), ("still-mask", 256)] {
        let filename = format!("{id}.png");
        let path = state.sources_dir().join(&filename);
        if size == 256 {
            image::GrayImage::new(size, size).save(&path).unwrap();
        } else {
            image::RgbaImage::new(size, size).save(&path).unwrap();
        }
        assert!(state.library.add(MediaEntry::from_result("source", &json!({
            "id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"image",
        }))).await);
    }
    let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let file_hash = |id| hash(&std::fs::read(state.sources_dir().join(format!("{id}.png"))).unwrap());
    let runtime = "a".repeat(64);
    let assets = StillAssets {
        original: descriptor(ids[0], 2), copy: descriptor(ids[1], 2), mask: descriptor(ids[2], 256),
        regions: vec![],
        snapshot: serde_json::from_value(json!({
            "sourceId":ids[0],"maskSourceId":ids[2],"sourceHash":file_hash(ids[0]),
            "maskHash":hash(&vec![0; 256*256]),"maskFileHash":file_hash(ids[2]),
            "modelHash":SELFIE_MODEL_HASH,"runtimeHash":runtime,
            "correction":{"custom":false,"strokes":[],"feather":0},
        })).unwrap(),
    };
    let mut composition = Composition::new(CanvasSpec::default());
    composition.sources.insert(assets.original.id.clone(), assets.original.clone());
    composition.tracks = serde_json::from_value(json!([{
        "kind":"image","id":"image","name":"Image","clips":[{
            "id":"still","sourceId":ids[0],"timelineStartTick":0,"durationTicks":200000,
            "transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal",
        }],
    }])).unwrap();
    let composition = bind_still(&composition, &CompositionClipId::parse("still").unwrap(),
        &plan(&json!({"mode":"auto","feather":0})).unwrap(), &assets, &assets.snapshot.source_hash,
        SELFIE_MODEL_HASH, &runtime).unwrap();
    let mut source_ids = ids.map(str::to_owned);
    source_ids.sort();
    state.db.create_space_composition_project("Saved still", &serde_json::to_value(composition).unwrap(),
        &source_ids, "owner", space).await.unwrap()
}

async fn qualify_atomic_save(state:&video_kadr_backend::state::AppState,token:&str,space:&str,draft:&video_kadr_backend::domain::nested_composition::NestedCompositionProject,expected:&[u8]){
 let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(state.storage.join("app.db")).foreign_keys(true)).await.unwrap();
 let app=support::router(state.clone());
 let count=||async{sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM composition_projects").fetch_one(&pool).await.unwrap()};
 let before=count().await;
 let body=json!({"schemaVersion":2,"mode":"composition","name":"Atomic imported video","spaceId":space,"document":draft});
 sqlx::query("CREATE TRIGGER reject_adopted_video BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'controlled atomic save rejection'); END").execute(&pool).await.unwrap();
 let response=app.clone().oneshot(request("/api/composition-projects",token,body.clone())).await.unwrap();assert!(!response.status().is_success());assert_eq!(count().await,before);
 sqlx::query("DROP TRIGGER reject_adopted_video").execute(&pool).await.unwrap();
 let response=app.clone().oneshot(request("/api/composition-projects",token,body)).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1<<20).await.unwrap();assert!(status.is_success(),"{}",String::from_utf8_lossy(&bytes));
 let saved:Value=serde_json::from_slice(&bytes).unwrap();let id=saved["id"].as_str().unwrap();let owned=state.db.get_composition_project_for(id,"owner").await.unwrap().unwrap();
 assert!(owned.document["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["concatCutout"].get("origin").is_none());
 let cache_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM concat_video_project_caches WHERE project_id=?").bind(id).fetch_one(&pool).await.unwrap();assert_eq!(cache_count,1);assert_eq!(count().await,before+1);
 let update=|value:Value|Request::builder().method("PUT").uri(format!("/api/composition-projects/{id}")).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(value.to_string())).unwrap();
 let update_body=json!({"schemaVersion":2,"mode":"composition","name":"Updated adopted video","document":draft,"baseRevision":owned.revision});
 sqlx::query("CREATE TRIGGER reject_adopted_video BEFORE INSERT ON concat_video_model_choices BEGIN SELECT RAISE(ABORT,'controlled atomic update rejection'); END").execute(&pool).await.unwrap();
 let response=app.clone().oneshot(update(update_body.clone())).await.unwrap();assert!(!response.status().is_success());let unchanged=state.db.get_composition_project_for(id,"owner").await.unwrap().unwrap();assert_eq!(unchanged.revision,owned.revision);assert_eq!(unchanged.document,owned.document);assert_eq!(unchanged.name,owned.name);
 sqlx::query("DROP TRIGGER reject_adopted_video").execute(&pool).await.unwrap();
 let response=app.clone().oneshot(update(update_body.clone())).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1<<20).await.unwrap();assert!(status.is_success(),"{}",String::from_utf8_lossy(&bytes));let updated=state.db.get_composition_project_for(id,"owner").await.unwrap().unwrap();assert_eq!(updated.revision,owned.revision+1);assert_eq!(updated.document,owned.document);
 assert!(!app.clone().oneshot(update(update_body)).await.unwrap().status().is_success());
 let response=app.clone().oneshot(request("/api/compositions/nested-preview",token,json!({"project":updated.document,"timeUs":100000,"renderedOutputs":{},"savedProject":{"projectId":id,"expectedRevision":updated.revision}}))).await.unwrap();let status=response.status();let pixels=to_bytes(response.into_body(),1<<20).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&pixels));assert_eq!(pixels.as_ref(),expected);
 // Reapplying an origin pointing at this project's previous revision must
 // adopt within its own next revision, without leaving a stale self-reference.
 let mut self_draft=updated.document.clone();self_draft["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["concatCutout"]["origin"]=json!({"projectId":id,"expectedRevision":updated.revision,"sourceId":"prepared-slot"});
 let response=app.clone().oneshot(update(json!({"schemaVersion":2,"mode":"composition","name":updated.name,"document":self_draft,"baseRevision":updated.revision}))).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1<<20).await.unwrap();assert!(status.is_success(),"{}",String::from_utf8_lossy(&bytes));let own=state.db.get_composition_project_for(id,"owner").await.unwrap().unwrap();assert_eq!(own.document,owned.document);assert_eq!(own.revision,updated.revision+1);
 let origin_project=serde_json::to_value(draft).unwrap()["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"][0]["concatCutout"]["origin"]["projectId"].as_str().unwrap().to_owned();
 sqlx::query("UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor='owner'").bind(&origin_project).execute(&pool).await.unwrap();
 let cancel=tokio_util::sync::CancellationToken::new();let context=SavedProject{project_id:id.into(),expected_revision:own.revision};
 assert!(BoundMasks::load(state,"owner",None,draft,&cancel).await.is_err());
 let response=app.clone().oneshot(request("/api/compositions/nested-preview",token,json!({"project":draft,"timeUs":100000,"renderedOutputs":{},"savedProject":context}))).await.unwrap();let status=response.status();let pixels=to_bytes(response.into_body(),1<<20).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&pixels));assert_eq!(pixels.as_ref(),expected);
 let stale=SavedProject{expected_revision:own.revision-1,..context.clone()};assert!(BoundMasks::load(state,"owner",Some(&stale),draft,&cancel).await.is_err());
 let response=app.clone().oneshot(update(json!({"schemaVersion":2,"mode":"composition","name":own.name,"document":draft,"baseRevision":own.revision}))).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1<<20).await.unwrap();assert!(status.is_success(),"{}",String::from_utf8_lossy(&bytes));let own=state.db.get_composition_project_for(id,"owner").await.unwrap().unwrap();assert_eq!(own.document,owned.document);
 sqlx::query("UPDATE composition_project_members SET role='owner' WHERE project_id=? AND actor='owner'").bind(&origin_project).execute(&pool).await.unwrap();
 // A bad owned registration must refuse, even with a valid external provider.
 assert!(BoundMasks::load(state,"owner",None,draft,&cancel).await.is_ok());
 let latest=SavedProject{project_id:id.into(),expected_revision:own.revision};
 sqlx::query("UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor='owner'").bind(id).execute(&pool).await.unwrap();
 assert!(BoundMasks::load(state,"owner",Some(&latest),draft,&cancel).await.is_err());
 sqlx::query("UPDATE composition_project_members SET role='owner' WHERE project_id=? AND actor='owner'").bind(id).execute(&pool).await.unwrap();
 let hash:String=sqlx::query_scalar("SELECT manifest_hash FROM concat_video_project_caches WHERE project_id=? LIMIT 1").bind(id).fetch_one(&pool).await.unwrap();
 sqlx::query("UPDATE concat_video_project_caches SET manifest_hash=? WHERE project_id=?").bind("0".repeat(64)).bind(id).execute(&pool).await.unwrap();
 let context=SavedProject{project_id:id.into(),expected_revision:own.revision};assert!(BoundMasks::load(state,"owner",Some(&context),draft,&cancel).await.is_err());
 sqlx::query("UPDATE concat_video_project_caches SET manifest_hash=? WHERE project_id=?").bind(hash).bind(id).execute(&pool).await.unwrap();
 assert!(BoundMasks::load(state,"owner",Some(&context),draft,&cancel).await.is_ok());
 Box::pin(qualify_archive_export(state,token,space,&own,expected)).await;

}

async fn qualify_archive_export(state:&video_kadr_backend::state::AppState,token:&str,space:&str,own:&video_kadr_backend::db::CompositionProject,expected:&[u8]){
 let mut document=own.document.clone();let clips=document["compositions"]["concat-timeline-0"]["content"]["tracks"][0]["clips"].as_array_mut().unwrap();clips[0]["enabled"]=json!(false);let mut copy=clips[0].clone();copy["id"]=json!("disabled-copy");copy["placement"]["timelineStartTick"]=json!(200000);clips.push(copy);
 let own=state.db.update_composition_project_for(&own.id,"owner",&own.name,&document,&own.source_ids,own.revision).await.unwrap().unwrap();
 let app=support::router(state.clone());let export=|id:&str|Request::builder().uri(format!("/api/composition-projects/{id}/archive")).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();
 let response=app.clone().oneshot(export(&own.id)).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),8*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));
 let root=tempfile::tempdir().unwrap();let parsed=video_kadr_backend::project_archive::read_project_archive(bytes.as_ref(),root.path()).await.unwrap();assert_eq!(parsed.manifest.schema_version,2);assert_eq!(parsed.manifest.project.document,own.document);assert_eq!(parsed.video_caches.len(),1);assert_eq!(parsed.manifest.video_caches[0].source_id,"prepared-slot");assert!(!String::from_utf8_lossy(&bytes).contains(&own.id));
 let entry=parsed.manifest.video_caches[0].clone();let cancel=tokio_util::sync::CancellationToken::new();let mut restored=video_kadr_backend::services::concat_video_analysis::portable::restore(tokio::fs::File::open(parsed.video_caches[0].path()).await.unwrap(),&entry.identity,root.path(),8*1024*1024,&cancel).await.unwrap();
 let mut registered=cache::restore_for_model(state,"owner",&own.id,"prepared-slot",own.revision,&entry.identity.runtime_hash,&entry.identity.model_hash,&cancel).await.unwrap().unwrap();assert_eq!(restored.cache().identity().unwrap(),registered.identity().unwrap());
 for time in [0,100]{assert_eq!(restored.cache().automatic(time,&cancel).unwrap().map(|mask|mask.probabilities.clone()),registered.automatic(time,&cancel).unwrap().map(|mask|mask.probabilities.clone()));}
 drop(restored);drop(parsed);assert_eq!(std::fs::read_dir(root.path()).unwrap().count(),0);
 let missing=state.db.create_space_composition_project("Missing archived cache",&document,&own.source_ids,"owner",space).await.unwrap();assert_eq!(app.clone().oneshot(export(&missing.id)).await.unwrap().status(),StatusCode::CONFLICT);
 assert!(std::fs::read_dir(state.staging_dir()).unwrap().all(|entry|!entry.unwrap().file_name().to_string_lossy().contains("veproj-export")));
 Box::pin(support::video_archive::import_and_preview(&bytes,Some("concat-timeline-0"),100000,expected,false)).await;
 Box::pin(support::video_archive::template_round_trip(state,&own.id,token,"concat-timeline-0",100000,expected,false)).await;
 // Physical-ID flat projects use the same archive path and acquire fresh IDs.
 let physical=own.source_ids[0].clone();let mut flat=own.document["compositions"]["concat-timeline-0"]["content"].clone();let mut descriptor=flat["sources"].as_object_mut().unwrap().remove("prepared-slot").unwrap();descriptor["id"]=json!(physical);flat["sources"][&physical]=descriptor;
 for clip in flat["tracks"][0]["clips"].as_array_mut().unwrap(){clip["sourceId"]=json!(physical);}
 let flat=state.db.create_space_composition_project("Flat archived mask",&flat,&own.source_ids,"owner",space).await.unwrap();
 cache::transfer_for_model(state,"owner",cache::ProjectSource{project:&own.id,source_id:"prepared-slot",revision:own.revision},cache::ProjectSource{project:&flat.id,source_id:&physical,revision:flat.revision},&entry.identity.runtime_hash,&entry.identity.model_hash,&cancel).await.unwrap();
 let response=app.oneshot(export(&flat.id)).await.unwrap();let status=response.status();let flat_bytes=to_bytes(response.into_body(),8*1024*1024).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&flat_bytes));Box::pin(support::video_archive::import_and_preview(&flat_bytes,None,100000,expected,false)).await;

}
