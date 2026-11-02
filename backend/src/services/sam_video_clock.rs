//! Preserve exact source PTS independently of the lossless mask carrier FPS.
use crate::{
    domain::{composition::FrameRateRatio, media_probe::Rational},
    process_control::{capture_output_cancellable, ProcessRuntime},
};
use anyhow::{ensure, Context, Result};
use std::path::Path;
use tokio::{process::Command, time::Instant};
use tokio_util::sync::CancellationToken;
#[allow(clippy::too_many_arguments)]
pub async fn probe(
    runtime: &ProcessRuntime,
    tool: &Path,
    source: &Path,
    count: u64,
    rate: FrameRateRatio,
    tolerance: f64,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<Option<Vec<u64>>> {
    ensure!(
        count > 0 && count <= 360000,
        "SAM presentation frame budget"
    );
    let mut command = Command::new(tool);
    command.args(["-v","error","-protocol_whitelist",crate::tools::OFFLINE_PROTOCOLS,"-select_streams","v:0","-show_entries","frame=best_effort_timestamp,duration,pkt_duration:stream=time_base,start_pts,duration_ts","-of","json"]).arg(source);
    let mut policy = runtime.probe_policy(source);
    policy.output.capture_bytes = 64 * 1024 * 1024;
    let output = capture_output_cancellable(
        runtime,
        command,
        policy,
        super::local_enhance_video::remaining(deadline, cancel)?,
        cancel,
    )
    .await?;
    ensure!(output.status.success(), "SAM source timestamp probe failed");
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let frames = document["frames"]
        .as_array()
        .context("SAM presentation frames missing")?;
    ensure!(
        frames.len() as u64 == count,
        "SAM presentation frame count changed"
    );
    let integer = |v: &serde_json::Value| v.as_i64().or_else(|| v.as_str()?.parse::<i64>().ok());
    let stream = &document["streams"][0];
    let base = Rational::parse(
        stream["time_base"]
            .as_str()
            .context("SAM source time base missing")?,
    )
    .context("invalid SAM source time base")?;
    ensure!(
        base.numerator > 0 && base.denominator > 0,
        "invalid SAM source time base"
    );
    let ticks: Vec<i64> = frames
        .iter()
        .map(|f| {
            integer(&f["best_effort_timestamp"])
                .context("SAM source presentation timestamp missing")
        })
        .collect::<Result<_>>()?;
    ensure!(
        ticks.windows(2).all(|p| p[0] < p[1]),
        "SAM presentation timestamps must increase"
    );
    let origin = ticks[0];
    ensure!(
        integer(&stream["start_pts"]).unwrap_or(origin) == origin,
        "SAM source leading presentation gap is unsupported"
    );
    let relative: Vec<i128> = ticks
        .into_iter()
        .map(|v| i128::from(v) - i128::from(origin))
        .collect();
    let times: Vec<f64> = relative
        .iter()
        .map(|v| *v as f64 * base.numerator as f64 / base.denominator as f64)
        .collect();
    if super::local_person_video::constant_clock(&times, rate, tolerance).is_ok() {
        return Ok(None);
    }
    let last = *relative.last().unwrap();
    let last_frame = frames.last().unwrap();
    let duration = integer(&last_frame["duration"])
        .or_else(|| integer(&last_frame["pkt_duration"]))
        .filter(|v| *v > 0);
    let end = duration
        .map(|v| last + i128::from(v))
        .or_else(|| {
            integer(&stream["duration_ts"])
                .map(i128::from)
                .filter(|v| *v > last)
        })
        .context("VFR final frame duration missing")?;
    let micros = |ticks: i128| -> Result<u64> {
        ensure!(ticks >= 0, "negative SAM presentation time");
        let numerator = (ticks as u128)
            .checked_mul(base.numerator as u128)
            .and_then(|v| v.checked_mul(1_000_000))
            .context("SAM timestamp overflow")?;
        let value = numerator.div_ceil(base.denominator as u128);
        ensure!(
            value <= 9_007_199_254_740_991,
            "SAM timestamp exceeds portable microsecond clock"
        );
        Ok(value as u64)
    };
    let mut clock = relative
        .into_iter()
        .map(micros)
        .collect::<Result<Vec<_>>>()?;
    clock.push(micros(end)?);
    crate::domain::video_region::validate_frame_times(Some(&clock), count)?;
    Ok(Some(clock))
}
