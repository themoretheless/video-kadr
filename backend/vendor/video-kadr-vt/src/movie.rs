//! Streaming HEVC Main SDR movie writer. Callers own publication/cancellation.
use crate::{Encoder, EncoderCodec, Error, Surface};
use filmcraft_isobmff::{
    Brand, ColorInfo, HevcConfig, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions,
};
use std::io::{Seek, Write};
use video_kadr_hevc_frame::Nv12;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub struct HevcMovie<W: Write + Seek> {
    encoder: Encoder,
    output: Option<W>,
    mux: Option<Mp4Writer<W>>,
    options: Option<WriterOptions>,
    config: Option<Vec<u8>>,
    track: usize,
    width: usize,
    height: usize,
    rate: u32,
    duration: u32,
    keyint: u32,
    frames: u64,
    failed: bool,
    pixel_aspect: (u32, u32),
    audio: Option<AudioTrack>,
}
struct AudioTrack {
    config: TrackConfig,
    track: Option<usize>,
    written: u64,
    expected: u64,
    pcm_frame_bytes: Option<usize>,
}
impl<W: Write + Seek> HevcMovie<W> {
    pub fn new_with_rate_control(
        output: W,
        mov: bool,
        width: usize,
        height: usize,
        control: crate::rate::RateControl,
        keyint: u32,
    ) -> Result<Self> {
        Self::initialize(
            output,
            mov,
            width,
            height,
            control.rate_num,
            control.rate_den,
            control.target_bps,
            keyint,
            Some(control),
        )
    }
    /// Bitrate is an average target, not a strict CBR or maximum-rate guarantee.
    pub fn new(
        output: W,
        mov: bool,
        width: usize,
        height: usize,
        rate: u32,
        duration: u32,
        bitrate: u32,
        keyint: u32,
    ) -> Result<Self> {
        Self::initialize(
            output, mov, width, height, rate, duration, bitrate, keyint, None,
        )
    }

    fn initialize(
        output: W,
        mov: bool,
        width: usize,
        height: usize,
        rate: u32,
        duration: u32,
        bitrate: u32,
        keyint: u32,
        control: Option<crate::rate::RateControl>,
    ) -> Result<Self> {
        if width == 0
            || height == 0
            || width % 2 != 0
            || height % 2 != 0
            || width > u16::MAX as usize
            || height > u16::MAX as usize
            || rate == 0
            || rate > i32::MAX as u32
            || duration == 0
            || keyint == 0
        {
            return Err(Box::new(Error(
                "invalid HEVC movie geometry or timing".into(),
            )));
        }
        let mut encoder = if let Some(control) = control {
            Encoder::new_with_rate_control(EncoderCodec::Hevc, width, height, 8, control)?
        } else {
            Encoder::new_with_bitrate(EncoderCodec::Hevc, width, height, 8, bitrate)?
        };
        encoder.set_colour(1, 1, 1)?;
        Ok(Self {
            encoder,
            output: Some(output),
            mux: None,
            options: Some(WriterOptions::new(if mov {
                Brand::Mov
            } else {
                Brand::Mp4
            })),
            config: None,
            track: 0,
            width,
            height,
            rate,
            duration,
            keyint,
            frames: 0,
            failed: false,
            pixel_aspect: (1, 1),
            audio: None,
        })
    }
    pub fn container_options(
        &mut self,
        pixel_aspect: (u32, u32),
        metadata: Vec<(String, String)>,
    ) -> Result<()> {
        if self.failed || self.frames != 0 || pixel_aspect.0 == 0 || pixel_aspect.1 == 0 {
            return Err(Box::new(Error("invalid HEVC container options".into())));
        }
        self.pixel_aspect = pixel_aspect;
        self.options.as_mut().unwrap().metadata = metadata;
        Ok(())
    }
    /// Configure a prepared PCM or AAC track before the first video frame.
    /// expected_media includes AAC priming; config.media_start trims that delay.
    pub fn audio_track(&mut self, config: TrackConfig, expected_media: u64) -> Result<()> {
        if self.failed
            || self.frames != 0
            || self.audio.is_some()
            || expected_media == 0
            || config.timescale == 0
        {
            return Err(Box::new(Error("invalid HEVC audio track state".into())));
        }
        let pcm_frame_bytes = match &config.entry.codec {
            filmcraft_isobmff::CodecConfig::Pcm(pcm) => {
                if !matches!(pcm.channels, 1 | 2 | 6)
                    || !matches!(pcm.bits, 16 | 24)
                    || pcm.float
                    || pcm.sample_rate != f64::from(config.timescale)
                {
                    return Err(Box::new(Error("unsupported HEVC PCM layout".into())));
                }
                Some(pcm.channels as usize * usize::from(pcm.bits / 8))
            }
            filmcraft_isobmff::CodecConfig::Aac(_) => None,
            _ => return Err(Box::new(Error("HEVC audio must be PCM or AAC".into()))),
        };
        self.options.as_mut().unwrap().movie_timescale = config.timescale;
        self.audio = Some(AudioTrack {
            config,
            track: None,
            written: 0,
            expected: expected_media,
            pcm_frame_bytes,
        });
        Ok(())
    }

