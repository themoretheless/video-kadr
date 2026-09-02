use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::resource_classes::ResourceClass;
use crate::ports::telemetry::{ResourceObservation, TelemetryEvent, TelemetryPort};

#[derive(Debug, Default)]
pub struct PrometheusTelemetry {
    queue_wait_count: [AtomicU64; 3],
    queue_wait_micros: [AtomicU64; 3],
    job_terminal: [AtomicU64; 12],
    cache_hit: AtomicU64,
    cache_miss: AtomicU64,
    process_count: [AtomicU64; 6],
    process_micros: [AtomicU64; 6],
    bytes_ingress: AtomicU64,
    bytes_egress: AtomicU64,
}

impl TelemetryPort for PrometheusTelemetry {
    fn record(&self, event: TelemetryEvent) {
        match event {
            TelemetryEvent::QueueWait { class, duration } => {
                let index = class_index(class);
                self.queue_wait_count[index].fetch_add(1, Ordering::Relaxed);
                self.queue_wait_micros[index].fetch_add(
                    duration.as_micros().try_into().unwrap_or(u64::MAX),
                    Ordering::Relaxed,
                );
            }
            TelemetryEvent::JobTerminal { kind, outcome } => {
                if let (Some(kind), Some(outcome)) =
                    (job_index(kind), outcome_index(outcome.label()))
                {
                    self.job_terminal[kind * 3 + outcome].fetch_add(1, Ordering::Relaxed);
                }
            }
            TelemetryEvent::CacheLookup { result: "hit" } => {
                self.cache_hit.fetch_add(1, Ordering::Relaxed);
            }
            TelemetryEvent::CacheLookup { result: "miss" } => {
                self.cache_miss.fetch_add(1, Ordering::Relaxed);
            }
            TelemetryEvent::CacheLookup { .. } => {}
            TelemetryEvent::ProcessFinished {
                tool,
                outcome,
                duration,
            } => {
                if let (Some(tool), Some(outcome)) =
                    (tool_index(tool), process_outcome_index(outcome))
                {
                    let index = tool * 3 + outcome;
                    self.process_count[index].fetch_add(1, Ordering::Relaxed);
                    self.process_micros[index].fetch_add(
                        duration.as_micros().try_into().unwrap_or(u64::MAX),
                        Ordering::Relaxed,
                    );
                }
            }
            TelemetryEvent::Bytes {
                direction: "ingress",
                amount,
            } => {
                self.bytes_ingress.fetch_add(amount, Ordering::Relaxed);
            }
            TelemetryEvent::Bytes {
                direction: "egress",
                amount,
            } => {
                self.bytes_egress.fetch_add(amount, Ordering::Relaxed);
            }
            TelemetryEvent::Bytes { .. } => {}
        }
    }

