//! Add nclc to our unfragmented, moov-at-end QTRLE output without moving media bytes.
use anyhow::{ensure, Context, Result};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};
fn code(value: Option<&str>, transfer: bool) -> Result<Option<u16>> {
    let Some(value) = value else { return Ok(None) };
    let n = match (value, transfer) {
        ("unknown" | "unspecified" | "reserved", _) => return Ok(None),
        ("bt709", _) => 1,
        ("bt470m", false) | ("gamma22", true) => 4,
        ("bt470bg", false) | ("gamma28", true) => 5,
        ("smpte170m", _) => 6,
        ("smpte240m", _) => 7,
        ("film", false) | ("linear", true) => 8,
        ("bt2020", false) | ("log", true) => 9,
        ("smpte428", false) | ("log_sqrt", true) => 10,
        ("smpte431", false) | ("iec61966-2-4", true) => 11,
        ("smpte432", false) | ("bt1361e", true) => 12,
        ("iec61966-2-1", true) => 13,
        ("bt2020-10", true) => 14,
        ("bt2020-12", true) => 15,
        ("smpte2084", true) => 16,
        ("smpte428", true) => 17,
        ("arib-std-b67", true) => 18,
        ("jedec-p22", false) => 22,
        _ => anyhow::bail!("unknown source color code {value}"),
    };
    Ok(Some(n))
}
fn size(bytes: &[u8]) -> Result<usize> {
    ensure!(bytes.len() >= 8, "short MOV atom");
    let n = u32::from_be_bytes(bytes[..4].try_into()?) as usize;
    ensure!(n >= 8 && n <= bytes.len(), "invalid bounded MOV atom");
    Ok(n)
}
fn rewrite(bytes: &mut Vec<u8>, color: &[u8], depth: usize) -> Result<usize> {
    ensure!(
        depth <= 8 && bytes.len() <= 16 * 1024 * 1024,
        "MOV metadata budget"
    );
    let tag: [u8; 4] = bytes[4..8].try_into()?;
    let mut changed = 0;
    let start = if matches!(&tag, b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl") {
        8
    } else if tag == *b"stsd" {
        ensure!(bytes.len() >= 16, "short stsd");
        16
    } else {
        return Ok(0);
    };
    let mut at = start;
    while at < bytes.len() {
        let n = size(&bytes[at..])?;
        let kind: [u8; 4] = bytes[at + 4..at + 8].try_into()?;
        let mut child = bytes[at..at + n].to_vec();
        let edits = if tag == *b"stsd" && kind == *b"rle " {
            ensure!(child.len() >= 86, "short QTRLE sample entry");
            let mut next = 86;
            let mut found = None;
            while next < child.len() {
                let length = size(&child[next..])?;
                if &child[next + 4..next + 8] == b"colr" {
                    ensure!(found.is_none(), "duplicate color atom");
                    found = Some((next, length));
                }
                next += length;
            }
            if let Some((offset, length)) = found {
                child.splice(offset..offset + length, color.iter().copied());
            } else {
                child.extend_from_slice(color);
            }
            1
        } else {
            rewrite(&mut child, color, depth + 1)?
        };
        if edits > 0 {
            let length: u32 = child.len().try_into()?;
            child[..4].copy_from_slice(&length.to_be_bytes());
            bytes.splice(at..at + n, child.iter().copied());
            changed += edits;
        }
        at += child.len();
    }
    let length: u32 = bytes.len().try_into()?;
    bytes[..4].copy_from_slice(&length.to_be_bytes());
    Ok(changed)
}
pub(super) fn preserve(path: &Path, primaries: Option<&str>, transfer: Option<&str>) -> Result<()> {
    let (p, t) = (code(primaries, false)?, code(transfer, true)?);
    if p.is_none() && t.is_none() {
        return Ok(());
    }
    let mut color = Vec::new();
    color.extend_from_slice(&18u32.to_be_bytes());
    color.extend_from_slice(b"colrnclc");
    color.extend_from_slice(&p.unwrap_or(2).to_be_bytes());
    color.extend_from_slice(&t.unwrap_or(2).to_be_bytes());
    color.extend_from_slice(&0u16.to_be_bytes());
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let length = file.metadata()?.len();
    let mut at = 0u64;
    let mut found = None;
    while at < length {
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header)?;
        let n = u32::from_be_bytes(header[..4].try_into()?);
        let size = if n == 1 {
            let mut value = [0u8; 8];
            file.read_exact(&mut value)?;
            u64::from_be_bytes(value)
        } else if n == 0 {
            length - at
        } else {
            u64::from(n)
        };
        ensure!(size >= 8 && size <= length - at, "invalid MOV root size");
        if &header[4..] == b"moov" {
            ensure!(
                found.is_none() && at + size == length && size <= 16 * 1024 * 1024 && n > 1,
                "requires bounded final moov"
            );
            found = Some((at, size));
        }
        at += size;
    }
    let (offset, size) = found.context("MOV moov missing")?;
    file.seek(SeekFrom::Start(offset))?;
    let mut metadata = vec![0u8; size as usize];
    file.read_exact(&mut metadata)?;
    ensure!(
        rewrite(&mut metadata, &color, 0)? == 1,
        "expected one QTRLE sample entry"
    );
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&metadata)?;
    file.set_len(offset + metadata.len() as u64)?;
    file.sync_all()?;
    Ok(())
}
