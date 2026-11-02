//! Bounded read-only INSV v3 index and selected ExtraMetadata protobuf fields.
//! Container offsets were checked on local footage; field numbers come from the
//! installed Studio's embedded extra_info.proto descriptor. Unknown blocks stay opaque.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
const MAGIC: &[u8; 32] = b"8db42d694ccc418790edff439fe026bf";
const FOOTER: u64 = 72;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub kind: u8,
    pub format: u8,
    pub length: u32,
    pub offset: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calibration {
    pub field_number: u32,
    pub value: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCropInfo {
    pub source_size: [u32; 2],
    pub destination_size: [u32; 2],
    pub offset: [i32; 2],
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub version: u32,
    pub camera_model: Option<String>,
    pub firmware: Option<String>,
    pub dimension: Option<[u32; 2]>,
    pub window_crop_info: Option<WindowCropInfo>,
    pub resolution: Option<[u32; 2]>,
    pub frame_rate: Option<u32>,
    pub first_frame_timestamp: Option<i64>,
    pub gyro_type: Option<u64>,
    pub gyro_config: Option<[u32; 2]>,
    pub gyro_timestamp: Option<f64>,
    pub has_gyro_timestamp: Option<bool>,
    pub rolling_shutter_time: Option<f64>,
    pub pts_type: Option<u64>,
    pub is_raw_gyro: Option<bool>,
    pub calibrations: Vec<Calibration>,
    pub blocks: Vec<Block>,
}
fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().expect("checked four-byte range"))
}
fn range<R: Read + Seek>(reader: &mut R, offset: u64, length: usize) -> Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(offset))?;
    let mut out = vec![0; length];
    reader.read_exact(&mut out)?;
    Ok(out)
}
pub fn inspect<R: Read + Seek>(reader: &mut R) -> Result<Option<Metadata>> {
    let size = reader.seek(SeekFrom::End(0))?;
    if size < FOOTER {
        return Ok(None);
    }
    let footer = range(reader, size - FOOTER, FOOTER as usize)?;
    if &footer[40..] != MAGIC {
        return Ok(None);
    }
    let version = u32le(&footer[36..40]);
    ensure!(
        version == 3,
        "unsupported Insta360 trailer version {version}"
    );
    let extra = u32le(&footer[32..36]) as u64;
    ensure!(
        extra >= FOOTER + 6 && extra <= size,
        "invalid Insta360 trailer extent"
    );
    let base = size - extra;
    let index_end = size - FOOTER;
    let header = range(reader, index_end - 6, 6)?;
    ensure!(
        header[0] == 0 && header[1] == 0,
        "unsupported Insta360 index format"
    );
    let length = u32le(&header[2..]) as usize;
    ensure!(
        length > 0 && length <= 2560 && length % 10 == 0,
        "invalid Insta360 index size"
    );
    let index_start = index_end
        .checked_sub(6 + length as u64)
        .context("index underflow")?;
    ensure!(index_start >= base, "index outside trailer");
    let index = range(reader, index_start, length)?;
    let mut blocks = vec![];
    let mut seen = [false; 256];
    for row in index.chunks_exact(10) {
        let length = u32le(&row[2..6]);
        if length == 0 {
            continue;
        }
        ensure!(
            row[0] != 0 && !seen[row[0] as usize],
            "invalid or duplicate Insta360 block type"
        );
        seen[row[0] as usize] = true;
        let offset = base
            .checked_add(u32le(&row[6..10]) as u64)
            .context("block overflow")?;
        let end = offset
            .checked_add(length as u64)
            .context("block overflow")?;
        ensure!(
            offset >= base && end.checked_add(6).is_some_and(|v| v <= index_start),
            "Insta360 block outside trailer"
        );
        let actual = range(reader, end, 6)?;
        ensure!(
            actual[0] == row[1] && actual[1] == row[0] && u32le(&actual[2..]) == length,
            "Insta360 index/header mismatch"
        );
        blocks.push(Block {
            kind: row[0],
            format: row[1],
            length,
            offset,
        });
    }
    let mut ordered: Vec<_> = blocks.iter().collect();
    ordered.sort_by_key(|b| b.offset);
    ensure!(
        ordered
            .windows(2)
            .all(|pair| pair[0].offset + pair[0].length as u64 + 6 <= pair[1].offset),
        "overlapping Insta360 blocks"
    );
    let mut result = Metadata {
        version,
        camera_model: None,
        firmware: None,
        dimension: None,
        window_crop_info: None,
        resolution: None,
        frame_rate: None,
        first_frame_timestamp: None,
        gyro_type: None,
        gyro_config: None,
        gyro_timestamp: None,
        has_gyro_timestamp: None,
        rolling_shutter_time: None,
        pts_type: None,
        is_raw_gyro: None,
        calibrations: vec![],
        blocks,
    };
    if let Some(block) = result.blocks.iter().find(|b| b.kind == 1) {
        ensure!(block.format == 1, "unsupported Insta360 metadata encoding");
        ensure!(
            block.length <= 256 * 1024,
            "Insta360 metadata exceeds budget"
        );
        let data = range(reader, block.offset, block.length as usize)?;
        for (number, value) in fields(&data)? {
            match (number, value) {
                (2, Field::Bytes(b)) => result.camera_model = Some(text(b, 128)?),
                (3, Field::Bytes(b)) => result.firmware = Some(text(b, 128)?),
                (19, Field::Bytes(b)) => result.dimension = vector(b)?,
                (27, Field::Bytes(b)) => result.window_crop_info = Some(window_crop(b)?),
                (44, Field::Bytes(b)) => result.resolution = vector(b)?,
                (20, Field::Varint(v)) => result.frame_rate = Some(u32::try_from(v)?),
                (24, Field::Varint(v)) => result.first_frame_timestamp = Some(v as i64),
                (51, Field::Varint(v)) => result.gyro_type = Some(v),
                (65, Field::Bytes(b)) => result.gyro_config = vector(b)?,
                (28, Field::Double(v)) => {
                    ensure!(v.is_finite(), "nonfinite gyro timestamp");
                    result.gyro_timestamp = Some(v);
                }
                (29, Field::Varint(v)) => result.has_gyro_timestamp = Some(v != 0),
                (25, Field::Double(v)) => {
                    ensure!(v.is_finite(), "nonfinite rolling shutter time");
                    result.rolling_shutter_time = Some(v);
                }
                (64, Field::Varint(v)) => result.pts_type = Some(v),
                (62, Field::Varint(v)) => result.is_raw_gyro = Some(v != 0),
                (n @ (5 | 17 | 34 | 53 | 54 | 55 | 56 | 111 | 112 | 197), Field::Bytes(b)) => {
                    result.calibrations.push(Calibration {
                        field_number: n,
                        value: text(b, 16384)?,
                    })
                }
                // Serial numbers, GPS, IP and creation-time fields are intentionally not returned.
                _ => {}
            }
        }
    }
    Ok(Some(result))
}
fn window_crop(data: &[u8]) -> Result<WindowCropInfo> {
    let mut values = [0u64; 6];
    let mut seen = [false; 6];
    for (number, value) in fields(data)? {
        if (1..=6).contains(&number) {
            let i = number as usize - 1;
            ensure!(!seen[i], "duplicate window crop field");
            let Field::Varint(v) = value else {
                anyhow::bail!("invalid window crop wire type")
            };
            values[i] = v;
            seen[i] = true;
        }
    }
    // Proto3 zero offsets may be absent. Dimensions must be explicit and bounded.
    let mut dimensions = [0u32; 4];
    for i in 0..4 {
        ensure!(
            seen[i] && (1..=65536).contains(&values[i]),
            "invalid window crop dimensions"
        );
        dimensions[i] = values[i] as u32;
    }
    let mut offset = [0i32; 2];
    for i in 0..2 {
        let v = values[4 + i];
        ensure!(
            v <= i32::MAX as u64 || v >= (i32::MIN as i64 as u64),
            "invalid signed window crop offset"
        );
        offset[i] = v as i32;
        ensure!(
            offset[i].unsigned_abs() <= 65536,
            "window crop offset exceeds budget"
        );
    }
    Ok(WindowCropInfo {
        source_size: dimensions[..2].try_into().unwrap(),
        destination_size: dimensions[2..].try_into().unwrap(),
        offset,
    })
}
fn text(b: &[u8], max: usize) -> Result<String> {
    ensure!(b.len() <= max, "Insta360 text exceeds budget");
    let s = std::str::from_utf8(b)?;
    ensure!(
        !s.chars().any(char::is_control),
        "invalid control character in Insta360 text"
    );
    Ok(s.into())
}
enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Double(f64),
    Fixed,
}
fn varint(data: &[u8], position: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let b = *data.get(*position).context("truncated protobuf varint")?;
        *position += 1;
        ensure!(shift < 63 || b <= 1, "protobuf varint overflow");
        value |= ((b & 127) as u64) << shift;
        if b < 128 {
            return Ok(value);
        }
    }
    anyhow::bail!("unterminated protobuf varint")
}
fn fields(data: &[u8]) -> Result<Vec<(u32, Field<'_>)>> {
    let mut out = vec![];
    let mut p = 0;
    while p < data.len() {
        ensure!(out.len() < 4096, "too many metadata fields");
        let key = varint(data, &mut p)?;
        let n = u32::try_from(key >> 3)?;
        ensure!(n > 0 && n < 1 << 29, "invalid protobuf field");
        let field = match key & 7 {
            0 => Field::Varint(varint(data, &mut p)?),
            2 => {
                let size = usize::try_from(varint(data, &mut p)?)?;
                let end = p.checked_add(size).context("protobuf length overflow")?;
                let bytes = data.get(p..end).context("truncated protobuf bytes")?;
                p = end;
                Field::Bytes(bytes)
            }
            1 => {
                let end = p.checked_add(8).context("protobuf range overflow")?;
                let bytes = data.get(p..end).context("truncated protobuf double")?;
                p = end;
                let v = f64::from_le_bytes(bytes.try_into()?);
                Field::Double(v)
            }
            5 => {
                let size = 4;
                p = p.checked_add(size).context("protobuf range overflow")?;
                ensure!(p <= data.len(), "truncated protobuf fixed field");
                Field::Fixed
            }
            _ => anyhow::bail!("unsupported protobuf wire type"),
        };
        out.push((n, field));
    }
    Ok(out)
}
fn vector(data: &[u8]) -> Result<Option<[u32; 2]>> {
    let mut v = [None, None];
    for (n, f) in fields(data)? {
        if let (1 | 2, Field::Varint(x)) = (n, f) {
            v[(n - 1) as usize] = Some(u32::try_from(x)?);
        }
    }
    Ok(v[0].zip(v[1]).map(|(x, y)| [x, y]))
}