    fn render_prometheus(&self, resources: &[ResourceObservation]) -> Option<String> {
        let mut output = String::from(
            "# HELP video_kadr_queue_wait_seconds Queue admission wait by bounded resource class.\n\
             # TYPE video_kadr_queue_wait_seconds summary\n",
        );
        for class in ResourceClass::ALL {
            let index = class_index(class);
            let count = self.queue_wait_count[index].load(Ordering::Relaxed);
            let seconds =
                self.queue_wait_micros[index].load(Ordering::Relaxed) as f64 / 1_000_000.0;
            let _ = writeln!(
                output,
                "video_kadr_queue_wait_seconds_sum{{class=\"{}\"}} {seconds}",
                class.label()
            );
            let _ = writeln!(
                output,
                "video_kadr_queue_wait_seconds_count{{class=\"{}\"}} {count}",
                class.label()
            );
        }
        output.push_str(
            "# HELP video_kadr_resource_permits Current admission permits.\n\
             # TYPE video_kadr_resource_permits gauge\n",
        );
        for resource in resources {
            let used = resource.limit.saturating_sub(resource.available);
            let _ = writeln!(
                output,
                "video_kadr_resource_permits{{class=\"{}\",state=\"available\"}} {}",
                resource.class.label(),
                resource.available
            );
            let _ = writeln!(
                output,
                "video_kadr_resource_permits{{class=\"{}\",state=\"used\"}} {used}",
                resource.class.label()
            );
        }
        output.push_str("# TYPE video_kadr_cache_lookups_total counter\n");
        let _ = writeln!(
            output,
            "video_kadr_cache_lookups_total{{result=\"hit\"}} {}",
            self.cache_hit.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            output,
            "video_kadr_cache_lookups_total{{result=\"miss\"}} {}",
            self.cache_miss.load(Ordering::Relaxed)
        );
        output.push_str("# TYPE video_kadr_jobs_terminal_total counter\n");
        for (kind_index, kind) in ["import", "proxy", "edit", "composition"]
            .iter()
            .enumerate()
        {
            for (outcome_index, outcome) in ["succeeded", "failed", "cancelled"].iter().enumerate()
            {
                let count =
                    self.job_terminal[kind_index * 3 + outcome_index].load(Ordering::Relaxed);
                let _ = writeln!(
                    output,
                    "video_kadr_jobs_terminal_total{{kind=\"{kind}\",outcome=\"{outcome}\"}} {count}"
                );
            }
        }
        output.push_str("# TYPE video_kadr_process_duration_seconds summary\n");
        for (tool_index, tool) in ["ffmpeg", "yt-dlp"].iter().enumerate() {
            for (outcome_index, outcome) in ["succeeded", "failed", "cancelled"].iter().enumerate()
            {
                let index = tool_index * 3 + outcome_index;
                let count = self.process_count[index].load(Ordering::Relaxed);
                let seconds =
                    self.process_micros[index].load(Ordering::Relaxed) as f64 / 1_000_000.0;
                let _ = writeln!(
                    output,
                    "video_kadr_process_duration_seconds_sum{{tool=\"{tool}\",outcome=\"{outcome}\"}} {seconds}"
                );
                let _ = writeln!(
                    output,
                    "video_kadr_process_duration_seconds_count{{tool=\"{tool}\",outcome=\"{outcome}\"}} {count}"
                );
            }
        }
        output.push_str("# TYPE video_kadr_bytes_total counter\n");
        let _ = writeln!(
            output,
            "video_kadr_bytes_total{{direction=\"ingress\"}} {}",
            self.bytes_ingress.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            output,
            "video_kadr_bytes_total{{direction=\"egress\"}} {}",
            self.bytes_egress.load(Ordering::Relaxed)
        );
        output.push_str("# EOF\n");
        Some(output)
    }
}

fn class_index(class: ResourceClass) -> usize {
    match class {
        ResourceClass::Ingest => 0,
        ResourceClass::Analysis => 1,
        ResourceClass::Export => 2,
    }
}

fn job_index(kind: &str) -> Option<usize> {
    ["import", "proxy", "edit", "composition"]
        .iter()
        .position(|value| *value == kind)
}

fn outcome_index(outcome: &str) -> Option<usize> {
    ["succeeded", "failed", "cancelled"]
        .iter()
        .position(|value| *value == outcome)
}

fn tool_index(tool: &str) -> Option<usize> {
    ["ffmpeg", "yt-dlp"].iter().position(|value| *value == tool)
}

fn process_outcome_index(outcome: &str) -> Option<usize> {
    ["succeeded", "failed", "cancelled"]
        .iter()
        .position(|value| *value == outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn exposition_has_only_bounded_labels_and_never_identifiers() {
        let telemetry = PrometheusTelemetry::default();
        telemetry.record(TelemetryEvent::QueueWait {
            class: ResourceClass::Export,
            duration: Duration::from_millis(3),
        });
        let output = telemetry
            .render_prometheus(&[ResourceObservation {
                class: ResourceClass::Export,
                available: 1,
                limit: 2,
            }])
            .unwrap();
        assert!(output.contains("class=\"export\""));
        for forbidden in ["job_id", "request_id", "url", "filename"] {
            assert!(!output.contains(forbidden));
        }
    }
}
