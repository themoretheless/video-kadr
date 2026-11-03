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
