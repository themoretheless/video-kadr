//! Read-only MP4 video clock qualification for embedded IMU synchronization.
//! An average frame-rate label does not establish CFR: every stts run must have
//! the same positive sample delta. No video payload or per-frame PTS is decoded.
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoClock {
    pub numerator: u32,
    pub denominator: u32,
    pub sample_count: u64,
}
#[derive(Clone, Copy)]
struct Atom {
    kind: [u8; 4],
    payload: u64,
    end: u64,
}
fn read<R: Read + Seek>(r: &mut R, at: u64, len: usize) -> Result<Vec<u8>> {
    r.seek(SeekFrom::Start(at))?;
    let mut b = vec![0; len];
    r.read_exact(&mut b)?;
    Ok(b)
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes(b.try_into().expect("checked four-byte range"))
}
fn atoms<R: Read + Seek>(r: &mut R, start: u64, end: u64) -> Result<Vec<Atom>> {
    let mut out = vec![];
    let mut at = start;
    while at < end {
        ensure!(
            out.len() < 4096 && end - at >= 8,
            "invalid or excessive MP4 atoms"
        );
        let h = read(r, at, 8)?;
        let mut size = be32(&h[..4]) as u64;
        let mut header = 8;
        if size == 1 {
            size = u64::from_be_bytes(read(r, at + 8, 8)?.try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = end - at;
        }
        let next = at.checked_add(size).context("MP4 atom overflow")?;
        ensure!(size >= header && next <= end, "MP4 atom outside container");
        out.push(Atom {
            kind: h[4..8].try_into().unwrap(),
            payload: at + header,
            end: next,
        });
        at = next;
    }
    Ok(out)
}
fn child<R: Read + Seek>(r: &mut R, parent: Atom, kind: &[u8; 4]) -> Result<Option<Atom>> {
    let matching: Vec<_> = atoms(r, parent.payload, parent.end)?
        .into_iter()
        .filter(|a| &a.kind == kind)
        .collect();
    ensure!(matching.len() <= 1, "duplicate MP4 clock atom");
    Ok(matching.first().copied())
}
fn required<R: Read + Seek>(r: &mut R, parent: Atom, kind: &[u8; 4]) -> Result<Atom> {
    child(r, parent, kind)?.context("missing MP4 video clock atom")
}
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}
/// Returns None for a non-MP4 stream. A recognized movie with a variable video
/// decode clock returns an error instead of silently using its nominal fps.
pub fn inspect<R: Read + Seek>(r: &mut R) -> Result<Option<VideoClock>> {
    let size = r.seek(SeekFrom::End(0))?;
    if size < 8 {
        return Ok(None);
    }
    let first = read(r, 0, 8)?;
    if &first[4..8] != b"ftyp" {
        return Ok(None);
    }
    let (mut at, mut count, mut moov) = (0, 0, None);
    while at + 8 <= size {
        count += 1;
        ensure!(count <= 4096, "excessive top-level MP4 atoms");
        let h = read(r, at, 8)?;
        let mut n = be32(&h[..4]) as u64;
        let mut header = 8;
        if n == 1 {
            n = u64::from_be_bytes(read(r, at + 8, 8)?.try_into().unwrap());
            header = 16;
        } else if n == 0 {
            n = size - at;
        }
        let end = at.checked_add(n).context("MP4 atom overflow")?;
        ensure!(n >= header && end <= size, "invalid MP4 atom extent");
        if &h[4..8] == b"moov" {
            moov = Some(Atom {
                kind: *b"moov",
                payload: at + header,
                end,
            });
            break;
        }
        at = end;
    }
    let Some(movie) = moov else {
        anyhow::bail!("missing MP4 movie clock")
    };
    let mut clocks = vec![];
    for track in atoms(r, movie.payload, movie.end)?
        .into_iter()
        .filter(|a| a.kind == *b"trak")
    {
        let media = required(r, track, b"mdia")?;
        let handler = required(r, media, b"hdlr")?;
        ensure!(handler.end - handler.payload >= 12, "truncated MP4 handler");
        if read(r, handler.payload + 8, 4)? != b"vide" {
            continue;
        }
        if let Some(edts) = child(r, track, b"edts")? {
            let elst = required(r, edts, b"elst")?;
            ensure!(elst.end - elst.payload >= 8, "truncated MP4 edit list");
            let h = read(r, elst.payload, 8)?;
            let version = h[0];
            ensure!(
                be32(&h[4..]) == 1,
                "multiple video edits require explicit PTS synchronization"
            );
            let row = match version {
                0 => 12,
                1 => 20,
                _ => anyhow::bail!("unsupported MP4 edit list version"),
            };
            ensure!(
                elst.end - elst.payload == 8 + row,
                "invalid MP4 edit list size"
            );
            let data = read(r, elst.payload + 8, row as usize)?;
            let (media, rate) = if version == 0 {
                (&data[4..8], &data[8..12])
            } else {
                (&data[8..16], &data[16..20])
            };
            ensure!(
                media.iter().all(|v| *v == 0) && rate == [0, 1, 0, 0],
                "nontrivial video edit requires explicit PTS synchronization"
            );
        }
        let mdhd = required(r, media, b"mdhd")?;
        ensure!(mdhd.end - mdhd.payload >= 16, "truncated MP4 media header");
        let version = read(r, mdhd.payload, 1)?[0];
        let timescale_offset = match version {
            0 => 12,
            1 => 20,
            _ => anyhow::bail!("unsupported MP4 media header version"),
        };
        ensure!(
            mdhd.end - mdhd.payload >= timescale_offset + 4,
            "truncated MP4 media timescale"
        );
        let scale = be32(&read(r, mdhd.payload + timescale_offset, 4)?);
        ensure!(scale > 0, "invalid MP4 timescale");
        let minf = required(r, media, b"minf")?;
        let stbl = required(r, minf, b"stbl")?;
        let stts = required(r, stbl, b"stts")?;
        ensure!(
            stts.end - stts.payload >= 8,
            "truncated MP4 sample time table"
        );
        let h = read(r, stts.payload, 8)?;
        ensure!(
            h[..4] == [0; 4],
            "unsupported MP4 sample time table version"
        );
        let runs = be32(&h[4..]) as usize;
        ensure!(
            runs > 0 && runs <= 65536 && stts.end - stts.payload == 8 + runs as u64 * 8,
            "invalid MP4 sample time table size"
        );
        let table = read(r, stts.payload + 8, runs * 8)?;
        let mut delta = None;
        let mut samples = 0u64;
        for row in table.chunks_exact(8) {
            let n = be32(&row[..4]);
            let d = be32(&row[4..]);
            ensure!(n > 0 && d > 0, "invalid MP4 sample timing");
            ensure!(
                delta.is_none_or(|previous| d == previous),
                "variable video clock requires explicit frame PTS synchronization"
            );
            delta = Some(d);
            samples = samples
                .checked_add(n as u64)
                .context("MP4 sample count overflow")?;
        }
        // Composition-time offsets can make presentation timestamps differ from the
        // constant decode clock. B-frames with constant offsets are safe; varying
        // offsets require a proper PTS list, which this bounded clock path does not infer.
        if let Some(ctts) = child(r, stbl, b"ctts")? {
            ensure!(
                ctts.end - ctts.payload >= 8,
                "truncated MP4 composition time table"
            );
            let h = read(r, ctts.payload, 8)?;
            ensure!(
                h[0] <= 1 && h[1..4] == [0; 3],
                "unsupported MP4 composition time version"
            );
            let runs = be32(&h[4..]) as usize;
            ensure!(
                runs > 0 && runs <= 65536 && ctts.end - ctts.payload == 8 + runs as u64 * 8,
                "invalid MP4 composition time table"
            );
            let table = read(r, ctts.payload + 8, runs * 8)?;
            let mut composition_samples = 0u64;
            for row in table.chunks_exact(8) {
                let n = be32(&row[..4]);
                ensure!(n > 0, "invalid MP4 composition time run");
                composition_samples = composition_samples
                    .checked_add(n as u64)
                    .context("MP4 composition sample count overflow")?;
            }
            ensure!(
                composition_samples == samples,
                "composition timestamps do not cover video samples"
            );
            let offset = &table[4..8];
            ensure!(
                table.chunks_exact(8).all(|row| &row[4..8] == offset),
                "reordered video PTS requires explicit presentation timing synchronization"
            );
        }
        let delta = delta.unwrap();
        let g = gcd(scale, delta);
        clocks.push(VideoClock {
            numerator: scale / g,
            denominator: delta / g,
            sample_count: samples,
        });
    }
    let first = *clocks.first().context("no MP4 video clock")?;
    ensure!(
        clocks.iter().all(|c| c.numerator == first.numerator
            && c.denominator == first.denominator
            && c.sample_count == first.sample_count),
        "video tracks have different capture clocks"
    );
    Ok(Some(first))
}
