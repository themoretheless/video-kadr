//! Reproducible in-process Axum framework baseline.
//!
//! Run with `cargo bench --bench http_baseline`. This intentionally excludes
//! network and persistence so regressions can be assigned to HTTP layers first.

use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

const WARMUP_REQUESTS: usize = 250;
const MEASURED_REQUESTS: usize = 5_000;

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Tokio benchmark runtime");
    runtime.block_on(async {
        let router = Router::new().route("/baseline", get(|| async { "ok" }));
        for _ in 0..WARMUP_REQUESTS {
            request(router.clone()).await;
        }

        let started = Instant::now();
        let mut samples = Vec::with_capacity(MEASURED_REQUESTS);
        for _ in 0..MEASURED_REQUESTS {
            let request_started = Instant::now();
            request(router.clone()).await;
            samples.push(request_started.elapsed().as_nanos() as u64);
        }
        let elapsed = started.elapsed();
        samples.sort_unstable();

        println!(
            "{{\"requests\":{MEASURED_REQUESTS},\"throughputRps\":{:.2},\"p50Us\":{:.2},\"p95Us\":{:.2},\"p99Us\":{:.2},\"peakRssBytes\":{}}}",
            MEASURED_REQUESTS as f64 / elapsed.as_secs_f64(),
            percentile(&samples, 0.50) as f64 / 1_000.0,
            percentile(&samples, 0.95) as f64 / 1_000.0,
            percentile(&samples, 0.99) as f64 / 1_000.0,
            peak_rss_bytes(),
        );
    });
}

async fn request(router: Router) {
    let response = router
        .oneshot(
            Request::builder()
                .uri("/baseline")
                .body(Body::empty())
                .expect("baseline request"),
        )
        .await
        .expect("baseline response");
    assert!(response.status().is_success());
}

fn percentile(sorted: &[u64], fraction: f64) -> u64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

#[cfg(unix)]
fn peak_rss_bytes() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return 0;
    }
    let usage = unsafe { usage.assume_init() };
    #[cfg(target_os = "macos")]
    return usage.ru_maxrss as u64;
    #[cfg(not(target_os = "macos"))]
    return usage.ru_maxrss as u64 * 1_024;
}

#[cfg(not(unix))]
fn peak_rss_bytes() -> u64 {
    0
}
