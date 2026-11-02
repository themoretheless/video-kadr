//! Native rich graphic text layout from Filmcraft (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use filmcraft_text::{layout_rich, Align, Caps, Layout, ParagraphStyle, StyleRun, TextStyle};
use serde::{Deserialize, Serialize};
use std::{ops::Range, sync::Arc};
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicCharStyle {
    pub font: Option<String>,
    pub font_style: Option<String>,
    pub size: Option<f32>,
    pub fill: Option<[f32; 4]>,
    pub faux_bold: Option<bool>,
    pub faux_italic: Option<bool>,
    pub underline: Option<bool>,
    pub tracking: Option<f32>,
    pub baseline_shift: Option<f32>,
    pub caps: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicText {
    pub text: String,
    pub font: String,
    pub style: String,
    pub size: f32,
    pub align: u32,
    pub tracking: f32,
    pub kerning: bool,
    pub ligatures: bool,
    pub leading: f32,
    pub baseline_shift: f32,
    pub faux_bold: bool,
    pub faux_italic: bool,
    pub caps: u32,
    pub underline: bool,
    pub box_width: f32,
    pub box_height: f32,
    pub vertical: bool,
    pub runs: Vec<(Range<usize>, GraphicCharStyle)>,
}
impl Default for GraphicText {
    fn default() -> Self {
        Self {
            text: String::new(),
            font: "Inter".into(),
            style: "Regular".into(),
            size: 100.,
            align: 0,
            tracking: 0.,
            kerning: true,
            ligatures: true,
            leading: 0.,
            baseline_shift: 0.,
            faux_bold: false,
            faux_italic: false,
            caps: 0,
            underline: false,
            box_width: 0.,
            box_height: 0.,
            vertical: false,
            runs: Vec::new(),
        }
    }
}
impl GraphicText {
    /// Validate transport data before shaping. Ranges use UTF-8 byte offsets.
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::ensure;
        ensure!(
            self.align <= 3 && self.caps <= 2,
            "unknown graphic text alignment or caps"
        );
        ensure!(
            [
                self.size,
                self.tracking,
                self.leading,
                self.baseline_shift,
                self.box_width,
                self.box_height
            ]
            .iter()
            .all(|v| v.is_finite()),
            "non-finite graphic text geometry"
        );
        ensure!(self.size > 0., "graphic text size must be positive");
        ensure!(
            self.runs.len() <= u16::MAX as usize,
            "too many graphic text runs"
        );
        let mut previous_end = 0;
        for (range, style) in &self.runs {
            ensure!(
                range.start <= range.end
                    && range.start >= previous_end
                    && range.end <= self.text.len(),
                "overlapping or out-of-range graphic text run"
            );
            ensure!(
                self.text.is_char_boundary(range.start) && self.text.is_char_boundary(range.end),
                "graphic text run splits UTF-8 character"
            );
            previous_end = range.end;
            ensure!(
                [style.size, style.tracking, style.baseline_shift]
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite()),
                "non-finite graphic character style"
            );
            ensure!(
                style.caps.is_none_or(|v| v <= 2),
                "unknown graphic character caps"
            );
            ensure!(
                style
                    .fill
                    .is_none_or(|c| c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v))),
                "invalid graphic character fill"
            );
        }
        Ok(())
    }
}

pub fn text_styles(t: &GraphicText) -> (TextStyle, ParagraphStyle) {
    (
        TextStyle {
            family: if t.font.is_empty() {
                filmcraft_text::fonts::DEFAULT_FAMILY.into()
            } else {
                t.font.clone()
            },
            style: if t.style.is_empty() {
                "Regular".into()
            } else {
                t.style.clone()
            },
            size: t.size,
            tracking: t.tracking,
            kerning: t.kerning,
            ligatures: t.ligatures,
            baseline_shift: t.baseline_shift,
            faux_bold: t.faux_bold,
            faux_italic: t.faux_italic,
            caps: caps_of(t.caps),
            underline: t.underline,
        },
        ParagraphStyle {
            align: match t.align {
                1 => Align::Center,
                2 => Align::Right,
                3 => Align::Justify,
                _ => Align::Left,
            },
            leading: t.leading,
            width: (t.box_width > 0.0 && !t.vertical).then_some(t.box_width),
            height: (t.box_width > 0.0 && t.box_height > 0.0 && !t.vertical)
                .then_some(t.box_height),
            rtl: None,
            vertical: t.vertical,
        },
    )
}

