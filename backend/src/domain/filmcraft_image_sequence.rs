//! Original Filmcraft numbered still sequences (52318524, MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt. Pure filename and frame-clock logic.
use anyhow::{Context, Result};
use filmcraft_time::{FrameRate, Tick};

#[derive(Debug, PartialEq, Eq)]
pub struct Numbered {
    pub dir: String,
    pub prefix: String,
    pub number: u64,
    pub digits: usize,
    pub suffix: String,
}
impl Numbered {
    pub fn parse(path: &str) -> Option<Self> {
        let cut = path.rfind(['/', '\\']).map_or(0, |i| i + 1);
        let (dir, name) = path.split_at(cut);
        let extension = name.rfind('.').filter(|i| *i > 0)?;
        if !["png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp"]
            .contains(&name[extension + 1..].to_ascii_lowercase().as_str())
        {
            return None;
        }
        let stem = &name[..extension];
        let end = stem.rfind(|c: char| c.is_ascii_digit())? + 1;
        let start = stem[..end]
            .rfind(|c: char| !c.is_ascii_digit())
            .map_or(0, |i| i + 1);
        let digits = &stem[start..end];
        if digits.len() > 18 {
            return None;
        }
        Some(Self {
            dir: dir.into(),
            prefix: stem[..start].into(),
            number: digits.parse().ok()?,
            digits: digits.len(),
            suffix: name[end..].into(),
        })
    }
    pub fn number_of(&self, name: &str) -> Option<u64> {
        let middle = name
            .strip_prefix(self.prefix.as_str())?
            .strip_suffix(self.suffix.as_str())?;
        if middle.is_empty() || middle.len() > 18 || !middle.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if middle.len() != self.digits && (middle.len() < self.digits || middle.starts_with('0')) {
            return None;
        }
        middle.parse().ok()
    }
}

pub fn frames(first: &str, siblings: &[String]) -> Result<Vec<Option<String>>> {
    let first = Numbered::parse(first).context("sequence source must be a numbered still image")?;
    let mut numbers = siblings
        .iter()
        .filter_map(|name| first.number_of(name).map(|n| (n, name)))
        .filter(|(n, _)| *n >= first.number)
        .collect::<Vec<_>>();
    numbers.sort();
    numbers.dedup_by_key(|(n, _)| *n);
    let last = numbers.last().map_or(first.number, |(n, _)| *n);
    let mut frames = vec![None; (last - first.number).min(1_000_000) as usize + 1];
    for (number, name) in numbers {
        let i = (number - first.number) as usize;
        if i < frames.len() {
            frames[i] = Some(format!("{}{name}", first.dir));
        }
    }
    Ok(frames)
}

/// Original nearest-earlier / nearest-later order, with eight present candidates.
pub fn candidates(frames: &[Option<String>], rate: FrameRate, time: i64) -> Vec<usize> {
    if frames.is_empty() {
        return vec![];
    }
    let wanted = rate
        .frame_at(Tick(time.max(0)))
        .clamp(0, frames.len() as i64 - 1) as usize;
    (0..=wanted)
        .rev()
        .chain(wanted + 1..frames.len())
        .filter(|i| frames[*i].is_some())
        .take(8)
        .collect()
}

/// The sequence source uses the resolved root media rate, independently from a
/// Subclip item's default rate used for mask-tracking step duration.
pub fn rate(media: &serde_json::Value) -> Result<FrameRate> {
    let value = media
        .pointer("/interpret/frame_rate")
        .filter(|v| !v.is_null())
        .or_else(|| media.pointer("/info/video/frame_rate"));
    Ok(value
        .map(|v| serde_json::from_value::<FrameRate>(v.clone()))
        .transpose()?
        .unwrap_or_default()
        .sane())
}
