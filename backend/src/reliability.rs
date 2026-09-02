//! Deterministic reliability gates shared by runtime telemetry and CI tools.

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const MAX_HISTOGRAM_SAMPLES: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticalPhase {
    Queue,
    Probe,
    FirstPreviewFrame,
    Render,
    Publish,
}

impl CriticalPhase {
    pub const ALL: [Self; 5] = [
        Self::Queue,
        Self::Probe,
        Self::FirstPreviewFrame,
        Self::Render,
        Self::Publish,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Probe => "probe",
            Self::FirstPreviewFrame => "first_preview_frame",
            Self::Render => "render",
            Self::Publish => "publish",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailPercentiles {
    pub count: usize,
    pub p50_micros: u64,
    pub p95_micros: u64,
    pub p99_micros: u64,
}

#[derive(Debug, Clone, Default)]
pub struct CorrectedHistogram {
    samples_micros: Vec<u64>,
}

impl CorrectedHistogram {
    pub fn record(&mut self, latency: Duration, expected_interval: Duration) -> Result<()> {
        let latency = duration_micros(latency);
        let interval = duration_micros(expected_interval);
        if interval == 0 {
            return Err(anyhow!("expected sampling interval must be non-zero"));
        }
        let correction_count = latency.div_ceil(interval).max(1);
        if self
            .samples_micros
            .len()
            .saturating_add(correction_count as usize)
            > MAX_HISTOGRAM_SAMPLES
        {
            return Err(anyhow!("tail histogram sample budget exceeded"));
        }
        for index in 0..correction_count {
            self.samples_micros
                .push(latency.saturating_sub(index.saturating_mul(interval)));
        }
        Ok(())
    }

    pub fn percentiles(&self) -> Option<TailPercentiles> {
        let mut samples = self.samples_micros.clone();
        samples.sort_unstable();
        Some(TailPercentiles {
            count: samples.len(),
            p50_micros: percentile(&samples, 50)?,
            p95_micros: percentile(&samples, 95)?,
            p99_micros: percentile(&samples, 99)?,
        })
    }
}

fn duration_micros(value: Duration) -> u64 {
    value.as_micros().try_into().unwrap_or(u64::MAX)
}

fn percentile(samples: &[u64], percentile: usize) -> Option<u64> {
    let index = samples
        .len()
        .checked_mul(percentile)?
        .div_ceil(100)
        .saturating_sub(1);
    samples.get(index).copied()
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CircuitKey {
    pub source_fingerprint: String,
    pub tool_fingerprint: String,
}

impl CircuitKey {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("source", &self.source_fingerprint),
            ("tool", &self.tool_fingerprint),
        ] {
            if value.is_empty()
                || value.len() > 128
                || !value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
            {
                return Err(anyhow!("circuit {name} fingerprint is invalid"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct RetryCircuit {
    state: CircuitState,
    consecutive_failures: u32,
    failure_threshold: u32,
    opened_at: Option<u64>,
    cool_down_secs: u64,
    half_open_probe_used: bool,
}

impl RetryCircuit {
    pub fn new(failure_threshold: u32, cool_down: Duration) -> Result<Self> {
        if failure_threshold == 0 || cool_down.is_zero() {
            return Err(anyhow!("circuit retry budget must be non-zero"));
        }
        Ok(Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            failure_threshold,
            opened_at: None,
            cool_down_secs: cool_down.as_secs().max(1),
            half_open_probe_used: false,
        })
    }

    pub fn allow(&mut self, now_secs: u64) -> bool {
        if self.state == CircuitState::Open
            && self
                .opened_at
                .is_some_and(|opened| now_secs.saturating_sub(opened) >= self.cool_down_secs)
        {
            self.state = CircuitState::HalfOpen;
            self.half_open_probe_used = false;
        }
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen if !self.half_open_probe_used => {
                self.half_open_probe_used = true;
                true
            }
            CircuitState::HalfOpen => false,
        }
    }

    pub fn record_success(&mut self) {
        self.state = CircuitState::Closed;
        self.consecutive_failures = 0;
        self.opened_at = None;
        self.half_open_probe_used = false;
    }

    pub fn record_failure(&mut self, now_secs: u64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.state == CircuitState::HalfOpen
            || self.consecutive_failures >= self.failure_threshold
        {
            self.state = CircuitState::Open;
            self.opened_at = Some(now_secs);
            self.half_open_probe_used = false;
        }
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulatedAction {
    AcquireLease,
    Cancel,
    Finish,
    Fail,
    Retry,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationResult {
    pub seed: u64,
    pub actions: Vec<SimulatedAction>,
    pub terminal_writes: u32,
    pub lease_held: bool,
    pub retry_scheduled: bool,
}

pub fn simulate_lifecycle(seed: u64, steps: usize) -> SimulationResult {
    let mut random = XorShift64(seed.max(1));
    let mut actions = Vec::new();
    let mut terminal = false;
    let mut terminal_writes = 0_u32;
    let mut lease_held = false;
    let mut retry_scheduled = false;
    for _ in 0..steps.min(10_000) {
        let action = match random.next() % 6 {
            0 => SimulatedAction::AcquireLease,
            1 => SimulatedAction::Cancel,
            2 => SimulatedAction::Finish,
            3 => SimulatedAction::Fail,
            4 => SimulatedAction::Retry,
            _ => SimulatedAction::Shutdown,
        };
        actions.push(action);
        match action {
            SimulatedAction::AcquireLease if !terminal => lease_held = true,
            SimulatedAction::Cancel | SimulatedAction::Finish | SimulatedAction::Fail
                if !terminal =>
            {
                terminal = true;
                terminal_writes += 1;
                lease_held = false;
            }
            SimulatedAction::Retry if terminal => {
                terminal = false;
                retry_scheduled = true;
            }
            SimulatedAction::Shutdown => lease_held = false,
            _ => {}
        }
    }
    SimulationResult {
        seed,
        actions,
        terminal_writes,
        lease_held,
        retry_scheduled,
    }
}

struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub target: String,
    pub logical_cpus: usize,
    pub workloads: Vec<PerformanceWorkload>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceWorkload {
    pub name: String,
    pub temperature: String,
    pub iterations: usize,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub checksum: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegressionFinding {
    pub workload: String,
    pub median_ratio: f64,
    pub p95_ratio: f64,
}

pub fn compare_three_runs(
    baseline: &PerformanceReport,
    candidates: &[PerformanceReport],
    threshold: f64,
) -> Result<Vec<RegressionFinding>> {
    if candidates.len() != 3 || !threshold.is_finite() || threshold <= 0.0 {
        return Err(anyhow!(
            "baseline comparison requires three runs and a positive threshold"
        ));
    }
    for candidate in candidates {
        if candidate.schema_version != baseline.schema_version
            || candidate.tool_version != baseline.tool_version
            || candidate.target != baseline.target
            || candidate.logical_cpus != baseline.logical_cpus
        {
            return Err(anyhow!(
                "performance environment/schema does not match baseline"
            ));
        }
    }
    let baseline = workload_map(&baseline.workloads)?;
    let candidate_maps = candidates
        .iter()
        .map(|report| workload_map(&report.workloads))
        .collect::<Result<Vec<_>>>()?;
    let mut findings = Vec::new();
    for (key, reference) in baseline {
        let runs = candidate_maps
            .iter()
            .map(|map| {
                map.get(&key)
                    .copied()
                    .ok_or_else(|| anyhow!("candidate workload set differs"))
            })
            .collect::<Result<Vec<_>>>()?;
        if runs.iter().any(|run| run.checksum != reference.checksum) {
            return Err(anyhow!("performance checksum differs for {key}"));
        }
        let median_ratio = median3(runs[0].median_ms, runs[1].median_ms, runs[2].median_ms)
            / reference.median_ms.max(f64::EPSILON);
        let p95_ratio = median3(runs[0].p95_ms, runs[1].p95_ms, runs[2].p95_ms)
            / reference.p95_ms.max(f64::EPSILON);
        if median_ratio > 1.0 + threshold || p95_ratio > 1.0 + threshold {
            findings.push(RegressionFinding {
                workload: key,
                median_ratio,
                p95_ratio,
            });
        }
    }
    Ok(findings)
}

fn workload_map(
    workloads: &[PerformanceWorkload],
) -> Result<BTreeMap<String, &PerformanceWorkload>> {
    let mut map = BTreeMap::new();
    for workload in workloads {
        if workload.iterations == 0
            || !workload.median_ms.is_finite()
            || !workload.p95_ms.is_finite()
            || workload.median_ms < 0.0
            || workload.p95_ms < workload.median_ms
        {
            return Err(anyhow!("invalid performance workload"));
        }
        let key = format!("{}:{}", workload.name, workload.temperature);
        if map.insert(key, workload).is_some() {
            return Err(anyhow!("duplicate performance workload"));
        }
    }
    Ok(map)
}

fn median3(left: f64, middle: f64, right: f64) -> f64 {
    let mut values = [left, middle, right];
    values.sort_by(f64::total_cmp);
    values[1]
}

#[derive(Debug, Clone)]
pub struct FaultScript<T> {
    scripted: VecDeque<T>,
}

impl<T> FaultScript<T> {
    pub fn new(scripted: impl IntoIterator<Item = T>) -> Self {
        Self {
            scripted: scripted.into_iter().collect(),
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.scripted.pop_front()
    }

    pub fn is_exhausted(&self) -> bool {
        self.scripted.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrected_histogram_reports_tail_not_average() {
        let mut histogram = CorrectedHistogram::default();
        histogram
            .record(Duration::from_millis(100), Duration::from_millis(10))
            .unwrap();
        let tail = histogram.percentiles().unwrap();
        assert_eq!(tail.count, 10);
        assert_eq!(tail.p99_micros, 100_000);
        assert!(tail.p50_micros >= 50_000);
    }

    #[test]
    fn circuit_is_keyed_bounded_and_observable_through_half_open() {
        CircuitKey {
            source_fingerprint: "source-a".into(),
            tool_fingerprint: "ffmpeg-8".into(),
        }
        .validate()
        .unwrap();
        let mut circuit = RetryCircuit::new(2, Duration::from_secs(5)).unwrap();
        assert!(circuit.allow(0));
        circuit.record_failure(0);
        circuit.record_failure(1);
        assert_eq!(circuit.state(), CircuitState::Open);
        assert!(!circuit.allow(4));
        assert!(circuit.allow(6));
        assert_eq!(circuit.state(), CircuitState::HalfOpen);
        assert!(!circuit.allow(6));
        circuit.record_success();
        assert_eq!(circuit.state(), CircuitState::Closed);
    }

    #[test]
    fn lifecycle_simulation_is_seeded_and_never_double_writes_without_retry() {
        for seed in 1..=1_000 {
            let first = simulate_lifecycle(seed, 128);
            assert_eq!(first, simulate_lifecycle(seed, 128));
            let mut terminal_since_retry = false;
            for action in &first.actions {
                match action {
                    SimulatedAction::Retry if terminal_since_retry => terminal_since_retry = false,
                    SimulatedAction::Cancel | SimulatedAction::Finish | SimulatedAction::Fail
                        if !terminal_since_retry =>
                    {
                        terminal_since_retry = true
                    }
                    SimulatedAction::Cancel | SimulatedAction::Finish | SimulatedAction::Fail => {}
                    _ => {}
                }
            }
            assert!(!first.lease_held || !terminal_since_retry);
        }
    }

    fn report(scale: f64) -> PerformanceReport {
        PerformanceReport {
            schema_version: 1,
            tool_version: "0.1.0".into(),
            target: "linux-x86_64".into(),
            logical_cpus: 8,
            workloads: vec![PerformanceWorkload {
                name: "probe".into(),
                temperature: "warm".into(),
                iterations: 100,
                median_ms: 10.0 * scale,
                p95_ms: 20.0 * scale,
                checksum: 7,
            }],
        }
    }

    #[test]
    fn comparator_requires_matching_environment_and_three_run_median() {
        let baseline = report(1.0);
        assert!(compare_three_runs(&baseline, &[report(1.0), report(1.1)], 0.2).is_err());
        assert!(
            compare_three_runs(&baseline, &[report(1.0), report(1.5), report(1.0)], 0.2)
                .unwrap()
                .is_empty()
        );
        let findings =
            compare_three_runs(&baseline, &[report(1.3), report(1.4), report(0.9)], 0.2).unwrap();
        assert_eq!(findings.len(), 1);
        let mut mismatch = report(1.0);
        mismatch.logical_cpus = 4;
        assert!(compare_three_runs(&baseline, &[report(1.0), report(1.0), mismatch], 0.2).is_err());
    }

    #[test]
    fn network_fault_scripts_are_replayable_and_exhaustive() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Fault {
            Latency,
            Reset,
            Partial,
            SlowClose,
            Redirect,
        }
        let expected = [
            Fault::Latency,
            Fault::Reset,
            Fault::Partial,
            Fault::SlowClose,
            Fault::Redirect,
        ];
        let mut script = FaultScript::new(expected);
        for fault in expected {
            assert_eq!(script.pop(), Some(fault));
        }
        assert!(script.is_exhausted());
    }
}
