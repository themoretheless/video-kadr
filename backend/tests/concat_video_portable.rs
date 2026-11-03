use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::services::concat_video_analysis::{portable, PublishedCache, RunCache};
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
#[tokio::test]
async fn standalone_transport_preserves_nonblank_planes_and_revalidates_export_reads() {
    let root = tempfile::tempdir().unwrap();
    let generation = root
        .path()
        .join("concat-video-cache-00000000-0000-0000-0000-000000000001");
    let stages = root.path().join("import");
    std::fs::create_dir(&generation).unwrap();
    std::fs::create_dir(&stages).unwrap();
    let model = video_kadr_backend::domain::cutout::SELFIE_MODEL_HASH;
    let video = hash(b"registered encoded source");
    let runtime = "b".repeat(64);
    let mut runs = vec![];
    for (index, value) in [17u8, 201].into_iter().enumerate() {
        let plane = vec![value; 256 * 256];
        write(&generation.join(format!("{index}.masks")), &plane);
        runs.push(json!({"startMillis":index*100,"metadata":{"schemaVersion":10,"width":2,"height":2,"maskWidth":256,"maskHeight":256,"maskBytes":plane.len(),"blankFrames":0,"maskHash":hash(&plane),"frames":1,"bytes":16,"sourceHash":hash(&[index as u8;16]),"modelHash":model,"runtimeHash":runtime,"outputHash":hash(&[value;16])}}));
    }
    let manifest = serde_json::to_vec(&json!({"schemaVersion":1,"videoHash":video,"modelHash":model,"runtimeHash":runtime,"runs":runs})).unwrap();
    write(&generation.join("manifest.json"), &manifest);
    let receipt = PublishedCache {
        directory: generation.clone(),
        manifest_hash: hash(&manifest),
        video_hash: video,
        model_hash: model.into(),
        runtime_hash: runtime,
    };
    let cancel = CancellationToken::new();
    let budget = 1024 * 1024;
    let mut cache = RunCache::restore(&receipt, budget, &cancel).await.unwrap();
    let identity = cache.portable_identity().unwrap();
    let payload = root.path().join("portable.bin");
    cache
        .write_portable(
            tokio::fs::File::create(&payload).await.unwrap(),
            budget,
            &cancel,
        )
        .await
        .unwrap();
    let bytes = std::fs::read(&payload).unwrap();
    let mut restored = portable::restore(
        bytes.as_slice(),
        &identity,
        &stages,
        bytes.len() as u64,
        &cancel,
    )
    .await
    .unwrap();
    for (time, value) in [(0, 17), (100, 201)] {
        let mask = restored.cache().automatic(time, &cancel).unwrap().unwrap();
        assert!(mask.probabilities.iter().all(|p| *p == value));
    }
    let second = root.path().join("second.bin");
    restored
        .cache()
        .write_portable(
            tokio::fs::File::create(&second).await.unwrap(),
            budget,
            &cancel,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(second).unwrap(), bytes);
    drop(restored);
    assert_eq!(std::fs::read_dir(&stages).unwrap().count(), 0);
    let mut changed = vec![17; 256 * 256];
    changed[0] = 18;
    write(&generation.join("0.masks"), &changed);
    assert!(cache
        .write_portable(tokio::io::sink(), budget, &cancel)
        .await
        .is_err());
}
#[tokio::test]
async fn empty_cache_does_not_produce_a_portable_mask_store() {
    let mut cache = RunCache::new_for_model(
        "a".repeat(64),
        "b".repeat(64),
        video_kadr_backend::domain::cutout::SELFIE_MODEL_HASH.into(),
    )
    .unwrap();
    assert!(cache
        .write_portable(tokio::io::sink(), 1024 * 1024, &CancellationToken::new())
        .await
        .is_err());
}
