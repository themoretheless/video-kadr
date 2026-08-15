use std::hint::black_box;
use std::time::{Duration, Instant};

use serde_json::json;
use video_kadr_backend::db::Db;
use video_kadr_backend::jobs::persistence_profile::{
    should_evaluate_external_store, PersistenceSample,
};
use video_kadr_backend::jobs::{JobKind, QueueLimits, SqliteJobStore};

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime");
    runtime.block_on(async {
        let dir = tempfile::tempdir().expect("benchmark tempdir");
        let db = Db::open(dir.path()).await.expect("open benchmark database");
        let store = SqliteJobStore::new(db);
        let limits = QueueLimits {
            max_new_jobs: 10_000,
            ..QueueLimits::default()
        };
        let mut writes = Vec::with_capacity(1_000);
        for index in 0..1_000_u32 {
            let started = Instant::now();
            black_box(
                store
                    .enqueue(
                        format!("bench-{index}"),
                        JobKind::Edit,
                        &json!({"videoId": "fixture", "index": index}),
                        &format!("bench-key-{index}"),
                        limits,
                    )
                    .await
                    .expect("enqueue benchmark job"),
            );
            writes.push(started.elapsed());
        }
        writes.sort_unstable();
        let p95 = writes[writes.len() * 95 / 100];
        let sample = PersistenceSample {
            job_rows: writes.len() as u64,
            media_rows: 0,
            p95_write: p95,
        };
        println!(
            "sqlite_wal jobs={} p95_write_ms={:.3} evaluate_external_store={}",
            sample.job_rows,
            duration_ms(sample.p95_write),
            should_evaluate_external_store(sample)
        );
    });
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
