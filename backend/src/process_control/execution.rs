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
    command: Command,
    policy: ProcessPolicy,
    limit: Duration,
) -> Result<Output> {
    capture_output_cancellable(runtime, command, policy, limit, &CancellationToken::new()).await
}

/// Cancellation waits for the complete process group to stop before returning.
pub(crate) async fn capture_output_cancellable(
    runtime: &ProcessRuntime,
    mut command: Command,
    policy: ProcessPolicy,
    limit: Duration,
    cancel: &CancellationToken,
) -> Result<Output> {
    anyhow::ensure!(!cancel.is_cancelled(), "external command cancelled");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let prepared = runtime.prepare(command, policy)?;
    let output_budget = prepared.output_budget();
    let mut child = prepared
        .spawn()
        .context("failed to spawn external command")?;
    let child_id = child.id();
    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let captured = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            terminate_child_tree(&mut child, child_id).await;
            anyhow::bail!("external command cancelled");
        }
        captured = timeout(limit, async {
        let (stdout, stderr, status) = tokio::try_join!(
            drain_bounded(&mut stdout, output_budget.capture_bytes),
            drain_bounded(&mut stderr, output_budget.capture_bytes),
            child.wait(),
        )?;
        Ok::<_, std::io::Error>((stdout, stderr, status))
        }) => captured,
    };

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
        // Child exit can precede delivery of its final pipe bytes. Drain after
        // cleaning up descendants, with a bound for independently escaped holders.
        let _ = timeout(Duration::from_millis(200), async {
            while !out_done || !err_done {
                tokio::select! {
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
                }
            }
        }).await;
    } else if matches!(status, ProcessStatus::Cancelled | ProcessStatus::TimedOut) {
        terminate_child_tree(&mut child, child_id).await;
    }
    Ok((status, err_buf.into_string()))
}


/// Read bounded records until the observer has enough data. Early completion,
/// parse errors, cancellation and timeout all terminate and reap this process
/// group before returning; the callback cannot leave a decoder scanning EOF.
pub(crate) async fn capture_lines_until<T>(
    runtime:&ProcessRuntime,mut command:Command,policy:ProcessPolicy,
    limit:Duration,cancel:&CancellationToken,mut observe:impl FnMut(&str)->Result<Option<T>>,
)->Result<(Option<T>,String)>{
    anyhow::ensure!(!cancel.is_cancelled()&&!limit.is_zero(),"external record probe cancelled or zero timeout");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let prepared=runtime.prepare(command,policy)?;let budget=prepared.output_budget();
    let mut child=prepared.spawn()?;let pid=child.id();
    let mut stdout=BoundedLineReader::new(child.stdout.take().expect("stdout piped"),budget);
    let mut stderr=BoundedLineReader::new(child.stderr.take().expect("stderr piped"),budget);
    let(mut out_done,mut err_done)=(false,false);let(mut out_bytes,mut err_bytes)=(0usize,0usize);
    let mut diagnostic=BoundedTail::new(budget.capture_bytes);let deadline=Instant::now()+limit;
    let result:Result<Option<T>>=loop{
        if out_done&&err_done{
            break tokio::select!{biased;
                _=cancel.cancelled()=>Err(anyhow::anyhow!("external record probe cancelled")),
                _=tokio::time::sleep_until(deadline)=>Err(ProcessTimeout.into()),
                status=child.wait()=>match status{Ok(status)if status.success()=>Ok(None),Ok(_)=>Err(anyhow::anyhow!("external record probe failed")),Err(error)=>Err(error.into())}
            };
        }
        tokio::select!{biased;
            _=cancel.cancelled()=>break Err(anyhow::anyhow!("external record probe cancelled")),
            _=tokio::time::sleep_until(deadline)=>break Err(ProcessTimeout.into()),
            line=stdout.next_line_strict(),if !out_done=>match line{
                Ok(Some(line))=>{
                    out_bytes=out_bytes.saturating_add(line.len()+1);
                    if out_bytes>budget.capture_bytes{break Err(ProcessOutputLimit.into());}
                    match observe(&line){Ok(Some(value))=>break Ok(Some(value)),Ok(None)=>{},Err(error)=>break Err(error)}
                },Ok(None)=>out_done=true,Err(error)=>break Err(error.into()),
            },
            line=stderr.next_line_strict(),if !err_done=>match line{
                Ok(Some(line))=>{err_bytes=err_bytes.saturating_add(line.len()+1);if err_bytes>budget.capture_bytes{break Err(ProcessOutputLimit.into());}diagnostic.push_line(&line);},
                Ok(None)=>err_done=true,Err(error)=>break Err(error.into()),
            }
        }
    };
    if !matches!(result,Ok(None)){terminate_child_tree(&mut child,pid).await;}
    // Successful EOF can still leave pipe-holding descendants after the leader.
    else{terminate_process_group(pid).await;}
    if !err_done{
        let tail=timeout(Duration::from_millis(200),async{
            while let Some(line)=stderr.next_line_strict().await?{
                err_bytes=err_bytes.saturating_add(line.len()+1);
                if err_bytes>budget.capture_bytes{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"external stderr exceeded budget"));}
                diagnostic.push_line(&line);
            }
            Ok::<_,std::io::Error>(())
        }).await;
        if let Ok(Err(error))=tail{return Err(error).context("external record probe stderr");}
    }
    let diagnostic=diagnostic.into_string();
    match result{Ok(value)=>Ok((value,diagnostic)),Err(error)=>Err(error).with_context(||format!("external record probe: {diagnostic}"))}
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
    retained: Vec<u8>,
    saw_data: bool,
}

