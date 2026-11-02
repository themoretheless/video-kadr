//! Policy-bound PCM conforming for trusted local media and bounded disk-backed sample reads.
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
const RATE: u64 = 48000;
const PAGE_FRAMES: u64 = 512;
#[derive(Debug, Clone, Copy)]
pub struct PcmConformSpec {
    /// Source coordinates after normalizing the decoded stream's first PTS to zero.
    pub start_sample: u64,
    pub end_sample: u64,
    pub channels: u32,
    pub staging_bytes: u64,
}
impl PcmConformSpec {
    pub fn validate(self) -> Result<()> {
        ensure!(
            matches!(self.channels, 1 | 2) && self.start_sample < self.end_sample,
            "invalid PCM conform range/channels"
        );
        let bytes = (self.end_sample - self.start_sample)
            .checked_mul(u64::from(self.channels) * 4)
            .context("PCM conform size overflow")?;
        ensure!(
            self.staging_bytes > 0
                && self.staging_bytes <= 8 * 1024 * 1024 * 1024
                && bytes <= self.staging_bytes,
            "PCM conform exceeds staging budget"
        );
        ensure!(
            u128::from(self.end_sample) * 1_000_000 / u128::from(RATE) <= 9_007_199_254_740_991,
            "PCM conform clock exceeds exact range"
        );
        Ok(())
    }
}
struct OwnedPcm(PathBuf);
impl Drop for OwnedPcm {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub struct ConformedPcm {
    // File closes before temporary removal, including on Windows.
    file: Option<File>,
    _owned: OwnedPcm,
    spec: PcmConformSpec,
    decoded_frames: u64,
    page_start: Option<u64>,
    page: Vec<u8>,
}
impl Drop for ConformedPcm {
    fn drop(&mut self) {
        drop(self.file.take());
    }
}
impl ConformedPcm {
    pub(crate) fn adopt_source_stem(stem:super::concat_retime_audio::PreparedCurvePcm,budget:u64)->Result<Self>{
        let(path,start,frames)=stem.into_owned_parts();let owned=OwnedPcm(path);
        let spec=PcmConformSpec{start_sample:start,end_sample:start.checked_add(frames).context("source stem sample extent overflow")?,channels:2,staging_bytes:budget};spec.validate()?;
        let file=File::open(&owned.0)?;ensure!(file.metadata()?.len()==frames.checked_mul(8).context("source stem byte extent overflow")?,"source stem PCM is incomplete");
        Ok(Self{file:Some(file),_owned:owned,spec,decoded_frames:frames,page_start:None,page:Vec::new()})
    }
    pub(crate) fn staged_path(&self)->&Path{&self._owned.0}
    pub fn decoded_frames(&self) -> u64 {
        self.decoded_frames
    }
    pub fn channels(&self) -> u32 {
        self.spec.channels
    }
    /// Nearest sample conversion complements the integer-microsecond authored clock.
    /// Requested selected-source samples beyond a short decoded stream are silence.
    pub fn read(&mut self, source_time_us: u64) -> Result<(u32, [f64; 2])> {
        let numerator = u128::from(source_time_us) * u128::from(RATE);
        let mut absolute = u64::try_from((numerator + 500000) / 1_000_000)?;
        // Reverse starts at sourceOutUs-1; rounding must preserve the exclusive end.
        if absolute == self.spec.end_sample
            && numerator < u128::from(self.spec.end_sample) * 1_000_000
        {
            absolute -= 1;
        }
        ensure!(
            absolute >= self.spec.start_sample && absolute < self.spec.end_sample,
            "PCM source read outside conformed range"
        );
        let frame = absolute - self.spec.start_sample;
        if frame >= self.decoded_frames {
            return Ok((self.spec.channels, [0.0; 2]));
        }
        let page_start = frame / PAGE_FRAMES * PAGE_FRAMES;
        let channels = u64::from(self.spec.channels);
        if self.page_start != Some(page_start) {
            let count = (self.decoded_frames - page_start).min(PAGE_FRAMES) * channels * 4;
            self.page.resize(count as usize, 0);
            let file = self.file.as_mut().context("PCM reader closed")?;
            file.seek(SeekFrom::Start(page_start * channels * 4))?;
            file.read_exact(&mut self.page)?;
            self.page_start = Some(page_start);
        }
        let offset = ((frame - page_start) * channels * 4) as usize;
        let mut sample = [0.0; 2];
        for (channel, value) in sample
            .iter_mut()
            .take(self.spec.channels as usize)
            .enumerate()
        {
            let at = offset + channel * 4;
            *value = f64::from(f32::from_le_bytes(self.page[at..at + 4].try_into()?));
        }
        ensure!(
            sample.iter().all(|value| value.is_finite()),
            "non-finite conformed PCM"
        );
        Ok((self.spec.channels, sample))
    }
}
/// Caller supplies trusted local paths and a writable staging directory. Output is private
/// temporary data, never a library artifact. Channel count must come from trusted probing.
pub async fn conform_pcm(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    spec: PcmConformSpec,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<ConformedPcm> {
    conform_pcm_with_tempo(runtime, ffmpeg, source, staging, spec, timeout, cancel, None).await
}
pub(crate) async fn conform_pcm_with_tempo(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    spec: PcmConformSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    tempo_graph: Option<&str>,
) -> Result<ConformedPcm> {
    conform_pcm_with_tempo_and_stream(runtime,ffmpeg,source,staging,spec,timeout,cancel,tempo_graph,None,false,false).await
}
/// Decode an explicitly selected absolute container stream (not an audio ordinal).
#[allow(clippy::too_many_arguments)]
pub async fn conform_pcm_stream(runtime:&ProcessRuntime,ffmpeg:&Path,source:&Path,staging:&Path,spec:PcmConformSpec,timeout:Duration,cancel:&CancellationToken,stream_index:u32)->Result<ConformedPcm>{
    conform_pcm_with_tempo_and_stream(runtime,ffmpeg,source,staging,spec,timeout,cancel,None,Some(stream_index),true,false).await
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn conform_pcm_with_tempo_and_stream(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    spec: PcmConformSpec,
    timeout: Duration,
    cancel: &CancellationToken,
    tempo_graph: Option<&str>,
    stream_index:Option<u32>,
    stream_relative:bool,
    allow_empty_source:bool,
) -> Result<ConformedPcm> {
    spec.validate()?;
    ensure!(
        !timeout.is_zero() && !cancel.is_cancelled(),
        "PCM conform cancelled or zero timeout"
    );
    let path = staging.join(format!(".pcm-{}.f32le", uuid::Uuid::new_v4()));
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    let owned = OwnedPcm(path.clone());
    drop(file);
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
        .arg(source)
        .args(["-vn", "-sn", "-dn"]);
    let binding=stream_index.map_or_else(||"0:a:0".to_owned(),|index|format!("0:{index}"));
    // Concat explicit stream time starts at that stream's first sample. Legacy
    // default media audio retains its container-relative leading silence.
    let origin=if stream_relative{"asetpts=PTS-STARTPTS,"}else{""};
    if let Some(graph) = tempo_graph {
        let graph = graph.replacen("[0:a:0]", &format!("[{binding}]{origin}aresample=48000:async=1:first_pts=0,"), 1);
        command.arg("-filter_complex").arg(format!("{graph};[native_tempo]atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS[native_pcm]", spec.start_sample, spec.end_sample)).args(["-map", "[native_pcm]"]);
    } else {
        command.args(["-map", binding.as_str(), "-af"]).arg(format!(
            "{origin}aresample={RATE}:async=1:first_pts=0,atrim=start_sample={}:end_sample={}",
            spec.start_sample, spec.end_sample
        ));
    }
    command
        .arg("-ar")
        .arg(RATE.to_string())
        .arg("-ac")
        .arg(spec.channels.to_string())
        .args(["-c:a", "pcm_f32le", "-f", "f32le", "-fs"])
        .arg(spec.staging_bytes.to_string())
        .args(["-progress", "pipe:1"])
        .arg(&path);
    let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let (status, diagnostic) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(vec![source.to_path_buf()], &path),
        |_| None,
        &progress,
        cancel,
        timeout,
    )
    .await?;
    ensure!(
        matches!(status, ProcessStatus::Ok) && !cancel.is_cancelled(),
        "PCM conform failed or cancelled: {status:?} {diagnostic}"
    );
    let size = tokio::fs::metadata(&path).await?.len();
    let frame_bytes = u64::from(spec.channels) * 4;
    let maximum = (spec.end_sample - spec.start_sample) * frame_bytes;
    ensure!(
        (size > 0 || allow_empty_source) && size % frame_bytes == 0 && size <= maximum && size <= spec.staging_bytes,
        "PCM conform produced invalid or over-budget samples"
    );
    let file = File::open(&path)?;
    Ok(ConformedPcm {
        file: Some(file),
        _owned: owned,
        spec,
        decoded_frames: size / frame_bytes,
        page_start: None,
        page: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    fn wav(path: &Path, frames: usize, channels: u16) {
        let bytes = (frames * channels as usize * 2) as u32;
        let mut data = Vec::new();
        data.extend(b"RIFF");
        data.extend((36 + bytes).to_le_bytes());
        data.extend(b"WAVEfmt ");
        data.extend(16u32.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(channels.to_le_bytes());
        data.extend(48000u32.to_le_bytes());
        data.extend((48000u32 * u32::from(channels) * 2).to_le_bytes());
        data.extend((channels * 2).to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(b"data");
        data.extend(bytes.to_le_bytes());
        for i in 0..frames {
            data.extend(((i % 1000) as i16 - 500).to_le_bytes());
            if channels == 2 {
                data.extend(16384i16.to_le_bytes());
            }
        }
        std::fs::write(path, data).unwrap();
    }
    fn available() -> bool {
        StdCommand::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
    }
    fn spec(channels: u32) -> PcmConformSpec {
        PcmConformSpec {
            start_sample: 100,
            end_sample: 1200,
            channels,
            staging_bytes: 8800,
        }
    }
    fn time(sample: u64) -> u64 {
        (sample * 1_000_000 + 24000) / 48000
    }
    #[tokio::test]
    async fn real_pcm_conform_trims_keeps_channels_caches_reverse_reads_and_cleans_owned_file() {
        if !available() {
            eprintln!("FFmpeg unavailable: PCM conform fixture skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("stereo.wav");
        wav(&source, 1152, 2);
        let runtime = ProcessRuntime::local_default();
        let cancel = CancellationToken::new();
        let mut pcm = conform_pcm(
            &runtime,
            Path::new("ffmpeg"),
            &source,
            dir.path(),
            spec(2),
            Duration::from_secs(5),
            &cancel,
        )
        .await
        .unwrap();
        let owned = pcm._owned.0.clone();
        assert!(owned.exists());
        assert_eq!(pcm.channels(), 2);
        assert_eq!(pcm.decoded_frames(), 1052);
        for index in [100, 101, 611, 612, 1124, 1151, 612, 100] {
            let (channels, sample) = pcm.read(time(index)).unwrap();
            assert_eq!(channels, 2);
            assert_eq!(sample, [((index % 1000) as f64 - 500.0) / 32768.0, 0.5]);
            assert!(pcm.page.len() <= 4096);
        }
        assert_eq!(pcm.read(time(1170)).unwrap(), (2, [0.0, 0.0]));
        assert!(pcm.read(time(99)).is_err());
        assert!(pcm.read(time(1200)).is_err());
        drop(pcm);
        assert!(!owned.exists());
        assert!(source.exists());
    }
    #[tokio::test]
    async fn real_mono_pcm_and_failed_source_leave_no_owned_staging() {
        if !available() {
            eprintln!("FFmpeg unavailable: mono PCM fixture skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("mono.wav");
        wav(&source, 1200, 1);
        let runtime = ProcessRuntime::local_default();
        let cancel = CancellationToken::new();
        let mut pcm = conform_pcm(
            &runtime,
            Path::new("ffmpeg"),
            &source,
            dir.path(),
            spec(1),
            Duration::from_secs(5),
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(pcm.read(time(100)).unwrap(), (1, [-400.0 / 32768.0, 0.0]));
        assert_eq!(pcm.read(24999).unwrap(), (1, [-301.0 / 32768.0, 0.0]));
        assert!(pcm.read(25000).is_err());
        let clip = serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":2083,"sourceOutTick":25000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0})).unwrap();
        let layer = crate::domain::nested_audio::NestedAudioLayer {
            video_transitions: None, concat_retime_piece:None, concat_retime_owner:None,export_fade_clock:false,gain_track:None,
            composition: "child".into(),
            instance_path: vec!["group".into()],
            root_range_us: 0..22917,
            clip_local_start_us: 0,
            crossfade: Default::default(),
            clip,
        };
        let block = crate::domain::nested_pcm::mix_block(
            &[layer],
            0,
            8,
            48000,
            &std::sync::atomic::AtomicBool::new(false),
            |_, time| pcm.read(time),
        )
        .unwrap();
        for (index, frame) in block.iter().enumerate() {
            let expected = (-400.0 + index as f64) / 32768.0 * std::f64::consts::FRAC_1_SQRT_2;
            assert!((frame[0] - expected).abs() < 1e-12);
            assert!((frame[0] - frame[1]).abs() < 1e-12);
        }
        drop(pcm);
        let invalid = dir.path().join("invalid.wav");
        std::fs::write(&invalid, b"bad audio").unwrap();
        assert!(conform_pcm(
            &runtime,
            Path::new("ffmpeg"),
            &invalid,
            dir.path(),
            spec(1),
            Duration::from_secs(5),
            &cancel
        )
        .await
        .is_err());
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|v| v
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pcm-")));
    }
    #[test]
    fn budget_range_and_channel_admission() {
        assert!(spec(2).validate().is_ok());
        assert!(PcmConformSpec {
            staging_bytes: 8799,
            ..spec(2)
        }
        .validate()
        .is_err());
        assert!(PcmConformSpec {
            start_sample: 1200,
            ..spec(2)
        }
        .validate()
        .is_err());
        assert!(spec(6).validate().is_err());
        assert!(PcmConformSpec {
            end_sample: u64::MAX,
            ..spec(2)
        }
        .validate()
        .is_err());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_and_cancellation_reap_decoder_and_remove_owned_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("decoder");
        std::fs::write(&executable, b"#!/bin/sh\nexec sleep 10\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let source = dir.path().join("source.wav");
        wav(&source, 1200, 2);
        let runtime = ProcessRuntime::local_default();
        let cancel = CancellationToken::new();
        assert!(conform_pcm(
            &runtime,
            &executable,
            &source,
            dir.path(),
            spec(2),
            Duration::from_millis(50),
            &cancel
        )
        .await
        .is_err());
        let trigger = cancel.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            trigger.cancel()
        });
        assert!(conform_pcm(
            &runtime,
            &executable,
            &source,
            dir.path(),
            spec(2),
            Duration::from_secs(5),
            &cancel
        )
        .await
        .is_err());
        task.await.unwrap();
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|v| v
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".pcm-")));
    }
}

/// Probe the actual first audio stream through the same cancellable process boundary.
/// Probe the actual first decoded audio stream, retaining original scalar/layout,
/// rate and PTS metadata for variable-frame DSP. This shares cancellation and
/// offline process policy with PCM conformance.
pub async fn probe_pcm_metadata(
    runtime: &ProcessRuntime,
    ffprobe: &Path,
    source: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<crate::domain::media_probe::ProbeResult> {
    probe_pcm_metadata_for_stream(runtime,ffprobe,source,timeout,cancel,None).await
}
/// Resolve one absolute stream and reject video/missing streams without fallback.
pub async fn probe_pcm_metadata_for_stream(
    runtime: &ProcessRuntime,
    ffprobe: &Path,
    source: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
    stream_index:Option<u32>,
) -> Result<crate::domain::media_probe::ProbeResult> {
    ensure!(!timeout.is_zero() && !cancel.is_cancelled(), "PCM probe cancelled or zero timeout");
    let mut command = Command::new(ffprobe);
    let selector=stream_index.map_or_else(||"a:0".to_owned(),|index|index.to_string());
    command.args(["-v","error","-protocol_whitelist",super::OFFLINE_PROTOCOLS,
        "-select_streams",selector.as_str(),"-show_streams","-show_format","-of","json"]).arg(source);
    let output = crate::process_control::capture_output_cancellable(runtime, command,
        runtime.probe_policy(source), timeout, cancel).await?;
    ensure!(output.status.success() && !cancel.is_cancelled(), "PCM metadata probe failed or cancelled");
    let value: serde_json::Value=serde_json::from_slice(&output.stdout).context("invalid PCM probe JSON")?;
    let probe=crate::domain::media_probe::ProbeResult::from_ffprobe_json(&value)?;
    let audio=probe.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Audio).context("source has no probed audio stream")?;
    ensure!(stream_index.map_or(true,|index|audio.index==index),"PCM probe selected a different stream");
    ensure!(audio.channels.is_some_and(|ch|matches!(ch,1|2)), "native PCM supports probed mono/stereo only");
    Ok(probe)
}
pub async fn probe_pcm_channels(
    runtime: &ProcessRuntime,
    ffprobe: &Path,
    source: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<u32> {
    let probe=probe_pcm_metadata(runtime,ffprobe,source,timeout,cancel).await?;
    Ok(probe.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Audio)
        .and_then(|s|s.channels).context("source has no probed audio channels")?)
}

#[cfg(test)]
mod pts_tests {
    use super::*;
    #[tokio::test]
    async fn delayed_audio_keeps_silence_relative_to_media_origin_including_nonzero_container_pts()
    {
        let ffmpeg = std::process::Command::new("ffmpeg")
            .arg("-version")
            .output();
        if !ffmpeg.is_ok_and(|v| v.status.success()) {
            eprintln!("FFmpeg unavailable: delayed PCM fixture skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for origin in [0, 5] {
            let source = dir.path().join(format!("delayed-{origin}.mkv"));
            let fixture = std::process::Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:s=2x2:r=2:d=2",
                    "-f",
                    "lavfi",
                    "-i",
                    "aevalsrc=0.25:s=48000:d=1",
                    "-filter_complex",
                ])
                .arg(format!(
                    "[0:v]setpts=PTS+{origin}/TB[v];[1:a]asetpts=PTS+({origin}+0.5)/TB[a]"
                ))
                .args([
                    "-map",
                    "[v]",
                    "-map",
                    "[a]",
                    "-c:v",
                    "ffv1",
                    "-c:a",
                    "pcm_f32le",
                    "-copyts",
                ])
                .arg(&source)
                .output()
                .unwrap();
            assert!(
                fixture.status.success(),
                "{}",
                String::from_utf8_lossy(&fixture.stderr)
            );
            let mut pcm = conform_pcm(
                &ProcessRuntime::local_default(),
                Path::new("ffmpeg"),
                &source,
                dir.path(),
                PcmConformSpec {
                    start_sample: 0,
                    end_sample: 96000,
                    channels: 1,
                    staging_bytes: 384000,
                },
                Duration::from_secs(5),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            for (time, value) in [
                (0, 0.0),
                (499979, 0.0),
                (500000, 0.25),
                (1499979, 0.25),
                (1500000, 0.0),
                (1999979, 0.0),
            ] {
                assert_eq!(
                    pcm.read(time).unwrap(),
                    (1, [value, 0.0]),
                    "media origin={origin}, source time={time}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tempo_pitch_tests {
    use super::*;
    #[tokio::test]
    async fn real_tempo_conform_preserves_pitch_and_cleans_staging_for_slow_and_ramp() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("pitch.wav");
        let encoded = std::process::Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=400:sample_rate=48000:duration=2","-c:a","pcm_s16le"]).arg(&source).output().unwrap();
        assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
        let runtime = ProcessRuntime::local_default();
        let cancel = CancellationToken::new();
        for ramped in [false,true] {
            let mut placement: crate::domain::composition::ClipPlacement = serde_json::from_value(serde_json::json!({"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":2000000,"speed":0.5})).unwrap();
            if ramped {
                placement.speed=1.0;
                placement.speed_ramp=Some(serde_json::from_value(serde_json::json!({"interpolation":"linear","audioPolicy":"preserve_pitch","points":[{"sourceProgressTick":0,"speed":1.0},{"sourceProgressTick":2000000,"speed":2.0}]})).unwrap());
            }
            let (graph, duration) = crate::tools::native_tempo_source_filter(&placement,false).unwrap();
            assert!(duration>1250000);
            let spec = PcmConformSpec {start_sample:12000,end_sample:60000,channels:2,staging_bytes:384000};
            let mut pcm=conform_pcm_with_tempo(&runtime,Path::new("ffmpeg"),&source,dir.path(),spec,Duration::from_secs(10),&cancel,Some(&graph)).await.unwrap();
            let owned=pcm._owned.0.clone();
            assert_eq!(pcm.decoded_frames(),48000);
            let mut previous=0.0; let mut crossings=0; let mut energy=0.0;
            for frame in 12000..60000u64 {
                let (channels,sample)=pcm.read((frame*1000000+24000)/48000).unwrap();
                assert_eq!(channels,2); assert_eq!(sample[0],sample[1]);
                if previous<0.0 && sample[0]>=0.0 {crossings+=1;}
                previous=sample[0]; energy+=sample[0]*sample[0];
            }
            assert!((397..=403).contains(&crossings),"ramped{ramped}: {crossings}Hz instead of400Hz");
            assert!(energy/48000.0>0.001,"tempo output must not be silence");
            drop(pcm); assert!(!owned.exists()); assert!(source.exists());
            let cancelled=CancellationToken::new();cancelled.cancel();
            assert!(conform_pcm_with_tempo(&runtime,Path::new("ffmpeg"),&source,dir.path(),spec,Duration::from_secs(10),&cancelled,Some(&graph)).await.is_err());
        }
    }
}

#[cfg(test)]
mod source_metadata_tests {
    use super::*;
    #[tokio::test]
    async fn trusted_audio_metadata_retains_decoder_rate_scalar_and_layout() {
        let dir=tempfile::tempdir().unwrap();
        let runtime=ProcessRuntime::local_default();
        for (codec,extension,format) in [("pcm_u8","wav","u8"),("pcm_s16le","wav","s16"),("pcm_s32le","wav","s32"),("pcm_f32le","wav","flt"),("pcm_f64le","wav","dbl"),("alac","m4a","s16p")] {
            let path=dir.path().join(format!("{codec}.{extension}"));
            let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","aevalsrc=0.4*sin(2*PI*317*t)|0.3*cos(2*PI*997*t):s=44100:d=0.04","-c:a",codec])
                .args(if codec=="alac" {vec!["-sample_fmt","s16p"]} else {vec![]}).arg(&path).output().unwrap();
            assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
            let probe=probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();
            assert_eq!(probe.streams.len(),1);
            let stream=&probe.streams[0];
            assert_eq!(stream.sample_rate,Some(44100));
            assert_eq!(stream.channels,Some(2));
            assert_eq!(stream.sample_format.as_deref(),Some(format));
            let (_,rate,channels)=crate::tools::packet_pcm::Format::from_probe(&probe).unwrap();
            assert_eq!((rate,channels),(44100,2));
            let binding=crate::tools::native_audio_export::PcmSource::from_probe(path.clone(),probe.clone()).unwrap();
            assert_eq!(binding.channels,2);
            assert_eq!(binding.probe.as_ref(),Some(&probe));
            let mut invalid=probe.clone();invalid.streams[0].channels=Some(6);
            assert!(crate::tools::native_audio_export::PcmSource::from_probe(path.clone(),invalid).is_err());
            let cancelled=CancellationToken::new();cancelled.cancel();
            assert!(probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::from_secs(10),&cancelled).await.is_err());
            assert!(probe_pcm_metadata(&runtime,Path::new("ffprobe"),&path,Duration::ZERO,&CancellationToken::new()).await.is_err());
        }
        let image=dir.path().join("no-audio.ppm");
        std::fs::write(&image,b"P6\n1 1\n255\n\x00\x00\x00").unwrap();
        assert!(probe_pcm_metadata(&runtime,Path::new("ffprobe"),&image,Duration::from_secs(10),&CancellationToken::new()).await.is_err());
    }
}
