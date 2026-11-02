//! Explicit actual-runtime cancellation; host paths remain opt-in qualification.
use sha2::{Digest, Sha256};
use video_kadr_backend::{
    process_control::ProcessRuntime,
    services::{
        local_sam,
        local_sam_reidentify::{self, Input},
    },
};
#[tokio::test]
#[ignore = "requires pinned local SAM/ORT; cancels a real child after learned seed inference"]
async fn real_neural_child_cancels_after_progress_without_publishing_or_retaining_stage() {
    let config = local_sam::Config {
        executable: std::env::var("VIDEO_KADR_TEST_SAM_RUNTIME").unwrap().into(),
        encoder: std::env::var("VIDEO_KADR_TEST_SAM_ENCODER").unwrap().into(),
        decoder: std::env::var("VIDEO_KADR_TEST_SAM_DECODER").unwrap().into(),
        runtime_library: std::env::var("VIDEO_KADR_TEST_CHATTERBOX_LIBRARY")
            .unwrap()
            .into(),
    };
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("frames.rgba");
    let seed = folder.path().join("seed.gray");
    let output = folder.path().join("unpublished.masks");
    let frame: Vec<u8> = (0..128 * 96)
        .flat_map(|i| {
            let (x, y) = (i % 128, i / 128);
            if (x as i32 - 36).pow(2) + (y as i32 - 48).pow(2) < 18 * 18 {
                [220, 25, 45, 255]
            } else {
                [22, 130, 180, 255]
            }
        })
        .collect();
    let bytes: Vec<_> = frame.iter().chain(&frame).copied().collect();
    std::fs::write(&source, &bytes).unwrap();
    let mask: Vec<u8> = (0..65536)
        .map(|i| {
            let x = (i % 256) as f64 / 256. * 128.;
            let y = (i / 256) as f64 / 256. * 96.;
            if (x - 36.).powi(2) + (y - 48.).powi(2) < 18f64.powi(2) {
                255
            } else {
                0
            }
        })
        .collect();
    std::fs::write(&seed, &mask).unwrap();
    let source_hash = format!("{:x}", Sha256::digest(&bytes));
    let seed_hash = format!("{:x}", Sha256::digest(&mask));
    let cancel = tokio_util::sync::CancellationToken::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let runtime = ProcessRuntime::local_default();
    let work = local_sam_reidentify::reidentify(
        &runtime,
        &config,
        Input {
            source: &source,
            source_hash: &source_hash,
            seed: &seed,
            seed_hash: &seed_hash,
            width: 128,
            height: 96,
            frames: 2,
            seed_frame: 0,
            exemplars: &[],
        },
        &output,
        &cancel,
        std::time::Duration::from_secs(120),
        &tx,
    );
    tokio::pin!(work);
    let first=tokio::time::timeout(std::time::Duration::from_secs(60),async{loop{tokio::select!{result=&mut work=>panic!("neural process ended before cancellation checkpoint: {result:?}"),value=rx.recv()=>{let value=value.expect("progress sender live");if value>0.&&value<95.{break value;}}}}}).await.expect("real learned seed progress");
    assert!(first > 0.);
    let checkpoint = std::time::Instant::now();
    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), &mut work)
        .await
        .expect("native child cancellation completed promptly");
    assert!(result.is_err());
    assert!(!output.exists());
    assert!(checkpoint.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert_eq!(std::fs::read(&seed).unwrap(), mask);
    assert!(!std::fs::read_dir(folder.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".sam-reidentify-")));
}
