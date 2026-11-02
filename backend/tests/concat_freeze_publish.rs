mod support;
use serde_json::{json, Value};
use std::{path::Path, process::Command, time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{
    db::CompositionProject,
    domain::composition::{Composition, TransformSpec},
    library::MediaEntry,
    services::{
        concat_freeze_capture::Tools,
        concat_freeze_publish::{self as freeze, Target},
    },
    state::AppState,
};
async fn setup() -> (AppState, tempfile::TempDir, String, CompositionProject) {
    let (state, dir) = support::make_state(true, false).await;
    let owner = state
        .db
        .register_auth_user("freeze-owner", "freeze test password")
        .await
        .unwrap()
        .unwrap()
        .user
        .username;
    let path = state.sources_dir().join("source.nut");
    let result = Command::new("/opt/homebrew/bin/ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=10:d=1",
            "-c:v",
            "ffv1",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":"source","filename":"source.nut","url":"/files/sources/source.nut","mediaType":"video","width":16,"height":16}))).await);
    let mut doc = serde_json::to_value(Composition::new(Default::default())).unwrap();
    doc["sources"] = json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":false}});
    doc["tracks"] = json!([{"kind":"video","id":"v","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false}]}]);
    let saved = state
        .db
        .create_owned_composition_project("Freeze", &doc, &["source".into()], &owner)
        .await
        .unwrap();
    (state, dir, owner, saved)
}
async fn prepare(
    state: &AppState,
    actor: &str,
    saved: &CompositionProject,
) -> freeze::PreparedFreeze {
    freeze::prepare(
        state,
        actor,
        Target {
            project: saved.id.clone(),
            revision: saved.revision,
            composition: None,
            clip: "clip".into(),
            time: 0.1,
        },
        Tools {
            ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe"),
        },
        Duration::from_secs(30),
        &CancellationToken::new(),
    )
    .await
    .unwrap()
    .unwrap()
}
fn image_document(mut doc: Value, media: MediaEntry) -> Value {
    doc["sources"][&media.id] = json!({"id":media.id,"kind":"image","durationTicks":0,"width":media.width.unwrap(),"height":media.height.unwrap(),"hasAudio":false});
    doc["tracks"].as_array_mut().unwrap().insert(0,json!({"kind":"image","id":"still-track","name":"Still","clips":[{"id":"still-clip","sourceId":media.id,"timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}));
    doc
}
fn jpeg_count(path: &Path) -> usize {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                jpeg_count(&entry.path())
            } else {
                usize::from(entry.path().extension().is_some_and(|ext| ext == "jpg"))
            }
        })
        .sum()
}
async fn clean(state: &AppState, saved: &CompositionProject) {
    assert_eq!(state.library.list().await.len(), 1);
    assert_eq!(jpeg_count(&state.sources_dir()), 0);
    assert!(state.db.all_space_source_ids().await.unwrap().is_empty());
    assert!(tokio::fs::read_dir(state.staging_dir())
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .is_none());
    let current = state
        .db
        .get_composition_project_for(&saved.id, "freeze-owner")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.document, saved.document);
}
#[tokio::test]
async fn publication_is_private_persisted_and_commits_one_revision_with_the_media_reference() {
    let (state, _dir, owner, saved) = setup().await;
    let prepared = prepare(&state, &owner, &saved).await;
    let result = freeze::publish(
        &state,
        prepared,
        &CancellationToken::new(),
        |doc, media| async move { Ok(image_document(doc, media)) },
    )
    .await
    .unwrap();
    assert_eq!(result.project.revision, saved.revision + 1);
    assert!(result.project.source_ids.contains(&result.media.id));
    let entry = state.library.get(&result.media.id).await.unwrap();
    let path = state.library.resolve_media_path(&entry).await.unwrap();
    assert!(path.is_file());
    assert_eq!(image::open(path).unwrap().width(), 1280);
    let space = state.db.source_space_id(&entry.id).await.unwrap().unwrap();
    assert!(state.db.is_space_member(&space, &owner).await.unwrap());
    assert!(!state.db.is_space_member(&space, "outsider").await.unwrap());
    let reloaded = video_kadr_backend::library::Library::load(
        state.sources_dir().parent().unwrap().to_path_buf(),
    )
    .await;
    assert!(reloaded.get(&entry.id).await.is_some());
    assert!(tokio::fs::read_dir(state.staging_dir())
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn stale_before_publication_or_during_build_does_not_leave_media_claims() {
    for during in [false, true] {
        let (state, _dir, owner, saved) = setup().await;
        let prepared = prepare(&state, &owner, &saved).await;
        if !during {
            state
                .db
                .update_composition_project_for(
                    &saved.id,
                    &owner,
                    "Changed",
                    &saved.document,
                    &saved.source_ids,
                    saved.revision,
                )
                .await
                .unwrap()
                .unwrap();
        }
        let st = state.clone();
        let old = saved.clone();
        let actor = owner.clone();
        let result = freeze::publish(
            &state,
            prepared,
            &CancellationToken::new(),
            move |doc, media| async move {
                if during {
                    st.db
                        .update_composition_project_for(
                            &old.id,
                            &actor,
                            "Changed",
                            &old.document,
                            &old.source_ids,
                            old.revision,
                        )
                        .await?
                        .unwrap();
                }
                Ok(image_document(doc, media))
            },
        )
        .await;
        assert!(result.is_err());
        clean(&state, &saved).await;
        let current = state
            .db
            .get_composition_project_for(&saved.id, &owner)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current.revision, saved.revision + 1);
        assert_eq!(current.name, "Changed");
    }
}
#[tokio::test]
async fn failed_build_source_mutation_or_revoked_editor_rolls_back_only_the_new_still() {
    for mode in ["build", "source", "role", "metadata", "jpeg"] {
        let (state, _dir, owner, saved) = setup().await;
        let prepared = prepare(&state, &owner, &saved).await;
        let st = state.clone();
        let project = saved.id.clone();
        let actor = owner.clone();
        let result=freeze::publish(&state,prepared,&CancellationToken::new(),move|doc,media|async move{
            if mode=="build"{anyhow::bail!("native edit refused")}
            if mode=="source"{tokio::fs::write(st.sources_dir().join("source.nut"),b"changed original").await?;}
            if mode=="jpeg"{let file=st.library.resolve_media_path(&media).await?;tokio::fs::write(file,b"corrupt JPEG").await?;}
            if mode=="role"{let pool=sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(st.sources_dir().parent().unwrap().join("app.db"))).await?;sqlx::query("UPDATE composition_project_members SET role='viewer' WHERE project_id=? AND actor=?").bind(project).bind(actor).execute(&pool).await?;}
            let id=media.id.clone();let mut changed=image_document(doc,media);if mode=="metadata"{changed["sources"][id]["width"]=json!(16);}
            Ok(changed)
        }).await;
        assert!(result.is_err(), "{mode}");
        clean(&state, &saved).await;
        assert!(state.sources_dir().join("source.nut").is_file());
        if mode == "source" {
            assert_eq!(
                tokio::fs::read(state.sources_dir().join("source.nut"))
                    .await
                    .unwrap(),
                b"changed original"
            );
        }
    }
}
#[tokio::test]
async fn cancellation_and_library_save_failure_do_not_commit_the_project() {
    for broken_library in [false, true] {
        let (state, dir, owner, saved) = setup().await;
        let prepared = prepare(&state, &owner, &saved).await;
        if broken_library {
            tokio::fs::create_dir(dir.path().join("library.json.tmp"))
                .await
                .unwrap();
        }
        let cancel = CancellationToken::new();
        let cancelled = cancel.clone();
        let result = freeze::publish(&state, prepared, &cancel, move |doc, media| async move {
            cancelled.cancel();
            Ok(image_document(doc, media))
        })
        .await;
        assert!(result.is_err());
        clean(&state, &saved).await;
    }
}
#[tokio::test]
async fn preparation_requires_an_editor_and_noop_creates_nothing() {
    let (state, _dir, owner, saved) = setup().await;
    for (actor, time) in [("outsider", 0.1), (owner.as_str(), 0.)] {
        let result = freeze::prepare(
            &state,
            actor,
            Target {
                project: saved.id.clone(),
                revision: saved.revision,
                composition: None,
                clip: "clip".into(),
                time,
            },
            Tools {
                ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
                ffprobe: Path::new("/opt/homebrew/bin/ffprobe"),
            },
            Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await;
        if actor == "outsider" {
            assert!(result.is_err())
        } else {
            assert!(result.unwrap().is_none())
        }
    }
    clean(&state, &saved).await;
}

#[tokio::test]
async fn nested_projects_keep_the_owner_and_unrelated_nodes_when_publishing_a_still() {
    let (state, _dir, owner, saved) = setup().await;
    let doc = json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":saved.document,"instances":[]},"other":{"content":Composition::new(Default::default()),"instances":[]}}});
    let nested = state
        .db
        .create_owned_composition_project("Nested freeze", &doc, &saved.source_ids, &owner)
        .await
        .unwrap();
    let prepared = freeze::prepare(
        &state,
        &owner,
        Target {
            project: nested.id.clone(),
            revision: nested.revision,
            composition: Some("root".into()),
            clip: "clip".into(),
            time: 0.1,
        },
        Tools {
            ffmpeg: Path::new("/opt/homebrew/bin/ffmpeg"),
            ffprobe: Path::new("/opt/homebrew/bin/ffprobe"),
        },
        Duration::from_secs(30),
        &CancellationToken::new(),
    )
    .await
    .unwrap()
    .unwrap();
    let result = freeze::publish(
        &state,
        prepared,
        &CancellationToken::new(),
        |mut doc, media| async move {
            let content = doc["compositions"]["root"]["content"].clone();
            doc["compositions"]["root"]["content"] = image_document(content, media);
            Ok(doc)
        },
    )
    .await
    .unwrap();
    assert_eq!(result.project.id, nested.id);
    assert_eq!(result.project.revision, nested.revision + 1);
    assert_eq!(
        result.project.document["compositions"]["other"],
        nested.document["compositions"]["other"]
    );
    assert_eq!(
        state
            .db
            .composition_source_reference_count(&result.media.id)
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn concurrent_publications_keep_only_the_winning_still() {
    let (state, _dir, owner, saved) = setup().await;
    let first = prepare(&state, &owner, &saved).await;
    let second = prepare(&state, &owner, &saved).await;
    let token = CancellationToken::new();
    let (a, b) = tokio::join!(
        freeze::publish(&state, first, &token, |doc, media| async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(image_document(doc, media))
        }),
        freeze::publish(&state, second, &token, |doc, media| async move {
            Ok(image_document(doc, media))
        })
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let winner = a.or(b).unwrap();
    let current = state
        .db
        .get_composition_project_for(&saved.id, &owner)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.revision, saved.revision + 1);
    assert!(current.source_ids.contains(&winner.media.id));
    assert_eq!(state.library.list().await.len(), 2);
    assert_eq!(jpeg_count(&state.sources_dir()), 1);
    let claims = state.db.all_space_source_ids().await.unwrap();
    assert_eq!(claims.len(), 1);
    assert!(claims.contains(&winner.media.id));
}
#[tokio::test]
async fn rollback_never_deletes_a_still_already_referenced_by_a_committed_editor() {
    let (state, _dir, owner, saved) = setup().await;
    let prepared = prepare(&state, &owner, &saved).await;
    let st = state.clone();
    let old = saved.clone();
    let actor = owner.clone();
    let result = freeze::publish(
        &state,
        prepared,
        &CancellationToken::new(),
        move |doc, media| async move {
            let changed = image_document(doc, media);
            let refs: Vec<String> = changed["sources"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect();
            st.db
                .update_composition_project_for(
                    &old.id,
                    &actor,
                    "Concurrent editor",
                    &changed,
                    &refs,
                    old.revision,
                )
                .await?
                .unwrap();
            Ok(changed)
        },
    )
    .await;
    assert!(result.is_err());
    let current = state
        .db
        .get_composition_project_for(&saved.id, &owner)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.name, "Concurrent editor");
    assert_eq!(current.source_ids.len(), 2);
    let still = current
        .source_ids
        .iter()
        .find(|id| id.as_str() != "source")
        .unwrap();
    let entry = state.library.get(still).await.unwrap();
    assert!(state
        .library
        .resolve_media_path(&entry)
        .await
        .unwrap()
        .is_file());
    assert_eq!(jpeg_count(&state.sources_dir()), 1);
    assert!(state.db.source_space_id(still).await.unwrap().is_some());
}

#[tokio::test]
async fn retired_still_cannot_be_attached_after_rollback_claim_is_released() {
    let (state, _dir, owner, saved) = setup().await;
    let id = uuid::Uuid::new_v4().to_string();
    state.db.claim_project_freeze_media(&saved.id, &owner, &id, saved.revision).await.unwrap();
    assert!(state.db.retire_unused_freeze_media(&id).await.unwrap());
    state.db.release_space_source_claim(&id).await.unwrap();
    let media = MediaEntry::from_result("source", &json!({"id":id,"filename":format!("{id}.jpg"),"mediaType":"image","width":16,"height":16}));
    let document = image_document(saved.document.clone(), media);
    let mut refs = vec!["source".to_string(), id.clone()];
    refs.sort();
    let result = state.db.update_composition_project_for(&saved.id, &owner, &saved.name, &document, &refs, saved.revision).await;
    let error = result.unwrap_err();
    assert!(error.to_string().contains("retired"), "{error:#}");
    assert!(state.db.create_composition_project("Delayed copy", &document, &refs).await.is_err());
    let current = state.db.get_composition_project_for(&saved.id, &owner).await.unwrap().unwrap();
    assert_eq!(current.revision, saved.revision);
    assert_eq!(current.source_ids, saved.source_ids);
    assert!(!state.db.retire_unused_freeze_media("source").await.unwrap());
}

#[tokio::test]
async fn startup_recovers_aborted_publications_and_preserves_committed_stills() {
    for commit in [false, true] {
        let (state, _dir, owner, saved) = setup().await;
        let prepared = prepare(&state, &owner, &saved).await;
        let publisher = state.clone();
        let editor = state.clone();
        let old = saved.clone();
        let (ready, observed) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            freeze::publish(&publisher, prepared, &CancellationToken::new(), move |doc, media| async move {
                let id = media.id.clone();
                let changed = image_document(doc, media);
                if commit {
                    let mut refs = vec!["source".to_string(), id.clone()]; refs.sort();
                    editor.db.update_composition_project_for(&old.id, &owner, &old.name, &changed, &refs, old.revision).await?.unwrap();
                }
                ready.send(id).unwrap();
                std::future::pending::<anyhow::Result<Value>>().await
            }).await
        });
        let id = tokio::time::timeout(Duration::from_secs(10), observed).await.unwrap().unwrap();
        task.abort();
        assert!(matches!(task.await, Err(error) if error.is_cancelled()));
        // Reload both stores, rather than recovering from the publisher's in-memory library.
        let mut restarted = state.clone();
        restarted.library = std::sync::Arc::new(video_kadr_backend::library::Library::load(state.storage.clone()).await);
        restarted.db = std::sync::Arc::new(video_kadr_backend::db::Db::open(&state.storage).await.unwrap());
        if !commit {
            let blocked = restarted.storage.join("library.json.tmp");
            tokio::fs::create_dir(&blocked).await.unwrap();
            assert!(freeze::recover_at_startup(&restarted).await.is_err());
            assert!(restarted.library.get(&id).await.is_some());
            assert_eq!(jpeg_count(&restarted.sources_dir()), 1);
            tokio::fs::remove_dir(&blocked).await.unwrap();
        }
        assert_eq!(freeze::recover_at_startup(&restarted).await.unwrap(), 1);
        assert_eq!(freeze::recover_at_startup(&restarted).await.unwrap(), 0);
        if commit {
            let entry = restarted.library.get(&id).await.unwrap();
            assert!(restarted.library.resolve_media_path(&entry).await.unwrap().is_file());
            assert_eq!(restarted.db.composition_source_reference_count(&id).await.unwrap(), 1);
        } else {
            clean(&restarted, &saved).await;
        }
    }
}


#[tokio::test]
async fn startup_recovers_reservations_and_files_before_library_registration() {
    for phase in 0..3 {
        let (state, _dir, owner, saved) = setup().await;
        let id = uuid::Uuid::new_v4().to_string();
        let space = state.db.claim_project_freeze_media(&saved.id, &owner, &id, saved.revision).await.unwrap();
        if phase >= 1 {
            tokio::fs::write(state.staging_dir().join(format!("freeze-{id}.jpg.tmp")), b"partial").await.unwrap();
        }
        if phase >= 2 {
            let root = state.sources_dir().join("spaces").join(space);
            tokio::fs::create_dir_all(&root).await.unwrap();
            tokio::fs::write(root.join(format!("{id}.jpg")), b"unregistered").await.unwrap();
        }
        assert_eq!(freeze::recover_at_startup(&state).await.unwrap(), 1);
        clean(&state, &saved).await;
        assert_eq!(freeze::recover_at_startup(&state).await.unwrap(), 0);
    }
}

#[tokio::test]
async fn native_publication_saves_the_real_head_still_tail_splice_flat_and_nested() {
    for nested in [false, true] {
        let (state, _dir, owner, mut saved) = setup().await;
        let converted=imported_document(&saved);
        saved=state.db.create_owned_composition_project("Native source freeze",&converted,&saved.source_ids,&owner).await.unwrap();
        let original_track=saved.document["tracks"][0]["id"].clone();
        if nested {
            let doc=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":saved.document,"instances":[],"durationUs":1000000},"other":{"content":Composition::new(Default::default()),"instances":[]}}});
            saved=state.db.create_owned_composition_project("Native nested freeze",&doc,&saved.source_ids,&owner).await.unwrap();
        }
        let prepared=freeze::prepare(&state,&owner,Target{project:saved.id.clone(),revision:saved.revision,composition:nested.then(||"root".into()),clip:"clip".into(),time:0.25},Tools{ffmpeg:Path::new("/opt/homebrew/bin/ffmpeg"),ffprobe:Path::new("/opt/homebrew/bin/ffprobe")},Duration::from_secs(30),&CancellationToken::new()).await.unwrap().unwrap();
        let result=freeze::publish_native(&state,prepared,&CancellationToken::new()).await.unwrap();
        assert_eq!(result.project.revision,saved.revision+1);
        let document=&result.project.document;
        let content=if nested {
            assert_eq!(document["compositions"]["other"],saved.document["compositions"]["other"]);
            assert_eq!(document["compositions"]["root"]["durationUs"],2000000);
            &document["compositions"]["root"]["content"]
        } else { document };
        let tracks=content["tracks"].as_array().unwrap();
        let video=tracks.iter().find(|track|track["kind"]=="video").unwrap();
        let image=tracks.iter().find(|track|track["kind"]=="image").unwrap();
        assert_eq!(video["id"],original_track);assert_eq!(video["clips"].as_array().unwrap().len(),2);
        assert_eq!(video["clips"][0]["id"],"clip");
        assert_eq!(video["clips"][0]["placement"]["sourceOutTick"],250000);
        assert_eq!(video["clips"][1]["placement"]["timelineStartTick"],1250000);
        assert_eq!(image["clips"][0]["sourceId"],result.media.id);
        assert_eq!(image["clips"][0]["durationTicks"],1000000);
        assert_eq!(image["clips"][0]["timelineStartTick"],250000);
        assert!(state.library.resolve_media_path(&result.media).await.unwrap().is_file());
        let loaded=state.db.get_composition_project_for(&saved.id,&owner).await.unwrap().unwrap();
        assert_eq!(loaded.document,*document);
        assert_eq!(freeze::recover_at_startup(&state).await.unwrap(),0);
    }
}

#[tokio::test]
async fn native_conversion_refusal_rolls_back_the_captured_still() {
    let (state,_dir,owner,saved)=setup().await;
    let mut document=saved.document.clone();document["tracks"][0]["clips"][0]["transform"]["x"]=json!({"mode":"keyframes","track":{"timeBase":24,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":24,"value":100.}]}});
    document["tracks"][0]["clips"][0]["transform"]["position"]=json!({"mode":"xy","x":{"mode":"constant","value":0.},"y":{"mode":"constant","value":0.}});
    let saved=state.db.create_owned_composition_project("Unsupported position refusal",&document,&saved.source_ids,&owner).await.unwrap();
    let prepared=prepare(&state,&owner,&saved).await;
    let error=match freeze::publish_native(&state,prepared,&CancellationToken::new()).await { Ok(_)=>panic!("unqualified native transform admitted"),Err(error)=>error };
    assert!(format!("{error:#}").contains("Native anchor/path cannot be represented"),"{error:#}");
    clean(&state,&saved).await;
    assert_eq!(freeze::recover_at_startup(&state).await.unwrap(),0);
}

fn imported_document(saved: &CompositionProject) -> Value {
        let source_doc=json!({"version":1,"activeTimelineId":"t","media":[{"id":"source","path":"/unopened/source.nut","kind":"video","duration":1.,"width":16,"height":16,"hasAudio":false}],"timelines":[{"id":"t","name":"Native freeze","video":{"width":1920,"height":1080,"rateNum":25,"rateDen":1},"tracks":[{"id":"v","name":"Video"}],"clips":[{"id":"clip","trackId":"v","mediaId":"source","kind":"video","start":0.,"duration":1.,"sourceStart":0.,"speed":1.}]}]});
        let imported=video_kadr_backend::domain::concat_project_import::import(serde_json::from_value(json!({"schemaVersion":1,"document":source_doc,"mediaBindings":saved.document["sources"]})).unwrap()).unwrap();
        assert_eq!(imported.status,"converted","{:?}",imported.diagnostics);
        let project=imported.project.unwrap();
        let converted=serde_json::to_value(&project.compositions[&project.root].content).unwrap();
    converted
}

#[tokio::test]
async fn freeze_http_enforces_auth_revision_admission_and_commits_the_real_splice() {
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
    use tower::ServiceExt;
    let (state,_dir,owner,saved)=setup().await;
    let document=imported_document(&saved);
    let saved=state.db.create_owned_composition_project("HTTP freeze",&document,&saved.source_ids,&owner).await.unwrap();
    let token=state.db.login_auth_user(&owner,"freeze test password").await.unwrap().unwrap().token;
    let other=state.db.register_auth_user("freeze-http-other","freeze test password").await.unwrap().unwrap();
    let app=support::router(state.clone());
    let path=format!("/api/composition-projects/{}/freeze-frame",saved.id);
    let body=json!({"revision":saved.revision,"clipId":"clip","time":0.25});
    let request=|token:Option<&str>,body:&Value| {
        let mut req=Request::builder().method("POST").uri(&path).header("content-type","application/json");
        if let Some(token)=token { req=req.header("authorization",format!("Bearer {token}")); }
        req.body(Body::from(serde_json::to_vec(body).unwrap())).unwrap()
    };
    assert_eq!(app.clone().oneshot(request(None,&body)).await.unwrap().status(),StatusCode::UNAUTHORIZED);
    assert_eq!(app.clone().oneshot(request(Some(&other.token),&body)).await.unwrap().status(),StatusCode::FORBIDDEN);
    state.db.set_review_member(&saved.id,&owner,&other.user.username,video_kadr_backend::domain::project_collaboration::ProjectRole::Viewer).await.unwrap().unwrap();
    assert_eq!(app.clone().oneshot(request(Some(&other.token),&body)).await.unwrap().status(),StatusCode::FORBIDDEN);
    let mut wrong_scope=body.clone();wrong_scope["compositionId"]=json!("root");
    assert_eq!(app.clone().oneshot(request(Some(&token),&wrong_scope)).await.unwrap().status(),StatusCode::BAD_REQUEST);
    let mut stale=body.clone();stale["revision"]=json!(saved.revision+1);
    assert_eq!(app.clone().oneshot(request(Some(&token),&stale)).await.unwrap().status(),StatusCode::CONFLICT);
    let mut oversized=body.clone();oversized["clipId"]=json!("a".repeat(9000));
    assert_eq!(app.clone().oneshot(request(Some(&token),&oversized)).await.unwrap().status(),StatusCode::PAYLOAD_TOO_LARGE);
    let mut forged=body.clone();forged["sourcePath"]=json!("/unopened/forged.mov");
    assert_eq!(app.clone().oneshot(request(Some(&token),&forged)).await.unwrap().status(),StatusCode::BAD_REQUEST);
    let mut noop=body.clone();noop["time"]=json!(0.);
    let response=app.clone().oneshot(request(Some(&token),&noop)).await.unwrap();
    assert_eq!(response.status(),StatusCode::OK);
    let no_change:Value=serde_json::from_slice(&to_bytes(response.into_body(),1024).await.unwrap()).unwrap();assert_eq!(no_change,json!({"changed":false}));
    assert_eq!(jpeg_count(&state.sources_dir()),0);
    let mut slots=vec![];while let Some(slot)=state.try_acquire_upload_slot(){slots.push(slot)}
    assert_eq!(app.clone().oneshot(request(Some(&token),&body)).await.unwrap().status(),StatusCode::TOO_MANY_REQUESTS);drop(slots);
    state.db.set_review_member(&saved.id,&owner,&other.user.username,video_kadr_backend::domain::project_collaboration::ProjectRole::Editor).await.unwrap().unwrap();
    let response=app.clone().oneshot(request(Some(&other.token),&body)).await.unwrap();
    let status=response.status();
    let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),4*1024*1024).await.unwrap()).unwrap();
    assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result["changed"],true);assert_eq!(result["project"]["revision"],saved.revision+1);
    assert_eq!(result["project"]["document"]["tracks"][0]["kind"],"image");
    assert_eq!(result["project"]["document"]["tracks"][1]["clips"].as_array().unwrap().len(),2);
    assert_eq!(jpeg_count(&state.sources_dir()),1);
    assert_eq!(app.oneshot(request(Some(&token),&body)).await.unwrap().status(),StatusCode::CONFLICT);
    assert_eq!(jpeg_count(&state.sources_dir()),1);
    let loaded=state.db.get_composition_project_for(&saved.id,&owner).await.unwrap().unwrap();
    assert_eq!(loaded.document,result["project"]["document"]);
    let nested_document=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":document,"instances":[],"durationUs":1000000},"other":{"content":Composition::new(Default::default()),"instances":[]}}});
    let nested=state.db.create_owned_composition_project("HTTP nested freeze",&nested_document,&saved.source_ids,&owner).await.unwrap();
    let nested_request=Request::builder().method("POST").uri(format!("/api/composition-projects/{}/freeze-frame",nested.id)).header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(serde_json::to_vec(&json!({"revision":nested.revision,"compositionId":"root","clipId":"clip","time":0.25})).unwrap())).unwrap();
    let response=support::router(state.clone()).oneshot(nested_request).await.unwrap();let status=response.status();
    let result:Value=serde_json::from_slice(&to_bytes(response.into_body(),4*1024*1024).await.unwrap()).unwrap();assert_eq!(status,StatusCode::OK,"{result}");
    assert_eq!(result["project"]["document"]["compositions"]["root"]["durationUs"],2000000);
    assert_eq!(result["project"]["document"]["compositions"]["other"],nested_document["compositions"]["other"]);
    assert_eq!(jpeg_count(&state.sources_dir()),2);
    assert_eq!(freeze::recover_at_startup(&state).await.unwrap(),0);
}


#[tokio::test]
async fn ordinary_native_video_publishes_without_changing_picture_or_audio_ownership(){
    let(state,_dir,owner,saved)=setup().await;
    let original=&saved.document["tracks"][0]["clips"][0];
    let prepared=prepare(&state,&owner,&saved).await;
    let result=freeze::publish_native(&state,prepared,&CancellationToken::new()).await.unwrap();
    let doc=&result.project.document;let video=&doc["tracks"][1];let image=&doc["tracks"][0]["clips"][0];
    assert_eq!(result.project.revision,saved.revision+1);
    assert_eq!(video["clips"].as_array().unwrap().len(),2);
    for clip in video["clips"].as_array().unwrap(){assert_eq!(clip["transform"],original["transform"]);assert_eq!(clip["independentAudio"],original["independentAudio"]);}
    let mut still_transform=original["transform"].clone();
    still_transform["scaleX"]["value"]=json!(16./result.media.width.unwrap() as f64);
    still_transform["scaleY"]["value"]=json!(16./result.media.height.unwrap() as f64);
    assert_eq!(image["transform"],still_transform);
    assert_eq!(image["timelineStartTick"],100000);assert_eq!(image["durationTicks"],1000000);
    assert_eq!(video["clips"][1]["placement"]["timelineStartTick"],1100000);
    assert_eq!(video["clips"][1]["placement"]["sourceInTick"],100000);
    assert!(state.library.resolve_media_path(&result.media).await.unwrap().is_file());
}

#[tokio::test]
async fn keyed_native_pose_is_published_with_rebased_tail_and_saved_document(){
    let(state,_dir,owner,saved)=setup().await;let mut document=saved.document.clone();
    document["tracks"][0]["clips"][0]["transform"]["x"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"ease_in_out","keyframes":[{"tick":0,"value":0.},{"tick":1000000,"value":100.}]}});
    let saved=state.db.create_owned_composition_project("Keyed native freeze",&document,&saved.source_ids,&owner).await.unwrap();
    let prepared=prepare(&state,&owner,&saved).await;let result=freeze::publish_native(&state,prepared,&CancellationToken::new()).await.unwrap();
    assert_eq!(result.project.revision,saved.revision+1);
    let tail=&result.project.document["tracks"][1]["clips"][1];
    assert_eq!(tail["transform"]["x"]["track"]["keyframes"][0]["tick"],-100000);
    assert_eq!(tail["transform"]["x"]["track"]["keyframes"][1]["tick"],900000);
    assert_eq!(result.project.document["tracks"][1]["clips"][0]["transform"]["x"],document["tracks"][0]["clips"][0]["transform"]["x"]);
    let stored=state.db.get_composition_project_for(&saved.id,&owner).await.unwrap().unwrap();assert_eq!(stored.document,result.project.document);
    assert!(state.library.resolve_media_path(&result.media).await.unwrap().is_file());
}


#[tokio::test]
async fn fractional_native_keys_are_published_with_their_clock_and_reloaded() {
    let (state,_dir,owner,saved)=setup().await;
    let mut document=saved.document.clone();
    document["tracks"][0]["clips"][0]["transform"]["x"]=json!({"mode":"keyframes","track":{"timeBase":24,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":24,"value":100.}]}});
    let saved=state.db.create_owned_composition_project("Fractional native freeze",&document,&saved.source_ids,&owner).await.unwrap();
    let prepared=prepare(&state,&owner,&saved).await;
    let result=freeze::publish_native(&state,prepared,&CancellationToken::new()).await.unwrap();
    let video=result.project.document["tracks"].as_array().unwrap().iter().find(|track|track["kind"]=="video").unwrap();
    let tail=&video["clips"][1];
    assert_eq!(tail["transform"]["x"]["track"]["sampleClock"],json!({"rateNum":1,"rateDen":1,"offsetNum":100000}));
    assert_eq!(tail["transform"]["x"]["track"]["keyframes"],document["tracks"][0]["clips"][0]["transform"]["x"]["track"]["keyframes"]);
    let loaded=state.db.get_composition_project_for(&saved.id,&owner).await.unwrap().unwrap();
    assert_eq!(loaded.document,result.project.document);
    let restored:Composition=serde_json::from_value(loaded.document).unwrap();
    restored.validate_native_surface().unwrap();
    assert_eq!(freeze::recover_at_startup(&state).await.unwrap(),0);
}
