use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;
use video_kadr_backend::db::Db;
use video_kadr_backend::domain::artifact_graph::Fingerprint;
use video_kadr_backend::domain::media_probe::ProbeResult;
use video_kadr_backend::library::{Library, MediaEntry};
use video_kadr_backend::model::EditRequest;
use video_kadr_backend::services::render::{EditPlan, SourceMediaMetadata};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PerfReport {
    schema_version: u32,
    generated_at_unix: u64,
    tool_version: &'static str,
    git_commit: Option<String>,
    git_dirty: Option<bool>,
    rustc_version: Option<String>,
    target: String,
    logical_cpus: usize,
    workloads: Vec<WorkloadStats>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkloadStats {
    name: &'static str,
    temperature: &'static str,
    iterations: usize,
    median_ms: f64,
    p95_ms: f64,
    checksum: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let only = std::env::args()
        .skip_while(|argument| argument != "--only")
        .nth(1);
    let warm_iterations = env_iterations("PERF_WARM_ITERATIONS", 200);
    let cold_iterations = env_iterations("PERF_COLD_ITERATIONS", 20);
    let root = TemporaryRoot::new()?;
    let mut workloads = Vec::new();

    if selected(&only, "probe") {
        workloads.extend(measure_probe(warm_iterations, cold_iterations)?);
    }
    if selected(&only, "library-list") {
        workloads
            .extend(measure_library_list(root.path(), warm_iterations, cold_iterations).await?);
    }
    if selected(&only, "cache-hit") {
        workloads.extend(measure_cache_hit(root.path(), warm_iterations, cold_iterations).await?);
    }
    if selected(&only, "plan-compile") {
        workloads.extend(measure_plan_compile(warm_iterations, cold_iterations)?);
    }
    anyhow::ensure!(!workloads.is_empty(), "unknown or empty workload selection");

    let report = PerfReport {
        schema_version: SCHEMA_VERSION,
        generated_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        tool_version: env!("CARGO_PKG_VERSION"),
        git_commit: std::env::var("GIT_COMMIT").ok(),
        git_dirty: std::env::var("GIT_DIRTY")
            .ok()
            .and_then(|value| value.parse().ok()),
        rustc_version: std::env::var("RUSTC_VERSION").ok(),
        target: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        logical_cpus: std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1),
        workloads,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn selected(only: &Option<String>, workload: &str) -> bool {
    only.as_deref().is_none_or(|selected| selected == workload)
}

fn measure_probe(warm: usize, cold: usize) -> Result<Vec<WorkloadStats>> {
    let fixture = probe_fixture();
    let parsed: serde_json::Value = serde_json::from_str(fixture)?;
    let mut checksum = 0_u64;
    let warm_samples = sample_sync(warm, || {
        let probe = ProbeResult::from_ffprobe_json(black_box(&parsed))?;
        checksum = checksum.wrapping_add(u64::from(probe.width));
        Ok(())
    })?;
    let warm_stats = summarize("probe", "warm", warm_samples, checksum);

    checksum = 0;
    let cold_samples = sample_sync(cold, || {
        let value: serde_json::Value = serde_json::from_str(black_box(fixture))?;
        let probe = ProbeResult::from_ffprobe_json(&value)?;
        checksum = checksum.wrapping_add(u64::from(probe.height));
        Ok(())
    })?;
    Ok(vec![
        warm_stats,
        summarize("probe", "cold", cold_samples, checksum),
    ])
}

async fn measure_library_list(
    root: &std::path::Path,
    warm: usize,
    cold: usize,
) -> Result<Vec<WorkloadStats>> {
    let storage = root.join("library");
    tokio::fs::create_dir_all(storage.join("sources")).await?;
    let mut entries = Vec::new();
    for index in 0..100_u32 {
        let filename = format!("source-{index:03}.mp4");
        tokio::fs::write(storage.join("sources").join(&filename), b"fixture").await?;
        entries.push(MediaEntry {
            id: format!("source-{index}"),
            kind: "source".into(),
            filename,
            storage_key: None,
            url: format!("/files/sources/source-{index:03}.mp4"),
            media_type: Some("video".into()),
            title: Some(format!("Fixture {index}")),
            duration: Some(10.0),
            width: Some(1920),
            height: Some(1080),
            fps: Some(30.0),
            vcodec: Some("h264".into()),
            acodec: Some("aac".into()),
            size_bytes: Some(7),
            created_at: u64::from(index),
        });
    }
    tokio::fs::write(storage.join("library.json"), serde_json::to_vec(&entries)?).await?;

    let library = Library::load(storage.clone()).await;
    let mut checksum = 0_u64;
    let mut warm_samples = Vec::with_capacity(warm);
    for _ in 0..warm {
        let started = Instant::now();
        checksum = checksum.wrapping_add(library.list().await.len() as u64);
        warm_samples.push(started.elapsed());
    }
    let warm_stats = summarize("library-list", "warm", warm_samples, checksum);

    checksum = 0;
    let mut cold_samples = Vec::with_capacity(cold);
    for _ in 0..cold {
        let started = Instant::now();
        let library = Library::load(storage.clone()).await;
        checksum = checksum.wrapping_add(library.list().await.len() as u64);
        cold_samples.push(started.elapsed());
    }
    Ok(vec![
        warm_stats,
        summarize("library-list", "cold", cold_samples, checksum),
    ])
}

async fn measure_cache_hit(
    root: &std::path::Path,
    warm: usize,
    cold: usize,
) -> Result<Vec<WorkloadStats>> {
    let storage = root.join("cache");
    tokio::fs::create_dir_all(&storage).await?;
    let db = Db::open(&storage).await?;
    db.cache_put("fixture", &json!({"id": "output"}), "output.mp4")
        .await?;
    let mut checksum = 0_u64;
    let mut warm_samples = Vec::with_capacity(warm);
    for _ in 0..warm {
        let started = Instant::now();
        checksum = checksum.wrapping_add(db.cache_get("fixture").await?.is_some() as u64);
        warm_samples.push(started.elapsed());
    }
    let warm_stats = summarize("cache-hit", "warm", warm_samples, checksum);
    drop(db);

    checksum = 0;
    let mut cold_samples = Vec::with_capacity(cold);
    for _ in 0..cold {
        let started = Instant::now();
        let db = Db::open(&storage).await?;
        checksum = checksum.wrapping_add(db.cache_get("fixture").await?.is_some() as u64);
        drop(db);
        cold_samples.push(started.elapsed());
    }
    Ok(vec![
        warm_stats,
        summarize("cache-hit", "cold", cold_samples, checksum),
    ])
}

fn measure_plan_compile(warm: usize, cold: usize) -> Result<Vec<WorkloadStats>> {
    let fixture = edit_fixture();
    let edit: EditRequest = serde_json::from_str(fixture)?;
    let source = Fingerprint::digest(b"perf-source");
    let mut checksum = 0_u64;
    let warm_samples = sample_sync(warm, || {
        let plan = EditPlan::compile(
            source.clone(),
            black_box(edit.clone()),
            SourceMediaMetadata::new(1920, 1080, 12.5)?,
        )?;
        checksum = checksum.wrapping_add(u64::from(plan.plan_fingerprint.as_str().as_bytes()[0]));
        Ok(())
    })?;
    let warm_stats = summarize("plan-compile", "warm", warm_samples, checksum);

    checksum = 0;
    let cold_samples = sample_sync(cold, || {
        let edit: EditRequest = serde_json::from_str(black_box(fixture))?;
        let plan = EditPlan::compile(
            source.clone(),
            edit,
            SourceMediaMetadata::new(1920, 1080, 12.5)?,
        )?;
        checksum = checksum.wrapping_add(u64::from(plan.plan_fingerprint.as_str().as_bytes()[0]));
        Ok(())
    })?;
    Ok(vec![
        warm_stats,
        summarize("plan-compile", "cold", cold_samples, checksum),
    ])
}

fn sample_sync(
    mut iterations: usize,
    mut operation: impl FnMut() -> Result<()>,
) -> Result<Vec<Duration>> {
    iterations = iterations.max(1);
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        operation()?;
        samples.push(started.elapsed());
    }
    Ok(samples)
}

fn summarize(
    name: &'static str,
    temperature: &'static str,
    mut samples: Vec<Duration>,
    checksum: u64,
) -> WorkloadStats {
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95_index = (samples.len() * 95).div_ceil(100).saturating_sub(1);
    let p95 = samples[p95_index];
    WorkloadStats {
        name,
        temperature,
        iterations: samples.len(),
        median_ms: milliseconds(median),
        p95_ms: milliseconds(p95),
        checksum,
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn env_iterations(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn probe_fixture() -> &'static str {
    r#"{
      "format": {"format_name":"mov,mp4", "duration":"12.5", "size":"4096"},
      "streams": [
        {"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"avg_frame_rate":"30000/1001","time_base":"1/30000","duration":"12.5","pix_fmt":"yuv420p"},
        {"index":1,"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"time_base":"1/48000","duration":"12.5"}
      ]
    }"#
}

fn edit_fixture() -> &'static str {
    r#"{"videoId":"fixture","trim":{"start":1.0,"end":11.0},"scale":{"w":1280,"h":720},"brightness":0.1,"contrast":1.1,"saturation":0.9,"format":"av1","quality":28,"fps":29.97}"#
}

struct TemporaryRoot(PathBuf);

impl TemporaryRoot {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("video-kadr-perf-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_uses_sorted_median_and_p95() {
        let samples: Vec<_> = (1..=100).map(Duration::from_millis).rev().collect();
        let stats = summarize("test", "warm", samples, 7);
        assert_eq!(stats.median_ms, 51.0);
        assert_eq!(stats.p95_ms, 95.0);
        assert_eq!(stats.checksum, 7);
    }

    #[test]
    fn fixture_contracts_remain_parseable() {
        let probe: serde_json::Value = serde_json::from_str(probe_fixture()).unwrap();
        assert_eq!(ProbeResult::from_ffprobe_json(&probe).unwrap().width, 1920);
        assert!(serde_json::from_str::<EditRequest>(edit_fixture()).is_ok());
    }
}
