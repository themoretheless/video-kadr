use super::*;
#[tokio::test]
async fn disabled_speech_is_explicit_and_private_sources_remain_authorized() {
    let (state, _directory) = make_state(true, false).await;
    std::fs::write(state.sources_dir().join("speech.wav"), b"placeholder").unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"speech","filename":"speech.wav","url":"/files/sources/speech.wav","mediaType":"audio"}))).await);
    let owner = state
        .db
        .register_auth_user("speech-owner-disabled", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Private speech")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Speech",
            &json!({"schemaVersion":1,"sources":{"speech":{"id":"speech"}}}),
            &["speech".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = router(state);
    let (status, models, _) = send(&app, get("/api/speech/models")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(models["available"], false);
    assert_eq!(send(&app,get("/api/speech/jobs")).await.0,StatusCode::UNAUTHORIZED);
    let (status,empty_history,_)=send(&app,bearer(get("/api/speech/jobs"),&owner.token)).await;
    assert_eq!(status,StatusCode::OK);
    assert_eq!(empty_history["jobs"],json!([]));

    let request = json!({"sourceId":"speech","startUs":0,"endUs":1000000,"language":"en"});
    assert_eq!(
        send(&app, post_json("/api/speech/transcribe", request.clone()))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            bearer(post_json("/api/speech/transcribe", request.clone()), &owner.token)
        )
        .await
        .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
#[tokio::test]
#[ignore = "requires explicit local Whisper model and public audio fixture"]
async fn real_whisper_http_private_audio_captions_and_native_export() {
    let model = std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL").expect("Whisper test model");
    let audio = std::env::var("VIDEO_KADR_TEST_WHISPER_AUDIO").expect("Whisper test audio");
    let (mut state, _directory) = make_state(true, false).await;
    state = state.with_speech_config(video_kadr_backend::services::local_speech::SpeechConfig {
        model: Some(model.into()),
        ..Default::default()
    });
    let tools = std::sync::Arc::make_mut(&mut state.tools);
    tools.ffmpeg_encoders.push("ffv1".into());
    tools.ffmpeg_muxers.push("matroska".into());
    std::fs::copy(audio, state.sources_dir().join("speech.wav")).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"speech","filename":"speech.wav","url":"/files/sources/speech.wav","mediaType":"audio"}))).await);
    let owner = state
        .db
        .register_auth_user("speech-owner", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let outsider = state
        .db
        .register_auth_user("speech-outsider", "shared test password")
        .await
        .unwrap()
        .unwrap();
    let space = state
        .db
        .create_space(&owner.user.username, "Private speech")
        .await
        .unwrap();
    state
        .db
        .create_space_composition_project(
            "Speech",
            &json!({"schemaVersion":1,"sources":{"speech":{"id":"speech"}}}),
            &["speech".into()],
            &owner.user.username,
            &space.id,
        )
        .await
        .unwrap();
    let app = router(state.clone());
    let (status, models, _) = send(&app, get("/api/speech/models")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(models["available"], true);
    let request = json!({"sourceId":"speech","startUs":0,"endUs":11000000,"language":"en"});
    assert_eq!(
        send(&app, post_json("/api/speech/transcribe", request.clone()))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            bearer(
                post_json("/api/speech/transcribe", request.clone()),
                &outsider.token
            )
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, transcript, _) = send(
        &app,
        bearer(post_json("/api/speech/transcribe", request.clone()), &owner.token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{transcript}");
    assert_eq!(
        transcript["modelHash"],
        "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7"
    );
    let (queued, accepted, _) = send(&app, bearer(post_json("/api/speech/jobs", request.clone()), &owner.token)).await;
    assert_eq!(queued, StatusCode::ACCEPTED, "{accepted}");
    let job_id = accepted["jobId"].as_str().unwrap();
    for suffix in ["", "/cancel", "/retry", "/discard"] {
        let uri = format!("/api/jobs/{job_id}{suffix}");
        let build = || if suffix.is_empty() {get(&uri)} else {post_json(&uri,json!({}))};
        assert_eq!(send(&app, build()).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(send(&app, bearer(build(), &outsider.token)).await.0, StatusCode::FORBIDDEN);
    }
    let done = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let (status, job, _) = send(&app,bearer(get(&format!("/api/jobs/{job_id}")),&owner.token)).await;
            assert_eq!(status,StatusCode::OK);
            if job["status"] == "done" || job["status"] == "error" {break job;}
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await;
    let done=match done{Ok(done)=>done,Err(_)=>panic!("speech completion observation expired: {:?}",state.get_job(job_id).await.unwrap())};
    assert_eq!(done["status"],"done","{done}");
    assert_eq!(done["result"],transcript);
    assert_eq!(state.library.list().await.iter().filter(|entry|entry.kind != "source").count(),0,
        "speech result must not enter public media library");
    // Reopen SQLite to prove the access guard does not depend on an in-memory owner map.
    let reopened = AppState::new_with_runtime(state.storage.clone(),2,(*state.tools).clone(),
        Library::load(state.storage.clone()).await,Db::open(&state.storage).await.unwrap(),state.encode_budget.clone(),2).unwrap();
    let reopened_app = router(reopened);
    assert_eq!(send(&reopened_app,get(&format!("/api/jobs/{job_id}"))).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(send(&reopened_app,bearer(get(&format!("/api/jobs/{job_id}")),&owner.token)).await.1["result"],transcript);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let control = tempfile::tempdir().unwrap();
        let executable=control.path().join("controlled-whisper");
        let ready=control.path().join("controlled-whisper.ready");
        let late=control.path().join("controlled-whisper.late");
        std::fs::write(&executable,b"#!/bin/sh\ntrap '' TERM\necho 'whisper_print_progress_callback: progress = 33%' >&2\ntouch \"$0.ready\"\n(trap '' TERM; sleep 1; touch \"$0.late\") & wait\n").unwrap();
        std::fs::set_permissions(&executable,std::fs::Permissions::from_mode(0o700)).unwrap();
        let controlled=state.clone().with_speech_config(video_kadr_backend::services::local_speech::SpeechConfig {
            executable, ..(*state.speech).clone()
        });
        let controlled_app=router(controlled);
        let (status,queued,_)=send(&controlled_app,bearer(post_json("/api/speech/jobs",request.clone()),&owner.token)).await;
        assert_eq!(status,StatusCode::ACCEPTED,"{queued}");
        let cancel_id=queued["jobId"].as_str().unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10),async {
            while !ready.exists(){tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
        }).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let (_,observed,_)=send(&controlled_app,bearer(get(&format!("/api/jobs/{cancel_id}")),&owner.token)).await;
                if observed["progress"]==json!(44.75){break;}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        assert_eq!(send(&controlled_app,bearer(post_json(&format!("/api/jobs/{cancel_id}/cancel"),json!({})),&owner.token)).await.0,StatusCode::OK);
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            while std::fs::read_dir(state.storage.join("speech-staging")).unwrap().count()!=0 {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        assert!(!late.exists(),"HTTP cancellation left recognition descendant alive");
        let (_,cancelled,_)=send(&controlled_app,bearer(get(&format!("/api/jobs/{cancel_id}")),&owner.token)).await;
        assert_eq!(cancelled["status"],"cancelled");
        assert!(cancelled.get("result").is_none());
    }
    assert_eq!(send(&app,get("/api/speech/jobs")).await.0,StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app,bearer(get("/api/speech/jobs"),&outsider.token)).await.1["jobs"],json!([]));
    let (status,history,_)=send(&reopened_app,bearer(get("/api/speech/jobs?limit=1"),&owner.token)).await;
    assert_eq!(status,StatusCode::OK,"{history}");
    assert_eq!(history["jobs"].as_array().unwrap().len(),1);
    assert!(history["jobs"][0].get("result").is_none());
    #[cfg(unix)]
    {
    let cursor=history["nextCursor"].as_str().unwrap();
    let (_,second,_)=send(&reopened_app,bearer(get(&format!("/api/speech/jobs?limit=1&cursor={cursor}")),&owner.token)).await;
    assert_eq!(second["jobs"].as_array().unwrap().len(),1);
    assert_ne!(history["jobs"][0]["jobId"],second["jobs"][0]["jobId"]);
    let ids=vec![history["jobs"][0]["jobId"].as_str().unwrap(),second["jobs"][0]["jobId"].as_str().unwrap()];
    assert!(ids.contains(&job_id));
    assert!(second["nextCursor"].is_null());
    }
    for uri in ["/api/speech/jobs?limit=0","/api/speech/jobs?cursor=unknown"] {
        assert_eq!(send(&app,bearer(get(uri),&owner.token)).await.0,StatusCode::BAD_REQUEST);
    }
    state.db.set_space_member(&space.id,&owner.user.username,&outsider.user.username,video_kadr_backend::db::SpaceRole::Viewer).await.unwrap().unwrap();
    let (_,member_job,_)=send(&app,bearer(post_json("/api/speech/jobs",request.clone()),&outsider.token)).await;
    let member_id=member_job["jobId"].as_str().unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30),async {
        loop {
            let (_,job,_)=send(&app,bearer(get(&format!("/api/jobs/{member_id}")),&outsider.token)).await;
            if job["status"]=="done" {break;}
            assert_ne!(job["status"],"error","{job}");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    assert_eq!(send(&app,bearer(get("/api/speech/jobs"),&outsider.token)).await.1["jobs"][0]["jobId"],member_id);
    state.db.remove_space_member(&space.id,&owner.user.username,&outsider.user.username).await.unwrap().unwrap();
    assert_eq!(send(&app,bearer(get("/api/speech/jobs"),&outsider.token)).await.1["jobs"],json!([]),"revoked source must disappear from saved history");
    assert_eq!(send(&app,bearer(get(&format!("/api/jobs/{member_id}")),&outsider.token)).await.0,StatusCode::FORBIDDEN);
    let segments = transcript["segments"].as_array().unwrap();
    assert!(!segments.is_empty());
    assert!(segments.iter().any(|cue| cue["text"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("country")));
    assert_eq!(
        std::fs::read_dir(state.storage.join("speech-staging"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        send(
            &app,
            bearer(
                post_json(
                    "/api/speech/transcribe",
                    json!({"sourceId":"speech","startUs":0,"endUs":12000000,"language":"en"})
                ),
                &owner.token
            )
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let fixture: Value =
        serde_json::from_str(include_str!("../fixtures/concat-merge-preview.json")).unwrap();
    let mut project = fixture["nativeCaptionPastes"][0]["before"].clone();
    let node = &mut project["compositions"]["new-timeline"];
    node["durationUs"] = json!(13000000);
    node["content"]["canvas"]["width"] = json!(320);
    node["content"]["canvas"]["height"] = json!(180);
    node["content"]["canvas"]["fpsMilli"] = json!(25000);
    node["content"]["canvas"]["fpsRate"] = json!({"numerator":25,"denominator":1});
    node["content"]["sources"] = json!({"speech":{"id":"speech","kind":"audio","durationTicks":11000000,"width":0,"height":0,"hasAudio":true,"audioChannels":1}});
    let mut audio = fixture["nativeVersions"][0]["original"]["compositions"]["new-timeline"]
        ["content"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|track| track["kind"] == "audio")
        .flat_map(|track| track["clips"].as_array().unwrap())
        .next()
        .unwrap()
        .clone();
    audio["id"] = json!("speech-clip");
    audio["sourceId"] = json!("speech");
    audio["placement"] =
        json!({"timelineStartTick":2000000,"sourceInTick":0,"sourceOutTick":11000000,"speed":2});
    node["content"]["tracks"] = json!([{"kind":"audio","id":"speech-audio","name":"Speech","muted":false,"solo":false,"locked":false,"clips":[audio]}]);
    let (status, plan, _) = send(
        &app,
        post_json(
            "/api/speech/clip-plan",
            json!({"project":project,"compositionId":"new-timeline","clipId":"speech-clip"}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{plan}");
    assert_eq!(plan["sourceId"], "speech");
    assert_eq!(plan["sourceStartUs"], 0);
    assert_eq!(plan["sourceEndUs"], 10999999);
    assert_eq!(plan["timelineEndUs"], 7500000);
    let(status,mut aligned,_)=send(&app,post_json("/api/speech/align",json!({"project":project,"compositionId":"new-timeline","clipId":"speech-clip","sourceId":"speech","transcript":transcript}))).await;
    assert_eq!(status, StatusCode::OK, "{aligned}");
    assert_eq!(aligned["clips"].as_array().unwrap().len(), segments.len());
    for (cue, segment) in aligned["clips"].as_array().unwrap().iter().zip(segments) {
        assert_eq!(
            cue["timelineStartTick"].as_u64().unwrap(),
            2000000 + (segment["startUs"].as_u64().unwrap() + 1) / 2
        );
        assert_eq!(
            cue["timelineEndTick"].as_u64().unwrap(),
            2000000 + (segment["endUs"].as_u64().unwrap() + 1) / 2
        );
        assert_eq!(cue["text"], segment["text"]);
    }
    assert_eq!(send(&app,post_json("/api/speech/align",json!({"project":project,"compositionId":"new-timeline","clipId":"speech-clip","sourceId":"different","transcript":transcript}))).await.0,StatusCode::BAD_REQUEST);
    let mut word_request=request.clone();word_request["segmentation"]=json!("word");
    let (status,queued,_)=send(&app,bearer(post_json("/api/speech/jobs",word_request),&owner.token)).await;
    assert_eq!(status,StatusCode::ACCEPTED,"{queued}");
    let word_id=queued["jobId"].as_str().unwrap();
    let word_transcript=tokio::time::timeout(Duration::from_secs(30),async {
        loop {
            let (_,job,_)=send(&app,bearer(get(&format!("/api/jobs/{word_id}")),&owner.token)).await;
            assert_ne!(job["status"],"error","{job}");
            if job["status"]=="done" {break job["result"].clone();}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    assert_eq!(word_transcript["segmentation"],"word");
    let country=word_transcript["segments"].as_array().unwrap().iter().find(|cue|cue["text"]=="country").unwrap();
    assert_eq!(country["startUs"],5650000);assert_eq!(country["endUs"],6410000);
    let (status,word_aligned,_)=send(&app,post_json("/api/speech/align",json!({"project":project,"compositionId":"new-timeline","clipId":"speech-clip","sourceId":"speech","transcript":word_transcript}))).await;
    assert_eq!(status,StatusCode::OK,"{word_aligned}");
    let country=word_aligned["clips"].as_array().unwrap().iter().find(|cue|cue["text"]=="country").unwrap();
    assert_eq!(country["timelineStartTick"],4825000);assert_eq!(country["timelineEndTick"],5205000);
    for (mut aligned,export_start) in [(aligned,2_000_000u64),(word_aligned,4_760_000u64)] {
    aligned["style"]["size"] = json!(20);
    aligned["style"]["background"] = json!(false);
    project["compositions"]["new-timeline"]["content"]["captionTracks"] = json!([aligned]);
    let (status, validation, _) = send(
        &app,
        post_json(
            "/api/compositions/nested-frame",
            json!({"project":project,"timeUs":2000000}),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{validation}");
    let(status,accepted,_)=send(&app,bearer(post_json("/api/compositions/nested-export",json!({"schemaVersion":1,"mode":"video_only","project":project,"compositionId":"new-timeline","startUs":export_start,"endUs":export_start+200000,"renderedOutputs":{}})),&owner.token)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{accepted}");
    let done = poll_terminal(&app, accepted["jobId"].as_str().unwrap()).await;
    assert_eq!(done["status"], "done", "{done}");
    let path = state
        .outputs_dir()
        .join(done["result"]["filename"].as_str().unwrap());
    let decoded = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout.len(), 320 * 180 * 4 * 5);
    assert!(decoded.stdout.chunks_exact(4).any(|pixel| pixel[0] > 0));
    if export_start==4_760_000 {assert_ne!(&decoded.stdout[..320*180*4],&decoded.stdout[320*180*4*4..],"word boundary must change rendered text");}
    for (index, frame) in decoded.stdout.chunks_exact(320 * 180 * 4).enumerate() {
        let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":project,"compositionId":"new-timeline","timeUs":export_start+index as u64*40000,"renderedOutputs":{}})),&owner.token)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            frame,
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref()
        );
    }
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires explicit local Whisper model and public audio fixture"]
async fn running_speech_requeues_after_shutdown_and_cold_server_recovery() {
    use std::os::unix::fs::PermissionsExt;
    let model = std::env::var("VIDEO_KADR_TEST_WHISPER_MODEL").unwrap();
    let audio = std::env::var("VIDEO_KADR_TEST_WHISPER_AUDIO").unwrap();
    for drift in ["unchanged","source","model","permission"] {
    let (state, _directory) = make_state(true, false).await;
    let control = tempfile::tempdir().unwrap();
    let executable = control.path().join("interrupted-whisper");
    let ready = control.path().join("interrupted-whisper.ready");
    std::fs::write(&executable,b"#!/bin/sh\ntrap '' TERM\ntouch \"$0.ready\"\nwhile :; do sleep 1; done\n").unwrap();
    std::fs::set_permissions(&executable,std::fs::Permissions::from_mode(0o700)).unwrap();
    let owned_model=control.path().join("test-model.bin");
    std::fs::copy(&model,&owned_model).unwrap();
    let real_config = video_kadr_backend::services::local_speech::SpeechConfig {model:Some(owned_model.clone()),..Default::default()};
    let state = state.with_speech_config(video_kadr_backend::services::local_speech::SpeechConfig {executable,..real_config.clone()});
    std::fs::copy(&audio,state.sources_dir().join("restart.wav")).unwrap();
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"restart","filename":"restart.wav","url":"/files/sources/restart.wav","mediaType":"audio"}))).await);
    let owner=state.db.register_auth_user("restart-owner","shared test password").await.unwrap().unwrap();
    let other=state.db.register_auth_user("restart-other","shared test password").await.unwrap().unwrap();
    let space=state.db.create_space(&other.user.username,"Restart speech").await.unwrap();
    state.db.create_space_composition_project("Restart",&json!({"schemaVersion":1,"sources":{"restart":{"id":"restart"}}}),&["restart".into()],&other.user.username,&space.id).await.unwrap();
    state.db.set_space_member(&space.id,&other.user.username,&owner.user.username,video_kadr_backend::db::SpaceRole::Viewer).await.unwrap().unwrap();
    let app=router(state.clone());
    let (status,accepted,_)=send(&app,bearer(post_json("/api/speech/jobs",json!({"sourceId":"restart","startUs":0,"endUs":11000000,"language":"en"})),&owner.token)).await;
    assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
    let id=accepted["jobId"].as_str().unwrap();
    let ready_result=tokio::time::timeout(Duration::from_secs(10),async {
        while !ready.exists(){tokio::time::sleep(Duration::from_millis(10)).await;}
    }).await;
    if ready_result.is_err(){panic!("speech worker readiness expired: {:?}",state.get_job(id).await.unwrap())}
    assert_eq!(send(&app,bearer(get(&format!("/api/jobs/{id}")),&owner.token)).await.1["status"],"running");
    state.begin_shutdown();
    assert!(state.wait_for_tasks(Duration::from_secs(10)).await,"shutdown retained speech tasks");
    assert_eq!(std::fs::read_dir(state.storage.join("speech-staging")).unwrap().count(),0);
    match drift {
        "source"=>{
            let entry=state.library.get("restart").await.unwrap();
            let path=state.library.resolve_media_path(&entry).await.unwrap();
            std::fs::write(path,b"replaced owned fixture").unwrap();
        },
        "model"=>{use std::io::Write;std::fs::OpenOptions::new().append(true).open(&owned_model).unwrap().write_all(b"changed").unwrap();},
        "permission"=>{state.db.remove_space_member(&space.id,&other.user.username,&owner.user.username).await.unwrap().unwrap();},
        _=>{},
    }
    // Shutdown preserves durable running work rather than recording a user cancellation.
    let recovered=AppState::new_with_runtime(state.storage.clone(),2,(*state.tools).clone(),Library::load(state.storage.clone()).await,Db::open(&state.storage).await.unwrap(),state.encode_budget.clone(),2).unwrap().with_speech_config(real_config);
    recovered.recover_jobs().await;
    assert_eq!(recovered.get_job(id).await.unwrap().unwrap().status,JobStatus::Pending);
    resume_pending_jobs(&recovered).await;
    let recovered_app=router(recovered.clone());
    let done=tokio::time::timeout(Duration::from_secs(30),async {
        loop {
            let job=serde_json::to_value(recovered.get_job(id).await.unwrap().unwrap()).unwrap();
            if job["status"]=="done"||job["status"]=="error" {break job;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.unwrap();
    assert_eq!(done["id"],id);
    if drift=="unchanged" {
        assert_eq!(done["status"],"done","{done}");
        assert!(done["result"]["segments"].as_array().unwrap().iter().any(|cue|cue["text"].as_str().unwrap().to_lowercase().contains("country")));
        assert_eq!(send(&recovered_app,bearer(get(&format!("/api/jobs/{id}")),&owner.token)).await.1["status"],"done");
    }else {
        assert_eq!(done["status"],"error","{drift}: {done}");
        assert!(done.get("result").is_none());
        assert_eq!(done["errorKind"],if drift=="permission"{"security"}else{"validation"});
    }
    assert_eq!(send(&recovered_app,bearer(get(&format!("/api/jobs/{id}")),&other.token)).await.0,StatusCode::FORBIDDEN);
    let (_,history,_)=send(&recovered_app,bearer(get("/api/speech/jobs"),&owner.token)).await;
    if drift=="permission" {
        assert_eq!(history["jobs"],json!([]));
        assert_eq!(send(&recovered_app,bearer(get(&format!("/api/jobs/{id}")),&owner.token)).await.0,StatusCode::FORBIDDEN);
    } else {assert_eq!(history["jobs"][0]["jobId"],id);}
    assert_eq!(std::fs::read_dir(state.storage.join("speech-staging")).unwrap().count(),0);
    recovered.begin_shutdown();
    assert!(recovered.wait_for_tasks(Duration::from_secs(5)).await);
    }
}
