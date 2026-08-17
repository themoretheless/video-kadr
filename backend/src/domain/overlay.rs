//! Compositing layers drawn on top of the graded frame: image/video overlays
//! (watermark, logo, picture-in-picture, chroma key), burned-in titles, and
//! subtitle burn-in. Everything a filter string will later interpolate is
//! validated here: colours are `#RRGGBB`, text is length-capped and free of
//! control characters, and asset references stay opaque ids.

use serde::{Deserialize, Serialize};

use super::edit::{
    asset_reference, bounded_text, clamped, EditSpecError, HexColor, MAX_TEXT_CHARS,
};
use crate::model;

/// Overlays may sit partly off-frame, so normalized positions are wider than
/// 0..1 while still being bounded.
const MIN_POSITION: f64 = -2.0;
const MAX_POSITION: f64 = 3.0;
const MIN_OVERLAY_SIZE: f64 = 0.001;
const MAX_OVERLAY_SIZE: f64 = 4.0;
const MAX_FADE_SECONDS: f64 = 600.0;
const MAX_TIMELINE_SECONDS: f64 = 86_400.0;
const MAX_FONT_SIZE: f64 = 512.0;
const MAX_BOX_PADDING: f64 = 256.0;
const MAX_BORDER_WIDTH: f64 = 64.0;
const MAX_SHADOW_OFFSET: f64 = 64.0;
const MAX_OUTLINE_WIDTH: f64 = 32.0;
const MAX_MARGIN: f64 = 2_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayKind {
    Image,
    Video,
}

