use std::time::Duration;

pub const SQLITE_JOB_ROWS_THRESHOLD: u64 = 1_000_000;
pub const SQLITE_MEDIA_ROWS_THRESHOLD: u64 = 100_000;
pub const SQLITE_P95_WRITE_THRESHOLD: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistenceSample {
    pub job_rows: u64,
    pub media_rows: u64,
    pub p95_write: Duration,
}

pub fn should_evaluate_external_store(sample: PersistenceSample) -> bool {
    sample.job_rows >= SQLITE_JOB_ROWS_THRESHOLD
        || sample.media_rows >= SQLITE_MEDIA_ROWS_THRESHOLD
        || sample.p95_write >= SQLITE_P95_WRITE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_remains_default_until_a_measured_threshold_is_crossed() {
        assert!(!should_evaluate_external_store(PersistenceSample {
            job_rows: 999_999,
            media_rows: 99_999,
            p95_write: Duration::from_millis(49),
        }));
        assert!(should_evaluate_external_store(PersistenceSample {
            job_rows: SQLITE_JOB_ROWS_THRESHOLD,
            media_rows: 1,
            p95_write: Duration::from_millis(1),
        }));
    }
}
