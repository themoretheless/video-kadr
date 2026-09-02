use std::error::Error;
use std::fmt;
use std::process::{Output, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

use super::{OutputBudget, ProcessPolicy, ProcessRuntime};

const TERMINATION_GRACE: Duration = Duration::from_millis(250);
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct ProcessTimeout;

impl fmt::Display for ProcessTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("external command timed out")
    }
}

impl Error for ProcessTimeout {}

pub(crate) fn is_process_timeout(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ProcessTimeout>().is_some()
}

#[cfg(test)]
pub(crate) fn test_timeout_error() -> anyhow::Error {
    ProcessTimeout.into()
}

#[derive(Debug)]
struct ProcessOutputLimit;

impl fmt::Display for ProcessOutputLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("external command exceeded its output budget")
    }
}

impl Error for ProcessOutputLimit {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessStatus {
    Ok,
    Failed,
    Cancelled,
    TimedOut,
}

pub(crate) async fn capture_output(
    runtime: &ProcessRuntime,
    mut command: Command,
    policy: ProcessPolicy,
    limit: Duration,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let prepared = runtime.prepare(command, policy)?;
    let output_budget = prepared.output_budget();
    let mut child = prepared
        .spawn()
        .context("failed to spawn external command")?;
    let child_id = child.id();
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let captured = timeout(limit, async {
        let (stdout, stderr, status) = tokio::try_join!(
            drain_bounded(&mut stdout, output_budget.capture_bytes),
            drain_bounded(&mut stderr, output_budget.capture_bytes),
            child.wait(),
        )?;
        Ok::<_, std::io::Error>((stdout, stderr, status))
    })
    .await;

    match captured {
        Ok(Ok((stdout, stderr, status))) => {
            if stdout.truncated || stderr.truncated {
                return Err(ProcessOutputLimit.into());
            }
            Ok(Output {
                status,
                stdout: stdout.bytes,
                stderr: stderr.bytes,
            })
        }
        Ok(Err(error)) => {
            terminate_child_tree(&mut child, child_id).await;
            Err(error).context("failed to read external command output")
        }
        Err(_) => {
            terminate_child_tree(&mut child, child_id).await;
            Err(ProcessTimeout.into())
        }
    }
}

pub(crate) async fn stream_with_progress(
    runtime: &ProcessRuntime,
    mut command: Command,
    policy: ProcessPolicy,
    parse: impl Fn(&str) -> Option<f64>,
    progress: &UnboundedSender<f64>,
    cancel: &CancellationToken,
    limit: Duration,
) -> Result<(ProcessStatus, String)> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let prepared = runtime.prepare(command, policy)?;
    let output_budget = prepared.output_budget();
    let mut child = prepared.spawn()?;
    let child_id = child.id();
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut out_lines = BoundedLineReader::new(stdout, output_budget);
    let mut err_lines = BoundedLineReader::new(stderr, output_budget);
    let mut err_buf = BoundedTail::new(output_budget.capture_bytes);
    let mut out_done = false;
    let mut err_done = false;
    let mut detached_pipe_holders = false;
    let deadline = Instant::now() + limit;

    let status = loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break ProcessStatus::Cancelled,
            _ = tokio::time::sleep_until(deadline) => break ProcessStatus::TimedOut,
            line = out_lines.next_line(), if !out_done => match line {
                Ok(Some(line)) => {
                    if let Some(percent) = parse(&line) {
                        let _ = progress.send(percent.clamp(0.0, 100.0));
                    }
                }
                _ => out_done = true,
            },
            line = err_lines.next_line(), if !err_done => match line {
                Ok(Some(line)) => {
                    if let Some(percent) = parse(&line) {
                        let _ = progress.send(percent.clamp(0.0, 100.0));
                    }
                    err_buf.push_line(&line);
                }
                _ => err_done = true,
            },
            result = child.wait() => {
                detached_pipe_holders = !out_done || !err_done;
                break match result {
                    Ok(status) if status.success() => ProcessStatus::Ok,
                    _ => ProcessStatus::Failed,
                };
            }
        }
    };

    if detached_pipe_holders {
        terminate_process_group(child_id).await;
    } else if matches!(status, ProcessStatus::Cancelled | ProcessStatus::TimedOut) {
        terminate_child_tree(&mut child, child_id).await;
    }
    Ok((status, err_buf.into_string()))
}

