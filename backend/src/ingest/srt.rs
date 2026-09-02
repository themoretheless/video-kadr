//! Budgeted SRT ingest adapter.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrtBudgets {
    pub max_reconnects: u32,
    pub max_reconnect_delay: Duration,
    pub max_latency: Duration,
    pub max_clock_drift: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SrtEvent {
    Data {
        bytes: Vec<u8>,
        latency: Duration,
        clock_drift: Duration,
    },
    Reconnected {
        after: Duration,
    },
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImmutableSourceArtifact {
    pub artifact_id: String,
    pub path: PathBuf,
    pub byte_length: u64,
    pub sha256: String,
    pub reconnects: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SrtIngestError {
    UnclosedStream,
    ReconnectBudget,
    ReconnectDelayBudget,
    LatencyBudget,
    ClockDriftBudget,
    EmptyRecording,
    Io(String),
}

impl std::fmt::Display for SrtIngestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnclosedStream => formatter.write_str("SRT stream did not close cleanly"),
            Self::ReconnectBudget => formatter.write_str("SRT reconnect budget exceeded"),
            Self::ReconnectDelayBudget => {
                formatter.write_str("SRT reconnect delay budget exceeded")
            }
            Self::LatencyBudget => formatter.write_str("SRT latency budget exceeded"),
            Self::ClockDriftBudget => formatter.write_str("SRT clock drift budget exceeded"),
            Self::EmptyRecording => formatter.write_str("SRT stream produced an empty recording"),
            Self::Io(error) => write!(formatter, "SRT recording I/O failed: {error}"),
        }
    }
}

impl std::error::Error for SrtIngestError {}

/// Consumes transport events into a private staging file and atomically
/// publishes it only after the transport emits `Closed`.
pub async fn ingest_events(
    events: impl IntoIterator<Item = SrtEvent>,
    staging_dir: &Path,
    source_dir: &Path,
    budgets: SrtBudgets,
) -> Result<ImmutableSourceArtifact, SrtIngestError> {
    tokio::fs::create_dir_all(staging_dir)
        .await
        .map_err(io_error)?;
    tokio::fs::create_dir_all(source_dir)
        .await
        .map_err(io_error)?;
    let id = Uuid::new_v4().to_string();
    let staging = staging_dir.join(format!("{id}.srt.part"));
    let destination = source_dir.join(format!("{id}.ts"));
    let result = ingest_to_staging(events, &staging, budgets).await;
    let (byte_length, sha256, reconnects) = match result {
        Ok(result) => result,
        Err(error) => {
            let _ = tokio::fs::remove_file(&staging).await;
            return Err(error);
        }
    };
    tokio::fs::rename(&staging, &destination)
        .await
        .map_err(io_error)?;
    Ok(ImmutableSourceArtifact {
        artifact_id: id,
        path: destination,
        byte_length,
        sha256,
        reconnects,
    })
}

async fn ingest_to_staging(
    events: impl IntoIterator<Item = SrtEvent>,
    staging: &Path,
    budgets: SrtBudgets,
) -> Result<(u64, String, u32), SrtIngestError> {
    let mut file = tokio::fs::File::create(staging).await.map_err(io_error)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut reconnects = 0_u32;
    let mut closed = false;
    for event in events {
        match event {
            SrtEvent::Data {
                bytes: packet,
                latency,
                clock_drift,
            } => {
                if latency > budgets.max_latency {
                    return Err(SrtIngestError::LatencyBudget);
                }
                if clock_drift > budgets.max_clock_drift {
                    return Err(SrtIngestError::ClockDriftBudget);
                }
                file.write_all(&packet).await.map_err(io_error)?;
                hasher.update(&packet);
                bytes = bytes
                    .checked_add(packet.len() as u64)
                    .ok_or_else(|| SrtIngestError::Io("recording size overflow".into()))?;
            }
            SrtEvent::Reconnected { after } => {
                reconnects = reconnects.saturating_add(1);
                if reconnects > budgets.max_reconnects {
                    return Err(SrtIngestError::ReconnectBudget);
                }
                if after > budgets.max_reconnect_delay {
                    return Err(SrtIngestError::ReconnectDelayBudget);
                }
            }
            SrtEvent::Closed => {
                closed = true;
                break;
            }
        }
    }
    if !closed {
        return Err(SrtIngestError::UnclosedStream);
    }
    if bytes == 0 {
        return Err(SrtIngestError::EmptyRecording);
    }
    file.flush().await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    Ok((bytes, format!("{:x}", hasher.finalize()), reconnects))
}

fn io_error(error: std::io::Error) -> SrtIngestError {
    SrtIngestError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budgets() -> SrtBudgets {
        SrtBudgets {
            max_reconnects: 2,
            max_reconnect_delay: Duration::from_secs(3),
            max_latency: Duration::from_millis(250),
            max_clock_drift: Duration::from_millis(100),
        }
    }

    #[tokio::test]
    async fn publishes_only_a_closed_recording() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging");
        let sources = temp.path().join("sources");
        let artifact = ingest_events(
            [
                SrtEvent::Data {
                    bytes: b"media".to_vec(),
                    latency: Duration::from_millis(20),
                    clock_drift: Duration::from_millis(4),
                },
                SrtEvent::Reconnected {
                    after: Duration::from_millis(50),
                },
                SrtEvent::Data {
                    bytes: b"-bytes".to_vec(),
                    latency: Duration::from_millis(30),
                    clock_drift: Duration::from_millis(5),
                },
                SrtEvent::Closed,
            ],
            &staging,
            &sources,
            budgets(),
        )
        .await
        .unwrap();
        assert_eq!(artifact.byte_length, 11);
        assert_eq!(artifact.reconnects, 1);
        assert_eq!(
            tokio::fs::read(&artifact.path).await.unwrap(),
            b"media-bytes"
        );
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn budget_failure_never_publishes_partial_media() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging");
        let sources = temp.path().join("sources");
        let error = ingest_events(
            [SrtEvent::Data {
                bytes: b"partial".to_vec(),
                latency: Duration::from_secs(2),
                clock_drift: Duration::ZERO,
            }],
            &staging,
            &sources,
            budgets(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, SrtIngestError::LatencyBudget);
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(sources).unwrap().count(), 0);
    }
}