impl<R: AsyncRead + Unpin> BoundedLineReader<R> {
    fn new(reader: R, budget: OutputBudget) -> Self {
        Self {
            reader: BufReader::new(reader),
            line_limit: budget.line_bytes,
            retained: Vec::with_capacity(budget.line_bytes.min(4096)),
            saw_data: false,
        }
    }

    async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        self.next_line_mode(false).await
    }

    async fn next_line_strict(&mut self) -> std::io::Result<Option<String>> {
        self.next_line_mode(true).await
    }

    async fn next_line_mode(&mut self,strict:bool) -> std::io::Result<Option<String>> {
        loop {
            let (consume, newline, chunk, eof) = {
                let available = self.reader.fill_buf().await?;
                if available.is_empty() {
                    (0, false, Vec::new(), true)
                } else {
                    let newline_at = available.iter().position(|byte| *byte == b'\n');
                    let consume = newline_at.map_or(available.len(), |index| index + 1);
                    let content_end = newline_at.unwrap_or(consume);
                    let remaining = self.line_limit.saturating_sub(self.retained.len());
                    if strict&&content_end>remaining{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"external command line exceeded budget"));}
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
                if !self.saw_data {
                    return Ok(None);
                }
                break;
            }
            self.saw_data = true;
            self.reader.consume(consume);
            self.retained.extend_from_slice(&chunk);
            if newline {
                break;
            }
        }
        if self.retained.last() == Some(&b'\r') {
            self.retained.pop();
        }
        let line = String::from_utf8_lossy(&self.retained).into_owned();
        self.retained.clear();
        self.saw_data = false;
        Ok(Some(line))
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

    #[tokio::test]
    async fn interrupted_line_read_preserves_consumed_prefix() {
        use tokio::io::AsyncWriteExt;
        let (mut writer, reader) = tokio::io::duplex(64);
        let mut lines = BoundedLineReader::new(reader, OutputBudget {
            capture_bytes: 128, line_bytes: 64,
        });
        writer.write_all(b"channels=").await.unwrap();
        assert!(timeout(Duration::from_millis(20), lines.next_line()).await.is_err());
        writer.write_all(b"2\n").await.unwrap();
        drop(writer);
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("channels=2"));
        assert_eq!(lines.next_line().await.unwrap(), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn captured_cancellation_reaps_descendants_and_precancel_does_not_spawn() {
        let directory = tempfile::tempdir().unwrap();
        let ready = directory.path().join("ready");
        let late = directory.path().join("late");
        let runtime = ProcessRuntime::local_default();
        let mut command = Command::new("sh");
        command.args(["-c", "trap '' TERM; touch \"$1\"; (trap '' TERM; sleep 1; touch \"$2\") & wait", "runner"])
            .arg(&ready).arg(&late);
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let worker = tokio::spawn(async move {
            capture_output_cancellable(&runtime, command, runtime.discovery_policy(),
                Duration::from_secs(10), &worker_cancel).await
        });
        timeout(Duration::from_secs(5), async {
            while !ready.exists() { tokio::time::sleep(Duration::from_millis(10)).await; }
        }).await.unwrap();
        cancel.cancel();
        let error = timeout(Duration::from_secs(3), worker).await.unwrap().unwrap().unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(!late.exists(), "descendant survived cancellation");
        let runtime = ProcessRuntime::local_default();
        let mut command = Command::new("touch");
        command.arg(&late);
        assert!(capture_output_cancellable(&runtime, command, runtime.discovery_policy(),
            Duration::from_secs(2), &cancel).await.is_err());
        assert!(!late.exists());
    }

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
