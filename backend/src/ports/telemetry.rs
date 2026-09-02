use std::sync::Mutex;
use std::time::Duration;

use crate::config::resource_classes::ResourceClass;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

impl JobOutcome {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelemetryEvent {
    QueueWait {
        class: ResourceClass,
        duration: Duration,
    },
    JobTerminal {
        kind: &'static str,
        outcome: JobOutcome,
    },
    CacheLookup {
        result: &'static str,
    },
    ProcessFinished {
        tool: &'static str,
        outcome: &'static str,
        duration: Duration,
    },
    Bytes {
        direction: &'static str,
        amount: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceObservation {
    pub class: ResourceClass,
    pub available: usize,
    pub limit: usize,
}

pub trait TelemetryPort: Send + Sync {
    fn record(&self, event: TelemetryEvent);

    fn render_prometheus(&self, _resources: &[ResourceObservation]) -> Option<String> {
        None
    }
}

#[derive(Debug, Default)]
pub struct NoopTelemetry;

impl TelemetryPort for NoopTelemetry {
    fn record(&self, _event: TelemetryEvent) {}
}

#[derive(Debug, Default)]
pub struct TestTelemetry {
    events: Mutex<Vec<TelemetryEvent>>,
}

impl TestTelemetry {
    pub fn events(&self) -> Vec<TelemetryEvent> {
        self.events.lock().expect("test telemetry lock").clone()
    }
}

impl TelemetryPort for TestTelemetry {
    fn record(&self, event: TelemetryEvent) {
        self.events.lock().expect("test telemetry lock").push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_and_test_adapters_share_the_exporter_neutral_contract() {
        let event = TelemetryEvent::CacheLookup { result: "hit" };
        NoopTelemetry.record(event.clone());
        let telemetry = TestTelemetry::default();
        telemetry.record(event.clone());
        assert_eq!(telemetry.events(), vec![event]);
    }
}
