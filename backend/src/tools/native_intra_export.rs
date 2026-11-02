//! Native pinned Filmcraft video codecs with guarded MOV/MXF output.
use super::native_rgba_export::{RasterAudio, RasterVideoSpec};
use crate::domain::{filmcraft_yuv422_pixels, group_surface::GroupSurface};
use anyhow::{ensure, Context, Result};
use filmcraft_isobmff::{
    Brand, ColorInfo, FieldInfo, FourCc, Mp4Writer, PcmConfig, SampleEntry, TrackConfig,
    WriteSample, WriterOptions,
};
use std::{
    fs::{File, OpenOptions},
    future::Future,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

pub(super) struct GuardedFile {
    pub(super) file: File,
    pub(super) cancel: CancellationToken,
    pub(super) deadline: Instant,
}
impl GuardedFile {
    pub(super) fn check(&self) -> std::io::Result<()> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("native intra export cancelled"));
        }
        if Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "native intra export deadline",
            ));
        }
        Ok(())
    }
}
impl Read for GuardedFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.file.read(buf)
    }
}
impl Write for GuardedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.check()?;
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.check()?;
        self.file.flush()
    }
}
impl Seek for GuardedFile {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        self.check()?;
        self.file.seek(from)
    }
}
pub(super) struct Staging(pub(super) PathBuf, pub(super) CancellationToken);
impl Drop for Staging {
    fn drop(&mut self) {
        self.1.cancel();
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Clone, Copy)]
pub enum Codec {
    HevcMovie {
        mov: bool,
        target_kbps: u32,
        max_kbps: u32,
        cbr: bool,
        keyint: u32,
        audio: crate::domain::filmcraft_export_movie::AudioSettings,
    },
    Mjpeg {
        quality: u8,
    },
    Dnx(u8),
    ProRes(u8),
    Apv(u8),
    Mxf {
        prores: bool,
        start: i64,
        drop_frame: bool,
    },
    Atom {
        start: i64,
        drop_frame: bool,
    },
    H264Movie {
        multiplexer: crate::domain::filmcraft_export_movie::Multiplexer,
        audio: crate::domain::filmcraft_export_movie::H264AudioSettings,
        sar: (u32, u32),
        keyint: Option<u32>,
        profile: crate::domain::filmcraft_export_h264::H264Profile,
        level: Option<u8>,
        bitrate: Option<crate::domain::filmcraft_export_h264::ResolvedBitrate>,
    },
    MxfH264 {
        start: i64,
        drop_frame: bool,
        sar: (u32, u32),
        keyint: Option<u32>,
        profile: crate::domain::filmcraft_export_h264::H264Profile,
        level: Option<u8>,
        bitrate: Option<crate::domain::filmcraft_export_h264::ResolvedBitrate>,
    },
}
impl Codec {
    fn valid(self) -> bool {
        match self {
            Self::H264Movie { .. }
            | Self::MxfH264 { .. }
            | Self::Mxf { .. }
            | Self::Atom { .. } => true,
            Self::Mjpeg { .. } | Self::HevcMovie { .. } => true,
            Self::Dnx(p) => p <= 2,
            Self::ProRes(p) => p <= 3,
            Self::Apv(p) => p <= 1,
        }
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::MxfH264 { .. } | Self::Mxf { .. } | Self::Atom { .. } => "mxf",
            Self::H264Movie { .. } => "h264-mp4",
            Self::HevcMovie { .. } => "hevc",
            Self::Mjpeg { .. } => "mjpeg",
            Self::Dnx(_) => "dnx",
            Self::ProRes(_) => "prores",
            Self::Apv(_) => "apv",
        }
    }
    fn apv_encoder(p: u8, spec: RasterVideoSpec) -> Result<filmcraft_apv::Encoder> {
        let profile = match p {
            0 => filmcraft_apv::Profile::P422_10,
            1 => filmcraft_apv::Profile::P422_12,
            _ => anyhow::bail!("invalid native APV profile"),
        };
        Ok(filmcraft_apv::Encoder::new(
            profile,
            spec.width as u32,
            spec.height as u32,
        )?)
    }
    fn prores_profile(p: u8) -> Result<filmcraft_prores::Profile> {
        Ok(match p {
            0 => filmcraft_prores::Profile::Proxy,
            1 => filmcraft_prores::Profile::Lt,
            2 => filmcraft_prores::Profile::Standard,
            3 => filmcraft_prores::Profile::Hq,
            _ => anyhow::bail!("invalid native ProRes profile"),
        })
    }
    fn entry(self, spec: RasterVideoSpec) -> Result<SampleEntry> {
        Ok(match self {
            Self::Apv(p) => SampleEntry::apv(
                filmcraft_isobmff::ApvConfig::parse(
                    &Self::apv_encoder(p, spec)?.decoder_config_record(),
                )?,
                spec.width as u16,
                spec.height as u16,
            ),
            Self::H264Movie { .. }
            | Self::MxfH264 { .. }
            | Self::Mxf { .. }
            | Self::Atom { .. } => {
                anyhow::bail!("MXF requires its native sample descriptor")
            }
            Self::HevcMovie { .. } => anyhow::bail!("HEVC requires deferred hvcC initialization"),
            Self::Mjpeg { .. } => SampleEntry::jpeg(spec.width as u16, spec.height as u16),
            Self::Dnx(_) => {
                SampleEntry::dnx(FourCc(*b"AVdh"), spec.width as u16, spec.height as u16)
            }
            Self::ProRes(p) => SampleEntry::prores(
                FourCc(Self::prores_profile(p)?.fourcc()),
                spec.width as u16,
                spec.height as u16,
            ),
        })
    }
    fn encoder(
        self,
        spec: RasterVideoSpec,
        pass: filmcraft_h264enc::Pass,
    ) -> Result<NativeEncoder> {
        Ok(match self {
            Self::MxfH264 {
                sar,
                keyint,
                profile,
                level,
                bitrate,
                ..
            }
            | Self::H264Movie {
                sar,
                keyint,
                profile,
                level,
                bitrate,
                ..
            } => {
                let mut cfg = filmcraft_h264enc::EncoderConfig::new(
                    spec.width as u32,
                    spec.height as u32,
                    spec.frame_rate.numerator,
                    spec.frame_rate.denominator,
                );
                cfg.profile = profile.native();
                cfg.level = level;
                cfg.pass = pass;
                cfg.format = if matches!(self, Self::H264Movie { .. }) {
                    filmcraft_h264enc::PacketFormat::LengthPrefixed
                } else {
                    filmcraft_h264enc::PacketFormat::AnnexB
                };
                cfg.aud = !matches!(self, Self::H264Movie { .. });
                let fps = spec.frame_rate.numerator as f64 / spec.frame_rate.denominator as f64;
                cfg.keyint = keyint
                    .filter(|v| *v > 0)
                    .unwrap_or_else(|| (fps * 2.).round().max(1.) as u32);
                cfg.slices = (spec.height.div_ceil(16)).div_ceil(4).max(1);
                let target = ((spec.width as f64 * spec.height as f64 * fps * f64::from(0.3f32)
                    / 1000.)
                    .round() as u32)
                    .max(100);
                cfg.rate = filmcraft_h264enc::RateControl::Vbr {
                    target_kbps: target,
                    max_kbps: (u64::from(target) * 3 / 2).min(u64::from(u32::MAX)) as u32,
                };
                if let Some(bitrate) = bitrate {
                    cfg.rate = bitrate.native();
                }
                cfg.sar = (sar.0.clamp(1, 65535) as u16, sar.1.clamp(1, 65535) as u16);
                NativeEncoder::H264(Box::new(filmcraft_h264enc::Encoder::new(cfg)?))
            }
            Self::HevcMovie { .. } => NativeEncoder::Hevc,
            Self::Mjpeg { quality } => NativeEncoder::Mjpeg {
                quality: quality.clamp(1, 100),
            },
            Self::Apv(p) => NativeEncoder::Apv(Self::apv_encoder(p, spec)?),
            Self::Atom { .. } => return Self::Dnx(2).encoder(spec, pass),
            Self::Mxf { prores, .. } => {
                return (if prores {
                    Self::ProRes(3)
                } else {
                    Self::Dnx(2)
                })
                .encoder(spec, pass)
            }
            Self::Dnx(p) => NativeEncoder::Dnx(filmcraft_dnx::Encoder::new(
                match p {
                    0 => filmcraft_dnx::Profile::Lb,
                    1 => filmcraft_dnx::Profile::Sq,
                    2 => filmcraft_dnx::Profile::Hq,
                    _ => anyhow::bail!("invalid native intra profile"),
                },
                spec.width as u32,
                spec.height as u32,
            )?),
            Self::ProRes(p) => NativeEncoder::ProRes(filmcraft_prores::Encoder::new(
                Self::prores_profile(p)?,
                spec.width as u32,
                spec.height as u32,
            )),
        })
    }
}
enum NativeEncoder {
    Hevc,
    Mjpeg { quality: u8 },
    H264(Box<filmcraft_h264enc::Encoder>),
    Apv(filmcraft_apv::Encoder),
    Dnx(filmcraft_dnx::Encoder),
    ProRes(filmcraft_prores::Encoder),
}
impl NativeEncoder {
    fn frame(&mut self, surface: GroupSurface) -> Result<Vec<u8>> {
        let (w, h) = (surface.width as u32, surface.height as u32);
        if let Self::Mjpeg { quality } = self {
            ensure!(
                surface.rgba.len() == surface.width * surface.height * 4,
                "invalid MJPEG frame buffer"
            );
            let rgb = surface
                .rgba
                .chunks_exact(4)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect::<Vec<_>>();
            let mut packet = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut packet, *quality).encode(
                &rgb,
                w,
                h,
                image::ExtendedColorType::Rgb8,
            )?;
            return Ok(packet);
        }
        let raw = filmcraft_yuv422_pixels::yuv422_10(&surface)?;
        drop(surface);
        fn fill(raw: &[u8], y: &mut [u16], cb: &mut [u16], cr: &mut [u16]) {
            for (value, bytes) in y
                .iter_mut()
                .chain(cb.iter_mut())
                .chain(cr.iter_mut())
                .zip(raw.chunks_exact(2))
            {
                *value = u16::from_le_bytes([bytes[0], bytes[1]]);
            }
        }
        Ok(match self {
            Self::Mjpeg { .. } => unreachable!("MJPEG was encoded from source RGB8"),
            Self::Hevc => anyhow::bail!("HEVC requires native movie routing"),
            Self::H264(_) => anyhow::bail!("H.264 requires delayed packet handling"),
            Self::Apv(encoder) => {
                // Upstream export supplies 10-bit planes to both 10- and 12-bit profiles.
                let mut f =
                    filmcraft_apv::Frame::new(w, h, filmcraft_apv::ChromaFormat::Yuv422, 10, false);
                fill(&raw, &mut f.y, &mut f.cb, &mut f.cr);
                drop(raw);
                encoder.encode_raw_au(&f)?
            }
            Self::Dnx(encoder) => {
                let mut f =
                    filmcraft_dnx::Frame::new(w, h, filmcraft_dnx::ChromaFormat::Yuv422, 10, false);
                fill(&raw, &mut f.y, &mut f.cb, &mut f.cr);
                drop(raw);
                encoder.encode(&f)?
            }
            Self::ProRes(encoder) => {
                let mut f = filmcraft_prores::Frame::new(
                    w,
                    h,
                    filmcraft_prores::ChromaFormat::Yuv422,
                    10,
                    false,
                );
                fill(&raw, &mut f.y, &mut f.cb, &mut f.cr);
                drop(raw);
                encoder.encode(&f)?
            }
        })
    }
}

