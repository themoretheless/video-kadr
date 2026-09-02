//! Deterministic generated properties for wire normalization, artifacts, and
//! the durable job adapter. Seeds are printed by assertions and replay exactly.

use std::path::Path;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;
use video_kadr_backend::artifacts::{path_token, safe_relative_token, ArtifactFile};
use video_kadr_backend::db::Db;
use video_kadr_backend::domain::artifact_graph::Fingerprint;
use video_kadr_backend::domain::edit::EditSpec;
use video_kadr_backend::domain::output::OutputSpec;
use video_kadr_backend::jobs::{
    replay, EnqueueOutcome, ErrorKind, JobEvent, JobKind, QueueLimits, SqliteJobStore,
};
use video_kadr_backend::model::EditRequest;
use video_kadr_backend::render::chunks::{ChunkManifest, MediaParameters, SceneBoundary};
use video_kadr_backend::runtime::cpu_pool::{CpuPool, CpuPoolConfig};
use video_kadr_backend::services::render::{normalize_edit_request, EditPlan, SourceMediaMetadata};

#[derive(Clone, Copy)]
struct Generator(u64);

impl Generator {
    fn next(&mut self) -> u64 {
        let mut value = self.0.max(1);
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn unit(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
}

fn generated_request(seed: u64) -> (EditRequest, SourceMediaMetadata) {
    let mut generator = Generator(seed);
    let duration = 1.0 + generator.unit() * 119.0;
    let width = 16 + (generator.next() % 3_840) as u32;
    let height = 16 + (generator.next() % 2_160) as u32;
    let start = generator.unit() * duration * 0.75;
    let end = start + 0.01 + generator.unit() * duration;
    let request = serde_json::from_value(json!({
        "videoId": format!("generated-{seed}"),
        "trim": { "start": start, "end": end },
        "crop": {
            "x": generator.next() % u64::from(width.saturating_mul(2)),
            "y": generator.next() % u64::from(height.saturating_mul(2)),
            "w": 1 + generator.next() % u64::from(width.saturating_mul(2)),
            "h": 1 + generator.next() % u64::from(height.saturating_mul(2))
        },
        "speed": 0.1 + generator.unit() * 3.9,
        "volume": generator.unit() * 8.0,
        "fadeIn": generator.unit() * duration * 2.0,
        "fadeOut": generator.unit() * duration * 2.0,
        "brightness": -2.0 + generator.unit() * 4.0,
        "contrast": generator.unit() * 5.0,
        "saturation": generator.unit() * 5.0,
        "pan": -2.0 + generator.unit() * 4.0,
        "fps": 1 + generator.next() % 240,
        "format": "mp4"
    }))
    .unwrap();
    (
        request,
        SourceMediaMetadata::new_with_audio(
            width,
            height,
            duration,
            generator.next().is_multiple_of(2),
        )
        .unwrap(),
    )
}

fn chunk_parameters() -> MediaParameters {
    MediaParameters {
        container: "mp4".into(),
        video_codec: "h264".into(),
        pixel_format: "yuv420p".into(),
        width: 1_920,
        height: 1_080,
        time_base_numerator: 1,
        time_base_denominator: 30,
        audio_codec: Some("aac".into()),
        sample_rate: Some(48_000),
        channels: Some(2),
    }
}

#[test]
fn generated_chunk_plans_are_exact_partitions_and_scene_order_invariant() {
    for seed in 1..=512_u64 {
        let mut generator = Generator(seed);
        let total = 1 + generator.next() % 50_000;
        let maximum = 1 + generator.next() % 2_000;
        let mut scenes: Vec<_> = (0..24)
            .map(|index| SceneBoundary {
                frame: generator.next() % total.saturating_add(1),
                confirmed: index % 3 != 0,
            })
            .collect();
        let plan = |boundaries: &[SceneBoundary]| {
            ChunkManifest::plan(
                Fingerprint::digest(format!("plan-{seed}").as_bytes()),
                Fingerprint::digest(format!("source-{seed}").as_bytes()),
                Fingerprint::digest(format!("output-{seed}").as_bytes()),
                chunk_parameters(),
                total,
                maximum,
                boundaries,
            )
            .unwrap()
        };
        let forward = plan(&scenes);
        scenes.reverse();
        let reversed = plan(&scenes);
        assert_eq!(
            forward, reversed,
            "scene order changed chunk identity for seed {seed}"
        );
        assert_eq!(forward.chunks.first().unwrap().spec.frames.start, 0);
        assert_eq!(forward.chunks.last().unwrap().spec.frames.end, total);
        for (index, chunk) in forward.chunks.iter().enumerate() {
            assert_eq!(chunk.spec.index as usize, index);
            assert!(chunk.spec.frames.start < chunk.spec.frames.end);
            assert!(chunk.spec.frames.end - chunk.spec.frames.start <= maximum);
            if let Some(next) = forward.chunks.get(index + 1) {
                assert_eq!(chunk.spec.frames.end, next.spec.frames.start);
            }
        }
    }
}

#[test]
fn generated_edit_normalization_is_idempotent_and_plan_identity_is_canonical() {
    for seed in 1..=512 {
        let (request, metadata) = generated_request(seed);
        let once = normalize_edit_request(request.clone(), metadata)
            .unwrap_or_else(|error| panic!("seed {seed} failed first normalization: {error}"));
        let twice = normalize_edit_request(once.clone(), metadata)
            .unwrap_or_else(|error| panic!("seed {seed} failed second normalization: {error}"));
        assert_eq!(
            once, twice,
            "normalization is not idempotent for seed {seed}"
        );

        let source = Fingerprint::digest(format!("source-{seed}").as_bytes());
        let original_plan = EditPlan::compile(source.clone(), request, metadata)
            .unwrap_or_else(|error| panic!("seed {seed} failed original compile: {error}"));
        let normalized_plan = EditPlan::compile(source, once, metadata)
            .unwrap_or_else(|error| panic!("seed {seed} failed canonical compile: {error}"));
        assert_eq!(
            original_plan.plan_fingerprint, normalized_plan.plan_fingerprint,
            "plan identity drifted for seed {seed}"
        );
        let serialized = serde_json::to_vec(&original_plan).unwrap();
        let serialized_value: serde_json::Value = serde_json::from_slice(&serialized).unwrap();
        let reparsed_edit: EditSpec =
            serde_json::from_value(serialized_value["edit"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&reparsed_edit).unwrap(),
            serialized_value["edit"],
            "edit canonicalization drift for seed {seed}"
        );
        assert_eq!(
            serde_json::to_vec(&reparsed_edit).unwrap(),
            serde_json::to_vec(&original_plan.edit).unwrap(),
            "edit canonical bytes drift for seed {seed}"
        );
        let reparsed_output: OutputSpec =
            serde_json::from_value(serialized_value["output"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&reparsed_output).unwrap(),
            serialized_value["output"],
            "output canonicalization drift for seed {seed}"
        );
        assert_eq!(
            serde_json::to_vec(&reparsed_output).unwrap(),
            serde_json::to_vec(&original_plan.output).unwrap(),
            "output canonical bytes drift for seed {seed}"
        );
        let round_trip: EditPlan = serde_json::from_slice(&serialized).unwrap_or_else(|error| {
            panic!(
                "plan round-trip parse failed for seed {seed}: {error}; {}",
                String::from_utf8_lossy(&serialized)
            )
        });
        assert_eq!(
            round_trip, original_plan,
            "plan round-trip drift for seed {seed}"
        );
    }
}

#[tokio::test]
async fn generated_artifact_paths_manifests_and_symlinks_preserve_identity_and_containment() {
    for seed in 1..=256_u64 {
        let token = format!("clips/клип-{seed}/frame_{}.bin", seed.rotate_left(7));
        let relative = safe_relative_token(&token)
            .unwrap_or_else(|error| panic!("safe token rejected for seed {seed}: {error}"));
        assert_eq!(path_token(&relative).unwrap(), token);
    }
    for token in [
        "",
        ".",
        "..",
        "../escape",
        "/absolute",
        "clips//frame",
        "clips/./frame",
        "clips/../frame",
        "clips\\frame",
        "clips/\0/frame",
    ] {
        assert!(
            safe_relative_token(token).is_err(),
            "unsafe token accepted: {token:?}"
        );
    }
    let sixteen_components = (0..16)
        .map(|index| format!("part-{index}"))
        .collect::<Vec<_>>()
        .join("/");
    assert!(safe_relative_token(&sixteen_components).is_ok());
    let seventeen_components = format!("{sixteen_components}/overflow");
    assert!(safe_relative_token(&seventeen_components).is_err());

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let nested = root.join("generated/медиа");
    tokio::fs::create_dir_all(&nested).await.unwrap();
    let pool = CpuPool::new(CpuPoolConfig {
        threads: 1,
        queue_capacity: 2,
    })
    .unwrap();
    for seed in 1..=32_u64 {
        let relative = Path::new("generated/медиа").join(format!("{seed}.bin"));
        let absolute = root.join(&relative);
        let bytes = seed.to_be_bytes().repeat((seed % 7 + 1) as usize);
        tokio::fs::write(&absolute, &bytes).await.unwrap();
        let artifact = ArtifactFile::inspect(root, &relative, &pool, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(artifact.size, bytes.len() as u64);
        assert_eq!(
            artifact
                .verify(root, &pool, CancellationToken::new())
                .await
                .unwrap(),
            absolute.canonicalize().unwrap()
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside_directory = tempfile::tempdir().unwrap();
        let outside = outside_directory.path().join("outside.bin");
        std::fs::write(&outside, b"outside").unwrap();
        let link = root.join("generated/медиа/link.bin");
        symlink(&outside, &link).unwrap();
        assert!(ArtifactFile::inspect(
            root,
            Path::new("generated/медиа/link.bin"),
            &pool,
            CancellationToken::new(),
        )
        .await
        .is_err());
    }
}

#[tokio::test]
async fn generated_job_commands_match_the_sqlite_adapter_and_replay_model() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(directory.path()).await.unwrap();
    let store = SqliteJobStore::new(db.clone());
    store.migrate().await.unwrap();
    let limits = QueueLimits {
        dedupe_ttl: Duration::from_secs(300),
        rate_window: Duration::from_secs(60),
        max_new_jobs: 1_000,
    };

    for seed in 1..=96_u64 {
        let job_id = format!("generated-job-{seed}");
        let outcome = store
            .enqueue(
                job_id.clone(),
                JobKind::Edit,
                &json!({"seed": seed}),
                &format!("generated-dedupe-{seed}"),
                limits,
            )
            .await
            .unwrap();
        assert!(matches!(outcome, EnqueueOutcome::Created(_)));
        let mut model = db.load_job(&job_id).await.unwrap().unwrap();

        let mut events = Vec::new();
        if seed & 1 == 0 {
            events.push(JobEvent::Queued);
        }
        assert!(store.claim(&job_id, 30).await.unwrap().is_some());
        events.push(JobEvent::Started {
            stage: "processing".into(),
            attempt: 1,
        });
        events.push(match seed % 4 {
            0 => JobEvent::Succeeded {
                result: json!({"seed": seed}),
            },
            1 => JobEvent::Failed {
                kind: ErrorKind::Validation,
                message: format!("invalid-{seed}"),
            },
            2 => JobEvent::Cancelled,
            _ => JobEvent::Interrupted {
                reason: format!("restart-{seed}"),
            },
        });

        for (index, event) in events.into_iter().enumerate() {
            video_kadr_backend::jobs::event_log::apply(&mut model, &event)
                .unwrap_or_else(|error| panic!("model rejected seed {seed} step {index}: {error}"));
            assert!(
                store
                    .record_transition(
                        &model,
                        &event,
                        &format!("generated:{seed}:{index}"),
                        Some("property-tool-v1"),
                    )
                    .await
                    .unwrap(),
                "adapter deduplicated a fresh event for seed {seed}"
            );
            assert_eq!(
                db.load_job(&job_id).await.unwrap().unwrap(),
                model,
                "snapshot diverged for seed {seed} step {index}"
            );
            assert_eq!(
                replay(&job_id, &store.event_history(&job_id).await.unwrap()).unwrap(),
                model,
                "event replay diverged for seed {seed} step {index}"
            );
        }
    }
}
