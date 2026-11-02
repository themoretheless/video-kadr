//! Original Filmcraft character formatting editing operations (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{graphic_text_layout::GraphicCharStyle, graphic_text_runs::CharacterStyleRun};
impl GraphicCharStyle {
    pub fn is_empty(&self) -> bool {
        *self == GraphicCharStyle::default()
    }
    /// `self` with every field `over` sets replaced.
    pub fn merged(&self, over: &GraphicCharStyle) -> GraphicCharStyle {
        GraphicCharStyle {
            font: over.font.clone().or_else(|| self.font.clone()),
            font_style: over.font_style.clone().or_else(|| self.font_style.clone()),
            size: over.size.or(self.size),
            fill: over.fill.or(self.fill),
            faux_bold: over.faux_bold.or(self.faux_bold),
            faux_italic: over.faux_italic.or(self.faux_italic),
            underline: over.underline.or(self.underline),
            tracking: over.tracking.or(self.tracking),
            baseline_shift: over.baseline_shift.or(self.baseline_shift),
            caps: over.caps.or(self.caps),
        }
    }
}

/// Per-character styles of a text of `n` characters.
fn expand(runs: &[CharacterStyleRun], n: usize) -> Vec<GraphicCharStyle> {
    let mut v = vec![GraphicCharStyle::default(); n];
    for r in runs {
        for c in v.iter_mut().take(r.end.min(n)).skip(r.start.min(n)) {
            *c = c.merged(&r.style);
        }
    }
    v
}

/// Runs from per-character styles (adjacent equal styles merged, unstyled characters omitted).
fn compress(v: &[GraphicCharStyle]) -> Vec<CharacterStyleRun> {
    let mut out: Vec<CharacterStyleRun> = Vec::new();
    for (i, s) in v.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(r) if r.end == i && r.style == *s => r.end = i + 1,
            _ => out.push(CharacterStyleRun {
                start: i,
                end: i + 1,
                style: s.clone(),
            }),
        }
    }
    out
}

/// Normalise runs for a text of `n` characters: clamped, sorted, non-overlapping, merged.
pub fn normalize_runs(runs: &[CharacterStyleRun], n: usize) -> Vec<CharacterStyleRun> {
    compress(&expand(runs, n))
}

/// Merge `style` into characters `start..end` of a text of `n` characters.
pub fn apply_char_style(
    runs: &[CharacterStyleRun],
    n: usize,
    start: usize,
    end: usize,
    style: &GraphicCharStyle,
) -> Vec<CharacterStyleRun> {
    let mut v = expand(runs, n);
    for c in v.iter_mut().take(end.min(n)).skip(start.min(n)) {
        *c = c.merged(style);
    }
    compress(&v)
}

/// Remove every override from characters `start..end`.
pub fn clear_char_style(
    runs: &[CharacterStyleRun],
    n: usize,
    start: usize,
    end: usize,
) -> Vec<CharacterStyleRun> {
    let mut v = expand(runs, n);
    for c in v.iter_mut().take(end.min(n)).skip(start.min(n)) {
        *c = GraphicCharStyle::default();
    }
    compress(&v)
}

/// Keep runs on their characters when `old` text becomes `new` (one contiguous replacement:
/// common prefix and suffix are kept; inserted characters take the style of the character before
/// the insertion point, or after it at the very start).
pub fn adjust_runs(old: &str, new: &str, runs: &[CharacterStyleRun]) -> Vec<CharacterStyleRun> {
    if runs.is_empty() || old == new {
        return runs.to_vec();
    }
    let a: Vec<char> = old.chars().collect();
    let b: Vec<char> = new.chars().collect();
    let pre = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suf = a.len().min(b.len()) - pre;
    let suf = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suf)
        .take_while(|(x, y)| x == y)
        .count();
    let v = expand(runs, a.len());
    let inserted = b.len() - pre - suf;
    let fill = if pre > 0 {
        v[pre - 1].clone()
    } else {
        v.get(a.len() - suf).cloned().unwrap_or_default()
    };
    let mut out: Vec<GraphicCharStyle> = v[..pre].to_vec();
    out.extend(std::iter::repeat_n(fill, inserted));
    out.extend_from_slice(&v[a.len() - suf..]);
    compress(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_merges_fields_and_clearing_splits_ranges() {
        let red = GraphicCharStyle {
            fill: Some([1., 0., 0., 1.]),
            ..Default::default()
        };
        let underline = GraphicCharStyle {
            underline: Some(true),
            ..Default::default()
        };
        let a = apply_char_style(&[], 5, 0, 4, &red);
        let b = apply_char_style(&a, 5, 2, 5, &underline);
        assert_eq!(
            b,
            vec![
                CharacterStyleRun {
                    start: 0,
                    end: 2,
                    style: red.clone()
                },
                CharacterStyleRun {
                    start: 2,
                    end: 4,
                    style: red.merged(&underline)
                },
                CharacterStyleRun {
                    start: 4,
                    end: 5,
                    style: underline
                }
            ]
        );
        let c = clear_char_style(&b, 5, 1, 4);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].start, c[0].end), (0, 1));
        assert_eq!((c[1].start, c[1].end), (4, 5));
    }
    #[test]
    fn unicode_insertions_inherit_adjacent_style_and_keep_suffix() {
        let style = GraphicCharStyle {
            faux_bold: Some(true),
            ..Default::default()
        };
        let runs = vec![CharacterStyleRun {
            start: 0,
            end: 2,
            style: style.clone(),
        }];
        assert_eq!(
            adjust_runs("AБC", "A🙂БC", &runs),
            vec![CharacterStyleRun {
                start: 0,
                end: 3,
                style: style.clone()
            }]
        );
        assert_eq!(
            adjust_runs("AБC", "🙂AБC", &runs),
            vec![CharacterStyleRun {
                start: 0,
                end: 3,
                style
            }]
        );
        assert!(adjust_runs("AБC", "", &runs).is_empty());
    }
}