enum NativeMux {
    #[cfg(target_os = "macos")]
    Hevc(video_kadr_vt::movie::HevcMovie<GuardedFile>),
    Mov(Mp4Writer<GuardedFile>),
    Mxf(filmcraft_mxf::MxfWriter<GuardedFile>),
    Atom {
        output: GuardedFile,
        picture: filmcraft_mxf::MxfWriter<GuardedFile>,
        sound: Vec<filmcraft_mxf::MxfWriter<GuardedFile>>,
        sample_bytes: usize,
    },
}
impl NativeMux {
    fn write_sample(&mut self, track: usize, sample: WriteSample<'_>) -> Result<()> {
        match self {
            #[cfg(target_os = "macos")]
            Self::Hevc(mux) => mux
                .audio_packet(sample.data, sample.duration)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
            Self::Mov(mux) => mux.write_sample(track, sample)?,
            Self::Atom {
                picture,
                sound,
                sample_bytes,
                ..
            } => {
                if track == 0 {
                    picture.push_picture(sample.data, filmcraft_mxf::FrameInfo::intra())?;
                } else {
                    ensure!(
                        matches!(sound.len(), 1 | 2 | 6)
                            && matches!(*sample_bytes, 2 | 3)
                            && sample.data.len() % (sound.len() * *sample_bytes) == 0,
                        "invalid OP-Atom PCM layout"
                    );
                    let channels = sound.len();
                    for (channel, mux) in sound.iter_mut().enumerate() {
                        let mut mono = Vec::with_capacity(sample.data.len() / channels);
                        for frame in sample.data.chunks_exact(channels * *sample_bytes) {
                            mono.extend_from_slice(
                                &frame[channel * *sample_bytes..(channel + 1) * *sample_bytes],
                            );
                        }
                        mux.push_sound(0, &mono)?;
                    }
                }
            }
            Self::Mxf(mux) => {
                if track == 0 {
                    mux.push_picture(sample.data, filmcraft_mxf::FrameInfo::intra())?
                } else {
                    mux.push_sound(0, sample.data)?
                }
            }
        }
        Ok(())
    }
    fn finish(self) -> Result<GuardedFile> {
        Ok(match self {
            #[cfg(target_os = "macos")]
            Self::Hevc(mux) => mux.finish().map_err(|e| anyhow::anyhow!(e.to_string()))?,
            Self::Mov(mux) => mux.finish_faststart()?,
            Self::Mxf(mux) => mux.finish()?,
            Self::Atom {
                output,
                picture,
                sound,
                ..
            } => {
                let mut zip = zip::ZipWriter::new(output);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored)
                    .large_file(true);
                let mut files = vec![("export.mxf".to_string(), picture.finish()?)];
                for (channel, mux) in sound.into_iter().enumerate() {
                    files.push((format!("export_A{}.mxf", channel + 1), mux.finish()?));
                }
                for (name, mut file) in files {
                    file.seek(SeekFrom::Start(0))?;
                    zip.start_file(name, options)?;
                    std::io::copy(&mut file, &mut zip)?;
                }
                zip.finish()?
            }
        })
    }
}

