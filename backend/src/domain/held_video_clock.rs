//! Exact video-stream origins for source-relative held-frame seeks.
//! Callers resolve authorized paths; this bounded cache never grants file access.
use super::media_probe::Rational;
use anyhow::{ensure, Context, Result};
use std::{
    collections::VecDeque,
    fs::Metadata,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex, OnceLock,
    },
    time::{Duration, Instant, SystemTime},
};
#[derive(Clone, Copy)]
pub(crate) struct Clock {
    pub origin: i64,
    pub base: Rational,
}
impl Clock {
    pub fn seek_time(self, source_us: u64) -> Result<String> {
        let origin = i128::from(self.origin)
            .checked_mul(i128::from(self.base.numerator))
            .and_then(|v| v.checked_mul(1_000_000))
            .context("video origin overflow")?;
        // Seek no later than the exact target; the normalized integer-PTS filter picks
        // the held frame. This also handles negative and fractional source origins.
        let micros = origin
            .div_euclid(i128::from(self.base.denominator))
            .checked_add(i128::from(source_us))
            .context("video seek overflow")?;
        let micros = i64::try_from(micros).context("video seek exceeds codec clock")?;
        let sign = if micros < 0 { "-" } else { "" };
        let magnitude = micros.unsigned_abs();
        Ok(format!(
            "{sign}{}.{:06}",
            magnitude / 1_000_000,
            magnitude % 1_000_000
        ))
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    identity: [i64; 6],
}
fn stamp(metadata: &Metadata) -> Stamp {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Stamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        #[cfg(unix)]
        identity: [
            metadata.dev() as i64,
            metadata.ino() as i64,
            metadata.mtime(),
            metadata.mtime_nsec(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        ],
    }
}
struct Entry {
    path: PathBuf,
    stamp: Stamp,
    clock: Clock,
}
static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
const MAX_ENTRIES: usize = 128;
fn capture(tool: &Path, input: &Path, deadline: Instant, cancel: &AtomicBool) -> Result<Vec<u8>> {
    ensure!(
        !cancel.load(Ordering::Relaxed) && Instant::now() < deadline,
        "video clock probe cancelled/timeout"
    );
    let mut child = Command::new(tool)
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=time_base,start_pts",
            "-of",
            "json",
        ])
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start video clock probe")?;
    let stdout = child.stdout.take().context("video clock probe stdout")?;
    let (tx, rx) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut bytes = vec![];
        let result = stdout.take(65537).read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    let outcome = (|| -> Result<_> {
        let bytes = loop {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "video clock probe cancelled"
            );
            ensure!(Instant::now() < deadline, "video clock probe timeout");
            match rx.recv_timeout(Duration::from_millis(10)) {
                Ok(value) => break value?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => anyhow::bail!("video clock reader stopped"),
            }
        };
        ensure!(bytes.len() <= 65536, "video clock metadata budget exceeded");
        loop {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "video clock probe cancelled"
            );
            ensure!(Instant::now() < deadline, "video clock probe timeout");
            if let Some(status) = child.try_wait()? {
                ensure!(status.success(), "video clock probe failed");
                return Ok(bytes);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })();
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("video clock reader panicked"))?;
    outcome
}
pub(crate) fn resolve(
    ffmpeg: &Path,
    input: &Path,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Clock> {
    ensure!(
        !cancel.load(Ordering::Relaxed) && Instant::now() < deadline,
        "video clock probe cancelled/timeout"
    );
    let path = std::fs::canonicalize(input)?;
    let metadata = std::fs::metadata(&path)?;
    ensure!(metadata.is_file(), "video source is not a regular file");
    let before = stamp(&metadata);
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Some(clock) = cache
        .lock()
        .map_err(|_| anyhow::anyhow!("video clock cache stopped"))?
        .iter()
        .find(|entry| entry.path == path && entry.stamp == before)
        .map(|entry| entry.clock)
    {
        return Ok(clock);
    }
    let sibling = ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let tool = if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        })
    };
    let bytes = capture(&tool, &path, deadline, cancel)?;
    let document: serde_json::Value = serde_json::from_slice(&bytes)?;
    let stream = &document["streams"][0];
    let origin = stream["start_pts"]
        .as_i64()
        .or_else(|| stream["start_pts"].as_str()?.parse().ok())
        .context("video stream origin missing")?;
    let base = Rational::parse(
        stream["time_base"]
            .as_str()
            .context("video time base missing")?,
    )
    .context("invalid video time base")?;
    ensure!(
        base.numerator > 0 && base.denominator > 0,
        "invalid video time base"
    );
    let clock = Clock { origin, base };
    ensure!(
        before == stamp(&std::fs::metadata(&path)?)
            && !cancel.load(Ordering::Relaxed)
            && Instant::now() < deadline,
        "video source changed/cancelled during clock probe"
    );
    // No open descriptor is retained. Replace entries for mutated paths and bound
    // cache growth; inode/device/ctime/mtime changes invalidate Unix snapshots.
    let mut entries = cache
        .lock()
        .map_err(|_| anyhow::anyhow!("video clock cache stopped"))?;
    entries.retain(|entry| entry.path != path);
    if entries.len() >= MAX_ENTRIES {
        entries.pop_front();
    }
    entries.push_back(Entry {
        path,
        stamp: before,
        clock,
    });
    Ok(clock)
}