fn caps_of(c: u32) -> Caps {
    match c {
        1 => Caps::All,
        2 => Caps::Small,
        _ => Caps::Normal,
    }
}

/// The per-character style runs of a text layer as text-engine runs (overrides applied to the
/// layer's style).
pub fn text_runs(t: &GraphicText, base: &TextStyle) -> Vec<StyleRun> {
    t.runs
        .iter()
        .map(|(r, c)| StyleRun {
            range: r.clone(),
            style: TextStyle {
                family: c
                    .font
                    .clone()
                    .filter(|f| !f.is_empty())
                    .unwrap_or_else(|| base.family.clone()),
                style: c
                    .font_style
                    .clone()
                    .filter(|f| !f.is_empty())
                    .unwrap_or_else(|| base.style.clone()),
                size: c.size.map_or(base.size, |s| s.max(0.1)),
                tracking: c.tracking.unwrap_or(base.tracking),
                baseline_shift: c.baseline_shift.unwrap_or(base.baseline_shift),
                faux_bold: c.faux_bold.unwrap_or(base.faux_bold),
                faux_italic: c.faux_italic.unwrap_or(base.faux_italic),
                caps: c.caps.map_or(base.caps, caps_of),
                underline: c.underline.unwrap_or(base.underline),
                ..base.clone()
            },
        })
        .collect()
}

/// The laid-out text of a text layer (cached by the text engine).
pub fn text_layout(t: &GraphicText) -> Arc<Layout> {
    let (st, ps) = text_styles(t);
    let runs = text_runs(t, &st);
    layout_rich(&t.text, &st, &runs, &ps)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rich_runs_preserve_base_and_apply_character_overrides() {
        let mut t = GraphicText {
            text: "AБ office".into(),
            size: 24.,
            tracking: 2.,
            underline: true,
            ..Default::default()
        };
        t.runs.push((
            1..3,
            GraphicCharStyle {
                size: Some(-2.),
                font: Some(String::new()),
                tracking: Some(5.),
                underline: Some(false),
                caps: Some(2),
                fill: Some([1., 0., 0., 1.]),
                ..Default::default()
            },
        ));
        let (base, _) = text_styles(&t);
        let runs = text_runs(&t, &base);
        assert_eq!(runs[0].range, 1..3);
        assert_eq!(runs[0].style.family, base.family);
        assert_eq!(runs[0].style.size, 0.1);
        assert_eq!(runs[0].style.tracking, 5.);
        assert!(!runs[0].style.underline);
        assert_eq!(runs[0].style.caps, Caps::Small);
        assert_eq!(runs[0].style.kerning, base.kerning);
        assert_eq!(runs[0].style.ligatures, base.ligatures);
        let round: GraphicText = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(round, t);
        let layout = text_layout(&t);
        assert_eq!(layout.text_len, t.text.len());
        assert!(!layout.glyphs.is_empty());
    }
    #[test]
    fn vertical_text_disables_paragraph_box_and_keeps_multilingual_layout() {
        let mut t = GraphicText {
            text: "Hello Привет مرحبا".into(),
            size: 20.,
            box_width: 80.,
            box_height: 100.,
            align: 3,
            ..Default::default()
        };
        let (_, p) = text_styles(&t);
        assert_eq!(p.width, Some(80.));
        assert_eq!(p.height, Some(100.));
        t.vertical = true;
        let (_, p) = text_styles(&t);
        assert_eq!(p.width, None);
        assert_eq!(p.height, None);
        let l = text_layout(&t);
        assert!(l.vertical);
        assert!(!l.glyphs.is_empty());
        assert!(l.bounds.iter().all(|v| v.is_finite()));
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    #[test]
    fn refuses_split_utf8_overlapping_runs_and_nonfinite_styles() {
        let mut t = GraphicText {
            text: "AБC".into(),
            runs: vec![(1..3, GraphicCharStyle::default())],
            ..Default::default()
        };
        assert!(t.validate().is_ok());
        t.runs[0].0 = 1..2;
        assert!(t.validate().is_err());
        t.runs[0].0 = 1..3;
        t.runs.push((1..3, GraphicCharStyle::default()));
        assert!(t.validate().is_err());
        t.runs.pop();
        t.runs[0].1.tracking = Some(f32::NAN);
        assert!(t.validate().is_err());
        t.runs[0].1.tracking = None;
        t.runs[0].1.fill = Some([1., 0., 0., 1.1]);
        assert!(t.validate().is_err());
    }
}