impl OverlayKind {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            _ => Err(EditSpecError::InvalidOverlay),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChromaKeySpec {
    pub(crate) color: HexColor,
    pub(crate) similarity: f64,
    pub(crate) blend: f64,
}

impl ChromaKeySpec {
    pub fn from_wire(value: &model::ChromaKey) -> Result<Self, EditSpecError> {
        let spec = Self {
            color: HexColor::parse(&value.color)?,
            similarity: clamped(value.similarity, 0.01, 1.0, EditSpecError::InvalidOverlay)?,
            blend: clamped(value.blend, 0.0, 1.0, EditSpecError::InvalidOverlay)?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn color(self) -> HexColor {
        self.color
    }

    pub fn similarity(self) -> f64 {
        self.similarity
    }

    pub fn blend(self) -> f64 {
        self.blend
    }

    fn validate(self) -> Result<(), EditSpecError> {
        if !(0.01..=1.0).contains(&self.similarity) || !(0.0..=1.0).contains(&self.blend) {
            return Err(EditSpecError::InvalidOverlay);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlayAudioSpec {
    pub(crate) enabled: bool,
    pub(crate) volume: f64,
}

impl OverlayAudioSpec {
    pub fn from_wire(value: &model::OverlayAudio) -> Result<Self, EditSpecError> {
        Ok(Self {
            enabled: value.enabled,
            volume: clamped(value.volume, 0.0, 4.0, EditSpecError::InvalidOverlay)?,
        })
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn volume(self) -> f64 {
        self.volume
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverlaySpec {
    pub(crate) asset_id: String,
    pub(crate) kind: OverlayKind,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    /// `None` keeps the source aspect ratio.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<f64>,
    pub(crate) opacity: f64,
    pub(crate) rotation_degrees: f64,
    pub(crate) start_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) end_seconds: Option<f64>,
    pub(crate) fade_in_seconds: f64,
    pub(crate) fade_out_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) chroma_key: Option<ChromaKeySpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) audio: Option<OverlayAudioSpec>,
}

impl OverlaySpec {
    pub fn from_wire(value: &model::Overlay) -> Result<Self, EditSpecError> {
        let kind = OverlayKind::parse(&value.kind)?;
        let spec = Self {
            asset_id: asset_reference(&value.asset_id)?,
            kind,
            x: clamped(
                value.x,
                MIN_POSITION,
                MAX_POSITION,
                EditSpecError::InvalidOverlay,
            )?,
            y: clamped(
                value.y,
                MIN_POSITION,
                MAX_POSITION,
                EditSpecError::InvalidOverlay,
            )?,
            width: clamped(
                value.width,
                MIN_OVERLAY_SIZE,
                MAX_OVERLAY_SIZE,
                EditSpecError::InvalidOverlay,
            )?,
            height: value
                .height
                .map(|height| {
                    clamped(
                        height,
                        MIN_OVERLAY_SIZE,
                        MAX_OVERLAY_SIZE,
                        EditSpecError::InvalidOverlay,
                    )
                })
                .transpose()?,
            opacity: clamped(value.opacity, 0.0, 1.0, EditSpecError::InvalidOverlay)?,
            rotation_degrees: clamped(
                value.rotation,
                -360.0,
                360.0,
                EditSpecError::InvalidOverlay,
            )?,
            start_seconds: clamped(
                value.start,
                0.0,
                MAX_TIMELINE_SECONDS,
                EditSpecError::InvalidOverlay,
            )?,
            end_seconds: value
                .end
                .map(|end| {
                    clamped(
                        end,
                        0.0,
                        MAX_TIMELINE_SECONDS,
                        EditSpecError::InvalidOverlay,
                    )
                })
                .transpose()?,
            fade_in_seconds: clamped(
                value.fade_in,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidOverlay,
            )?,
            fade_out_seconds: clamped(
                value.fade_out,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidOverlay,
            )?,
            chroma_key: value
                .chroma_key
                .as_ref()
                .map(ChromaKeySpec::from_wire)
                .transpose()?,
            // Only a video overlay carries an audio branch.
            audio: value
                .audio
                .as_ref()
                .filter(|_| kind == OverlayKind::Video)
                .map(OverlayAudioSpec::from_wire)
                .transpose()?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }

    pub fn kind(&self) -> OverlayKind {
        self.kind
    }

    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> Option<f64> {
        self.height
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn rotation_degrees(&self) -> f64 {
        self.rotation_degrees
    }

    pub fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    pub fn end_seconds(&self) -> Option<f64> {
        self.end_seconds
    }

    pub fn fade_in_seconds(&self) -> f64 {
        self.fade_in_seconds
    }

    pub fn fade_out_seconds(&self) -> f64 {
        self.fade_out_seconds
    }

    pub fn chroma_key(&self) -> Option<ChromaKeySpec> {
        self.chroma_key
    }

    pub fn audio(&self) -> Option<OverlayAudioSpec> {
        self.audio
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.asset_id.is_empty()
            || !(MIN_OVERLAY_SIZE..=MAX_OVERLAY_SIZE).contains(&self.width)
            || !(0.0..=1.0).contains(&self.opacity)
            || self.start_seconds < 0.0
        {
            return Err(EditSpecError::InvalidOverlay);
        }
        if let Some(end) = self.end_seconds {
            if end <= self.start_seconds {
                return Err(EditSpecError::InvalidOverlay);
            }
        }
        if self.audio.is_some() && self.kind != OverlayKind::Video {
            return Err(EditSpecError::InvalidOverlay);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Center,
    Left,
    Right,
}

impl TextAlign {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "center" => Ok(Self::Center),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            _ => Err(EditSpecError::InvalidTitle),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleAnimation {
    None,
    Fade,
    SlideUp,
    Typewriter,
    Pop,
}

impl TitleAnimation {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "none" => Ok(Self::None),
            "fade" => Ok(Self::Fade),
            "slide-up" => Ok(Self::SlideUp),
            "typewriter" => Ok(Self::Typewriter),
            "pop" => Ok(Self::Pop),
            _ => Err(EditSpecError::InvalidTitle),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleBoxSpec {
    pub(crate) color: HexColor,
    pub(crate) opacity: f64,
    pub(crate) padding: f64,
}

impl TitleBoxSpec {
    pub fn from_wire(value: &model::TitleBox) -> Result<Self, EditSpecError> {
        Ok(Self {
            color: HexColor::parse(&value.color)?,
            opacity: clamped(value.opacity, 0.0, 1.0, EditSpecError::InvalidTitle)?,
            padding: clamped(
                value.padding,
                0.0,
                MAX_BOX_PADDING,
                EditSpecError::InvalidTitle,
            )?,
        })
    }

    pub fn color(self) -> HexColor {
        self.color
    }

    pub fn opacity(self) -> f64 {
        self.opacity
    }

    pub fn padding(self) -> f64 {
        self.padding
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleSpec {
    pub(crate) text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) font_asset_id: Option<String>,
    /// Design size in pixels at 1080p; the adapter scales it to output height.
    pub(crate) font_size: f64,
    pub(crate) color: HexColor,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) align: TextAlign,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text_box: Option<TitleBoxSpec>,
    pub(crate) border_width: f64,
    pub(crate) border_color: HexColor,
    pub(crate) shadow_x: f64,
    pub(crate) shadow_y: f64,
    pub(crate) shadow_color: HexColor,
    pub(crate) start_seconds: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) end_seconds: Option<f64>,
    pub(crate) fade_in_seconds: f64,
    pub(crate) fade_out_seconds: f64,
    pub(crate) animation: TitleAnimation,
}

impl TitleSpec {
    pub fn from_wire(value: &model::Title) -> Result<Self, EditSpecError> {
        let spec = Self {
            text: bounded_text(&value.text, MAX_TEXT_CHARS, EditSpecError::InvalidTitle)?,
            font_asset_id: value
                .font_asset_id
                .as_deref()
                .map(asset_reference)
                .transpose()?,
            font_size: clamped(
                value.font_size,
                1.0,
                MAX_FONT_SIZE,
                EditSpecError::InvalidTitle,
            )?,
            color: HexColor::parse(&value.color)?,
            x: clamped(
                value.x,
                MIN_POSITION,
                MAX_POSITION,
                EditSpecError::InvalidTitle,
            )?,
            y: clamped(
                value.y,
                MIN_POSITION,
                MAX_POSITION,
                EditSpecError::InvalidTitle,
            )?,
            align: TextAlign::parse(&value.align)?,
            text_box: value
                .r#box
                .as_ref()
                .map(TitleBoxSpec::from_wire)
                .transpose()?,
            border_width: clamped(
                value.border_width,
                0.0,
                MAX_BORDER_WIDTH,
                EditSpecError::InvalidTitle,
            )?,
            border_color: HexColor::parse(&value.border_color)?,
            shadow_x: clamped(
                value.shadow_x,
                -MAX_SHADOW_OFFSET,
                MAX_SHADOW_OFFSET,
                EditSpecError::InvalidTitle,
            )?,
            shadow_y: clamped(
                value.shadow_y,
                -MAX_SHADOW_OFFSET,
                MAX_SHADOW_OFFSET,
                EditSpecError::InvalidTitle,
            )?,
            shadow_color: HexColor::parse(&value.shadow_color)?,
            start_seconds: clamped(
                value.start,
                0.0,
                MAX_TIMELINE_SECONDS,
                EditSpecError::InvalidTitle,
            )?,
            end_seconds: value
                .end
                .map(|end| clamped(end, 0.0, MAX_TIMELINE_SECONDS, EditSpecError::InvalidTitle))
                .transpose()?,
            fade_in_seconds: clamped(
                value.fade_in,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidTitle,
            )?,
            fade_out_seconds: clamped(
                value.fade_out,
                0.0,
                MAX_FADE_SECONDS,
                EditSpecError::InvalidTitle,
            )?,
            animation: TitleAnimation::parse(&value.animation)?,
        };
        spec.validate()?;
        Ok(spec)
    }

    /// Raw user text. It still has to be filter-escaped by the adapter that
    /// puts it into `drawtext`; the domain only guarantees it is bounded.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn font_asset_id(&self) -> Option<&str> {
        self.font_asset_id.as_deref()
    }

    pub fn font_size(&self) -> f64 {
        self.font_size
    }

    pub fn color(&self) -> HexColor {
        self.color
    }

    pub fn position(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    pub fn align(&self) -> TextAlign {
        self.align
    }

    pub fn text_box(&self) -> Option<TitleBoxSpec> {
        self.text_box
    }

    pub fn border_width(&self) -> f64 {
        self.border_width
    }

    pub fn border_color(&self) -> HexColor {
        self.border_color
    }

    pub fn shadow(&self) -> (f64, f64, HexColor) {
        (self.shadow_x, self.shadow_y, self.shadow_color)
    }

    pub fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    pub fn end_seconds(&self) -> Option<f64> {
        self.end_seconds
    }

    pub fn fade_in_seconds(&self) -> f64 {
        self.fade_in_seconds
    }

    pub fn fade_out_seconds(&self) -> f64 {
        self.fade_out_seconds
    }

    pub fn animation(&self) -> TitleAnimation {
        self.animation
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.text.chars().count() > MAX_TEXT_CHARS
            || self
                .text
                .chars()
                .any(|character| character.is_control() && character != '\n')
            || !(1.0..=MAX_FONT_SIZE).contains(&self.font_size)
            || self.start_seconds < 0.0
        {
            return Err(EditSpecError::InvalidTitle);
        }
        if let Some(end) = self.end_seconds {
            if end <= self.start_seconds {
                return Err(EditSpecError::InvalidTitle);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitlePosition {
    Bottom,
    Top,
}

impl SubtitlePosition {
    pub fn parse(value: &str) -> Result<Self, EditSpecError> {
        match value {
            "bottom" => Ok(Self::Bottom),
            "top" => Ok(Self::Top),
            _ => Err(EditSpecError::InvalidSubtitles),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubtitlesSpec {
    pub(crate) asset_id: String,
    pub(crate) burn_in: bool,
    pub(crate) font_size: f64,
    pub(crate) color: HexColor,
    pub(crate) outline_width: f64,
    pub(crate) position: SubtitlePosition,
    pub(crate) margin_vertical: f64,
}

impl SubtitlesSpec {
    pub fn from_wire(value: &model::Subtitles) -> Result<Self, EditSpecError> {
        let spec = Self {
            asset_id: asset_reference(&value.asset_id)?,
            burn_in: value.burn_in,
            font_size: clamped(
                value.font_size,
                1.0,
                MAX_FONT_SIZE,
                EditSpecError::InvalidSubtitles,
            )?,
            color: HexColor::parse(&value.color)?,
            outline_width: clamped(
                value.outline_width,
                0.0,
                MAX_OUTLINE_WIDTH,
                EditSpecError::InvalidSubtitles,
            )?,
            position: SubtitlePosition::parse(&value.position)?,
            margin_vertical: clamped(
                value.margin_v,
                0.0,
                MAX_MARGIN,
                EditSpecError::InvalidSubtitles,
            )?,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }

    pub fn burn_in(&self) -> bool {
        self.burn_in
    }

    pub fn font_size(&self) -> f64 {
        self.font_size
    }

    pub fn color(&self) -> HexColor {
        self.color
    }

    pub fn outline_width(&self) -> f64 {
        self.outline_width
    }

    pub fn position(&self) -> SubtitlePosition {
        self.position
    }

    pub fn margin_vertical(&self) -> f64 {
        self.margin_vertical
    }

    pub(crate) fn validate(&self) -> Result<(), EditSpecError> {
        if self.asset_id.is_empty() || !(1.0..=MAX_FONT_SIZE).contains(&self.font_size) {
            return Err(EditSpecError::InvalidSubtitles);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay() -> model::Overlay {
        model::Overlay {
            asset_id: "ast_0123456789abcdef".into(),
            kind: "image".into(),
            x: 0.05,
            y: 0.05,
            width: 0.25,
            height: None,
            opacity: 1.0,
            rotation: 0.0,
            start: 0.0,
            end: None,
            fade_in: 0.0,
            fade_out: 0.0,
            chroma_key: None,
            audio: None,
        }
    }

    fn title() -> model::Title {
        model::Title {
            text: "Заголовок".into(),
            font_asset_id: None,
            font_size: 48.0,
            color: "#ffffff".into(),
            x: 0.5,
            y: 0.85,
            align: "center".into(),
            r#box: None,
            border_width: 0.0,
            border_color: "#000000".into(),
            shadow_x: 0.0,
            shadow_y: 0.0,
            shadow_color: "#000000".into(),
            start: 0.0,
            end: None,
            fade_in: 0.0,
            fade_out: 0.0,
            animation: "none".into(),
        }
    }

    #[test]
    fn colors_are_canonical_and_filter_safe() {
        let color = HexColor::parse("#00ff00").unwrap();
        assert_eq!(color.as_hex(), "#00FF00");
        assert_eq!(color.ffmpeg_color(), "0x00FF00");
        for rejected in ["00FF00", "#00FF0", "#00FF00FF", "#gggggg", "'#000000'"] {
            assert_eq!(HexColor::parse(rejected), Err(EditSpecError::InvalidColor));
        }
    }

    #[test]
    fn overlay_geometry_is_clamped_and_ranges_are_checked() {
        let mut wide = overlay();
        wide.width = 99.0;
        wide.opacity = 5.0;
        let spec = OverlaySpec::from_wire(&wide).unwrap();
        assert_eq!(spec.width(), MAX_OVERLAY_SIZE);
        assert_eq!(spec.opacity(), 1.0);

        let mut inverted = overlay();
        inverted.start = 5.0;
        inverted.end = Some(1.0);
        assert_eq!(
            OverlaySpec::from_wire(&inverted),
            Err(EditSpecError::InvalidOverlay)
        );

        let mut unknown = overlay();
        unknown.kind = "audio".into();
        assert_eq!(
            OverlaySpec::from_wire(&unknown),
            Err(EditSpecError::InvalidOverlay)
        );
    }

    #[test]
    fn overlay_audio_is_only_kept_for_video_overlays() {
        let mut image = overlay();
        image.audio = Some(model::OverlayAudio {
            enabled: true,
            volume: 1.0,
        });
        assert!(OverlaySpec::from_wire(&image).unwrap().audio().is_none());

        let mut video = image;
        video.kind = "video".into();
        assert!(OverlaySpec::from_wire(&video).unwrap().audio().is_some());
    }

    #[test]
    fn chroma_key_needs_a_hex_color_and_bounded_tolerances() {
        let mut keyed = overlay();
        keyed.chroma_key = Some(model::ChromaKey {
            color: "#00FF00".into(),
            similarity: 9.0,
            blend: 0.05,
        });
        let spec = OverlaySpec::from_wire(&keyed).unwrap();
        assert_eq!(spec.chroma_key().unwrap().similarity(), 1.0);

        keyed.chroma_key = Some(model::ChromaKey {
            color: "green".into(),
            similarity: 0.1,
            blend: 0.0,
        });
        assert_eq!(
            OverlaySpec::from_wire(&keyed),
            Err(EditSpecError::InvalidColor)
        );
    }

    #[test]
    fn title_text_is_bounded_and_free_of_control_characters() {
        assert_eq!(TitleSpec::from_wire(&title()).unwrap().text(), "Заголовок");

        let mut long = title();
        long.text = "a".repeat(MAX_TEXT_CHARS + 1);
        assert_eq!(
            TitleSpec::from_wire(&long),
            Err(EditSpecError::InvalidTitle)
        );

        let mut control = title();
        control.text = "line\u{0}break".into();
        assert_eq!(
            TitleSpec::from_wire(&control),
            Err(EditSpecError::InvalidTitle)
        );

        let mut wrapped = title();
        wrapped.text = "line\nbreak".into();
        assert!(TitleSpec::from_wire(&wrapped).is_ok());

        let mut unknown = title();
        unknown.animation = "explode".into();
        assert_eq!(
            TitleSpec::from_wire(&unknown),
            Err(EditSpecError::InvalidTitle)
        );
    }

    #[test]
    fn subtitles_reject_path_shaped_asset_ids() {
        let subtitles = model::Subtitles {
            asset_id: "../secrets.srt".into(),
            burn_in: true,
            font_size: 24.0,
            color: "#FFFFFF".into(),
            outline_width: 2.0,
            position: "bottom".into(),
            margin_v: 40.0,
        };
        assert_eq!(
            SubtitlesSpec::from_wire(&subtitles),
            Err(EditSpecError::InvalidAssetReference)
        );
    }
}