    /// Write source-prepared packets after the first video frame initializes hvcC.
    pub fn audio_packet(&mut self, data: &[u8], duration: u32) -> Result<()> {
        if self.failed {
            return Err(Box::new(Error("HEVC movie stopped after failure".into())));
        }
        self.failed = true;
        let audio = self
            .audio
            .as_mut()
            .ok_or_else(|| Error("HEVC audio track missing".into()))?;
        let track = audio
            .track
            .ok_or_else(|| Error("HEVC video must begin before audio packets".into()))?;
        let next = audio
            .written
            .checked_add(u64::from(duration))
            .ok_or_else(|| Error("HEVC audio duration overflow".into()))?;
        if duration == 0
            || data.is_empty()
            || next > audio.expected
            || audio
                .pcm_frame_bytes
                .is_some_and(|n| (duration as usize).checked_mul(n) != Some(data.len()))
        {
            return Err(Box::new(Error(
                "HEVC audio packet exceeds declared range or layout".into(),
            )));
        }
        self.mux.as_mut().unwrap().write_sample(
            track,
            WriteSample {
                data,
                duration,
                composition_offset: 0,
                is_sync: true,
            },
        )?;
        audio.written = next;
        self.failed = false;
        Ok(())
    }

    pub fn frame(&mut self, rgba: &[u8]) -> Result<()> {
        if self.failed {
            return Err(Box::new(Error("HEVC movie stopped after failure".into())));
        }
        self.failed = true;
        let nv12 = Nv12::from_rgba(self.width, self.height, rgba)
            .map_err(|e| Error(format!("HEVC upload: {e:?}")))?;
        let surface = Surface::from_nv12(&nv12)?;
        let pts = self
            .frames
            .checked_mul(u64::from(self.duration))
            .and_then(|v| i64::try_from(v).ok())
            .ok_or_else(|| Error("HEVC timestamp overflow".into()))?;
        let packet = self.encoder.encode_with_keyframe(
            &surface,
            pts,
            i64::from(self.duration),
            self.rate as i32,
            self.frames % u64::from(self.keyint) == 0,
        )?;
        if i128::from(packet.pts.value) * i128::from(self.rate)
            != i128::from(pts) * i128::from(packet.pts.timescale)
            || i128::from(packet.duration.value) * i128::from(self.rate)
                != i128::from(self.duration) * i128::from(packet.duration.timescale)
        {
            return Err(Box::new(Error("HEVC encoder changed sample timing".into())));
        }
        if self.frames % u64::from(self.keyint) == 0 && !packet.key_frame {
            return Err(Box::new(Error(
                "HEVC encoder ignored requested keyframe".into(),
            )));
        }
        if let Some(config) = &self.config {
            if *config != packet.decoder_configuration {
                return Err(Box::new(Error(
                    "HEVC configuration changed midstream".into(),
                )));
            }
        } else {
            if !packet.key_frame {
                return Err(Box::new(Error(
                    "HEVC stream does not start with a keyframe".into(),
                )));
            }
            let mut entry = SampleEntry::hevc(
                HevcConfig::parse(&packet.decoder_configuration)?,
                self.width as u16,
                self.height as u16,
            );
            entry.video.as_mut().unwrap().color = Some(ColorInfo::Nclc {
                primaries: 1,
                transfer: 1,
                matrix: 1,
            });
            entry.video.as_mut().unwrap().pixel_aspect=Some(self.pixel_aspect);
            let mut mux =
                Mp4Writer::new(self.output.take().unwrap(), self.options.take().unwrap())?;
            self.track = mux.add_track(TrackConfig::new(entry, self.rate))?;
            if let Some(audio) = &mut self.audio {
                audio.track = Some(mux.add_track(audio.config.clone())?);
            }
            self.config = Some(packet.decoder_configuration.clone());
            self.mux = Some(mux);
        }
        self.mux.as_mut().unwrap().write_sample(
            self.track,
            WriteSample {
                data: &packet.data,
                duration: self.duration,
                composition_offset: 0,
                is_sync: packet.key_frame,
            },
        )?;
        self.frames += 1;
        self.failed = false;
        Ok(())
    }
    pub fn finish(self) -> Result<W> {
        if self.failed || self.frames == 0 {
            return Err(Box::new(Error(
                "cannot finish failed or empty HEVC movie".into(),
            )));
        }
        if self.audio.as_ref().is_some_and(|a| a.written != a.expected) {
            return Err(Box::new(Error(
                "HEVC audio duration does not match declared range".into(),
            )));
        }
        Ok(self.mux.unwrap().finish()?)
    }
}