struct BoundedCapture {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn drain_bounded(
    reader: &mut (impl AsyncRead + Unpin),
    limit: usize,
) -> std::io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let retained = limit.saturating_sub(bytes.len()).min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok(BoundedCapture { bytes, truncated })
}

struct BoundedLineReader<R> {
    reader: BufReader<R>,
    line_limit: usize,
}

impl<R: AsyncRead + Unpin> BoundedLineReader<R> {
    fn new(reader: R, budget: OutputBudget) -> Self {
        Self {
            reader: BufReader::new(reader),
            line_limit: budget.line_bytes,
        }
    }

    async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        let mut retained = Vec::with_capacity(self.line_limit.min(4096));
        let mut saw_data = false;
        loop {
            let (consume, newline, chunk, eof) = {
                let available = self.reader.fill_buf().await?;
                if available.is_empty() {
                    (0, false, Vec::new(), true)
                } else {
                    let newline_at = available.iter().position(|byte| *byte == b'\n');
                    let consume = newline_at.map_or(available.len(), |index| index + 1);
                    let content_end = newline_at.unwrap_or(consume);
                    let remaining = self.line_limit.saturating_sub(retained.len());
                    let keep = content_end.min(remaining);
                    (
                        consume,
                        newline_at.is_some(),
                        available[..keep].to_vec(),
                        false,
                    )
                }
            };

            if eof {
                if !saw_data {
                    return Ok(None);
                }
                break;
            }
            saw_data = true;
            self.reader.consume(consume);
            retained.extend_from_slice(&chunk);
            if newline {
                break;
            }
        }
        if retained.last() == Some(&b'\r') {
            retained.pop();
        }
        Ok(Some(String::from_utf8_lossy(&retained).into_owned()))
    }
}

