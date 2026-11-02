//! Qualified subset of native block128 TimeMapRawData: one constant-rate trim.
//! Native header is24 bytes, followed by32-byte ranges and16-byte time pairs.
//! The opaque header word is never treated as a pointer. Retiming is fail-closed.
use super::{
    insta360_metadata::Metadata,
    insta360_video_clock::{self, VideoClock},
};
use anyhow::{ensure, Context, Result};
use std::io::{Read, Seek, SeekFrom};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineTrim {
    pub source_start_seconds: f64,
    pub source_frame_offset: usize,
    pub clip_frames: u64,
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().expect("checked u32 range"))
}
fn double(b: &[u8]) -> f64 {
    f64::from_le_bytes(b.try_into().expect("checked f64 range"))
}
/// Native supports general maps. This implementation intentionally qualifies
/// only complete, uniform, one-range, unit-slope translations on the source CFR.
pub fn parse(data: &[u8], video: VideoClock) -> Result<AffineTrim> {
    ensure!(
        data.len() >= 56 && data.len() <= 3_200_056,
        "unsupported time-map size"
    );
    let range_bytes = u32le(&data[..4]) as usize;
    let ranges = u32le(&data[4..8]);
    let item_bytes = u32le(&data[8..12]) as usize;
    let items = u32le(&data[12..16]) as usize;
    ensure!(
        ranges == 1
            && range_bytes == 32
            && items >= 2
            && items <= 200_000
            && item_bytes == items * 16
            && data.len() == 24 + range_bytes + item_bytes,
        "unsupported time-map header/range layout"
    );
    ensure!(
        video.numerator > 0 && video.denominator > 0 && video.sample_count == items as u64,
        "time-map/video frame count mismatch"
    );
    let step = 1000. * video.denominator as f64 / video.numerator as f64;
    let range: [f64; 4] = std::array::from_fn(|i| double(&data[24 + i * 8..32 + i * 8]));
    ensure!(
        range.iter().all(|v| v.is_finite()) && range[0].abs() <= 1e-7 && range[2] >= 0.,
        "unsupported time-map range"
    );
    let end = (items - 1) as f64 * step;
    let offset = range[2];
    ensure!(
        (range[1] - end).abs() <= 1e-5 && (range[3] - offset - end).abs() <= 1e-5,
        "unsupported time-map range endpoints"
    );
    let frame_offset = offset / step;
    ensure!(
        frame_offset.is_finite()
            && frame_offset >= 0.
            && frame_offset <= usize::MAX as f64 / 2.
            && (frame_offset - frame_offset.round()).abs() <= 1e-7,
        "trim offset is not on the original CFR frame clock"
    );
    for (i, row) in data[56..].chunks_exact(16).enumerate() {
        let clip = double(&row[..8]);
        let source = double(&row[8..]);
        let expected = i as f64 * step;
        ensure!(
            clip.is_finite()
                && source.is_finite()
                && (clip - expected).abs() <= 1e-5
                && (source - clip - offset).abs() <= 1e-5,
            "non-affine/nonuniform time-map requires explicit original frame PTS synchronization"
        );
    }
    Ok(AffineTrim {
        source_start_seconds: offset / 1000.,
        source_frame_offset: frame_offset.round() as usize,
        clip_frames: video.sample_count,
    })
}
pub fn inspect<R: Read + Seek>(reader: &mut R, metadata: &Metadata) -> Result<Option<AffineTrim>> {
    let Some(block) = metadata.blocks.iter().find(|b| b.kind == 128) else {
        return Ok(None);
    };
    ensure!(
        block.format == 0 && block.length >= 56 && block.length <= 3_200_056,
        "unsupported embedded time-map encoding/size"
    );
    let video = insta360_video_clock::inspect(reader)?
        .context("embedded time map requires a qualified presentation clock")?;
    reader.seek(SeekFrom::Start(block.offset))?;
    let mut data = vec![0; block.length as usize];
    reader.read_exact(&mut data)?;
    parse(&data, video).map(Some)
}
