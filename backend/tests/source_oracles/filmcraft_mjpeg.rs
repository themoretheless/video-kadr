//! Pinned Filmcraft export/lib.rs MjpegEncoder, MIT.
//! Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors.
//! Source 5231852443363f001c3f6b396dd9b1e6461ae2be; license docs/licenses/filmcraft-MIT.txt.
//! The encoder body is unchanged; only standalone test trait/error glue follows.
#![allow(dead_code)]
use anyhow::Result;
use filmcraft_isobmff::SampleEntry;
use filmcraft_time::FrameRate;
struct EncoderFrame {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}
struct EncodedPacket {
    data: Vec<u8>,
    key: bool,
    duration: u32,
    composition_offset: i32,
}
#[derive(Debug)]
enum ExportError {
    Encode(String),
}
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ExportError {}
trait VideoEncoder {
    fn sample_entry(&self) -> SampleEntry;
    fn timescale(&self) -> u32;
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>>;
    fn flush(&mut self) -> Result<Vec<EncodedPacket>>;
}

struct MjpegEncoder {
    w: u16,
    h: u16,
    quality: u8,
    rate: FrameRate,
}

impl VideoEncoder for MjpegEncoder {
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::jpeg(self.w, self.h)
    }
    fn timescale(&self) -> u32 {
        self.rate.num as u32
    }
    fn encode(&mut self, f: &EncoderFrame) -> Result<Vec<EncodedPacket>> {
        let rgb: Vec<u8> = f
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, self.quality);
        enc.encode(&rgb, f.width, f.height, image::ExtendedColorType::Rgb8)
            .map_err(|e| ExportError::Encode(e.to_string()))?;
        Ok(vec![EncodedPacket {
            data: out,
            key: true,
            duration: self.rate.den as u32,
            composition_offset: 0,
        }])
    }
    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
}

pub fn packet(rgba: Vec<u8>, width: u32, height: u32, quality: u8, rate: FrameRate) -> Vec<u8> {
    // The source factory clamps quality before constructing this encoder.
    let mut encoder = MjpegEncoder {
        w: width as u16,
        h: height as u16,
        quality: quality.clamp(1, 100),
        rate,
    };
    let packets = encoder
        .encode(&EncoderFrame {
            rgba,
            width,
            height,
        })
        .unwrap();
    assert_eq!(packets.len(), 1);
    assert!(packets[0].key);
    assert_eq!(packets[0].duration, rate.den as u32);
    assert_eq!(packets[0].composition_offset, 0);
    assert!(encoder.flush().unwrap().is_empty());
    packets.into_iter().next().unwrap().data
}