struct NativeAac {
    encoder: Box<filmcraft_aac::Encoder>,
    written: u64,
    total_media: u64,
}
impl NativeAac {
    fn new(frames: u64, bps: u32, rate: u32, channels: usize) -> Result<Self> {
        ensure!(frames > 0, "native AAC source is empty");
        let encoder = Box::new(filmcraft_aac::Encoder::new(
            filmcraft_aac::EncoderConfig::cbr(rate, channels, bps),
        )?);
        let total_media = frames
            .checked_add(u64::from(encoder.priming_samples()))
            .context("AAC media duration overflow")?;
        Ok(Self {
            encoder,
            written: 0,
            total_media,
        })
    }
    fn write(&mut self, mux: &mut NativeMux, track: usize, units: Vec<Vec<u8>>) -> Result<()> {
        for unit in units {
            let remaining = self
                .total_media
                .checked_sub(self.written)
                .context("AAC clock exceeded source range")?;
            let duration = remaining.min(1024) as u32;
            ensure!(duration > 0, "AAC emitted excess padding packets");
            mux.write_sample(
                track,
                WriteSample {
                    data: &unit,
                    duration,
                    composition_offset: 0,
                    is_sync: true,
                },
            )?;
            self.written += u64::from(duration);
        }
        Ok(())
    }
}