struct BoundedTail {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedTail {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(64 * 1024)),
            limit,
        }
    }

    fn push_line(&mut self, line: &str) {
        let line = line.as_bytes();
        if line.len().saturating_add(1) >= self.limit {
            let start = line.len().saturating_sub(self.limit.saturating_sub(1));
            self.bytes.clear();
            self.bytes.extend_from_slice(&line[start..]);
            self.bytes.push(b'\n');
            return;
        }
        let addition = line.len() + 1;
        let overflow = self
            .bytes
            .len()
            .saturating_add(addition)
            .saturating_sub(self.limit);
        if overflow > 0 {
            self.bytes.drain(..overflow);
        }
        self.bytes.extend_from_slice(line);
        self.bytes.push(b'\n');
    }

    fn into_string(self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

#[derive(Clone, Copy)]
enum SignalKind {
    Terminate,
    Kill,
}

#[cfg(unix)]
fn signal_process_group(child_id: Option<u32>, signal: SignalKind) {
    let Some(pid) = child_id.and_then(|id| i32::try_from(id).ok()) else {
        return;
    };
    let signal = match signal {
        SignalKind::Terminate => libc::SIGTERM,
        SignalKind::Kill => libc::SIGKILL,
    };
    // SAFETY: the id belongs to a child placed in a new process group before
    // spawn. A negative pid targets exactly that group.
    let _ = unsafe { libc::kill(-pid, signal) };
}

#[cfg(not(unix))]
fn signal_process_group(_child_id: Option<u32>, _signal: SignalKind) {}

#[cfg(unix)]
fn process_group_exists(child_id: Option<u32>) -> bool {
    let Some(pid) = child_id.and_then(|id| i32::try_from(id).ok()) else {
        return false;
    };
    // SAFETY: signal 0 performs only an existence/permission check. The
    // negative id addresses the process group created before spawn.
    if unsafe { libc::kill(-pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
async fn terminate_process_group(child_id: Option<u32>) {
    signal_process_group(child_id, SignalKind::Terminate);
    let deadline = Instant::now() + TERMINATION_GRACE;
    while process_group_exists(child_id) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    if process_group_exists(child_id) {
        signal_process_group(child_id, SignalKind::Kill);
    }
}

#[cfg(not(unix))]
async fn terminate_process_group(_child_id: Option<u32>) {}

async fn terminate_child_tree(child: &mut tokio::process::Child, child_id: Option<u32>) {
    #[cfg(unix)]
    {
        signal_process_group(child_id, SignalKind::Terminate);
        let deadline = Instant::now() + TERMINATION_GRACE;
        while process_group_exists(child_id) && Instant::now() < deadline {
            let _ = child.try_wait();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if process_group_exists(child_id) {
            signal_process_group(child_id, SignalKind::Kill);
        }
        let _ = timeout(REAP_TIMEOUT, child.wait()).await;
    }

    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
        let _ = timeout(REAP_TIMEOUT, child.wait()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_control::ProcessRuntimeConfig;

    fn tiny_output_runtime() -> ProcessRuntime {
        let mut config = ProcessRuntimeConfig::local_default();
        config.output = OutputBudget {
            capture_bytes: 128,
            line_bytes: 64,
        };
        ProcessRuntime::new(config).unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_terminates_short_command() {
        let runtime = ProcessRuntime::local_default();
        let mut command = Command::new("sleep");
        command.arg("2");
        let started = Instant::now();
        let error = capture_output(
            &runtime,
            command,
            runtime.discovery_policy(),
            Duration::from_millis(25),
        )
        .await
        .context("wrapped process failure")
        .unwrap_err();

        assert!(format!("{error:#}").contains("timed out"));
        assert!(is_process_timeout(&error));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn captured_output_overflow_is_rejected_after_pipe_drain() {
        let runtime = tiny_output_runtime();
        let mut command = Command::new("sh");
        command.arg("-c").arg("head -c 4096 /dev/zero");
        let error = capture_output(
            &runtime,
            command,
            runtime.discovery_policy(),
            Duration::from_secs(2),
        )
        .await
        .unwrap_err();
        assert!(error.downcast_ref::<ProcessOutputLimit>().is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn streaming_stderr_keeps_only_bounded_data() {
        let runtime = tiny_output_runtime();
        let mut command = Command::new("sh");
        command.arg("-c").arg("head -c 4096 /dev/zero >&2");
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let (status, stderr) = stream_with_progress(
            &runtime,
            command,
            runtime.discovery_policy(),
            |_| None,
            &progress,
            &CancellationToken::new(),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert_eq!(status, ProcessStatus::Ok);
        assert!(stderr.len() <= 128);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_exit_does_not_wait_for_background_pipe_holders() {
        let runtime = ProcessRuntime::local_default();
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("escaped-pipe-holder");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("(trap '' TERM; sleep 1; touch \"$1\") & printf 'progress=50\\n'")
            .arg("runner")
            .arg(&marker);
        let (progress, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let result = timeout(
            Duration::from_millis(700),
            stream_with_progress(
                &runtime,
                command,
                runtime.discovery_policy(),
                |line| {
                    line.strip_prefix("progress=")
                        .and_then(|value| value.parse().ok())
                },
                &progress,
                &CancellationToken::new(),
                Duration::from_secs(10),
            ),
        )
        .await
        .expect("child exit must win over pipe EOF")
        .unwrap();
        assert_eq!(result.0, ProcessStatus::Ok);
        assert_eq!(receiver.try_recv().unwrap(), 50.0);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!marker.exists(), "detached pipe holder survived cleanup");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_the_process_group() {
        let runtime = ProcessRuntime::local_default();
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("escaped-child");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("trap '' TERM; (trap '' TERM; sleep 1; touch \"$1\") & wait")
            .arg("runner")
            .arg(&marker);
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let (status, _) = stream_with_progress(
            &runtime,
            command,
            runtime.discovery_policy(),
            |_| None,
            &progress,
            &CancellationToken::new(),
            Duration::from_millis(50),
        )
        .await
        .unwrap();
        assert_eq!(status, ProcessStatus::TimedOut);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!marker.exists(), "background child survived timeout");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_kills_the_process_group() {
        let runtime = ProcessRuntime::local_default();
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("escaped-cancelled-child");
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("trap '' TERM; (trap '' TERM; sleep 1; touch \"$1\") & wait")
            .arg("runner")
            .arg(&marker);
        let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let worker = tokio::spawn(async move {
            stream_with_progress(
                &runtime,
                command,
                runtime.discovery_policy(),
                |_| None,
                &progress,
                &worker_cancellation,
                Duration::from_secs(10),
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();
        let (status, _) = worker.await.unwrap().unwrap();
        assert_eq!(status, ProcessStatus::Cancelled);
        tokio::time::sleep(Duration::from_millis(1200)).await;
        assert!(!marker.exists(), "background child survived cancellation");
    }
}
