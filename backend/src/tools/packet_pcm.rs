//! Disk-backed decoded PCM that retains source-rate packets for variable-frame DSP.
//! FFmpeg only decodes/trims/packs samples here; no rate conversion or DSP is applied.
use crate::process_control::{stream_with_progress, ProcessRuntime, ProcessStatus};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    marker::PhantomData,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub(crate) trait Scalar: Copy + Default {
    const FORMAT: &'static str;
    const CODEC: &'static str;
    const BYTES: usize;
    fn from_bytes(bytes: &[u8]) -> Self;
    fn valid(self) -> bool {
        true
    }
}
macro_rules! scalar {
    ($ty:ty, $format:literal, $codec:literal, $bytes:literal) => {
        impl Scalar for $ty {
            const FORMAT: &'static str = $format;
            const CODEC: &'static str = $codec;
            const BYTES: usize = $bytes;
            fn from_bytes(bytes: &[u8]) -> Self {
                Self::from_le_bytes(bytes.try_into().unwrap())
            }
            fn valid(self) -> bool {
                (self as f64).is_finite()
            }
        }
    };
}
scalar!(u8, "u8", "pcm_u8", 1);
scalar!(i16, "s16le", "pcm_s16le", 2);
scalar!(i32, "s32le", "pcm_s32le", 4);
scalar!(f32, "f32le", "pcm_f32le", 4);
scalar!(f64, "f64le", "pcm_f64le", 8);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    U8,
    S16,
    S32,
    F32,
    F64,
}
pub(crate) enum Decoded {
    U8(Frames<u8>),
    S16(Frames<i16>),
    S32(Frames<i32>),
    F32(Frames<f32>),
    F64(Frames<f64>),
}
impl Format {
    /// Derives shape from the first audio entry of the trusted selected-stream probe.
    pub(crate) fn from_probe(
        probe: &crate::domain::media_probe::ProbeResult,
    ) -> Result<(Self, u32, usize)> {
        let stream = probe
            .streams
            .iter()
            .find(|s| s.kind == crate::domain::media_probe::StreamKind::Audio)
            .context("source has no probed audio stream")?;
        let format = match stream.sample_format.as_deref() {
            Some("u8" | "u8p") => Self::U8,
            Some("s16" | "s16p") => Self::S16,
            Some("s32" | "s32p") => Self::S32,
            Some("flt" | "fltp") => Self::F32,
            Some("dbl" | "dblp") => Self::F64,
            _ => anyhow::bail!("missing or unsupported decoded source sample format"),
        };
        let rate = stream.sample_rate.context("source sample rate missing")?;
        let channels = stream.channels.context("source channel count missing")? as usize;
        ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "unsupported source audio shape"
        );
        Ok((format, rate, channels))
    }
}
/// Decoder dispatch derives scalar/rate/channels from probe data, never from
/// caller defaults. Old metadata without sampleFormat requires fresh probing.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn decode_probed(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    probe: &crate::domain::media_probe::ProbeResult,
    bounds: std::ops::Range<u64>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Decoded> {
    let (format, rate, channels) = Format::from_probe(probe)?;
    let stream_index=probe.streams.iter().find(|stream|stream.kind==crate::domain::media_probe::StreamKind::Audio).context("source audio stream missing")?.index;
    let spec = Spec {
        rate,
        channels,
        source_planar: probe
            .streams
            .iter()
            .find(|s| s.kind == crate::domain::media_probe::StreamKind::Audio)
            .and_then(|s| s.sample_format.as_deref())
            .is_some_and(|f| f.ends_with('p')),
        start: bounds.start,
        end: bounds.end,
        staging_bytes,
    };
    Ok(match format {
        Format::U8 => {
            Decoded::U8(decode_stream(runtime, ffmpeg, source, staging, spec, timeout, cancel,Some(stream_index)).await?)
        }
        Format::S16 => {
            Decoded::S16(decode_stream(runtime, ffmpeg, source, staging, spec, timeout, cancel,Some(stream_index)).await?)
        }
        Format::S32 => {
            Decoded::S32(decode_stream(runtime, ffmpeg, source, staging, spec, timeout, cancel,Some(stream_index)).await?)
        }
        Format::F32 => {
            Decoded::F32(decode_stream(runtime, ffmpeg, source, staging, spec, timeout, cancel,Some(stream_index)).await?)
        }
        Format::F64 => {
            Decoded::F64(decode_stream(runtime, ffmpeg, source, staging, spec, timeout, cancel,Some(stream_index)).await?)
        }
    })
}
#[derive(Clone, Copy)]
pub(crate) struct Spec {
    pub rate: u32,
    pub channels: usize,
    /// Original decoder layout; the disk spool itself is always packed.
    pub source_planar: bool,
    pub start: u64,
    pub end: u64,
    pub staging_bytes: u64,
}
struct OwnedDirectory(PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Packet {
    first: u64,
    frames: usize,
    offset: u64,
    hash: [u8; 32],
}
pub(crate) struct Frames<T: Scalar> {
    file: Option<File>,
    _owned: OwnedDirectory,
    spec: Spec,
    packets: Vec<Packet>,
    frames: u64,
    cached: Option<usize>,
    bytes: Vec<u8>,
    scalar: PhantomData<T>,
}
impl<T: Scalar> Drop for Frames<T> {
    fn drop(&mut self) {
        drop(self.file.take());
    }
}
impl<T: Scalar> Frames<T> {
    pub(crate) fn spec(&self) -> Spec {
        self.spec
    }
    pub(crate) fn end(&self) -> u64 {
        self.spec.start + self.frames
    }
    pub(crate) fn read_packet(&mut self, sample: i64, max_frames: usize) -> Result<Vec<Vec<T>>> {
        let sample = u64::try_from(sample).context("negative decoded PCM coordinate")?;
        ensure!(
            sample >= self.spec.start && sample < self.end() && (1..=65536).contains(&max_frames),
            "decoded PCM read outside source bounds"
        );
        let relative = sample - self.spec.start;
        let index = self
            .packets
            .partition_point(|p| p.first + p.frames as u64 <= relative);
        let packet = self
            .packets
            .get(index)
            .context("decoded PCM packet missing")?;
        if self.cached != Some(index) {
            self.cached = None;
            self.bytes
                .resize(packet.frames * self.spec.channels * T::BYTES, 0);
            let file = self.file.as_mut().context("decoded PCM reader closed")?;
            file.seek(SeekFrom::Start(packet.offset))?;
            file.read_exact(&mut self.bytes)?;
            ensure!(
                <[u8; 32]>::from(Sha256::digest(&self.bytes)) == packet.hash,
                "decoded PCM packet checksum mismatch"
            );
            self.cached = Some(index);
        }
        let offset = (relative - packet.first) as usize;
        let count = (packet.frames - offset).min(max_frames);
        let mut planar = vec![Vec::with_capacity(count); self.spec.channels];
        for frame in offset..offset + count {
            for (channel, dst) in planar.iter_mut().enumerate() {
                let at = (frame * self.spec.channels + channel) * T::BYTES;
                let value = T::from_bytes(&self.bytes[at..at + T::BYTES]);
                ensure!(value.valid(), "nonfinite decoded PCM sample");
                dst.push(value);
            }
        }
        Ok(planar)
    }
}
fn index<T: Scalar>(text: &str, spec: Spec, raw_bytes: u64) -> Result<(Vec<Packet>, u64)> {
    ensure!(
        text.lines()
            .any(|l| l == format!("#sample_rate 0: {}", spec.rate)),
        "decoded PCM sample rate mismatch"
    );
    ensure!(
        text.lines().any(|l| l == format!("#tb 0: 1/{}", spec.rate)),
        "decoded PCM time base mismatch"
    );
    ensure!(
        text.lines()
            .any(|l| l == format!("#codec_id 0: {}", T::CODEC)),
        "decoded PCM scalar mismatch"
    );
    ensure!(
        text.lines().any(|l| l == "#hash: SHA256"),
        "decoded PCM checksum format mismatch"
    );
    let mut packets = Vec::new();
    let mut frames = 0u64;
    let mut offset = 0u64;
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        ensure!(
            packets.len() < 1_000_000,
            "decoded PCM frame index exceeds budget"
        );
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        ensure!(
            fields.len() == 6 && fields[0] == "0",
            "invalid decoded PCM frame record"
        );
        let _: i64 = fields[1].parse()?;
        let _: i64 = fields[2].parse()?;
        let count: usize = fields[3].parse()?;
        let size: u64 = fields[4].parse()?;
        ensure!(
            (1..=65536).contains(&count) && size == (count * spec.channels * T::BYTES) as u64,
            "decoded PCM frame shape mismatch"
        );
        let hash = fields[5];
        ensure!(
            hash.len() == 64 && hash.is_ascii(),
            "invalid decoded PCM frame checksum"
        );
        let mut digest = [0; 32];
        for (i, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16)?;
        }
        packets.push(Packet {
            first: frames,
            frames: count,
            offset,
            hash: digest,
        });
        frames = frames
            .checked_add(count as u64)
            .context("decoded PCM duration overflow")?;
        offset = offset
            .checked_add(size)
            .context("decoded PCM byte offset overflow")?;
        ensure!(
            frames <= spec.end - spec.start && offset <= raw_bytes,
            "decoded PCM index exceeds source bounds"
        );
    }
    ensure!(offset == raw_bytes, "decoded PCM/index length mismatch");
    Ok((packets, frames))
}
/// `T`, rate and channels must come from trusted source probing. The framehash
/// headers and packet byte counts verify the negotiated output shape. No `-ar`
/// or `-ac` conversion is requested. Only private owned staging paths are used.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) async fn decode<T: Scalar>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    spec: Spec,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Frames<T>> {
    decode_stream(runtime,ffmpeg,source,staging,spec,timeout,cancel,None).await
}
#[allow(clippy::too_many_arguments)]
async fn decode_stream<T: Scalar>(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    source: &Path,
    staging: &Path,
    spec: Spec,
    timeout: Duration,
    cancel: &CancellationToken,
    stream_index:Option<u32>,
) -> Result<Frames<T>> {
    ensure!(
        (8000..=192000).contains(&spec.rate) && (1..=16).contains(&spec.channels),
        "invalid decoded PCM rate/channels"
    );
    let count = spec
        .end
        .checked_sub(spec.start)
        .context("invalid decoded PCM bounds")?;
    ensure!(
        count > 0 && count <= u64::from(spec.rate) * 6 * 60 * 60 && spec.end <= i64::MAX as u64,
        "decoded PCM source exceeds budget"
    );
    ensure!(
        spec.staging_bytes > 512
            && spec.staging_bytes <= 8 * 1024 * 1024 * 1024
            && !timeout.is_zero()
            && !cancel.is_cancelled(),
        "decoded PCM cancelled or invalid budget"
    );
    let maximum = count
        .checked_mul((spec.channels * T::BYTES) as u64)
        .context("decoded PCM size overflow")?;
    let metadata_budget = spec
        .staging_bytes
        .checked_sub(maximum)
        .context("decoded PCM staging budget exceeded")?
        .min(64 * 1024 * 1024);
    ensure!(
        metadata_budget >= 512,
        "decoded PCM frame index budget missing"
    );
    let directory = staging.join(format!(".packet-pcm-{}", uuid::Uuid::new_v4()));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&directory)?;
    let owned = OwnedDirectory(directory.clone());
    let raw = directory.join("samples.pcm");
    let metadata = directory.join("frames.sha256");
    let trim = format!(
        "atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS",
        spec.start, spec.end
    );
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-nostdin",
            "-v",
            "error",
            "-n",
            "-protocol_whitelist",
            super::OFFLINE_PROTOCOLS,
            "-i",
        ])
        .arg(source);
    let binding=stream_index.map_or_else(||"0:a:0".to_owned(),|index|format!("0:{index}"));
    for (path, format, limit) in [
        (&raw, T::FORMAT, maximum),
        (&metadata, "framehash", metadata_budget),
    ] {
        command
            .args(["-map", binding.as_str(), "-vn", "-sn", "-dn", "-af"])
            .arg(&trim)
            .args(["-c:a", T::CODEC, "-f", format, "-fs"])
            .arg(limit.to_string())
            .arg(path);
    }
    command.args(["-progress", "pipe:1"]);
    let (progress, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let (status, diagnostic) = stream_with_progress(
        runtime,
        command,
        runtime.render_policy(vec![source.to_path_buf()], &raw),
        |_| None,
        &progress,
        cancel,
        timeout,
    )
    .await?;
    ensure!(
        matches!(status, ProcessStatus::Ok) && !cancel.is_cancelled(),
        "decoded PCM failed or cancelled: {status:?} {diagnostic}"
    );
    let raw_bytes = std::fs::metadata(&raw)?.len();
    let metadata_bytes = std::fs::metadata(&metadata)?.len();
    ensure!(
        raw_bytes <= maximum
            && metadata_bytes <= metadata_budget
            && raw_bytes + metadata_bytes <= spec.staging_bytes,
        "decoded PCM staging output exceeds budget"
    );
    let text = std::fs::read_to_string(metadata)?;
    let (packets, frames) = index::<T>(&text, spec, raw_bytes)?;
    Ok(Frames {
        file: Some(File::open(raw)?),
        _owned: owned,
        spec,
        packets,
        frames,
        cached: None,
        bytes: Vec::new(),
        scalar: PhantomData,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::concat_pitch::{Pitch, Sample};
    use std::io::Write;
    trait Signal: Scalar + Sample + PartialEq + std::fmt::Debug {
        const WAV: u16;
        const TOLERANCE: f64;
        fn signal(index: usize, channel: usize) -> Self;
        fn bytes(self) -> Vec<u8>;
        fn number(self) -> f64;
        fn take(decoded: Decoded) -> Frames<Self>;
        fn block(block: super::super::packet_pitch::Block) -> Vec<Vec<Self>>;
    }
    macro_rules! signal {
        ($ty:ty, $variant:ident, $wav:literal, $tol:literal, $scale:literal, $bias:literal) => {
            impl Signal for $ty {
                fn block(block: super::super::packet_pitch::Block) -> Vec<Vec<Self>> {
                    match block {
                        super::super::packet_pitch::Block::$variant(values) => values,
                        _ => panic!("wrong Pitch output scalar"),
                    }
                }
                fn take(decoded: Decoded) -> Frames<Self> {
                    match decoded {
                        Decoded::$variant(frames) => frames,
                        _ => panic!("wrong probed scalar dispatch"),
                    }
                }
                const WAV: u16 = $wav;
                const TOLERANCE: f64 = $tol;
                fn signal(i: usize, ch: usize) -> Self {
                    ($bias + ((i * 37 + ch * 1061) % 2049) as f64 * $scale) as Self
                }
                fn bytes(self) -> Vec<u8> {
                    self.to_le_bytes().to_vec()
                }
                fn number(self) -> f64 {
                    self as f64
                }
            }
        };
    }
    signal!(u8, U8, 1, 0., 0.1, 16.);
    signal!(i16, S16, 1, 0., 17., -17000.);
    signal!(i32, S32, 1, 0., 700001., -700000001.);
    signal!(f32, F32, 3, 2e-7, 0.000244140625, -0.25);
    signal!(f64, F64, 3, 2e-12, 0.000244140626, -0.25);
    async fn qualify<T: Signal>() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        let runtime = ProcessRuntime::local_default();
        for rate in [8000, 44100, 48000, 192000] {
            for channels in [1, 2] {
                let frames = 8193usize;
                let size = (frames * channels * T::BYTES) as u32;
                let mut file = File::create(&source).unwrap();
                file.write_all(b"RIFF").unwrap();
                file.write_all(&(36 + size).to_le_bytes()).unwrap();
                file.write_all(b"WAVEfmt ").unwrap();
                file.write_all(&16u32.to_le_bytes()).unwrap();
                file.write_all(&T::WAV.to_le_bytes()).unwrap();
                file.write_all(&(channels as u16).to_le_bytes()).unwrap();
                file.write_all(&(rate as u32).to_le_bytes()).unwrap();
                file.write_all(&(rate as u32 * channels as u32 * T::BYTES as u32).to_le_bytes())
                    .unwrap();
                file.write_all(&((channels * T::BYTES) as u16).to_le_bytes())
                    .unwrap();
                file.write_all(&((T::BYTES * 8) as u16).to_le_bytes())
                    .unwrap();
                file.write_all(b"data").unwrap();
                file.write_all(&size.to_le_bytes()).unwrap();
                for i in 0..frames {
                    for ch in 0..channels {
                        file.write_all(&T::signal(i, ch).bytes()).unwrap();
                    }
                }
                drop(file);
                let spec = Spec {
                    rate,
                    channels,
                    source_planar: false,
                    start: 37,
                    end: (frames - 113) as u64,
                    staging_bytes: 4 * 1024 * 1024,
                };
                let cancel = CancellationToken::new();
                let probe = crate::tools::probe_video(&runtime, &source).await.unwrap();
                let decoded = decode_probed(
                    &runtime,
                    Path::new("ffmpeg"),
                    &source,
                    dir.path(),
                    &probe,
                    spec.start..spec.end,
                    spec.staging_bytes,
                    Duration::from_secs(20),
                    &cancel,
                )
                .await
                .unwrap();
                let mut decoded = T::take(decoded);
                let owned = decoded._owned.0.clone();
                assert_eq!(decoded.end(), spec.end);
                for at in [spec.start, 1024, 4096, spec.end - 1, 1024, spec.start] {
                    let packet = decoded.read_packet(at as i64, 65536).unwrap();
                    assert!(!packet[0].is_empty());
                    for (ch, values) in packet.iter().enumerate() {
                        for (i, &value) in values.iter().enumerate() {
                            assert_eq!(value, T::signal(at as usize + i, ch));
                        }
                    }
                }
                // Actual indexed decoder packets, not reconstructed uniform frames.
                let mut pitch = Pitch::<T>::new(rate, channels, 7.);
                let mut actual = vec![Vec::new(); channels];
                let mut at = spec.start;
                while at < decoded.end() {
                    let data = decoded
                        .read_packet(at as i64, (decoded.end() - at).min(65536) as usize)
                        .unwrap();
                    at += data[0].len() as u64;
                    let refs: Vec<&[T]> = data.iter().map(Vec::as_slice).collect();
                    let out = pitch.push_packet(&refs, false);
                    for (dst, src) in actual.iter_mut().zip(out) {
                        dst.extend(src);
                    }
                }
                let empty = vec![&[][..]; channels];
                for (dst, src) in actual.iter_mut().zip(pitch.push_packet(&empty, true)) {
                    dst.extend(src);
                }
                let fixture: serde_json::Value = serde_json::from_str(include_str!(
                    "../../tests/fixtures/concat-pitch-pcm.json"
                ))
                .unwrap();
                let chain = fixture["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| {
                        c["rate"] == rate
                            && c["channels"] == channels
                            && c["settings"][0]["params"]["semitones"].as_f64() == Some(7.)
                    })
                    .unwrap()["chain"]
                    .as_str()
                    .unwrap();
                let graph = format!(
                    "atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{chain}",
                    spec.start, spec.end
                );
                let reference = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&source)
                    .args(["-af", &graph, "-c:a", T::CODEC, "-f", T::FORMAT, "pipe:1"])
                    .output()
                    .unwrap();
                assert!(
                    reference.status.success(),
                    "{}",
                    String::from_utf8_lossy(&reference.stderr)
                );
                assert_eq!(
                    actual[0].len() * channels * T::BYTES,
                    reference.stdout.len()
                );
                for (i, raw) in reference.stdout.chunks_exact(T::BYTES).enumerate() {
                    let expected = T::from_bytes(raw).number();
                    let value = actual[i % channels][i / channels].number();
                    assert!(
                        (value - expected).abs() <= T::TOLERANCE,
                        "{} rate{rate} ch{channels} value{i}: {value} vs {expected}",
                        T::FORMAT
                    );
                }
                if rate == 48000 && channels == 1 {
                    let mismatched = Spec {
                        rate: 44100,
                        ..spec
                    };
                    assert!(decode::<T>(
                        &runtime,
                        Path::new("ffmpeg"),
                        &source,
                        dir.path(),
                        mismatched,
                        Duration::from_secs(20),
                        &cancel
                    )
                    .await
                    .is_err());
                    let mut corrupt = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(owned.join("samples.pcm"))
                        .unwrap();
                    let mut byte = [0];
                    corrupt.read_exact(&mut byte).unwrap();
                    byte[0] ^= 1;
                    corrupt.seek(SeekFrom::Start(0)).unwrap();
                    corrupt.write_all(&byte).unwrap();
                    drop(corrupt);
                    decoded.cached = None;
                    assert!(
                        decoded.read_packet(spec.start as i64, 1).is_err(),
                        "corrupted PCM must fail checksum validation"
                    );
                }
                drop(decoded);
                assert!(!owned.exists());
                let source_pcm = decode_probed(
                    &runtime,
                    Path::new("ffmpeg"),
                    &source,
                    dir.path(),
                    &probe,
                    spec.start..spec.end,
                    spec.staging_bytes,
                    Duration::from_secs(20),
                    &cancel,
                )
                .await
                .unwrap();
                let mut reader = super::super::packet_pitch::PlannedReader::new(
                    source_pcm,
                    &[crate::domain::concat_format_plan::Filter::Echo],
                    crate::domain::concat_format_plan::Format::F32,
                    7.,
                    81,
                )
                .unwrap();
                let natural = reference.stdout.len() / channels / T::BYTES;
                for start in [81 + natural as i64 / 2, 81 + natural as i64 - 31, 64, 81] {
                    let block = T::block(reader.render(start, 127, &mut || Ok(())).unwrap());
                    for ch in 0..channels {
                        for (i, &value) in block[ch].iter().enumerate() {
                            let at = start + i as i64 - 81;
                            let expected = if at < 0 || at as usize >= natural {
                                <T as Sample>::silence().number()
                            } else {
                                let offset = (at as usize * channels + ch) * T::BYTES;
                                T::from_bytes(&reference.stdout[offset..offset + T::BYTES]).number()
                            };
                            assert!(
                                (value.number() - expected).abs() <= T::TOLERANCE,
                                "typed reader {} rate{rate} ch{channels} start{start} i{i}",
                                T::FORMAT
                            );
                        }
                    }
                }
                assert!(reader
                    .render(123, 127, &mut || anyhow::bail!("cancelled"))
                    .is_err());
                let recovered = T::block(reader.render(81, 127, &mut || Ok(())).unwrap());
                for ch in 0..channels {
                    for (i, &value) in recovered[ch].iter().enumerate() {
                        let offset = (i * channels + ch) * T::BYTES;
                        let expected =
                            T::from_bytes(&reference.stdout[offset..offset + T::BYTES]).number();
                        assert!((value.number() - expected).abs() <= T::TOLERANCE);
                    }
                }
                drop(reader);
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
                if T::FORMAT == "s32le" && rate == 44100 && channels == 2 {
                    let decoded = decode_probed(
                        &runtime,
                        Path::new("ffmpeg"),
                        &source,
                        dir.path(),
                        &probe,
                        spec.start..spec.end,
                        spec.staging_bytes,
                        Duration::from_secs(20),
                        &cancel,
                    )
                    .await
                    .unwrap();
                    assert!(super::super::packet_pitch::PlannedReader::new(
                        decoded,
                        &[],
                        crate::domain::concat_format_plan::Format::S16,
                        7.,
                        81
                    )
                    .is_err());
                    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
                }
                qualify_float_reader::<f32>(&runtime, &source, dir.path(), &probe, spec, &graph)
                    .await;
                qualify_float_reader::<f64>(&runtime, &source, dir.path(), &probe, spec, &graph)
                    .await;
                qualify_frame_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_biquad_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_voice_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_pitch_node(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_sweet_chain(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_double_rate_reader(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_mixed_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
                assert!(source.exists());
                println!(
                    "{} rate{rate} ch{channels}: real packet decode/Pitch/cleanup passed",
                    T::FORMAT
                );
            }
        }
    }
    async fn qualify_float_reader<
        O: Signal + crate::domain::concat_resample::FloatOutput + 'static,
    >(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
        graph: &str,
    ) {
        let decoded = decode_probed(
            runtime,
            Path::new("ffmpeg"),
            source,
            staging,
            probe,
            spec.start..spec.end,
            spec.staging_bytes,
            Duration::from_secs(20),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let sink = if O::BYTES == 4 {
            crate::domain::concat_format_plan::Format::F32
        } else {
            crate::domain::concat_format_plan::Format::F64
        };
        let mut reader =
            super::super::packet_pitch::PlannedReader::new(decoded, &[], sink, 7., 81).unwrap();
        assert_eq!(reader.plan.stages[3].output, sink);
        let reference = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(source)
            .args(["-af", graph, "-c:a", O::CODEC, "-f", O::FORMAT, "pipe:1"])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let natural = reference.stdout.len() / spec.channels / O::BYTES;
        for (start, n) in [
            (61, natural + 40),
            (81 + natural as i64 / 2, 127),
            (81 + natural as i64 - 31, 127),
            (64, 127),
            (81, 127),
        ] {
            let block = O::block(reader.render(start, n, &mut || Ok(())).unwrap());
            for ch in 0..spec.channels {
                for (i, &value) in block[ch].iter().enumerate() {
                    let at = start + i as i64 - 81;
                    let expected = if at < 0 || at as usize >= natural {
                        0.
                    } else {
                        let offset = (at as usize * spec.channels + ch) * O::BYTES;
                        O::from_bytes(&reference.stdout[offset..offset + O::BYTES]).number()
                    };
                    assert!(
                        (value.number() - expected).abs() <= O::TOLERANCE,
                        "mixed reader to {} rate{} ch{} at{at}: {} vs {expected}",
                        O::FORMAT,
                        spec.rate,
                        ch,
                        value.number()
                    );
                }
            }
        }
        assert!(reader
            .render(123, 127, &mut || anyhow::bail!("cancelled"))
            .is_err());
        let recovered = O::block(reader.render(81, 127, &mut || Ok(())).unwrap());
        for ch in 0..spec.channels {
            for (i, &value) in recovered[ch].iter().enumerate() {
                let offset = (i * spec.channels + ch) * O::BYTES;
                let expected = O::from_bytes(&reference.stdout[offset..offset + O::BYTES]).number();
                assert!((value.number() - expected).abs() <= O::TOLERANCE);
            }
        }
        drop(reader);
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
    }
    async fn qualify_frame_chain(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
        graph: &str,
    ) {
        use crate::{
            domain::concat_format_plan::{Filter, Format},
            tools::{
                packet_echo::{floating32, Stage},
                packet_pitch_frames::Reader,
            },
        };
        let decoded = decode_probed(
            runtime,
            Path::new("ffmpeg"),
            source,
            staging,
            probe,
            spec.start..spec.end,
            spec.staging_bytes,
            Duration::from_secs(20),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let mut reader = Reader::new(decoded, &[Filter::Echo], Format::F32, 7.).unwrap();
        let mut echo = Stage::new(
            reader.plan.stages[4].input,
            48000,
            spec.channels,
            0.8,
            0.85,
            &[250.],
            &[0.4],
        )
        .unwrap();
        let chain = format!("{graph},ashowinfo,aecho=0.8:0.85:250:0.4,ashowinfo");
        let reference = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "info", "-i"])
            .arg(source)
            .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let mut sizes = std::collections::BTreeMap::<usize, Vec<usize>>::new();
        for line in String::from_utf8_lossy(&reference.stderr)
            .lines()
            .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
        {
            let id = line
                .split("Parsed_ashowinfo_")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let count = line
                .split("nb_samples:")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            sizes.entry(id).or_default().push(count);
        }
        assert!(sizes.keys().all(|id| [6, 8].contains(id)));
        for pass in 0..2 {
            if pass > 0 {
                reader.reset();
                echo.reset();
                let mut calls = 0;
                assert!(reader
                    .next_packet(&mut || {
                        calls += 1;
                        anyhow::ensure!(calls < 3, "cancelled");
                        Ok(())
                    })
                    .is_err());
                assert!(
                    reader.next_packet(&mut || Ok(())).is_err(),
                    "failed sequential streams require explicit reset"
                );
                reader.reset();
            }
            let mut first_sizes = Vec::new();
            let mut last_sizes = Vec::new();
            let mut actual = vec![Vec::new(); spec.channels];
            while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                first_sizes.push(packet.frames());
                let packet = echo.push_packet(packet).unwrap();
                last_sizes.push(packet.frames());
                for (dst, src) in actual.iter_mut().zip(floating32(packet)) {
                    dst.extend(src);
                }
            }
            assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
            while let Some(packet) = echo.finish_packet() {
                last_sizes.push(packet.frames());
                for (dst, src) in actual.iter_mut().zip(floating32(packet)) {
                    dst.extend(src);
                }
            }
            assert_eq!(
                first_sizes.as_slice(),
                sizes.get(&6).map(Vec::as_slice).unwrap_or(&[]),
                "decoded Pitch frame boundaries rate{} channels{}",
                spec.rate,
                spec.channels
            );
            assert_eq!(
                last_sizes.as_slice(),
                sizes.get(&8).map(Vec::as_slice).unwrap_or(&[]),
                "decoded Echo frame boundaries rate{} channels{}",
                spec.rate,
                spec.channels
            );
            assert_eq!(actual[0].len() * spec.channels * 4, reference.stdout.len());
            for (i, b) in reference.stdout.chunks_exact(4).enumerate() {
                let expected = f32::from_le_bytes(b.try_into().unwrap());
                let value = actual[i % spec.channels][i / spec.channels];
                assert!(value.is_finite()&&(value-expected).abs()<=2e-7,"decoded framed chain rate{} channels{} pass{pass} value{i}: {value} vs {expected}",spec.rate,spec.channels);
            }
        }
        drop(reader);
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
    }
    async fn qualify_biquad_chain(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
        graph: &str,
    ) {
        use crate::{
            domain::concat_format_plan::{Filter, Format},
            tools::{packet_biquad::Stage, packet_echo::floating32, packet_pitch_frames::Reader},
        };
        let decoded = decode_probed(
            runtime,
            Path::new("ffmpeg"),
            source,
            staging,
            probe,
            spec.start..spec.end,
            spec.staging_bytes,
            Duration::from_secs(20),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let mut reader = Reader::new(decoded, &[Filter::Biquad], Format::F32, 7.).unwrap();
        let mut echo = Stage::new(
            reader.plan.stages[4].input,
            crate::domain::concat_biquad_stream::Design::Highpass,
            48000,
            spec.channels,
            120.,
            0.707,
            0.,
        )
        .unwrap();
        let chain = format!("{graph},ashowinfo,highpass=f=120,ashowinfo");
        let reference = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "info", "-i"])
            .arg(source)
            .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            reference.status.success(),
            "{}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let mut sizes = std::collections::BTreeMap::<usize, Vec<usize>>::new();
        for line in String::from_utf8_lossy(&reference.stderr)
            .lines()
            .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
        {
            let id = line
                .split("Parsed_ashowinfo_")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let count = line
                .split("nb_samples:")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            sizes.entry(id).or_default().push(count);
        }
        assert!(sizes.keys().all(|id| [6, 8].contains(id)));
        for pass in 0..2 {
            if pass > 0 {
                reader.reset();
                echo.reset();
                let mut calls = 0;
                assert!(reader
                    .next_packet(&mut || {
                        calls += 1;
                        anyhow::ensure!(calls < 3, "cancelled");
                        Ok(())
                    })
                    .is_err());
                assert!(
                    reader.next_packet(&mut || Ok(())).is_err(),
                    "failed sequential streams require explicit reset"
                );
                reader.reset();
            }
            let mut first_sizes = Vec::new();
            let mut last_sizes = Vec::new();
            let mut actual = vec![Vec::new(); spec.channels];
            while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                first_sizes.push(packet.frames());
                let packet = echo.push_packet(packet).unwrap();
                last_sizes.push(packet.frames());
                for (dst, src) in actual.iter_mut().zip(floating32(packet)) {
                    dst.extend(src);
                }
            }
            assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
            assert_eq!(
                first_sizes.as_slice(),
                sizes.get(&6).map(Vec::as_slice).unwrap_or(&[]),
                "decoded Pitch frame boundaries rate{} channels{}",
                spec.rate,
                spec.channels
            );
            assert_eq!(
                last_sizes.as_slice(),
                sizes.get(&8).map(Vec::as_slice).unwrap_or(&[]),
                "decoded highpass frame boundaries rate{} channels{}",
                spec.rate,
                spec.channels
            );
            assert_eq!(actual[0].len() * spec.channels * 4, reference.stdout.len());
            for (i, b) in reference.stdout.chunks_exact(4).enumerate() {
                let expected = f32::from_le_bytes(b.try_into().unwrap());
                let value = actual[i % spec.channels][i / spec.channels];
                assert!(value.is_finite()&&(value-expected).abs()<=2e-7,"decoded highpass chain rate{} channels{} pass{pass} value{i}: {value} vs {expected}",spec.rate,spec.channels);
            }
        }
        drop(reader);
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
    }
    #[tokio::test]
    async fn real_large_flac_packet_survives_24x_double_resampling() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("large.flac");
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0.6*sin(2*PI*317*t)|0.4*cos(2*PI*997*t):s=8000:d=8.3",
                "-c:a",
                "flac",
                "-sample_fmt",
                "s16",
                "-frame_size",
                "65535",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let trace = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_frames",
                "-show_entries",
                "frame=nb_samples",
                "-of",
                "json",
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert!(trace.status.success());
        let trace: serde_json::Value = serde_json::from_slice(&trace.stdout).unwrap();
        assert_eq!(trace["frames"][0]["nb_samples"].as_u64(), Some(65535));
        let runtime = ProcessRuntime::local_default();
        let probe = crate::tools::probe_video(&runtime, &source).await.unwrap();
        let spec = Spec {
            rate: 8000,
            channels: 2,
            source_planar: probe.streams[0]
                .sample_format
                .as_deref()
                .unwrap()
                .ends_with('p'),
            start: 37,
            end: 66400 - 113,
            staging_bytes: 64 * 1024 * 1024,
        };
        qualify_double_rate_reader(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_mixed_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
    }
    async fn qualify_double_rate_reader(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
    ) {
        use crate::tools::{packet_echo::floating64, packet_rate_frames};
        let mut compared = 0;
        for output_rate in [48000, 192000] {
            let decoded = decode_probed(
                runtime,
                Path::new("ffmpeg"),
                source,
                staging,
                probe,
                spec.start..spec.end,
                spec.staging_bytes,
                Duration::from_secs(20),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let mut reader = packet_rate_frames::reader(decoded, output_rate).unwrap();
            let chain=format!("atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,aresample={output_rate}:osf=dbl,ashowinfo",spec.start,spec.end);
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "info", "-i"])
                .arg(source)
                .args(["-af", &chain, "-c:a", "pcm_f64le", "-f", "f64le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let sizes = String::from_utf8_lossy(&original.stderr)
                .lines()
                .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
                .map(|l| {
                    assert!(
                        l.contains("fmt:dbl ") && l.contains(&format!("rate:{output_rate} ")),
                        "{l}"
                    );
                    l.split("nb_samples:")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            if output_rate == 192000 && spec.rate == 8000 && spec.end - spec.start > 65535 {
                assert!(
                    sizes.iter().any(|&n| n > 1048576),
                    "large FLAC fixture must exercise a >1M-frame resampled packet"
                );
            }
            for pass in 0..2 {
                if pass > 0 {
                    reader.reset();
                }
                let mut actual = vec![Vec::new(); spec.channels];
                let mut emitted = Vec::new();
                while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                    emitted.push(packet.frames());
                    for (dst, ch) in actual.iter_mut().zip(floating64(packet)) {
                        dst.extend(ch);
                    }
                }
                assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                assert_eq!(
                    emitted, sizes,
                    "source double rate {} -> {output_rate} ch{} exact frames",
                    spec.rate, spec.channels
                );
                assert_eq!(actual[0].len() * spec.channels * 8, original.stdout.len());
                for (i, bytes) in original.stdout.chunks_exact(8).enumerate() {
                    let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                    let actual = actual[i % spec.channels][i / spec.channels];
                    assert!((actual-expected).abs()<=2e-12,"source double rate {} -> {output_rate} ch{} pass{pass} value{i}: {actual} vs {expected}",spec.rate,spec.channels);
                    compared += 1;
                }
            }
            reader.reset();
            let mut calls = 0;
            assert!(reader
                .next_packet(&mut || {
                    calls += 1;
                    anyhow::ensure!(calls < 3, "cancelled");
                    Ok(())
                })
                .is_err());
            assert!(reader.next_packet(&mut || Ok(())).is_err());
            reader.reset();
            reader.next_packet(&mut || Ok(())).unwrap();
            drop(reader);
            assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
        }
        println!(
            "Double source rate {} ch{}: {compared} values at 48/192 kHz",
            spec.rate, spec.channels
        );
    }
    async fn qualify_loudness_source(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
    ) {
        use crate::tools::{packet_echo::floating64, packet_rate_frames, packet_loudness, packet_double_rate, packet_timeline::Source};
        let mut compared = 0;
        for output_rate in [48000] {
            let decoded = decode_probed(
                runtime,
                Path::new("ffmpeg"),
                source,
                staging,
                probe,
                spec.start..spec.end,
                spec.staging_bytes,
                Duration::from_secs(20),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let upsample = packet_rate_frames::reader(decoded, 192000).unwrap();
            let loudness = packet_loudness::Reader::new(upsample,192000,spec.channels,-23.).unwrap();
            let mut reader = packet_double_rate::Reader::new(Box::new(loudness),192000,output_rate,spec.channels).unwrap();
            let chain=format!("atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,aresample=192000:osf=dbl,loudnorm=I=-23.0:TP=-1.5:LRA=11,aresample={output_rate}:osf=dbl,ashowinfo",spec.start,spec.end);
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "info", "-i"])
                .arg(source)
                .args(["-af", &chain, "-c:a", "pcm_f64le", "-f", "f64le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let sizes = String::from_utf8_lossy(&original.stderr)
                .lines()
                .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
                .map(|l| {
                    assert!(
                        l.contains("fmt:dbl ") && l.contains(&format!("rate:{output_rate} ")),
                        "{l}"
                    );
                    l.split("nb_samples:")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            if output_rate == 192000 && spec.rate == 8000 && spec.end - spec.start > 65535 {
                assert!(
                    sizes.iter().any(|&n| n > 1048576),
                    "large FLAC fixture must exercise a >1M-frame resampled packet"
                );
            }
            for pass in 0..2 {
                if pass > 0 {
                    reader.reset();
                }
                let mut actual = vec![Vec::new(); spec.channels];
                let mut emitted = Vec::new();
                while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                    emitted.push(packet.frames());
                    for (dst, ch) in actual.iter_mut().zip(floating64(packet)) {
                        dst.extend(ch);
                    }
                }
                assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                assert_eq!(
                    emitted, sizes,
                    "decoded Loudness {} -> {output_rate} ch{} exact frames",
                    spec.rate, spec.channels
                );
                assert_eq!(actual[0].len() * spec.channels * 8, original.stdout.len());
                for (i, bytes) in original.stdout.chunks_exact(8).enumerate() {
                    let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                    let actual = actual[i % spec.channels][i / spec.channels];
                    assert!((actual-expected).abs()<=2e-12,"decoded Loudness {} -> {output_rate} ch{} pass{pass} value{i}: {actual} vs {expected}",spec.rate,spec.channels);
                    compared += 1;
                }
            }
            reader.reset();
            let mut calls = 0;
            assert!(reader
                .next_packet(&mut || {
                    calls += 1;
                    anyhow::ensure!(calls < 3, "cancelled");
                    Ok(())
                })
                .is_err());
            assert!(reader.next_packet(&mut || Ok(())).is_err());
            reader.reset();
            reader.next_packet(&mut || Ok(())).unwrap();
            drop(reader);
            assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
        }
        println!(
            "Loudness decoded source {} ch{}: {compared} double values at 48 kHz",
            spec.rate, spec.channels
        );
    }
    async fn qualify_mixed_loudness_source(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
    ) {
        use crate::tools::{packet_echo::floating64, packet_rate_frames, packet_loudness, packet_double_rate, packet_timeline::Source};
        let mut compared = 0;
        for reversed in [false,true] {
            let output_rate=48000;
            let decoded = decode_probed(
                runtime,
                Path::new("ffmpeg"),
                source,
                staging,
                probe,
                spec.start..spec.end,
                spec.staging_bytes,
                Duration::from_secs(20),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            use crate::domain::concat_format_plan::{Format,Filter};
            use crate::tools::{packet_pitch_frames,packet_pitch_node,packet_typed_rate};
            let mut reader:Box<dyn Source> = if reversed {
                let pitch=packet_pitch_frames::Reader::new(decoded,&[Filter::Resample,Filter::Loudness],Format::F32,7.).unwrap();
                let format=pitch.plan.stages[3].output;
                let up=packet_typed_rate::reader(Box::new(pitch),format,48000,192000,spec.channels).unwrap();
                let loud=packet_loudness::Reader::new(up,192000,spec.channels,-23.).unwrap();
                Box::new(packet_double_rate::Reader::new(Box::new(loud),192000,48000,spec.channels).unwrap())
            } else {
                let up=packet_rate_frames::reader(decoded,192000).unwrap();
                let loud=packet_loudness::Reader::new(up,192000,spec.channels,-23.).unwrap();
                let down=packet_double_rate::Reader::new(Box::new(loud),192000,48000,spec.channels).unwrap();
                Box::new(packet_pitch_node::Node::new(Box::new(down),Format::F64,Format::F32,spec.channels,7.).unwrap())
            };
            let pitch="aresample=48000,asetrate=71918.739690,aresample=48000,atempo=0.66741993";
            let loud="loudnorm=I=-23.0:TP=-1.5:LRA=11";
            let mixed=if reversed {format!("{pitch},{loud}")}else{format!("{loud},{pitch}")};
            let chain=format!("atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{mixed},aresample=48000:osf=flt,ashowinfo",spec.start,spec.end);
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "info", "-i"])
                .arg(source)
                .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let sizes = String::from_utf8_lossy(&original.stderr)
                .lines()
                .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
                .map(|l| {
                    assert!(
                        l.contains("fmt:flt ") && l.contains(&format!("rate:{output_rate} ")),
                        "{l}"
                    );
                    l.split("nb_samples:")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            if output_rate == 192000 && spec.rate == 8000 && spec.end - spec.start > 65535 {
                assert!(
                    sizes.iter().any(|&n| n > 1048576),
                    "large FLAC fixture must exercise a >1M-frame resampled packet"
                );
            }
            for pass in 0..2 {
                if pass > 0 {
                    reader.reset();
                }
                let mut actual = vec![Vec::new(); spec.channels];
                let mut emitted = Vec::new();
                while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                    emitted.push(packet.frames());
                    for (dst, ch) in actual.iter_mut().zip(floating64(packet)) {
                        dst.extend(ch);
                    }
                }
                assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                assert_eq!(
                    emitted, sizes,
                    "mixed Loudness order{reversed} {} -> {output_rate} ch{} exact frames",
                    spec.rate, spec.channels
                );
                assert_eq!(actual[0].len() * spec.channels * 4, original.stdout.len());
                for (i, bytes) in original.stdout.chunks_exact(4).enumerate() {
                    let expected = f64::from(f32::from_le_bytes(bytes.try_into().unwrap()));
                    let actual = actual[i % spec.channels][i / spec.channels];
                    assert!((actual-expected).abs()<=2e-7,"mixed Loudness order{reversed} {} -> {output_rate} ch{} pass{pass} value{i}: {actual} vs {expected}",spec.rate,spec.channels);
                    compared += 1;
                }
            }
            reader.reset();
            let mut calls = 0;
            assert!(reader
                .next_packet(&mut || {
                    calls += 1;
                    anyhow::ensure!(calls < 3, "cancelled");
                    Ok(())
                })
                .is_err());
            assert!(reader.next_packet(&mut || Ok(())).is_err());
            reader.reset();
            reader.next_packet(&mut || Ok(())).unwrap();
            drop(reader);
            assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
        }
        println!(
            "Mixed Loudness decoded source {} ch{}: {compared} host values across both orders",
            spec.rate, spec.channels
        );
    }
    async fn qualify_sweet_chain(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
    ) {
        use crate::tools::{
            packet_echo::floating64,
            packet_voice_chain::{build, Setting},
        };
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/concat-sweet-plans.json"))
                .unwrap();
        let mut compared = 0;
        for (case, plan) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let decoded = decode_probed(
                runtime,
                Path::new("ffmpeg"),
                source,
                staging,
                probe,
                spec.start..spec.end,
                spec.staging_bytes,
                Duration::from_secs(20),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let mut reader = build(
                decoded,
                &[Setting::Sweet {
                    amount: plan["amount"].as_f64().unwrap() as f32,
                    pitch: plan["pitch"].as_f64().unwrap() as f32,
                }],
            )
            .unwrap();
            let chain = format!(
                "atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{},ashowinfo",
                spec.start,
                spec.end,
                plan["chain"].as_str().unwrap()
            );
            let original = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "info", "-i"])
                .arg(source)
                .args(["-af", &chain, "-c:a", "pcm_f64le", "-f", "f64le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                original.status.success(),
                "{}",
                String::from_utf8_lossy(&original.stderr)
            );
            let sizes = String::from_utf8_lossy(&original.stderr)
                .lines()
                .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
                .map(|l| {
                    l.split("nb_samples:")
                        .nth(1)
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            for pass in 0..2 {
                if pass > 0 {
                    reader.reset();
                }
                let mut actual = vec![Vec::new(); spec.channels];
                let mut emitted = Vec::new();
                while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                    emitted.push(packet.frames());
                    for (dst, ch) in actual.iter_mut().zip(floating64(packet)) {
                        dst.extend(ch);
                    }
                }
                assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                assert_eq!(
                    emitted, sizes,
                    "Sweet case{case} rate{} ch{} exact final packets",
                    spec.rate, spec.channels
                );
                assert_eq!(actual[0].len() * spec.channels * 8, original.stdout.len());
                for (i, bytes) in original.stdout.chunks_exact(8).enumerate() {
                    let expected = f64::from_le_bytes(bytes.try_into().unwrap());
                    let actual = actual[i % spec.channels][i / spec.channels];
                    assert!(
                        actual.is_finite() && (actual - expected).abs() <= 2e-12,
                        "Sweet case{case} rate{} ch{} pass{pass} value{i}: {actual} vs {expected}",
                        spec.rate,
                        spec.channels
                    );
                    compared += 1;
                }
            }
            reader.reset();
            let mut calls = 0;
            assert!(reader
                .next_packet(&mut || {
                    calls += 1;
                    anyhow::ensure!(calls < 9, "cancelled");
                    Ok(())
                })
                .is_err());
            assert!(reader.next_packet(&mut || Ok(())).is_err());
            reader.reset();
            reader.next_packet(&mut || Ok(())).unwrap();
            drop(reader);
            assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
        }
        println!("Sweet full chain rate{} channels{}: {compared} double values, 11 original plans and exact EOF packets",spec.rate,spec.channels);
    }
    async fn qualify_pitch_node(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
        graph: &str,
    ) {
        use crate::{
            domain::concat_format_plan::{Filter, Format},
            tools::{
                packet_echo::floating32, packet_pitch_frames::Reader, packet_pitch_node::Node,
                packet_timeline::Source,
            },
        };
        let decoded = decode_probed(
            runtime,
            Path::new("ffmpeg"),
            source,
            staging,
            probe,
            spec.start..spec.end,
            spec.staging_bytes,
            Duration::from_secs(20),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        let first = Reader::new(
            decoded,
            &[
                Filter::Resample,
                Filter::Pass,
                Filter::Resample,
                Filter::Tempo,
            ],
            Format::F32,
            7.,
        )
        .unwrap();
        assert_eq!(first.plan.stages[3].output, first.plan.stages[4].input);
        assert_eq!(first.plan.stages[4].input, first.plan.stages[4].output);
        let input = first.plan.stages[4].input;
        let output = first.plan.stages[7].output;
        assert_eq!(output, Format::F32);
        let mut node = Node::new(Box::new(first), input, output, spec.channels, -7.).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/concat-pitch-pcm.json"))
                .unwrap();
        let second = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["rate"] == 48000
                    && c["channels"] == spec.channels
                    && c["settings"][0]["params"]["semitones"].as_f64() == Some(-7.)
            })
            .unwrap();
        let chain = format!(
            "{graph},ashowinfo,{},ashowinfo",
            second["chain"].as_str().unwrap()
        );
        let original = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "info", "-i"])
            .arg(source)
            .args(["-af", &chain, "-c:a", "pcm_f32le", "-f", "f32le", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            original.status.success(),
            "{}",
            String::from_utf8_lossy(&original.stderr)
        );
        let sizes = String::from_utf8_lossy(&original.stderr)
            .lines()
            .filter(|l| l.contains("Parsed_ashowinfo_11 ") && l.contains("nb_samples:"))
            .map(|l| {
                l.split("nb_samples:")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        for pass in 0..2 {
            if pass > 0 {
                node.reset();
            }
            let mut actual = vec![Vec::new(); spec.channels];
            let mut emitted = Vec::new();
            while let Some(packet) = node.next_packet(&mut || Ok(())).unwrap() {
                emitted.push(packet.frames());
                for (dst, ch) in actual.iter_mut().zip(floating32(packet)) {
                    dst.extend(ch);
                }
            }
            assert_eq!(
                emitted, sizes,
                "ordered Pitch packet sizes rate{} ch{}",
                spec.rate, spec.channels
            );
            assert_eq!(actual[0].len() * spec.channels * 4, original.stdout.len());
            for (i, bytes) in original.stdout.chunks_exact(4).enumerate() {
                let expected = f32::from_le_bytes(bytes.try_into().unwrap());
                let actual = actual[i % spec.channels][i / spec.channels];
                assert!(
                    (actual - expected).abs() <= 2e-7,
                    "ordered Pitch rate{} ch{} pass{pass} value{i}: {actual} vs {expected}",
                    spec.rate,
                    spec.channels
                );
            }
        }
        node.reset();
        assert!(node
            .next_packet(&mut || anyhow::bail!("cancelled"))
            .is_err());
        assert!(node.next_packet(&mut || Ok(())).is_err());
        node.reset();
        node.next_packet(&mut || Ok(())).unwrap();
        drop(node);
        assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
    }
    async fn qualify_voice_chain(
        runtime: &ProcessRuntime,
        source: &Path,
        staging: &Path,
        probe: &crate::domain::media_probe::ProbeResult,
        spec: Spec,
        _graph: &str,
    ) {
        use crate::tools::{
            packet_echo::floating64,
            packet_voice::{Preset, Reader},
        };
        for (pitch, body, deep) in [
            (7f32, 0., false),
            (3., 0., false),
            (12., 0., false),
            (-3., 3., true),
            (-8., 0., true),
            (-1., 8., true),
        ] {
            let decoded = decode_probed(
                runtime,
                Path::new("ffmpeg"),
                source,
                staging,
                probe,
                spec.start..spec.end,
                spec.staging_bytes,
                Duration::from_secs(20),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
            let preset = if deep {
                Preset::Deep { pitch, body }
            } else {
                Preset::Chipmunk { pitch }
            };
            let mut reader = Reader::new(decoded, preset).unwrap();
            let ratio = 2f64.powf(f64::from(pitch) / 12.);
            let suffix = if deep {
                format!("highpass=f=55,equalizer=f=140:t=q:w=1.0:g={body:.2},equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60")
            } else {
                "highpass=f=120,alimiter=limit=0.94".to_owned()
            };
            let chain = format!("atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8},{suffix},ashowinfo",spec.start,spec.end,48000.*ratio,1./ratio);
            let reference = std::process::Command::new("ffmpeg")
                .args(["-nostdin", "-v", "info", "-i"])
                .arg(source)
                .args(["-af", &chain, "-c:a", "pcm_f64le", "-f", "f64le", "pipe:1"])
                .output()
                .unwrap();
            assert!(
                reference.status.success(),
                "{}",
                String::from_utf8_lossy(&reference.stderr)
            );
            let mut sizes = std::collections::BTreeMap::<usize, Vec<usize>>::new();
            for line in String::from_utf8_lossy(&reference.stderr)
                .lines()
                .filter(|l| l.contains("Parsed_ashowinfo_") && l.contains("nb_samples:"))
            {
                let id = line
                    .split("Parsed_ashowinfo_")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let count = line
                    .split("nb_samples:")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                sizes.entry(id).or_default().push(count);
            }
            let output_id = if deep { 10 } else { 8 };
            assert!(sizes.keys().all(|id| *id == output_id));
            for pass in 0..2 {
                if pass > 0 {
                    reader.reset();
                    let mut calls = 0;
                    assert!(reader
                        .next_packet(&mut || {
                            calls += 1;
                            anyhow::ensure!(calls < 3, "cancelled");
                            Ok(())
                        })
                        .is_err());
                    assert!(
                        reader.next_packet(&mut || Ok(())).is_err(),
                        "failed sequential streams require explicit reset"
                    );
                    reader.reset();
                }
                let mut last_sizes = Vec::new();
                let mut actual = vec![Vec::new(); spec.channels];
                while let Some(packet) = reader.next_packet(&mut || Ok(())).unwrap() {
                    last_sizes.push(packet.frames());
                    for (dst, src) in actual.iter_mut().zip(floating64(packet)) {
                        dst.extend(src);
                    }
                }
                assert!(reader.next_packet(&mut || Ok(())).unwrap().is_none());
                assert_eq!(
                    last_sizes.as_slice(),
                    sizes.get(&output_id).map(Vec::as_slice).unwrap_or(&[]),
                    "decoded voice frame boundaries rate{} channels{}",
                    spec.rate,
                    spec.channels
                );
                assert_eq!(actual[0].len() * spec.channels * 8, reference.stdout.len());
                for (i, b) in reference.stdout.chunks_exact(8).enumerate() {
                    let expected = f64::from_le_bytes(b.try_into().unwrap());
                    let value = actual[i % spec.channels][i / spec.channels];
                    assert!(value.is_finite()&&(value-expected).abs()<=2e-12,"decoded voice chain rate{} channels{} pass{pass} value{i}: {value} vs {expected}",spec.rate,spec.channels);
                }
            }
            let total = (reference.stdout.len() / (8 * spec.channels)) as i64;
            let origin = 19i64;
            let end = origin + total + 37;
            let mut timeline =
                crate::tools::packet_timeline::Timeline::new(reader, spec.channels, origin..end)
                    .unwrap();
            for (start, count) in [
                (-31, 83),
                (19, 1),
                (20, 17),
                (37, 509),
                (origin + total / 2, 127),
                (end - 51, 93),
                (19, 257),
                (end + 10, 9),
            ] {
                let data = timeline.render(start, count, &mut || Ok(())).unwrap();
                for ch in 0..spec.channels {
                    for i in 0..count {
                        let clock = start + i as i64;
                        let age = clock - origin;
                        let expected = if age >= 0 && age < total && clock < end {
                            let offset = (age as usize * spec.channels + ch) * 8;
                            f64::from_le_bytes(
                                reference.stdout[offset..offset + 8].try_into().unwrap(),
                            )
                        } else {
                            0.
                        };
                        assert!((data[ch][i]-expected).abs()<=2e-12,"voice timeline rate{} ch{} pitch{pitch} deep{deep} start{start} index{i}: {} vs {expected}",spec.rate,ch,data[ch][i]);
                    }
                }
            }
            timeline.reset();
            let mut calls = 0;
            assert!(timeline
                .render(origin + total / 2, 127, &mut || {
                    calls += 1;
                    anyhow::ensure!(calls < 6, "cancelled");
                    Ok(())
                })
                .is_err());
            assert!(timeline.render(origin, 1, &mut || Ok(())).is_err());
            timeline.reset();
            assert_eq!(
                timeline.render(origin - 10, 10, &mut || Ok(())).unwrap(),
                vec![vec![0.; 10]; spec.channels]
            );
            timeline.render(origin, 257, &mut || Ok(())).unwrap();
            drop(timeline);
            if (!deep && pitch == 7.) || (deep && pitch == -3. && body == 3.) {
                let input_us = spec.start * 1_000_000 / u64::from(spec.rate);
                let output_us = spec.end * 1_000_000 / u64::from(spec.rate);
                let duration = output_us - input_us;
                let age = duration / 4;
                let phase = if deep { age / 2 } else { 0 };
                let mut layer=crate::domain::nested_audio::NestedAudioLayer {
                    composition:"root".into(),instance_path:vec!["nested".into()],video_transitions:None, concat_retime_piece:None, concat_retime_owner:None,export_fade_clock:false,gain_track:None,
                    root_range_us:10000+age..10000+duration-duration/5,clip_local_start_us:age-phase,crossfade:Default::default(),
                    clip:serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"tone","placement":{"timelineStartTick":phase,"sourceInTick":input_us,"sourceOutTick":output_us,"speed":1.0,"sourceTimelineOffsetTick":phase},"gain":{"mode":"constant","value":0.5},"roleGain":1.25,"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0})).unwrap(),
                };
                layer
                    .clip
                    .dsp_effects
                    .push(crate::domain::filmcraft_dsp::EffectSpec { concat_source_keys: Default::default(),
                        id: if deep {
                            "concat.deep"
                        } else {
                            "concat.chipmunk"
                        }
                        .into(),
                        ..Default::default()
                    });
                let range = layer.root_range_us.clone();
                let binding = crate::tools::native_audio_export::PcmSource::from_probe(
                    source.to_owned(),
                    probe.clone(),
                )
                .unwrap();
                let mut clip = crate::tools::packet_clip::prepare_authored(
                    runtime,
                    Path::new("ffmpeg"),
                    staging,
                    layer,
                    &binding,
                    spec.staging_bytes,
                    Duration::from_secs(20),
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
                let first = (range.start * 48000).div_ceil(1_000_000);
                let last = (range.end * 48000).div_ceil(1_000_000);
                for (start, count) in [
                    (0, 127),
                    (first, 513),
                    (first + 513, 257),
                    (last.saturating_sub(73), 127),
                    (first + 5, 97),
                ] {
                    let block = clip.mix(start, count, &mut || Ok(())).unwrap();
                    for (i, frame) in block.iter().enumerate() {
                        let clock = start + i as u64;
                        let time = clock * 1_000_000 / 48000;
                        let mut expected = [0.; 2];
                        if range.contains(&time) && clock >= 480 && clock - 480 < total as u64 {
                            let value = |ch: usize| {
                                let offset = ((clock - 480) as usize * spec.channels + ch) * 8;
                                f64::from(f64::from_le_bytes(
                                    reference.stdout[offset..offset + 8].try_into().unwrap(),
                                ) as f32)
                                    * 0.625
                            };
                            if spec.channels == 1 {
                                let v = value(0);
                                expected = [v * std::f64::consts::FRAC_1_SQRT_2; 2];
                            } else {
                                expected = [value(0), value(1)];
                            }
                        }
                        for ch in 0..2 {
                            assert!(
                                (frame[ch] - expected[ch]).abs() <= 2e-7,
                                "clip host rate{} ch{} deep{deep} start{start} i{i}: {} vs {}",
                                spec.rate,
                                ch,
                                frame[ch],
                                expected[ch]
                            );
                        }
                    }
                }
                clip.reset();
                assert!(clip
                    .mix(first, 64, &mut || anyhow::bail!("cancelled"))
                    .is_err());
                assert!(clip.mix(first, 64, &mut || Ok(())).is_err());
                clip.reset();
                let mut total_calls = 0;
                clip.mix(first, 64, &mut || {
                    total_calls += 1;
                    Ok(())
                })
                .unwrap();
                clip.reset();
                let mut seen = 0;
                assert!(clip
                    .mix(first, 64, &mut || {
                        seen += 1;
                        anyhow::ensure!(seen < total_calls, "cancelled after host mix");
                        Ok(())
                    })
                    .is_err());
                assert!(clip.mix(first, 64, &mut || Ok(())).is_err());
                clip.reset();
                clip.mix(first, 64, &mut || Ok(())).unwrap();
                drop(clip);
            }

            assert_eq!(std::fs::read_dir(staging).unwrap().count(), 1);
        }
    }
    #[tokio::test]
    async fn real_planar_alac_readers_preserve_original_rounding() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.m4a");
        let runtime = ProcessRuntime::local_default();
        for rate in [8000, 44100, 48000, 192000] {
            for channels in [1, 2] {
                let wave = if channels == 1 {
                    format!("aevalsrc=0.6*sin(2*PI*317*t):s={rate}:d=0.25")
                } else {
                    format!("aevalsrc=0.6*sin(2*PI*317*t)|0.4*cos(2*PI*997*t):s={rate}:d=0.25")
                };
                let status = std::process::Command::new("ffmpeg")
                    .args([
                        "-nostdin",
                        "-v",
                        "error",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        &wave,
                        "-c:a",
                        "alac",
                        "-sample_fmt",
                        "s16p",
                    ])
                    .arg(&source)
                    .status()
                    .unwrap();
                assert!(status.success());
                let probe = crate::tools::probe_video(&runtime, &source).await.unwrap();
                assert_eq!(
                    probe
                        .streams
                        .iter()
                        .find(|s| s.kind == crate::domain::media_probe::StreamKind::Audio)
                        .unwrap()
                        .sample_format
                        .as_deref(),
                    Some("s16p")
                );
                let spec = Spec {
                    rate,
                    channels,
                    source_planar: true,
                    start: 37,
                    end: rate as u64 / 4 - 113,
                    staging_bytes: 4 * 1024 * 1024,
                };
                let fixture: serde_json::Value = serde_json::from_str(include_str!(
                    "../../tests/fixtures/concat-pitch-pcm.json"
                ))
                .unwrap();
                let pitch = fixture["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| {
                        c["rate"] == rate
                            && c["channels"] == channels
                            && c["settings"][0]["params"]["semitones"].as_f64() == Some(7.)
                    })
                    .unwrap()["chain"]
                    .as_str()
                    .unwrap();
                let graph = format!(
                    "atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS,{pitch}",
                    spec.start, spec.end
                );
                let decoded = decode_probed(
                    &runtime,
                    Path::new("ffmpeg"),
                    &source,
                    dir.path(),
                    &probe,
                    spec.start..spec.end,
                    spec.staging_bytes,
                    Duration::from_secs(20),
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
                match &decoded {
                    Decoded::S16(pcm) => assert!(pcm.spec().source_planar),
                    _ => panic!("lost ALAC scalar type"),
                };
                let mut reader = super::super::packet_pitch::PlannedReader::new(
                    decoded,
                    &[crate::domain::concat_format_plan::Filter::Echo],
                    crate::domain::concat_format_plan::Format::F32,
                    7.,
                    81,
                )
                .unwrap();
                let reference = std::process::Command::new("ffmpeg")
                    .args(["-nostdin", "-v", "error", "-i"])
                    .arg(&source)
                    .args(["-af", &graph, "-c:a", "pcm_s16le", "-f", "s16le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(reference.status.success());
                let natural = reference.stdout.len() / 2 / channels;
                for (start, count) in [
                    (61, natural + 40),
                    (81 + natural as i64 / 2, 127),
                    (64, 127),
                ] {
                    let block = match reader.render(start, count, &mut || Ok(())).unwrap() {
                        super::super::packet_pitch::Block::S16(v) => v,
                        _ => panic!("lost integer Pitch output"),
                    };
                    for ch in 0..channels {
                        for (i, &value) in block[ch].iter().enumerate() {
                            let at = start + i as i64 - 81;
                            let expected = if at < 0 || at as usize >= natural {
                                0
                            } else {
                                let off = (at as usize * channels + ch) * 2;
                                i16::from_le_bytes(
                                    reference.stdout[off..off + 2].try_into().unwrap(),
                                )
                            };
                            assert_eq!(value, expected, "ALAC rate{rate} ch{channels} at{at}");
                        }
                    }
                }
                drop(reader);
                qualify_float_reader::<f32>(&runtime, &source, dir.path(), &probe, spec, &graph)
                    .await;
                qualify_float_reader::<f64>(&runtime, &source, dir.path(), &probe, spec, &graph)
                    .await;
                qualify_frame_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_biquad_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_voice_chain(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_pitch_node(&runtime, &source, dir.path(), &probe, spec, &graph).await;
                qualify_sweet_chain(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_double_rate_reader(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
                qualify_mixed_loudness_source(&runtime, &source, dir.path(), &probe, spec).await;
                assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
                println!("planar ALAC rate{rate} ch{channels}: S16/F32/F64 readers passed");
            }
        }
    }
    #[tokio::test]
    async fn real_u8_packets_and_pitch() {
        qualify::<u8>().await;
    }
    #[tokio::test]
    async fn real_s16_packets_and_pitch() {
        qualify::<i16>().await;
    }
    #[tokio::test]
    async fn real_s32_packets_and_pitch() {
        qualify::<i32>().await;
    }
    #[tokio::test]
    async fn real_f32_packets_and_pitch() {
        qualify::<f32>().await;
    }
    #[tokio::test]
    async fn real_f64_packets_and_pitch() {
        qualify::<f64>().await;
    }
    #[test]
    fn inconsistent_indexes_are_refused() {
        let spec = Spec {
            rate: 48000,
            channels: 1,
            source_planar: false,
            start: 0,
            end: 1024,
            staging_bytes: 8192,
        };
        assert!(index::<f32>("", spec, 0).is_err());
        let header =
            "#sample_rate 0: 48000\n#tb 0: 1/48000\n#codec_id 0: pcm_f32le\n#hash: SHA256\n";
        assert!(index::<f32>(header, spec, 4).is_err());
        assert!(index::<f64>(header, spec, 0).is_err());
        let wrong_size = format!("{header}0,0,0,1024,4,{}\n", "0".repeat(64));
        assert!(index::<f32>(&wrong_size, spec, 4).is_err());
    }
    #[tokio::test]
    async fn failed_decodes_cancelled_calls_and_budget_failures_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("invalid.wav");
        std::fs::write(&source, b"invalid media").unwrap();
        let runtime = ProcessRuntime::local_default();
        let spec = Spec {
            rate: 48000,
            channels: 1,
            source_planar: false,
            start: 0,
            end: 1000,
            staging_bytes: 65536,
        };
        let cancel = CancellationToken::new();
        assert!(decode::<f32>(
            &runtime,
            Path::new("ffmpeg"),
            &source,
            dir.path(),
            spec,
            Duration::from_secs(5),
            &cancel
        )
        .await
        .is_err());
        cancel.cancel();
        assert!(decode::<f32>(
            &runtime,
            Path::new("ffmpeg"),
            &source,
            dir.path(),
            spec,
            Duration::from_secs(5),
            &cancel
        )
        .await
        .is_err());
        let small = Spec {
            staging_bytes: 1000,
            ..spec
        };
        assert!(decode::<f32>(
            &runtime,
            Path::new("ffmpeg"),
            &source,
            dir.path(),
            small,
            Duration::from_secs(5),
            &CancellationToken::new()
        )
        .await
        .is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert_eq!(std::fs::read(&source).unwrap(), b"invalid media");
    }
    #[test]
    fn probed_dispatch_preserves_scalar_kind_and_requires_complete_metadata() {
        use crate::domain::media_probe::ProbeResult;
        for (name, expected) in [
            ("u8", Format::U8),
            ("u8p", Format::U8),
            ("s16", Format::S16),
            ("s16p", Format::S16),
            ("s32", Format::S32),
            ("s32p", Format::S32),
            ("flt", Format::F32),
            ("fltp", Format::F32),
            ("dbl", Format::F64),
            ("dblp", Format::F64),
        ] {
            let probe = ProbeResult::from_ffprobe_json(&serde_json::json!({"streams":[
                {"index":0,"codec_type":"video"},
                {"index":1,"codec_type":"audio","sample_fmt":name,"sample_rate":"44100","channels":2},
                {"index":2,"codec_type":"audio","sample_fmt":"u8","sample_rate":"8000","channels":1}
            ]})).unwrap();
            assert_eq!(Format::from_probe(&probe).unwrap(), (expected, 44100, 2));
            let serialized = serde_json::to_value(&probe).unwrap();
            assert_eq!(serialized["streams"][1]["sampleFormat"], name);
            let mut legacy = serialized.clone();
            legacy["streams"][1]
                .as_object_mut()
                .unwrap()
                .remove("sampleFormat");
            let legacy: ProbeResult = serde_json::from_value(legacy).unwrap();
            assert!(Format::from_probe(&legacy).is_err());
        }
        for audio in [
            serde_json::json!({"codec_type":"audio","sample_fmt":"s64","sample_rate":"48000","channels":2}),
            serde_json::json!({"codec_type":"audio","sample_fmt":"flt","channels":2}),
            serde_json::json!({"codec_type":"audio","sample_fmt":"flt","sample_rate":"48000","channels":0}),
        ] {
            let probe =
                ProbeResult::from_ffprobe_json(&serde_json::json!({"streams":[audio]})).unwrap();
            assert!(Format::from_probe(&probe).is_err());
        }
    }
}
