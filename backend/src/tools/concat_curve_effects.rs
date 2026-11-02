//! Native per-piece source effects between source tempo and gain/fades.
use super::{
    audio_packet_spool::{AudioPacketSpool, PcmPacketFormat},
    concat_audio_frames::PreparedAudioFrames,
};
use crate::{
    domain::{
        concat_format_plan::{Filter, Format, Plan},
        filmcraft_dsp::EffectSpec,
        media_probe::StreamMetadata,
    },
    process_control::{capture_lines_until, capture_output_cancellable, ProcessRuntime},
};
use anyhow::{ensure, Context, Result};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
enum NativeStage {
    Echo(super::packet_echo::Stage),
    Modulation(super::packet_modulation::Stage),
    Biquad(super::packet_biquad::Stage),
    Limiter(super::packet_limiter::Stage),
    Crusher {
        core: crate::domain::concat_log_crusher::LogCrusher,
        channels: usize,
    },
}
impl NativeStage {
    fn push_packet(
        &mut self,
        packet: super::packet_pitch::Block,
    ) -> Result<super::packet_pitch::Block> {
        match self {
            Self::Echo(stage) => stage.push_packet(packet),
            Self::Modulation(stage) => stage.push_packet(packet),
            Self::Biquad(stage) => stage.push_packet(packet),
            Self::Limiter(stage) => stage.push_packet(packet),
            Self::Crusher { core, channels } => {
                let mut data = super::packet_echo::floating64(packet);
                ensure!(data.len() == *channels, "crusher channel mismatch");
                let frames = data[0].len();
                ensure!(
                    frames <= 1048576
                        && data
                            .iter()
                            .all(|ch| ch.len() == frames && ch.iter().all(|v| v.is_finite())),
                    "invalid crusher packet"
                );
                for channel in &mut data {
                    for sample in channel {
                        *sample = core.process(*sample);
                        ensure!(sample.is_finite(), "nonfinite crusher output");
                    }
                }
                Ok(super::packet_pitch::Block::F64(data))
            }
        }
    }
    fn finish_packet(&mut self) -> Option<super::packet_pitch::Block> {
        match self {
            Self::Echo(stage) => stage.finish_packet(),
            Self::Biquad(_) | Self::Limiter(_) | Self::Crusher { .. } | Self::Modulation(_) => None,
        }
    }
}
enum SourceOp {
    Echo(crate::domain::concat_audio::ReflectionRecipe),
    Biquad(crate::domain::concat_tone::SourceBiquad),
    Limiter(f64),
    Phaser(f64),
    Flanger,
    Crusher(crate::domain::concat_tone::SourceCrusher),
}
impl SourceOp {
    fn caps(&self) -> Filter {
        match self {
            Self::Echo(_) => Filter::Echo,
            Self::Biquad(_) => Filter::Biquad,
            Self::Limiter(_) => Filter::Limiter,
            Self::Phaser(_) => Filter::Phaser,
            Self::Flanger => Filter::Flanger,
            Self::Crusher(_) => Filter::Crusher,
        }
    }
}
pub(crate) struct PreparedEffectFrames {
    spool: AudioPacketSpool,
    format: Format,
}
impl PreparedEffectFrames {
    pub(crate) fn path(&self) -> &Path {
        self.spool.path()
    }
    pub(crate) fn bytes(&self) -> u64 {
        self.spool.bytes()
    }
    pub(crate) fn sample_format(&self) -> &'static str {
        self.format.name()
    }
}
fn packet_format(format: Format) -> Result<(PcmPacketFormat, &'static str, &'static str)> {
    Ok(match format.packed() {
        Format::S16 => (PcmPacketFormat::S16, "pcm_s16le", "s16le"),
        Format::S32 => (PcmPacketFormat::S32, "pcm_s32le", "s32le"),
        Format::F32 => (PcmPacketFormat::F32, "pcm_f32le", "f32le"),
        Format::F64 => (PcmPacketFormat::F64, "pcm_f64le", "f64le"),
        _ => anyhow::bail!("source effect precision needs additional transport"),
    })
}
struct Temp(PathBuf);
impl Temp {
    fn create(staging: &Path, suffix: &str) -> Result<Self> {
        let path = staging.join(format!(".curve-effect-{}.{suffix}", uuid::Uuid::new_v4()));
        drop(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        Ok(Self(path))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub(crate) fn validate_effects(specs: &[EffectSpec]) -> Result<()> {
    ensure!(
        specs.iter().filter(|s| !s.bypass).all(|s| matches!(
            s.id.as_str(),
            "concat.echo"
                | "concat.room"
                | "concat.hall"
                | "concat.plate"
                | "concat.cathedral"
                | "concat.stadium"
                | "concat.bass"
                | "concat.treble"
                | "concat.bright"
                | "concat.underwater"
                | "concat.telephone"
                | "concat.megaphone"
                | "concat.vinyl"
                | "concat.whisper"
                | "concat.radio"
                | "concat.lofi"
                | "concat.robot"
        )),
        "active source curve effects require source frame graph compilation"
    );
    crate::domain::filmcraft_dsp::validate_specs(specs)
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    ffprobe: &Path,
    capture: &PreparedAudioFrames,
    metadata: &StreamMetadata,
    prefix: &[String],
    has_volume: bool,
    is_mixed: bool,
    specs: &[EffectSpec],
    staging: &Path,
    budget: u64,
    limit: Duration,
    cancel: &CancellationToken,
) -> Result<PreparedEffectFrames> {
    validate_effects(specs)?;
    let started = std::time::Instant::now();
    let remaining = || {
        limit
            .checked_sub(started.elapsed())
            .filter(|v| !v.is_zero())
            .context("source curve effect timed out")
    };
    let effects = specs.iter().filter(|s| !s.bypass).collect::<Vec<_>>();
    ensure!(!effects.is_empty(), "missing source curve effects");
    let rate = metadata
        .sample_rate
        .context("source curve effect sample rate missing")?;
    let channels = metadata
        .channels
        .context("source curve effect channels missing")? as usize;
    ensure!(
        (1..=16).contains(&channels),
        "source curve effect channel budget exceeded"
    );
    let positioned = metadata
        .channel_layout
        .as_deref()
        .is_some_and(|v| !v.is_empty() && v != "unknown" && !v.ends_with(" channels"));
    let source_format = Format::parse(metadata.sample_format.as_deref().unwrap_or(""))
        .context("source curve effect precision missing")?;
    let mut operations = Vec::new();
    let mut has_tone = false;
    for effect in effects {
        if matches!(
            effect.id.as_str(),
            "concat.bass"
                | "concat.treble"
                | "concat.bright"
                | "concat.underwater"
                | "concat.telephone"
                | "concat.megaphone"
                | "concat.vinyl"
                | "concat.whisper"
                | "concat.radio"
                | "concat.lofi"
                | "concat.robot"
        ) {
            has_tone = true;
            let recipe = crate::domain::concat_tone::source_tone_recipe(effect)?;
            if let Some(speed) = recipe.modulation {
                operations.push(SourceOp::Phaser(speed));
                operations.push(SourceOp::Flanger);
            }
            for mut filter in recipe.filters {
                if !positioned {
                    filter.frequency = 0.;
                }
                operations.push(SourceOp::Biquad(filter));
            }
            if let Some(crusher) = recipe.crusher {
                operations.push(SourceOp::Crusher(crusher));
            }
            operations.push(SourceOp::Limiter(recipe.limit));
        } else {
            operations.push(SourceOp::Echo(
                crate::domain::concat_audio::source_reflection_recipe(effect)?,
            ));
            if effect.id == "concat.stadium" {
                operations.push(SourceOp::Biquad(crate::domain::concat_tone::SourceBiquad {
                    design: crate::domain::concat_biquad_stream::Design::Lowpass,
                    frequency: if positioned { 6500. } else { 0. },
                    width: 0.707,
                    gain: 0.,
                }));
            }
        }
    }
    // amix precedes the final resampler; volume constrains the post-effect side.
    let end_format = if has_volume {
        Format::F32P
    } else if has_tone
        || matches!(
            source_format,
            Format::F64 | Format::F64P | Format::S64 | Format::S64P
        )
    {
        Format::F64P
    } else {
        Format::F32P
    };
    let (formats, output_format) = if has_tone || !is_mixed {
        let tempos = prefix.iter().filter(|v| v.starts_with("atempo=")).count();
        let mut caps = vec![Filter::Tempo; tempos];
        caps.extend(operations.iter().map(SourceOp::caps));
        let operations_end = caps.len();
        // A single original branch reaches aresample through anull. The
        // resampler separates its output precision from integer effect inputs.
        if !is_mixed {
            if has_volume {
                caps.push(Filter::Volume);
            }
            caps.push(Filter::Resample);
        }
        let plan = Plan::build(
            source_format,
            &caps,
            if is_mixed { end_format } else { Format::F32P },
        )?;
        (
            plan.stages[tempos..operations_end]
                .iter()
                .map(|stage| stage.input)
                .collect::<Vec<_>>(),
            plan.stages
                .get(operations_end - 1)
                .context("empty source effect plan")?
                .output,
        )
    } else {
        (vec![end_format; operations.len()], end_format)
    };
    let input_format = *formats
        .first()
        .context("empty native source effect operations")?;
    let (packet_format, codec, raw_format) = packet_format(input_format)?;
    let output_packet_format = self::packet_format(output_format)?.0;
    let scalar_bytes = packet_format.bytes() as u64;
    let mut state_bytes = 0u64;
    for (op, format) in operations.iter().zip(&formats) {
        let bytes = match op {
            SourceOp::Echo(recipe) => {
                recipe.delays_ms[..recipe.count]
                    .iter()
                    .map(|delay| (f64::from(*delay * rate as f32) / 1000.) as u64)
                    .max()
                    .context("empty reflection taps")?
                    * channels as u64
                    * self::packet_format(*format)?.0.bytes() as u64
            }
            SourceOp::Biquad(_) => channels as u64 * 64 + 256,
            SourceOp::Crusher(_) => 64,
            SourceOp::Phaser(speed) => {
                ((f64::from(rate) * 0.003 + 0.5) as u64) * (channels as u64 * 8 + 24)
                    + (f64::from(rate) / speed + 0.5) as u64 * 8
                    + 1024
            }
            SourceOp::Flanger => {
                ((f64::from(rate) * 0.006 + 2.5) as u64) * (channels as u64 * 8 + 24)
                    + (f64::from(rate) / 0.8) as u64 * 4
                    + 1024
            }
            SourceOp::Limiter(_) => (rate as u64 / 200) * (channels as u64 * 48 + 16) + 1024,
        };
        state_bytes = state_bytes
            .checked_add(bytes)
            .context("source effect state overflow")?;
    }
    ensure!(
        state_bytes < budget,
        "source curve effect memory budget exceeded"
    );
    let available = budget - state_bytes;
    let pre = Temp::create(staging, "nut")?;
    let mut command = Command::new(ffmpeg);
    let mut graph = prefix.to_vec();
    graph.push(format!("aformat=sample_fmts={}", input_format.name()));
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-protocol_whitelist",
            super::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(capture.path())
        .args([
            "-map",
            "0:a:0",
            "-af",
            &graph.join(","),
            "-c:a",
            codec,
            "-f",
            "nut",
            "-fs",
            &available.to_string(),
        ])
        .arg(&pre.0);
    let output = capture_output_cancellable(
        runtime,
        command,
        runtime.render_policy(vec![capture.path().into()], pre.0.clone()),
        remaining()?,
        cancel,
    )
    .await?;
    ensure!(
        output.status.success(),
        "source effect tempo capture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pre_bytes = std::fs::metadata(&pre.0)?.len();
    ensure!(
        pre_bytes < available,
        "source effect tempo capture budget exhausted"
    );
    let mut command = Command::new(ffprobe);
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            super::OFFLINE_PROTOCOLS,
            "-select_streams",
            "a:0",
            "-show_frames",
            "-show_entries",
            "frame=nb_samples,best_effort_timestamp:stream=time_base",
            "-of",
            "compact=p=0:nk=0",
        ])
        .arg(&pre.0);
    let mut frames = Vec::new();
    let mut samples = 0u64;
    let mut time_base = None;
    let (_, diagnostic) = capture_lines_until::<()>(
        runtime,
        command,
        runtime.probe_policy(&pre.0),
        remaining()?,
        cancel,
        |line| {
            if let Some(value) = line.strip_prefix("time_base=") {
                let (num, den) = value
                    .split_once('/')
                    .context("invalid effect packet time base")?;
                let (num, den) = (num.parse::<i64>()?, den.parse::<i64>()?);
                ensure!(num > 0 && den > 0, "invalid effect packet time base");
                time_base = Some((num, den));
                return Ok(None);
            }
            let mut count = None;
            let mut pts = None;
            for field in line.split('|') {
                if let Some(v) = field.strip_prefix("nb_samples=") {
                    count = Some(v.parse::<usize>()?);
                }
                if let Some(v) = field.strip_prefix("best_effort_timestamp=") {
                    pts = Some(v.parse::<i64>()?);
                }
            }
            let count = count.context("missing effect input frame size")?;
            let pts = pts.context("missing effect input frame timestamp")?;
            ensure!(
                count > 0 && count <= 1048576 && frames.len() < 1_000_000,
                "source effect packet budget exceeded"
            );
            samples = samples
                .checked_add(count as u64)
                .context("source effect sample overflow")?;
            frames.push((count, pts));
            Ok(None)
        },
    )
    .await?;
    ensure!(
        diagnostic.trim().is_empty() && !frames.is_empty(),
        "source effect frame probe failed: {diagnostic}"
    );
    let (num, den) = time_base.context("effect packet time base missing")?;
    let frames = frames
        .into_iter()
        .map(|(count, pts)| -> Result<_> {
            let value = i128::from(pts)
                .checked_mul(i128::from(num))
                .and_then(|v| v.checked_mul(i128::from(rate)))
                .context("effect timestamp conversion overflow")?;
            ensure!(
                value % i128::from(den) == 0,
                "effect timestamp is not on source sample grid"
            );
            Ok((count, u64::try_from(value / i128::from(den))?))
        })
        .collect::<Result<Vec<_>>>()?;
    let raw_bytes = samples
        .checked_mul(channels as u64 * scalar_bytes)
        .context("source effect PCM overflow")?;
    let output_budget = available
        .checked_sub(pre_bytes)
        .and_then(|v| v.checked_sub(raw_bytes))
        .context("source effect aggregate staging budget exceeded")?;
    let raw = Temp::create(staging, raw_format)?;
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-protocol_whitelist",
            super::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(&pre.0)
        .args([
            "-map",
            "0:a:0",
            "-c:a",
            codec,
            "-f",
            raw_format,
            "-fs",
            &raw_bytes.to_string(),
        ])
        .arg(&raw.0);
    let output = capture_output_cancellable(
        runtime,
        command,
        runtime.render_policy(vec![pre.0.clone()], raw.0.clone()),
        remaining()?,
        cancel,
    )
    .await?;
    ensure!(
        output.status.success() && std::fs::metadata(&raw.0)?.len() == raw_bytes,
        "source effect decoded payload incomplete"
    );
    let check_cancel = cancel.clone();
    let stage = staging.to_owned();
    let deadline = std::time::Instant::now()
        .checked_add(remaining()?)
        .context("source effect deadline overflow")?;
    tokio::task::spawn_blocking(move || -> Result<PreparedEffectFrames> {
        let _pre = pre;
        let mut input = File::open(&raw.0)?;
        let _raw = raw;
        let check = || -> Result<()> {
            ensure!(
                !check_cancel.is_cancelled() && std::time::Instant::now() < deadline,
                "source curve effect cancelled or timed out"
            );
            Ok(())
        };
        use super::{packet_echo::Stage, packet_pitch::Block};
        let mut chain = Vec::new();
        for (operation, format) in operations.into_iter().zip(formats) {
            chain.push(match operation {
                SourceOp::Echo(recipe) => {
                    let decays = recipe.weights[..recipe.count]
                        .iter()
                        .map(|v| *v as f32)
                        .collect::<Vec<_>>();
                    NativeStage::Echo(Stage::new(
                        format,
                        rate,
                        channels,
                        0.8,
                        recipe.gain as f32,
                        &recipe.delays_ms[..recipe.count],
                        &decays,
                    )?)
                }
                SourceOp::Biquad(recipe) => NativeStage::Biquad(super::packet_biquad::Stage::new(
                    format,
                    recipe.design,
                    rate,
                    channels,
                    recipe.frequency,
                    recipe.width,
                    recipe.gain,
                )?),
                SourceOp::Phaser(speed) => NativeStage::Modulation(
                    super::packet_modulation::Stage::phaser(format, rate, channels, speed)?,
                ),
                SourceOp::Flanger => NativeStage::Modulation(
                    super::packet_modulation::Stage::flanger(format, rate, channels)?,
                ),
                SourceOp::Crusher(recipe) => {
                    ensure!(
                        format == Format::F64,
                        "crusher requires negotiated packed double"
                    );
                    NativeStage::Crusher {
                        core: crate::domain::concat_log_crusher::LogCrusher::new(
                            recipe.bits,
                            recipe.mix,
                        ),
                        channels,
                    }
                }
                SourceOp::Limiter(limit) => NativeStage::Limiter(
                    super::packet_limiter::Stage::new(format, rate, channels, limit, 50., true)?,
                ),
            });
        }
        let mut out = AudioPacketSpool::create_format(
            &stage,
            rate,
            channels,
            output_budget,
            output_packet_format,
        )?;
        let mut next_pts = vec![None; chain.len()];
        let push = |mut packet: Block,
                    pts: u64,
                    from: usize,
                    chain: &mut [NativeStage],
                    next_pts: &mut [Option<u64>],
                    out: &mut AudioPacketSpool|
         -> Result<()> {
            for (index, effect) in chain.iter_mut().enumerate().skip(from) {
                check()?;
                packet = effect.push_packet(packet)?;
                next_pts[index] = Some(
                    pts.checked_add(packet.frames() as u64)
                        .context("native Echo timestamp overflow")?,
                );
            }
            check()?;
            match packet {
                Block::S16(v) => out.push_i16_at(&v, pts),
                Block::S32(v) => out.push_i32_at(&v, pts),
                Block::F32(v) => out.push_f32_at(&v, pts),
                Block::F64(v) => out.push_f64_at(&v, pts),
                _ => anyhow::bail!("unexpected native Echo output precision"),
            }
        };
        for (count, pts) in frames {
            check()?;
            let mut bytes = vec![0u8; count * channels * packet_format.bytes()];
            input.read_exact(&mut bytes)?;
            macro_rules! read_packet {
                ($scalar:ty,$variant:ident) => {{
                    let size = std::mem::size_of::<$scalar>();
                    let mut packet = vec![vec![0 as $scalar; count]; channels];
                    for i in 0..count {
                        for ch in 0..channels {
                            let at = (i * channels + ch) * size;
                            packet[ch][i] =
                                <$scalar>::from_le_bytes(bytes[at..at + size].try_into().unwrap());
                        }
                    }
                    Block::$variant(packet)
                }};
            }
            let packet = match packet_format {
                PcmPacketFormat::S16 => read_packet!(i16, S16),
                PcmPacketFormat::S32 => read_packet!(i32, S32),
                PcmPacketFormat::F32 => read_packet!(f32, F32),
                PcmPacketFormat::F64 => read_packet!(f64, F64),
            };
            push(packet, pts, 0, &mut chain, &mut next_pts, &mut out)?;
        }
        for i in 0..chain.len() {
            while let Some(packet) = chain[i].finish_packet() {
                let pts = next_pts[i].context("Echo tail timestamp missing")?;
                next_pts[i] = Some(
                    pts.checked_add(packet.frames() as u64)
                        .context("Echo tail timestamp overflow")?,
                );
                push(packet, pts, i + 1, &mut chain, &mut next_pts, &mut out)?;
            }
        }
        check()?;
        out.seal()?;
        Ok(PreparedEffectFrames {
            spool: out,
            format: output_format,
        })
    })
    .await
    .context("source curve effect task failed")?
}
