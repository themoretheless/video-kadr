use serde_json::json;
use sha2::{Digest, Sha256};
use video_kadr_backend::{
    project_archive::*,
    services::concat_video_analysis::{portable, PublishedCache, RunCache},
};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[tokio::test]
async fn cache_records_roundtrip_flat_sources() {
    Box::pin(roundtrip(false)).await;
}
#[tokio::test]
async fn cache_records_roundtrip_logical_sources() {
    Box::pin(roundtrip(true)).await;
}
async fn roundtrip(logical: bool) {
    let root = tempfile::tempdir().unwrap();
    let stages = root.path().join("stages");
    let generation = root.path().join("generation");
    std::fs::create_dir(&stages).unwrap();
    std::fs::create_dir(&generation).unwrap();
    let encoded = b"procedural encoded identity; no codec fixture";
    let source_path = root.path().join("source");
    std::fs::write(&source_path, encoded).unwrap();
    let video = hash(encoded);
    let model = video_kadr_backend::domain::cutout::SELFIE_MODEL_HASH;
    let runtime = "b".repeat(64);
    let plane = vec![201u8; 256 * 256];
    let mask_path = generation.join("0.masks");
    std::fs::write(&mask_path, &plane).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&mask_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let cache_manifest=serde_json::to_vec(&json!({"schemaVersion":1,"videoHash":video,"modelHash":model,"runtimeHash":runtime,"runs":[{"startMillis":0,"metadata":{"schemaVersion":10,"width":2,"height":2,"maskWidth":256,"maskHeight":256,"maskBytes":plane.len(),"blankFrames":0,"maskHash":hash(&plane),"frames":1,"bytes":16,"sourceHash":"c".repeat(64),"outputHash":"d".repeat(64),"modelHash":model,"runtimeHash":runtime}}]})).unwrap();
    std::fs::write(generation.join("manifest.json"), &cache_manifest).unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut cache = RunCache::restore(
        &PublishedCache {
            directory: generation,
            manifest_hash: hash(&cache_manifest),
            video_hash: video.clone(),
            model_hash: model.into(),
            runtime_hash: runtime.clone(),
        },
        1024 * 1024,
        &cancel,
    )
    .await
    .unwrap();
    let identity = cache.portable_identity().unwrap();
    let cache_path = root.path().join("cache");
    cache
        .write_portable(
            tokio::fs::File::create(&cache_path).await.unwrap(),
            1024 * 1024,
            &cancel,
        )
        .await
        .unwrap();
    let bytes = std::fs::read(&cache_path).unwrap();
    let id = if logical { "slot" } else { "raw" };
    let descriptor = json!({"id":id,"kind":"video","durationTicks":200000,"width":2,"height":2,"hasAudio":false});
    let mut content = serde_json::to_value(
        video_kadr_backend::domain::composition::Composition::new(Default::default()),
    )
    .unwrap();
    content["sources"] = json!({id:descriptor});
    content["tracks"] = json!([{"id":"videos","kind":"video","name":"Videos","clips":[{"id":"clip","sourceId":id,"placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":100000,"speed":1},"transform":video_kadr_backend::domain::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","concatCutout":{"subject":"person","sourceHash":video,"modelHash":model,"runtimeHash":runtime}}]}]);
    let document = if logical {
        json!({"schemaVersion":1,"root":"main","compositions":{"main":{"content":content,"instances":[]}},"projectMedia":{"slot":{"source":descriptor,"librarySourceId":"raw","name":"Camera","placeholder":false}}})
    } else {
        content
    };
    let mut manifest = ProjectArchiveManifest::new(
        ArchivedProject {
            schema_version: 2,
            mode: "composition".into(),
            name: "Portable video".into(),
            document,
        },
        vec![ArchivedSource {
            source_id: "raw".into(),
            filename: "camera.mkv".into(),
            media_type: "video".into(),
            title: None,
            original_filename: None,
            duration: Some(0.2),
            width: Some(2),
            height: Some(2),
            favorite: false,
            tags: vec![],
            byte_length: encoded.len() as u64,
            sha256: video,
        }],
    );
    manifest.schema_version = ARCHIVE_VIDEO_FORMAT_VERSION;
    manifest.video_caches = vec![ArchivedVideoCache {
        key: "mask-store".into(),
        source_id: id.into(),
        identity: identity.clone(),
        byte_length: bytes.len() as u64,
        sha256: hash(&bytes),
    }];
    let media = vec![ArchiveMediaInput {
        source_id: "raw".into(),
        filename: "camera.mkv".into(),
        path: source_path,
    }];
    let inputs = vec![ArchiveVideoCacheInput {
        key: "mask-store".into(),
        path: cache_path,
    }];
    let archive = root.path().join("project.veproj");
    write_project_archive_with_video_caches(
        tokio::fs::File::create(&archive).await.unwrap(),
        &manifest,
        &media,
        &inputs,
    )
    .await
    .unwrap();
    let encoded_archive = std::fs::read(&archive).unwrap();
    assert_eq!(&encoded_archive[8..10], &2u16.to_be_bytes());
    let parsed = read_project_archive(encoded_archive.as_slice(), &stages)
        .await
        .unwrap();
    assert_eq!(parsed.manifest, manifest);
    assert_eq!(std::fs::read(parsed.media[0].path()).unwrap(), encoded);
    assert_eq!(parsed.video_caches[0].key(), "mask-store");
    let mut restored = portable::restore(
        tokio::fs::File::open(parsed.video_caches[0].path())
            .await
            .unwrap(),
        &identity,
        root.path(),
        1024 * 1024,
        &cancel,
    )
    .await
    .unwrap();
    assert!(restored
        .cache()
        .automatic(0, &cancel)
        .unwrap()
        .unwrap()
        .probabilities
        .iter()
        .all(|v| *v == 201));
    drop(restored);
    drop(parsed);
    assert_eq!(std::fs::read_dir(&stages).unwrap().count(), 0);
    for mode in 0..4 {
        let mut changed = encoded_archive.clone();
        match mode {
            0 => {
                changed.pop();
            }
            1 => changed.push(0),
            2 => *changed.last_mut().unwrap() ^= 1,
            _ => {
                let marker = changed.len() - bytes.len() - 46 - "mask-store".len();
                changed[marker] ^= 1;
            }
        }
        assert!(
            read_project_archive(changed.as_slice(), &stages)
                .await
                .is_err(),
            "mode {mode}"
        );
        assert_eq!(std::fs::read_dir(&stages).unwrap().count(), 0);
    }
    for mode in 0..5 {
        let mut bad = manifest.clone();
        match mode {
            0 => {
                bad.video_caches.push(bad.video_caches[0].clone());
            }
            1 => bad.video_caches[0].identity.video_hash = "0".repeat(64),
            2 => bad.video_caches[0].byte_length = MAX_ARCHIVE_ENTRY_BYTES,
            3 => bad.video_caches[0].source_id = "missing".into(),
            _ => bad.schema_version = 1,
        };
        assert!(
            write_project_archive_with_video_caches(tokio::io::sink(), &bad, &media, &inputs)
                .await
                .is_err(),
            "manifest mode {mode}"
        );
    }
    // Old archives retain their exact header and omit the optional cache field.
    manifest.schema_version = 1;
    manifest.video_caches.clear();
    write_project_archive(
        tokio::fs::File::create(&archive).await.unwrap(),
        &manifest,
        &media,
    )
    .await
    .unwrap();
    let old = std::fs::read(archive).unwrap();
    assert_eq!(&old[8..10], &1u16.to_be_bytes());
    assert!(!serde_json::to_string(&manifest)
        .unwrap()
        .contains("videoCaches"));
    drop(read_project_archive(old.as_slice(), &stages).await.unwrap());
    assert_eq!(std::fs::read_dir(&stages).unwrap().count(), 0);
}
