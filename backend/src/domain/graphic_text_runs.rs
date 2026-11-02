//! Filmcraft character-offset style runs to UTF-8 byte ranges (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::graphic_text_layout::GraphicCharStyle;
use serde::{Deserialize, Serialize};
use std::ops::Range;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterStyleRun {
    pub start: usize,
    pub end: usize,
    pub style: GraphicCharStyle,
}
/// Preserve original clamping and drop empty ranges and empty styles.
pub fn byte_runs(text: &str, runs: &[CharacterStyleRun]) -> Vec<(Range<usize>, GraphicCharStyle)> {
    if runs.is_empty() {
        return vec![];
    }
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(text.len()))
        .collect();
    let at = |c: usize| offsets[c.min(offsets.len() - 1)];
    runs.iter()
        .map(|r| (at(r.start)..at(r.end), r.style.clone()))
        .filter(|(r, s)| r.start < r.end && *s != GraphicCharStyle::default())
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn character_ranges_clamp_without_splitting_unicode() {
        let style = GraphicCharStyle {
            underline: Some(true),
            ..Default::default()
        };
        let runs = vec![
            CharacterStyleRun {
                start: 1,
                end: 3,
                style: style.clone(),
            },
            CharacterStyleRun {
                start: 3,
                end: usize::MAX,
                style: style.clone(),
            },
            CharacterStyleRun {
                start: 0,
                end: 1,
                style: GraphicCharStyle::default(),
            },
            CharacterStyleRun {
                start: 99,
                end: 100,
                style: style.clone(),
            },
            CharacterStyleRun {
                start: 3,
                end: 2,
                style: style.clone(),
            },
        ];
        assert_eq!(
            byte_runs("AБ🙂Z", &runs),
            vec![(1..7, style.clone()), (7..8, style)]
        );
        assert!(byte_runs("", &runs).is_empty());
    }
}