struct NativeWriter {
    encoder: NativeEncoder,
    mux: NativeMux,
    video: usize,
    audio: Option<(usize, GuardedFile)>,
    aac: Option<NativeAac>,
    pcm_bits: u16,
    audio_rate: u32,
    audio_channels: usize,
    audio_frames: u64,
    total_audio_frames: u64,
    next_frame: u64,
    spec: RasterVideoSpec,
    stored: i64,
    first_offset: Option<i64>,
    max_display: i64,
}
impl NativeWriter {
    fn new(
        file: GuardedFile,
        spec: RasterVideoSpec,
        codec: Codec,
        audio: Option<(PathBuf, u64)>,
        pixel_aspect: Option<(u32, u32)>,
        metadata: Vec<(String, String)>,
        temp_parent: PathBuf,
        pass: filmcraft_h264enc::Pass,
        native_audio: Option<crate::domain::filmcraft_export_movie::AudioSettings>,
    ) -> Result<Self> {
        if let Codec::HevcMovie {
            mov,
            target_kbps,
            max_kbps,
            cbr,
            keyint,
            audio: settings,
        } = codec
        {
            #[cfg(not(target_os = "macos"))]
            anyhow::bail!("native HEVC requires macOS VideoToolbox");
            #[cfg(target_os = "macos")]
            {
                let total_audio_frames = audio.as_ref().map_or(0, |(_, frames)| *frames);
                let control = video_kadr_vt::rate::RateControl {
                    mode: if cbr {
                        video_kadr_vt::rate::Mode::Cbr
                    } else {
                        video_kadr_vt::rate::Mode::Vbr {
                            max_bps: max_kbps
                                .checked_mul(1000)
                                .context("HEVC max bitrate overflow")?,
                        }
                    },
                    target_bps: target_kbps
                        .checked_mul(1000)
                        .context("HEVC target bitrate overflow")?,
                    rate_num: spec.frame_rate.numerator,
                    rate_den: spec.frame_rate.denominator,
                };
                let cancel = file.cancel.clone();
                let deadline = file.deadline;
                let mut mux = video_kadr_vt::movie::HevcMovie::new_with_rate_control(
                    file,
                    mov,
                    spec.width,
                    spec.height,
                    control,
                    keyint,
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                let mut tags = Vec::new();
                for (key, value) in metadata {
                    let tag = match key.as_str() {
                        "title" => "©nam",
                        "artist" => "©ART",
                        "copyright" => "©cpy",
                        "description" => "©des",
                        "comment" => "©cmt",
                        _ => anyhow::bail!("invalid HEVC metadata key"),
                    };
                    if key == "description" {
                        tags.push(("description".to_owned(), value.clone()));
                    }
                    tags.push((tag.to_owned(), value));
                }
                tags.push((
                    "©too".into(),
                    format!("Video Kadr {}", env!("CARGO_PKG_VERSION")),
                ));
                mux.container_options(
                    pixel_aspect
                        .map(|(n, d)| (n.max(1), d.max(1)))
                        .unwrap_or((1, 1)),
                    tags,
                )
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                let audio_rate = settings.sample_rate();
                let audio_channels = settings.channels() as usize;
                let pcm_bits = settings.pcm_bits();
                let multiplexer = if mov {
                    crate::domain::filmcraft_export_movie::Multiplexer::Mov
                } else {
                    crate::domain::filmcraft_export_movie::Multiplexer::Mp4
                };
                settings.validate_rate(multiplexer)?;
                let aac = if audio.is_some()
                    && settings.codec(multiplexer)
                        == crate::domain::filmcraft_export_movie::AudioCodec::Aac
                {
                    Some(NativeAac::new(
                        total_audio_frames,
                        settings.aac_bps(),
                        audio_rate,
                        audio_channels,
                    )?)
                } else {
                    None
                };
                let audio = if let Some((path, _)) = audio {
                    let (config, expected) = if let Some(aac) = &aac {
                        let mut config = TrackConfig::new(
                            SampleEntry::aac(
                                aac.encoder.audio_specific_config(),
                                audio_channels as u32,
                                audio_rate,
                            ),
                            audio_rate,
                        );
                        config.media_start = Some(i64::from(aac.encoder.priming_samples()));
                        (config, aac.total_media)
                    } else {
                        (
                            TrackConfig::new(
                                SampleEntry::pcm(PcmConfig {
                                    bits: pcm_bits,
                                    float: false,
                                    big_endian: false,
                                    signed: true,
                                    channels: audio_channels as u32,
                                    sample_rate: f64::from(audio_rate),
                                }),
                                audio_rate,
                            ),
                            total_audio_frames,
                        )
                    };
                    mux.audio_track(config, expected)
                        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
                    Some((
                        1,
                        GuardedFile {
                            file: File::open(path)?,
                            cancel,
                            deadline,
                        },
                    ))
                } else {
                    None
                };
                return Ok(Self {
                    encoder: NativeEncoder::Hevc,
                    mux: NativeMux::Hevc(mux),
                    video: 0,
                    audio,
                    aac,
                    pcm_bits,
                    audio_rate,
                    audio_channels,
                    audio_frames: 0,
                    total_audio_frames,
                    next_frame: 0,
                    spec,
                    stored: 0,
                    first_offset: None,
                    max_display: -1,
                });
            }
        }
        let total_audio_frames = audio.as_ref().map_or(0, |(_, frames)| *frames);
        let audio = audio.map(|(path, _)| path);
        let encoder = codec.encoder(spec, pass)?;
        if matches!(
            codec,
            Codec::MxfH264 { .. } | Codec::Mxf { .. } | Codec::Atom { .. }
        ) {
            let (prores, start, drop_frame, atom) = match codec {
                Codec::Mxf {
                    prores,
                    start,
                    drop_frame,
                } => (prores, start, drop_frame, false),
                Codec::Atom { start, drop_frame } => (false, start, drop_frame, true),
                Codec::MxfH264 {
                    start, drop_frame, ..
                } => (false, start, drop_frame, false),
                _ => unreachable!(),
            };
            use filmcraft_mxf::{
                PackageIds, Pattern, PictureCoding, PictureDesc, Rational, SoundDesc,
                StartTimecode, WriterConfig,
            };
            let rate = Rational::new(
                spec.frame_rate.numerator.try_into()?,
                spec.frame_rate.denominator.try_into()?,
            );
            let mut cfg = WriterConfig::new(
                if atom { Pattern::OpAtom } else { Pattern::Op1a },
                rate,
                PackageIds::from_seed(&uuid::Uuid::new_v4().to_string(), "Video Kadr"),
            );
            let coding = if matches!(codec, Codec::MxfH264 { .. }) {
                PictureCoding::Avc {
                    profile_idc: match codec {
                        Codec::MxfH264 { profile, .. } => profile.idc(),
                        _ => 100,
                    },
                    intra: false,
                }
            } else if prores {
                PictureCoding::ProRes { profile: 4 }
            } else {
                PictureCoding::Vc3 { cid: 1272 }
            };
            let mut picture = PictureDesc::new(coding, spec.width as u32, spec.height as u32);
            let (n, d) = pixel_aspect.unwrap_or((1, 1));
            let n = spec.width as u64 * n as u64;
            let d = spec.height as u64 * d as u64;
            let (mut a, mut b) = (n, d);
            while b != 0 {
                (a, b) = (b, a % b);
            }
            picture.aspect = Rational::new((n / a).try_into()?, (d / a).try_into()?);
            cfg.picture = Some(picture);
            cfg.timecode = Some(StartTimecode {
                frames: start,
                rate,
                drop_frame,
            });
            cfg.product = "Video Kadr".into();
            cfg.version = env!("CARGO_PKG_VERSION").into();
            let settings =
                native_audio.unwrap_or(crate::domain::filmcraft_export_movie::AudioSettings {
                    bits: 24,
                    sample_rate: Some(48000),
                    ..Default::default()
                });
            let audio_rate = settings.sample_rate();
            let audio_channels = settings.channels() as usize;
            let pcm_bits = settings.pcm_bits();
            let source = if let Some(path) = audio {
                if !atom {
                    cfg.sound.push(SoundDesc {
                        sample_rate: audio_rate,
                        channels: audio_channels as u32,
                        bits: u32::from(pcm_bits),
                    });
                }
                Some((
                    1,
                    GuardedFile {
                        file: File::open(path)?,
                        cancel: file.cancel.clone(),
                        deadline: file.deadline,
                    },
                ))
            } else {
                None
            };
            let mux = if atom {
                let temporary = || -> Result<GuardedFile> {
                    Ok(GuardedFile {
                        file: tempfile::tempfile_in(&temp_parent)?,
                        cancel: file.cancel.clone(),
                        deadline: file.deadline,
                    })
                };
                let mut sound = Vec::new();
                if source.is_some() {
                    for channel in 0..audio_channels {
                        let mut audio_cfg = WriterConfig::new(
                            Pattern::OpAtom,
                            rate,
                            filmcraft_mxf::PackageIds {
                                material: cfg.ids.material,
                                file: filmcraft_mxf::Umid::from_seed(
                                    format!("{}:A{}", uuid::Uuid::new_v4(), channel + 1).as_bytes(),
                                ),
                                material_name: cfg.ids.material_name.clone(),
                                file_name: format!("Video Kadr A{}", channel + 1),
                            },
                        );
                        audio_cfg.first_track_id = 3 + channel as u32;
                        audio_cfg.sound = vec![SoundDesc {
                            sample_rate: audio_rate,
                            channels: 1,
                            bits: u32::from(pcm_bits),
                        }];
                        audio_cfg.timecode = cfg.timecode;
                        sound.push(filmcraft_mxf::MxfWriter::new(temporary()?, audio_cfg)?);
                    }
                }
                let picture = filmcraft_mxf::MxfWriter::new(temporary()?, cfg)?;
                NativeMux::Atom {
                    output: file,
                    picture,
                    sound,
                    sample_bytes: usize::from(pcm_bits / 8),
                }
            } else {
                NativeMux::Mxf(filmcraft_mxf::MxfWriter::new(file, cfg)?)
            };
            return Ok(Self {
                encoder,
                mux,
                video: 0,
                audio: source,
                aac: None,
                pcm_bits,
                audio_rate,
                audio_channels,
                audio_frames: 0,
                total_audio_frames,
                next_frame: 0,
                spec,
                stored: 0,
                first_offset: None,
                max_display: -1,
            });
        }

        let mut options = WriterOptions::new(
            if matches!(
                codec,
                Codec::H264Movie {
                    multiplexer: crate::domain::filmcraft_export_movie::Multiplexer::Mp4,
                    ..
                }
            ) {
                Brand::Mp4
            } else {
                Brand::Mov
            },
        );
        if audio.is_some() {
            if let Codec::H264Movie { audio, .. } = codec {
                options.movie_timescale = audio.sample_rate();
            } else if let Some(settings) = native_audio {
                options.movie_timescale = settings.sample_rate();
            }
        }
        for (key, value) in metadata {
            let tag = match key.as_str() {
                "title" => "©nam",
                "artist" => "©ART",
                "copyright" => "©cpy",
                "description" => "©des",
                "comment" => "©cmt",
                _ => anyhow::bail!("invalid native video metadata key"),
            };
            if key == "description" {
                options.metadata.push(("description".into(), value.clone()));
            }
            options.metadata.push((tag.into(), value));
        }
        options.metadata.push((
            "©too".into(),
            format!("Video Kadr {}", env!("CARGO_PKG_VERSION")),
        ));
        let cancel = file.cancel.clone();
        let deadline = file.deadline;
        let mut mux = Mp4Writer::new(file, options)?;
        let mut entry =
            if let (Codec::H264Movie { .. }, NativeEncoder::H264(encoder)) = (codec, &encoder) {
                SampleEntry::avc(
                    filmcraft_isobmff::AvcConfig::parse(&encoder.avcc())?,
                    spec.width as u16,
                    spec.height as u16,
                )
            } else {
                codec.entry(spec)?
            };
        let video = entry.video.as_mut().context("native video entry missing")?;
        if !matches!(codec, Codec::H264Movie { .. } | Codec::Mjpeg { .. }) {
            video.color = Some(ColorInfo::Nclc {
                primaries: 1,
                transfer: 1,
                matrix: 1,
            });
            video.field_info = Some(FieldInfo {
                fields: 1,
                detail: 0,
            });
            let (n, d) = pixel_aspect.unwrap_or((1, 1));
            video.pixel_aspect = Some((n.max(1), d.max(1)));
        }
        let mut track = TrackConfig::new(entry, spec.frame_rate.numerator);
        if let (Codec::H264Movie { .. }, NativeEncoder::H264(encoder)) = (codec, &encoder) {
            track.media_start = (encoder.delay() > 0 && spec.frames > 1)
                .then_some(i64::from(spec.frame_rate.denominator));
        }
        let video = mux.add_track(track)?;
        let audio_settings = if let Codec::H264Movie {
            multiplexer, audio, ..
        } = codec
        {
            Some((multiplexer, audio))
        } else {
            native_audio.map(|settings| {
                (
                    crate::domain::filmcraft_export_movie::Multiplexer::Mov,
                    settings,
                )
            })
        };
        let audio_rate = audio_settings
            .map(|(_, s)| s.sample_rate())
            .unwrap_or(48000);
        let audio_channels = audio_settings
            .map(|(_, s)| s.channels() as usize)
            .unwrap_or(2);
        let pcm_bits = audio_settings.map(|(_, s)| s.pcm_bits()).unwrap_or(24);
        let aac = if let Some((multiplexer, settings)) = audio_settings.filter(|_| audio.is_some())
        {
            if settings.codec(multiplexer) == crate::domain::filmcraft_export_movie::AudioCodec::Aac
            {
                Some(NativeAac::new(
                    total_audio_frames,
                    settings.aac_bps(),
                    audio_rate,
                    audio_channels,
                )?)
            } else {
                None
            }
        } else {
            None
        };
        let audio = if let Some(path) = audio {
            let audio_track = if let Some(aac) = &aac {
                let entry = SampleEntry::aac(
                    aac.encoder.audio_specific_config(),
                    audio_channels as u32,
                    audio_rate,
                );
                let mut config = TrackConfig::new(entry, audio_rate);
                config.media_start = Some(i64::from(aac.encoder.priming_samples()));
                config
            } else {
                TrackConfig::new(
                    SampleEntry::pcm(PcmConfig {
                        bits: pcm_bits,
                        float: false,
                        big_endian: false,
                        signed: true,
                        channels: audio_channels as u32,
                        sample_rate: f64::from(audio_rate),
                    }),
                    audio_rate,
                )
            };
            let track = mux.add_track(audio_track)?;
            Some((
                track,
                GuardedFile {
                    file: File::open(path)?,
                    cancel,
                    deadline,
                },
            ))
        } else {
            None
        };
        Ok(Self {
            encoder,
            mux: NativeMux::Mov(mux),
            video,
            audio,
            aac,
            pcm_bits,
            audio_rate,
            audio_channels,
            audio_frames: 0,
            total_audio_frames,
            next_frame: 0,
            stored: 0,
            first_offset: None,
            max_display: -1,
            spec,
        })
    }
    fn frame(mut self, index: u64, surface: GroupSurface) -> Result<Self> {
        ensure!(index == self.next_frame, "native intra frame order changed");
        if matches!(self.encoder, NativeEncoder::Hevc) {
            #[cfg(target_os = "macos")]
            if let NativeMux::Hevc(mux) = &mut self.mux {
                mux.frame(&surface.rgba)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            } else {
                anyhow::bail!("invalid HEVC mux")
            }
            #[cfg(not(target_os = "macos"))]
            anyhow::bail!("native HEVC requires macOS");
        } else if let NativeEncoder::H264(encoder) = &mut self.encoder {
            let raw = crate::domain::filmcraft_h264_pixels::yuv420(&surface)?;
            let y_len = self.spec.width * self.spec.height;
            let c_len = y_len / 4;
            let frame = filmcraft_h264enc::YuvFrame {
                y: &raw[..y_len],
                u: &raw[y_len..y_len + c_len],
                v: &raw[y_len + c_len..],
                y_stride: self.spec.width,
                uv_stride: self.spec.width / 2,
            };
            let mut packets = encoder.try_encode(
                &frame,
                index as i64 * self.spec.frame_rate.denominator as i64,
            )?;
            if index + 1 == self.spec.frames {
                packets.extend(encoder.flush());
            }
            for packet in packets {
                if let NativeMux::Mov(mux) = &mut self.mux {
                    mux.write_sample(
                        self.video,
                        WriteSample {
                            data: &packet.data,
                            duration: self.spec.frame_rate.denominator,
                            composition_offset: i32::try_from(packet.pts - packet.dts)?,
                            is_sync: packet.keyframe,
                        },
                    )?;
                    self.stored += 1;
                    continue;
                }
                let NativeMux::Mxf(mux) = &mut self.mux else {
                    anyhow::bail!("invalid native H264 mux")
                };
                let offset = packet.pts - packet.dts;
                let first = *self.first_offset.get_or_insert(offset);
                let display =
                    self.stored + (offset - first) / self.spec.frame_rate.denominator as i64;
                let kind = if packet.keyframe {
                    filmcraft_mxf::CodedKind::Intra
                } else if display < self.max_display {
                    filmcraft_mxf::CodedKind::Bidirectional
                } else {
                    filmcraft_mxf::CodedKind::Predicted
                };
                self.max_display = self.max_display.max(display);
                mux.push_picture(
                    &packet.data,
                    filmcraft_mxf::FrameInfo {
                        key: packet.keyframe,
                        kind,
                        display: Some(display),
                    },
                )?;
                self.stored += 1;
            }
        } else {
            let packet = self.encoder.frame(surface)?;
            self.mux.write_sample(
                self.video,
                WriteSample {
                    data: &packet,
                    duration: self.spec.frame_rate.denominator,
                    composition_offset: 0,
                    is_sync: true,
                },
            )?;
        }
        if let Some((track, source)) = &mut self.audio {
            let target = RasterVideoSpec {
                frames: index + 1,
                ..self.spec
            }
            .audio_frames_at_rate(self.audio_rate)?;
            // Preserve every prepared source sample. Frame-relative ceil is
            // only a packetisation boundary, not the audio range duration.
            let target = if index + 1 == self.spec.frames {
                self.total_audio_frames
            } else {
                target.min(self.total_audio_frames)
            };
            let count = target
                .checked_sub(self.audio_frames)
                .context("native audio clock regressed")?;
            ensure!(
                count <= u64::from(self.audio_rate),
                "native audio page exceeds one second"
            );
            let mut raw = vec![0u8; count as usize * self.audio_channels * 4];
            source.read_exact(&mut raw)?;
            if let Some(aac) = &mut self.aac {
                let mut planar = vec![Vec::with_capacity(count as usize); self.audio_channels];
                for frame in raw.chunks_exact(self.audio_channels * 4) {
                    for (c, channel) in planar.iter_mut().enumerate() {
                        channel.push(f32::from_le_bytes(frame[c * 4..c * 4 + 4].try_into()?));
                    }
                }
                let units = aac
                    .encoder
                    .encode(&planar.iter().map(Vec::as_slice).collect::<Vec<_>>());
                aac.write(&mut self.mux, *track, units)?;
                if index + 1 == self.spec.frames {
                    let tail = aac.encoder.flush();
                    aac.write(&mut self.mux, *track, tail)?;
                }
            } else if count > 0 {
                let pcm = if self.pcm_bits == 24 {
                    filmcraft_yuv422_pixels::pcm24(&raw)?
                } else {
                    let mut out = Vec::with_capacity(raw.len() / 2);
                    for bytes in raw.chunks_exact(4) {
                        let sample = f32::from_le_bytes(bytes.try_into()?);
                        let value =
                            crate::domain::filmcraft_pcm_export::quantise(sample, 16) as i16;
                        out.extend_from_slice(&value.to_le_bytes());
                    }
                    out
                };
                self.mux.write_sample(
                    *track,
                    WriteSample {
                        data: &pcm,
                        duration: count as u32,
                        composition_offset: 0,
                        is_sync: true,
                    },
                )?;
            }
            self.audio_frames = target;
        }
        self.next_frame += 1;
        Ok(self)
    }
    fn finish(self) -> Result<()> {
        ensure!(
            self.next_frame == self.spec.frames,
            "native intra frame count changed"
        );
        if matches!(self.encoder, NativeEncoder::H264(_)) {
            ensure!(
                self.stored == self.spec.frames as i64,
                "native H.264 delayed frame count changed"
            );
        }
        ensure!(
            self.audio_frames == self.total_audio_frames,
            "native audio sample count changed"
        );
        if let Some(aac) = &self.aac {
            ensure!(
                aac.written == aac.total_media,
                "AAC logical source duration changed"
            );
        }
        let file = self.mux.finish()?;
        file.check()?;
        file.file.sync_all()?;
        Ok(())
    }
}

fn analyze_h264(
    mut encoder: Box<filmcraft_h264enc::Encoder>,
    spec: RasterVideoSpec,
    index: u64,
    surface: GroupSurface,
) -> Result<Box<filmcraft_h264enc::Encoder>> {
    let raw = crate::domain::filmcraft_h264_pixels::yuv420(&surface)?;
    let y = spec.width * spec.height;
    let c = y / 4;
    let frame = filmcraft_h264enc::YuvFrame {
        y: &raw[..y],
        u: &raw[y..y + c],
        v: &raw[y + c..],
        y_stride: spec.width,
        uv_stride: spec.width / 2,
    };
    drop(encoder.try_encode(&frame, index as i64 * spec.frame_rate.denominator as i64)?);
    if index + 1 == spec.frames {
        drop(encoder.flush());
    }
    Ok(encoder)
}

pub async fn encode<F, Fut>(
    output: &Path,
    spec: RasterVideoSpec,
    timeout: Duration,
    parent_cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    audio: Option<RasterAudio<'_>>,
    codec: Codec,
    pixel_aspect: Option<(u32, u32)>,
    metadata: &[(String, String)],
    native_pool: Option<crate::runtime::cpu_pool::CpuPool>,
    native_audio: Option<crate::domain::filmcraft_export_movie::AudioSettings>,
    mut frame: F,
) -> Result<()>
where
    F: FnMut(u64, u64) -> Fut,
    Fut: Future<Output = Result<GroupSurface>>,
{
    ensure!(
        codec.valid()
            && spec.width <= 3840
            && spec.height <= 2160
            && spec.width % 2 == 0
            && spec.height % 2 == 0,
        "invalid native intra canvas/profile"
    );
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("native intra deadline overflow")?;
    let cancel = parent_cancel.child_token();
    let _cancel_guard = cancel.clone().drop_guard();
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .context("native intra output requires parent")?;
    ensure!(!output.try_exists()?, "native intra output already exists");
    let staging = parent.join(format!(
        ".native-{}-{}.mov",
        codec.prefix(),
        uuid::Uuid::new_v4()
    ));
    let file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&staging)?;
    let _staging = Staging(staging.clone(), cancel.clone());
    let file = GuardedFile {
        file,
        cancel: cancel.clone(),
        deadline: deadline.into_std(),
    };
    let two_pass = matches!(
        codec,
        Codec::MxfH264 {
            bitrate: Some(crate::domain::filmcraft_export_h264::ResolvedBitrate {
                mode: crate::domain::filmcraft_export_h264::BitrateMode::Vbr2Pass,
                ..
            }),
            ..
        } | Codec::H264Movie {
            bitrate: Some(crate::domain::filmcraft_export_h264::ResolvedBitrate {
                mode: crate::domain::filmcraft_export_h264::BitrateMode::Vbr2Pass,
                ..
            }),
            ..
        }
    );
    let pass = if two_pass {
        let initial = if let Some(pool) = &native_pool {
            pool.execute(cancel.clone(), move |_| {
                codec.encoder(spec, filmcraft_h264enc::Pass::First)
            })
            .await
            .map_err(|error| anyhow::anyhow!("native initialization: {error}"))?
        } else {
            tokio::task::spawn_blocking(move || codec.encoder(spec, filmcraft_h264enc::Pass::First))
                .await??
        };
        let NativeEncoder::H264(mut analyzer) = initial else {
            anyhow::bail!("two-pass analysis requires H264")
        };
        for index in 0..spec.frames {
            let surface = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("native first pass cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("native first pass timeout"),result=frame(index,spec.time_us(index)?)=>result?,};
            surface.validate()?;
            ensure!(
                (surface.width, surface.height) == (spec.width, spec.height),
                "native first pass dimensions changed"
            );
            analyzer = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("native first pass cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("native first pass timeout"),result=async{
                if let Some(pool)=&native_pool {pool.execute(cancel.clone(),move |_|analyze_h264(analyzer,spec,index,surface)).await.map_err(|e|anyhow::anyhow!("native first pass CPU worker: {e:?}"))}
                else{tokio::task::spawn_blocking(move||analyze_h264(analyzer,spec,index,surface)).await?}
            }=>result?,};
            let _ = progress.send(47.5 * (index + 1) as f64 / spec.frames as f64);
        }
        ensure!(
            !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
            "native export cancelled or timed out between passes"
        );
        filmcraft_h264enc::Pass::Second(
            analyzer
                .pass_stats()
                .context("native first pass statistics missing")?,
        )
    } else {
        filmcraft_h264enc::Pass::Single
    };
    let audio = audio.map(|a| (a.path.to_path_buf(), a.frames));
    let metadata = metadata.to_vec();
    let temp_parent = parent.to_path_buf();
    let initialize = move || {
        NativeWriter::new(
            file,
            spec,
            codec,
            audio,
            pixel_aspect,
            metadata,
            temp_parent,
            pass,
            native_audio,
        )
    };
    let mut writer = if let Some(pool) = &native_pool {
        pool.execute(cancel.clone(), move |_| initialize())
            .await
            .map_err(|error| anyhow::anyhow!("native writer initialization: {error}"))?
    } else {
        tokio::task::spawn_blocking(initialize).await??
    };
    for index in 0..spec.frames {
        let surface = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("native intra export cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("native intra export timeout"),result=frame(index,spec.time_us(index)?)=>result?,};
        surface.validate()?;
        ensure!(
            (surface.width, surface.height) == (spec.width, spec.height),
            "native intra dimensions changed"
        );
        writer = tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("native intra export cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("native intra export timeout"),result=async {
            if let Some(pool) = &native_pool {
                pool.execute(cancel.clone(), move |_| writer.frame(index, surface))
                    .await.map_err(|error| anyhow::anyhow!("native intra CPU worker: {error:?}"))
            } else {
                tokio::task::spawn_blocking(move || writer.frame(index, surface)).await?
            }
        }=>result?,};
        let _ = progress.send(if two_pass {
            47.5 + 47.5 * (index + 1) as f64 / spec.frames as f64
        } else {
            95. * (index + 1) as f64 / spec.frames as f64
        });
    }
    tokio::select! {biased;_=cancel.cancelled()=>anyhow::bail!("native intra export cancelled"),_=tokio::time::sleep_until(deadline)=>anyhow::bail!("native intra export timeout"),result=async{if let Some(pool)=&native_pool{pool.execute(cancel.clone(),move |_|writer.finish()).await.map_err(|error|anyhow::anyhow!("native writer finish: {error}"))}else{tokio::task::spawn_blocking(move||writer.finish()).await?}}=>result?,}
    ensure!(
        !cancel.is_cancelled() && tokio::time::Instant::now() < deadline,
        "native intra export cancelled/timeout before publication"
    );
    tokio::fs::hard_link(&staging, output)
        .await
        .context("publish native intra without overwrite")?;
    let _ = progress.send(100.);
    Ok(())
}
