//! Stage 9: burned-in titles and subtitles. Owned by the titles agent.
//!
//! Reads `spec.titles()` and then `spec.subtitles()`. Each title becomes a
//! `drawtext` with its box, border, shadow, timing window and animation; the
//! subtitle asset becomes a `subtitles` burn-in. Every user string and every
//! font or subtitle path must be filtergraph-escaped before it is interpolated
//! into a filter option.
//!
//! Two properties carry the security of this stage:
//!
//! * `escape_filter_text` prefixes every character the filtergraph parser gives
//!   meaning to with a backslash, so user text can never end an option, end a
//!   filter, or open a pad label.
//! * `expansion=none` disables `drawtext`'s own `%{...}` text expansion, so a
//!   percent sign in a title stays a percent sign instead of reaching FFmpeg's
//!   expression evaluator.
//!
//! Fonts and subtitle files are checked on this host before the graph is built:
//! an absent font or a malformed subtitle file becomes a typed
//! `TitleRenderError` rather than a filter string FFmpeg would reject halfway
//! through the render.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::domain::edit::{EditSpec, HexColor};
use crate::domain::overlay::{
    SubtitlePosition, SubtitlesSpec, TextAlign, TitleAnimation, TitleSpec,
};

use super::{ComplexPlan, RenderContext};

/// Title font sizes are authored against a 1080p frame and scaled to the frame
/// the titles are actually drawn on.
const REFERENCE_HEIGHT: f64 = 1080.0;
/// Contract cap for a subtitle asset.
const MAX_SUBTITLE_BYTES: u64 = 4 * 1024 * 1024;
/// Upper bound on the `drawtext` filters one edit may emit. Only a typewriter
/// animation can multiply layers, and an unbounded chain would be a cheap way
/// to make FFmpeg spend minutes on a single frame.
const MAX_TEXT_LAYERS: usize = 512;
const TYPEWRITER_MAX_STEPS: usize = 24;
const TYPEWRITER_STEP_SECONDS: f64 = 0.08;
/// Duration of the built-in `fade`, `slide-up` and `pop` animations.
const ANIMATION_SECONDS: f64 = 0.4;
const POP_SECONDS: f64 = 0.3;
/// Travel of `slide-up` and overshoot of `pop`, as a fraction of frame height.
const SLIDE_FRACTION: f64 = 0.08;
const POP_FRACTION: f64 = 0.04;

/// Operator override for the default title font.
const DEFAULT_FONT_ENV: &str = "VIDEO_EDITOR_DEFAULT_FONT";
/// Fonts shipped with the platforms this backend runs on, in preference order.
/// All of them cover Cyrillic, which the default UI language needs.
const DEFAULT_FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

/// Everything this stage can refuse to render, as a value the caller can match
/// on instead of a formatted string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TitleRenderError {
    /// The installed FFmpeg has no `drawtext`, which means it was built without
    /// libfreetype.
    TextRenderingUnavailable,
    /// No bundled or configured default font exists on this host.
    DefaultFontMissing,
    /// A `fontAssetId` resolved to a path that is not a readable file.
    FontAssetUnreadable(String),
    SubtitleUnreadable(String),
    SubtitleMalformed(String),
    TooManyTextLayers(usize),
}

impl fmt::Display for TitleRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TextRenderingUnavailable => {
                write!(
                    formatter,
                    "FFmpeg has no drawtext filter (libfreetype missing)"
                )
            }
            Self::DefaultFontMissing => write!(
                formatter,
                "no default title font on this host; set {DEFAULT_FONT_ENV}"
            ),
            Self::FontAssetUnreadable(id) => {
                write!(formatter, "title font asset '{id}' is not a readable file")
            }
            Self::SubtitleUnreadable(reason) => {
                write!(formatter, "subtitle asset is not readable: {reason}")
            }
            Self::SubtitleMalformed(reason) => {
                write!(
                    formatter,
                    "subtitle asset is not a valid .srt/.vtt: {reason}"
                )
            }
            Self::TooManyTextLayers(count) => {
                write!(
                    formatter,
                    "titles need {count} drawtext layers, cap is {MAX_TEXT_LAYERS}"
                )
            }
        }
    }
}

impl std::error::Error for TitleRenderError {}

/// Subtitle container shapes this stage accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleFormat {
    SubRip,
    WebVtt,
}

/// Process-wide answer to "can this FFmpeg draw text?". Published once at
/// startup from the probed filter list; until it is, text rendering is assumed
/// available so hosts that were never probed behave exactly as before.
static TEXT_RENDERING: OnceLock<bool> = OnceLock::new();

/// Integration point for the process bootstrap: pass whether the probed FFmpeg
/// filter list contains `drawtext`. Later calls are ignored.
pub fn set_text_rendering_available(available: bool) {
    let _ = TEXT_RENDERING.set(available);
}

fn text_rendering_available() -> bool {
    TEXT_RENDERING.get().copied().unwrap_or(true)
}

fn require_text_rendering(available: bool) -> Result<(), TitleRenderError> {
    available
        .then_some(())
        .ok_or(TitleRenderError::TextRenderingUnavailable)
}

pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    let titles = spec.titles();
    // Soft (non-burned-in) subtitles would be a separate output stream, which
    // this stage does not own; only a burn-in touches the video chain.
    let subtitles = spec.subtitles().filter(|spec| spec.burn_in());
    if titles.is_empty() && subtitles.is_none() {
        return Ok(());
    }
    require_text_rendering(text_rendering_available())?;

    if !titles.is_empty() {
        let mut filters: Vec<String> = Vec::with_capacity(titles.len());
        for title in titles {
            let font = resolve_font(title.font_asset_id(), ctx)?;
            filters.extend(title_filters(title, &font, ctx)?);
        }
        if filters.len() > MAX_TEXT_LAYERS {
            return Err(TitleRenderError::TooManyTextLayers(filters.len()).into());
        }
        plan.chain_video(&filters, "titles")?;
    }

    if let Some(subtitles) = subtitles {
        let path = ctx.require_asset(subtitles.asset_id())?;
        // Hand FFmpeg only a file we have already parsed ourselves.
        load_subtitle_document(path)?;
        plan.chain_video(&[subtitles_filter(subtitles, path)?], "subtitles")?;
    }
    Ok(())
}

/// Filtergraph-escape a value interpolated into a filter option unquoted.
///
/// `:` ends an option, `,` and `;` end a filter, `[` and `]` open a pad label,
/// `'` opens a quoted run, `\` escapes, `=` splits key from value, and `%`
/// opens `drawtext` text expansion. A newline is escaped as a backslash plus
/// the newline itself, which survives the parser as a real line break.
fn escape_filter_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 16);
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '\'' | ':' | ',' | ';' | '[' | ']' | '=' | '%' | '\n'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// Escape a path for one quoted FFmpeg filter option, exactly as the LUT path
/// is escaped in the FFmpeg adapter. This is filtergraph escaping, not shell
/// escaping: the command is still passed as an argv vector.
fn escape_filter_path(path: &Path) -> anyhow::Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("title asset path is not valid UTF-8"))?;
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if matches!(character, '\\' | '\'' | ':' | ',' | ';' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(format!("'{escaped}'"))
}

fn seconds(value: f64) -> String {
    format!("{value:.3}")
}

fn ratio(value: f64) -> String {
    format!("{value:.6}")
}

/// Scale between the 1080p design frame and the frame the titles are drawn on.
/// Titles are composed before the export resize, so the reference is the source
/// height and the resize carries the title along with everything else.
fn height_scale(ctx: &RenderContext) -> f64 {
    if ctx.height == 0 {
        return 1.0;
    }
    f64::from(ctx.height) / REFERENCE_HEIGHT
}

/// 0 at `start`, 1 after `duration` seconds, clamped at both ends.
fn ramp_in(start: f64, duration: f64) -> String {
    format!(
        "min(1,max(0,(t-{})/{}))",
        seconds(start),
        seconds(duration.max(0.001))
    )
}

/// 1 until `duration` seconds before `end`, then down to 0.
fn ramp_out(end: f64, duration: f64) -> String {
    format!(
        "min(1,max(0,({}-t)/{}))",
        seconds(end),
        seconds(duration.max(0.001))
    )
}

fn resolve_font(asset_id: Option<&str>, ctx: &RenderContext) -> anyhow::Result<PathBuf> {
    let Some(id) = asset_id else {
        return Ok(default_font()?);
    };
    let path = ctx.require_asset(id)?;
    if !path.is_file() {
        return Err(TitleRenderError::FontAssetUnreadable(id.to_owned()).into());
    }
    Ok(path.to_path_buf())
}

/// Locate the default font on this host. An explicit override that does not
/// exist is an error rather than a silent fallback, so a misconfigured deploy
/// fails loudly instead of rendering in an unexpected typeface.
fn default_font() -> Result<PathBuf, TitleRenderError> {
    if let Some(configured) = std::env::var_os(DEFAULT_FONT_ENV) {
        let path = PathBuf::from(configured);
        return path
            .is_file()
            .then_some(path)
            .ok_or(TitleRenderError::DefaultFontMissing);
    }
    first_existing_font(DEFAULT_FONT_CANDIDATES).ok_or(TitleRenderError::DefaultFontMissing)
}

fn first_existing_font(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

/// Build the `drawtext` filters for one title. A typewriter animation needs one
/// filter per reveal step; every other animation needs exactly one.
fn title_filters(
    title: &TitleSpec,
    font: &Path,
    ctx: &RenderContext,
) -> anyhow::Result<Vec<String>> {
    let start = title.start_seconds();
    let end = title
        .end_seconds()
        .unwrap_or(ctx.output_duration_seconds)
        .max(start);
    if end - start <= f64::EPSILON {
        // The title never becomes visible, so it contributes no filter.
        return Ok(Vec::new());
    }

    let (anchor_x, anchor_y) = title.position();
    let x_expression = match title.align() {
        TextAlign::Center => format!("w*{}-text_w/2", ratio(anchor_x)),
        TextAlign::Left => format!("w*{}", ratio(anchor_x)),
        TextAlign::Right => format!("w*{}-text_w", ratio(anchor_x)),
    };
    let mut y_expression = format!("h*{}-text_h/2", ratio(anchor_y));

    let mut alpha_terms: Vec<String> = Vec::new();
    if title.fade_in_seconds() > 0.0 {
        alpha_terms.push(ramp_in(start, title.fade_in_seconds()));
    }
    if title.fade_out_seconds() > 0.0 {
        alpha_terms.push(ramp_out(end, title.fade_out_seconds()));
    }
    match title.animation() {
        TitleAnimation::None | TitleAnimation::Typewriter => {}
        TitleAnimation::Fade => {
            alpha_terms.push(ramp_in(start, ANIMATION_SECONDS));
            alpha_terms.push(ramp_out(end, ANIMATION_SECONDS));
        }
        TitleAnimation::SlideUp => {
            // Starts one slide below the anchor and eases up to it.
            y_expression.push_str(&format!(
                "+h*{}*(1-{})",
                ratio(SLIDE_FRACTION),
                ramp_in(start, ANIMATION_SECONDS)
            ));
        }
        TitleAnimation::Pop => {
            let progress = ramp_in(start, POP_SECONDS);
            alpha_terms.push(ramp_in(start, POP_SECONDS / 2.0));
            // One decaying oscillation: below the anchor, past it, then settled.
            y_expression.push_str(&format!(
                "+h*{}*(1-{progress})*cos({progress}*6.283185)",
                ratio(POP_FRACTION)
            ));
        }
    }

    let mut options: Vec<String> = vec![
        // Positions and sizes are expressions, so they stay correct even if the
        // frame this chain runs on is not the probed source size.
        format!(
            "fontsize={:.0}",
            (title.font_size() * height_scale(ctx)).round().max(1.0)
        ),
        format!("fontcolor={}", title.color().ffmpeg_color()),
        format!("x='{x_expression}'"),
        format!("y='{y_expression}'"),
    ];
    if title.border_width() > 0.0 {
        options.push(format!("borderw={:.0}", title.border_width()));
        options.push(format!(
            "bordercolor={}",
            title.border_color().ffmpeg_color()
        ));
    }
    let (shadow_x, shadow_y, shadow_color) = title.shadow();
    if shadow_x.abs() > f64::EPSILON || shadow_y.abs() > f64::EPSILON {
        options.push(format!("shadowx={shadow_x:.0}"));
        options.push(format!("shadowy={shadow_y:.0}"));
        options.push(format!("shadowcolor={}", shadow_color.ffmpeg_color()));
    }
    if let Some(text_box) = title.text_box() {
        options.push("box=1".to_owned());
        options.push(format!(
            "boxcolor={}@{}",
            text_box.color().ffmpeg_color(),
            ratio(text_box.opacity())
        ));
        options.push(format!("boxborderw={:.0}", text_box.padding()));
    }
    if !alpha_terms.is_empty() {
        options.push(format!("alpha='{}'", alpha_terms.join("*")));
    }
    let tail = options.join(":");
    let fontfile = escape_filter_path(font)?;

    let draw = |text: &str, from: f64, to: f64| {
        format!(
            "drawtext=fontfile={fontfile}:text={}:expansion=none:{tail}:enable='between(t,{},{})'",
            escape_filter_text(text),
            seconds(from),
            seconds(to)
        )
    };

    if title.animation() != TitleAnimation::Typewriter {
        return Ok(vec![draw(title.text(), start, end)]);
    }

    // Typewriter: reveal a growing prefix. `drawtext` cannot slice its own text
    // from an expression, so each step is its own filter with its own window.
    // The step count is capped, and long text reveals several characters per
    // step instead of emitting one filter per character.
    let characters: Vec<char> = title.text().chars().collect();
    let steps = characters.len().clamp(1, TYPEWRITER_MAX_STEPS);
    let reveal = (TYPEWRITER_STEP_SECONDS * steps as f64)
        .min((end - start) * 0.6)
        .max(0.001);
    let mut filters = Vec::with_capacity(steps);
    for step in 1..=steps {
        let taken = (characters.len() * step).div_ceil(steps);
        let prefix: String = characters.iter().take(taken).collect();
        let from = start + reveal * ((step - 1) as f64 / steps as f64);
        // The last step stays on screen for the rest of the title window.
        let to = if step == steps {
            end
        } else {
            start + reveal * (step as f64 / steps as f64)
        };
        filters.push(draw(&prefix, from, to));
    }
    Ok(filters)
}

/// ASS colours are `&HAABBGGRR&`: alpha first, then the channels reversed.
fn ass_color(color: HexColor) -> String {
    let hex = color.as_hex();
    let channel = |from: usize| {
        hex.get(from..from + 2)
            .and_then(|digits| u8::from_str_radix(digits, 16).ok())
            .unwrap_or(0)
    };
    format!(
        "&H00{:02X}{:02X}{:02X}&",
        channel(5),
        channel(3),
        channel(1)
    )
}

/// Burn in a subtitle asset. `force_style` is built entirely from validated
/// numbers and enums, so the only untrusted part of the filter is the path,
/// which goes through the same escaping as every other filter path.
fn subtitles_filter(spec: &SubtitlesSpec, path: &Path) -> anyhow::Result<String> {
    // libass alignment uses the numeric keypad layout: 2 is bottom centre and
    // 8 is top centre.
    let alignment = match spec.position() {
        SubtitlePosition::Bottom => 2,
        SubtitlePosition::Top => 8,
    };
    let style = format!(
        "FontSize={:.0},PrimaryColour={},OutlineColour=&H00000000&,BorderStyle=1,Outline={:.0},Alignment={alignment},MarginV={:.0}",
        spec.font_size(),
        ass_color(spec.color()),
        spec.outline_width(),
        spec.margin_vertical()
    );
    Ok(format!(
        "subtitles=filename={}:force_style='{style}'",
        escape_filter_path(path)?
    ))
}

fn load_subtitle_document(path: &Path) -> Result<SubtitleFormat, TitleRenderError> {
    let metadata = fs::metadata(path)
        .map_err(|error| TitleRenderError::SubtitleUnreadable(error.to_string()))?;
    if !metadata.is_file() {
        return Err(TitleRenderError::SubtitleUnreadable(
            "not a file".to_owned(),
        ));
    }
    if metadata.len() > MAX_SUBTITLE_BYTES {
        return Err(TitleRenderError::SubtitleUnreadable("too large".to_owned()));
    }
    let bytes =
        fs::read(path).map_err(|error| TitleRenderError::SubtitleUnreadable(error.to_string()))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| TitleRenderError::SubtitleMalformed("not UTF-8".to_owned()))?;
    parse_subtitle_document(&text)
}

/// Validate the shape of an `.srt`/`.vtt` document: a WebVTT header decides the
/// format, and every cue line must carry two parsable timestamps.
fn parse_subtitle_document(text: &str) -> Result<SubtitleFormat, TitleRenderError> {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let format = if body.trim_start().starts_with("WEBVTT") {
        SubtitleFormat::WebVtt
    } else {
        SubtitleFormat::SubRip
    };
    let mut cues = 0_usize;
    for line in body.lines() {
        let Some((left, right)) = line.split_once("-->") else {
            continue;
        };
        let start = parse_timestamp(left.trim())
            .ok_or_else(|| TitleRenderError::SubtitleMalformed(format!("cue start in '{line}'")))?;
        // WebVTT cues may carry positioning settings after the end timestamp.
        let end_token = right.split_whitespace().next().unwrap_or_default();
        let end = parse_timestamp(end_token)
            .ok_or_else(|| TitleRenderError::SubtitleMalformed(format!("cue end in '{line}'")))?;
        if end < start {
            return Err(TitleRenderError::SubtitleMalformed(format!(
                "cue ends before it starts in '{line}'"
            )));
        }
        cues += 1;
    }
    if cues == 0 {
        return Err(TitleRenderError::SubtitleMalformed("no cues".to_owned()));
    }
    Ok(format)
}

/// Parse `HH:MM:SS,mmm`, `HH:MM:SS.mmm` or `MM:SS.mmm` into seconds.
fn parse_timestamp(value: &str) -> Option<f64> {
    let (clock, fraction) = value.split_once([',', '.']).unwrap_or((value, "0"));
    if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut total = 0.0_f64;
    for part in parts {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        total = total * 60.0 + part.parse::<f64>().ok()?;
    }
    let scale = 10.0_f64.powi(i32::try_from(fraction.len()).ok()?);
    Some(total + fraction.parse::<f64>().ok()? / scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Write;

    use crate::domain::artifact_graph::Fingerprint;
    use crate::domain::edit::EditExtensions;
    use crate::model;
    use crate::services::render::{EditPlan, SourceMediaMetadata};

    fn context() -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, true, Some(30.0), 10.0)
    }

    fn plan() -> ComplexPlan {
        ComplexPlan::new(super::super::InputSpec::source("/in.mp4"), true)
    }

    fn wire_title(text: &str) -> model::Title {
        model::Title {
            text: text.into(),
            font_asset_id: None,
            font_size: 48.0,
            color: "#FFFFFF".into(),
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

    fn title(text: &str) -> TitleSpec {
        TitleSpec::from_wire(&wire_title(text)).expect("test fixture is valid")
    }

    /// Compile a wire request into the spec the render stages consume.
    fn spec(request: serde_json::Value) -> EditSpec {
        let request: model::EditRequest =
            serde_json::from_value(request).expect("test fixture is valid");
        let extensions = EditExtensions::from_request(&request).expect("test fixture is valid");
        EditPlan::compile(
            Fingerprint::digest(b"source"),
            request,
            SourceMediaMetadata::new(1920, 1080, 10.0).expect("test fixture is valid"),
        )
        .expect("test fixture is valid")
        .edit
        .with_extensions(extensions)
        .expect("test fixture is valid")
    }

    fn single_filter(title: &TitleSpec) -> String {
        let filters = title_filters(title, Path::new("/fonts/Default.ttf"), &context())
            .expect("filters build");
        assert_eq!(filters.len(), 1, "{filters:?}");
        filters.into_iter().next().unwrap_or_default()
    }

    /// The `text=` option value, up to the option separator that follows it.
    fn drawn_text(filter: &str) -> String {
        let after = filter
            .split_once(":text=")
            .expect("drawtext carries text")
            .1;
        after
            .split_once(":expansion=none")
            .expect("text is followed by expansion=none")
            .0
            .to_owned()
    }

    #[test]
    fn no_titles_and_no_subtitles_emit_nothing() {
        let mut plan = plan();
        apply(
            &mut plan,
            &spec(serde_json::json!({ "videoId": "x" })),
            &context(),
        )
        .expect("stage runs");
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn a_plain_title_becomes_one_anchored_drawtext() {
        let filter = single_filter(&title("Заголовок"));
        assert!(
            filter.starts_with(
                "drawtext=fontfile='/fonts/Default.ttf':text=Заголовок:expansion=none:"
            ),
            "{filter}"
        );
        assert!(filter.contains("fontsize=48"), "{filter}");
        assert!(filter.contains("fontcolor=0xFFFFFF"), "{filter}");
        assert!(filter.contains("x='w*0.500000-text_w/2'"), "{filter}");
        assert!(filter.contains("y='h*0.850000-text_h/2'"), "{filter}");
        assert!(
            filter.contains("enable='between(t,0.000,10.000)'"),
            "{filter}"
        );
        // Nothing was asked for, so nothing extra is emitted.
        assert!(!filter.contains("alpha="), "{filter}");
        assert!(!filter.contains("box="), "{filter}");
        assert!(!filter.contains("borderw="), "{filter}");
    }

    #[test]
    fn font_size_scales_from_the_1080p_reference_to_the_output_height() {
        let half = RenderContext::new(960, 540, 10.0, true, None, 10.0);
        let filters =
            title_filters(&title("A"), Path::new("/f.ttf"), &half).expect("filters build");
        assert!(filters[0].contains("fontsize=24"), "{filters:?}");

        // A zero-height probe must not scale the size to nothing.
        let unknown = RenderContext::new(0, 0, 10.0, true, None, 10.0);
        let filters =
            title_filters(&title("A"), Path::new("/f.ttf"), &unknown).expect("filters build");
        assert!(filters[0].contains("fontsize=48"), "{filters:?}");
    }

    #[test]
    fn alignment_moves_the_anchor_not_the_text() {
        let mut wire = wire_title("A");
        wire.align = "left".into();
        let left = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));
        assert!(left.contains("x='w*0.500000'"), "{left}");

        wire.align = "right".into();
        let right = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));
        assert!(right.contains("x='w*0.500000-text_w'"), "{right}");
    }

    #[test]
    fn box_border_and_shadow_reach_the_filter() {
        let mut wire = wire_title("A");
        wire.r#box = Some(model::TitleBox {
            color: "#101010".into(),
            opacity: 0.5,
            padding: 12.0,
        });
        wire.border_width = 3.0;
        wire.border_color = "#FF0000".into();
        wire.shadow_x = 2.0;
        wire.shadow_y = -4.0;
        wire.shadow_color = "#00FF00".into();
        let filter = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));

        assert!(
            filter.contains("borderw=3:bordercolor=0xFF0000"),
            "{filter}"
        );
        assert!(
            filter.contains("shadowx=2:shadowy=-4:shadowcolor=0x00FF00"),
            "{filter}"
        );
        assert!(
            filter.contains("box=1:boxcolor=0x101010@0.500000:boxborderw=12"),
            "{filter}"
        );
    }

    #[test]
    fn timed_fades_become_one_clamped_alpha_expression() {
        let mut wire = wire_title("A");
        wire.start = 2.0;
        wire.end = Some(6.0);
        wire.fade_in = 0.5;
        wire.fade_out = 1.0;
        let filter = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));

        assert!(
            filter.contains("alpha='min(1,max(0,(t-2.000)/0.500))*min(1,max(0,(6.000-t)/1.000))'"),
            "{filter}"
        );
        assert!(
            filter.contains("enable='between(t,2.000,6.000)'"),
            "{filter}"
        );
    }

    #[test]
    fn a_title_whose_window_is_empty_emits_no_filter() {
        let mut wire = wire_title("A");
        // No explicit end, and the whole output is already over by then.
        wire.start = 30.0;
        let filters = title_filters(
            &TitleSpec::from_wire(&wire).expect("valid"),
            Path::new("/f.ttf"),
            &context(),
        )
        .expect("filters build");
        assert!(filters.is_empty(), "{filters:?}");
    }

    #[test]
    fn fade_animation_adds_its_own_alpha_ramps() {
        let mut wire = wire_title("A");
        wire.animation = "fade".into();
        let filter = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));
        assert!(
            filter.contains("alpha='min(1,max(0,(t-0.000)/0.400))*min(1,max(0,(10.000-t)/0.400))'"),
            "{filter}"
        );
    }

    #[test]
    fn slide_up_animation_offsets_the_y_expression() {
        let mut wire = wire_title("A");
        wire.animation = "slide-up".into();
        let filter = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));
        assert!(
            filter.contains("y='h*0.850000-text_h/2+h*0.080000*(1-min(1,max(0,(t-0.000)/0.400)))'"),
            "{filter}"
        );
        assert!(!filter.contains("alpha="), "{filter}");
    }

    #[test]
    fn pop_animation_snaps_alpha_and_overshoots_in_y() {
        let mut wire = wire_title("A");
        wire.animation = "pop".into();
        let filter = single_filter(&TitleSpec::from_wire(&wire).expect("valid"));
        assert!(
            filter.contains("alpha='min(1,max(0,(t-0.000)/0.150))'"),
            "{filter}"
        );
        assert!(
            filter.contains("+h*0.040000*(1-min(1,max(0,(t-0.000)/0.300)))*cos(min(1,max(0,(t-0.000)/0.300))*6.283185)"),
            "{filter}"
        );
    }

    #[test]
    fn typewriter_animation_reveals_a_growing_prefix_per_step() {
        let mut wire = wire_title("abc");
        wire.animation = "typewriter".into();
        let filters = title_filters(
            &TitleSpec::from_wire(&wire).expect("valid"),
            Path::new("/f.ttf"),
            &context(),
        )
        .expect("filters build");

        assert_eq!(filters.len(), 3);
        assert_eq!(drawn_text(&filters[0]), "a");
        assert_eq!(drawn_text(&filters[1]), "ab");
        assert_eq!(drawn_text(&filters[2]), "abc");
        assert!(
            filters[0].contains("enable='between(t,0.000,0.080)'"),
            "{filters:?}"
        );
        // The finished text stays up until the end of the title window.
        assert!(
            filters[2].contains("enable='between(t,0.160,10.000)'"),
            "{filters:?}"
        );
    }

    #[test]
    fn typewriter_steps_are_capped_for_long_text() {
        let mut wire = wire_title(&"я".repeat(500));
        wire.animation = "typewriter".into();
        let filters = title_filters(
            &TitleSpec::from_wire(&wire).expect("valid"),
            Path::new("/f.ttf"),
            &context(),
        )
        .expect("filters build");

        assert_eq!(filters.len(), TYPEWRITER_MAX_STEPS);
        // Each step reveals a whole group of characters, never a broken one.
        assert_eq!(drawn_text(&filters[0]).chars().count(), 21);
        assert_eq!(
            drawn_text(&filters[TYPEWRITER_MAX_STEPS - 1])
                .chars()
                .count(),
            500
        );
    }

    #[test]
    fn adversarial_text_is_escaped_into_literal_characters() {
        // Each case is text a user can type; none of it may end the option,
        // end the filter, open a label, or reach the expression evaluator.
        let cases = [
            ("a:b", "a\\:b"),
            ("it's", "it\\'s"),
            ("back\\slash", "back\\\\slash"),
            ("100% done", "100\\% done"),
            ("%{pts\\:hms}", "\\%{pts\\\\\\:hms}"),
            ("one,two", "one\\,two"),
            ("end;next", "end\\;next"),
            ("[label]", "\\[label\\]"),
            ("key=value", "key\\=value"),
            ("line\nbreak", "line\\\nbreak"),
        ];
        for (raw, expected) in cases {
            assert_eq!(escape_filter_text(raw), expected, "raw: {raw:?}");
            assert_eq!(drawn_text(&single_filter(&title(raw))), expected);
        }
    }

    #[test]
    fn an_injected_filter_fragment_stays_inside_the_text_option() {
        let injection = "x':drawtext=text='pwned',hflip[v];[v]drawbox=color=red@1:t=fill";
        let filter = single_filter(&title(injection));

        // Every separator inside the payload is escaped, so the filter still
        // has exactly one unescaped `drawtext=` and no unescaped separators
        // after the text option.
        assert_eq!(filter.matches("drawtext=").count(), 1);
        assert!(filter.contains("drawtext\\=text\\="), "{filter}");
        let drawn = drawn_text(&filter);
        for separator in [':', ',', ';', '[', ']', '\'', '='] {
            let unescaped = drawn
                .char_indices()
                .filter(|(index, character)| {
                    *character == separator && !drawn[..*index].ends_with('\\')
                })
                .count();
            assert_eq!(unescaped, 0, "unescaped {separator:?} in {drawn}");
        }
    }

    #[test]
    fn expansion_is_disabled_on_every_emitted_drawtext() {
        for animation in ["none", "fade", "slide-up", "typewriter", "pop"] {
            let mut wire = wire_title("100%{pts}");
            wire.animation = animation.into();
            let filters = title_filters(
                &TitleSpec::from_wire(&wire).expect("valid"),
                Path::new("/f.ttf"),
                &context(),
            )
            .expect("filters build");
            assert!(!filters.is_empty());
            for filter in filters {
                assert!(filter.contains(":expansion=none:"), "{animation}: {filter}");
            }
        }
    }

    #[test]
    fn a_font_path_with_filter_syntax_is_escaped_and_quoted() {
        let filter = title_filters(
            &title("A"),
            Path::new("/fonts/od:d,name[1].ttf"),
            &context(),
        )
        .expect("filters build")
        .remove(0);
        assert!(
            filter.starts_with("drawtext=fontfile='/fonts/od\\:d\\,name\\[1\\].ttf':"),
            "{filter}"
        );
    }

    #[test]
    fn a_missing_default_font_is_a_typed_error() {
        assert_eq!(
            first_existing_font(&["/definitely/not/a/font.ttf"])
                .ok_or(TitleRenderError::DefaultFontMissing),
            Err(TitleRenderError::DefaultFontMissing)
        );
    }

    #[test]
    fn missing_freetype_is_a_typed_error_instead_of_a_rejected_graph() {
        assert_eq!(require_text_rendering(true), Ok(()));
        assert_eq!(
            require_text_rendering(false),
            Err(TitleRenderError::TextRenderingUnavailable)
        );
    }

    #[test]
    fn an_unresolved_font_asset_fails_the_render() {
        let ctx = context();
        assert!(resolve_font(Some("ast_missing"), &ctx).is_err());

        let directory = tempfile::tempdir().expect("temp dir");
        let ctx = context().with_asset("ast_font", directory.path().join("absent.ttf"));
        let error = resolve_font(Some("ast_font"), &ctx).expect_err("unreadable font");
        assert_eq!(
            error.downcast_ref::<TitleRenderError>(),
            Some(&TitleRenderError::FontAssetUnreadable(
                "ast_font".to_owned()
            ))
        );
    }

    #[test]
    fn the_subtitle_style_string_carries_every_validated_option() {
        let spec = SubtitlesSpec::from_wire(&model::Subtitles {
            asset_id: "ast_0123456789abcdef".into(),
            burn_in: true,
            font_size: 24.0,
            color: "#FFCC00".into(),
            outline_width: 2.0,
            position: "top".into(),
            margin_v: 40.0,
        })
        .expect("valid");
        let filter = subtitles_filter(&spec, Path::new("/assets/sub:1.srt")).expect("filter");

        assert_eq!(
            filter,
            "subtitles=filename='/assets/sub\\:1.srt':force_style='FontSize=24,PrimaryColour=&H0000CCFF&,OutlineColour=&H00000000&,BorderStyle=1,Outline=2,Alignment=8,MarginV=40'"
        );
    }

    #[test]
    fn bottom_subtitles_use_the_bottom_alignment() {
        let spec = SubtitlesSpec::from_wire(&model::Subtitles {
            asset_id: "ast_0123456789abcdef".into(),
            burn_in: true,
            font_size: 24.0,
            color: "#FFFFFF".into(),
            outline_width: 0.0,
            position: "bottom".into(),
            margin_v: 10.0,
        })
        .expect("valid");
        let filter = subtitles_filter(&spec, Path::new("/a.srt")).expect("filter");
        assert!(filter.contains("Alignment=2,MarginV=10"), "{filter}");
    }

    #[test]
    fn subtitle_documents_are_parsed_before_ffmpeg_sees_them() {
        let srt = "1\n00:00:01,000 --> 00:00:03,500\nПривет\n";
        assert_eq!(parse_subtitle_document(srt), Ok(SubtitleFormat::SubRip));

        let vtt = "WEBVTT\n\n00:01.000 --> 00:03.000 line:90%\nHello\n";
        assert_eq!(parse_subtitle_document(vtt), Ok(SubtitleFormat::WebVtt));

        for broken in [
            "",
            "not a subtitle file at all",
            "1\n00:00:0x,000 --> 00:00:03,000\ntext\n",
            "1\n00:00:05,000 --> 00:00:03,000\ntext\n",
            "1\n00:00:01,000 --> \ntext\n",
        ] {
            assert!(
                matches!(
                    parse_subtitle_document(broken),
                    Err(TitleRenderError::SubtitleMalformed(_))
                ),
                "{broken:?}"
            );
        }
    }

    #[test]
    fn timestamps_accept_both_subrip_and_webvtt_spellings() {
        assert_eq!(parse_timestamp("00:00:01,500"), Some(1.5));
        assert_eq!(parse_timestamp("01:02:03.250"), Some(3723.25));
        assert_eq!(parse_timestamp("00:02.000"), Some(2.0));
        for broken in ["", "1", "00:00:01,", "aa:bb:cc,000", "1:2:3:4.0"] {
            assert_eq!(parse_timestamp(broken), None, "{broken:?}");
        }
    }

    #[test]
    fn a_burned_in_subtitle_asset_is_chained_after_the_titles() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("cues.srt");
        let mut file = fs::File::create(&path).expect("temp file");
        file.write_all("1\n00:00:00,000 --> 00:00:02,000\nПривет\n".as_bytes())
            .expect("write cues");

        let spec = spec(serde_json::json!({
            "videoId": "x",
            "subtitles": { "assetId": "ast_0123456789abcdef", "burnIn": true }
        }));
        let ctx = context().with_asset("ast_0123456789abcdef", &path);
        let mut plan = plan();
        apply(&mut plan, &spec, &ctx).expect("stage runs");

        assert!(!plan.is_trivial());
        let rendered = plan.render();
        assert!(
            rendered.starts_with("[0:v]subtitles=filename='"),
            "{rendered}"
        );
        assert!(rendered.ends_with("[subtitles_1]"), "{rendered}");
        assert_eq!(plan.video_label(), "subtitles_1");
    }

    #[test]
    fn subtitles_that_are_not_burned_in_leave_the_video_untouched() {
        let spec = spec(serde_json::json!({
            "videoId": "x",
            "subtitles": { "assetId": "ast_0123456789abcdef", "burnIn": false }
        }));
        let mut plan = plan();
        apply(&mut plan, &spec, &context()).expect("stage runs");
        assert!(plan.is_trivial());
    }

    #[test]
    fn a_malformed_subtitle_asset_fails_the_render() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("cues.srt");
        let mut file = fs::File::create(&path).expect("temp file");
        file.write_all(b"this is not a subtitle document")
            .expect("write");

        let error = load_subtitle_document(&path).expect_err("malformed");
        assert!(matches!(error, TitleRenderError::SubtitleMalformed(_)));

        assert!(matches!(
            load_subtitle_document(directory.path()),
            Err(TitleRenderError::SubtitleUnreadable(_))
        ));
    }

    #[test]
    fn titles_and_subtitles_chain_onto_the_plan_in_stage_order() {
        let directory = tempfile::tempdir().expect("temp dir");
        let font = directory.path().join("Font.ttf");
        fs::write(&font, b"not really a font").expect("write font");
        let cues = directory.path().join("cues.srt");
        fs::write(&cues, b"1\n00:00:00,000 --> 00:00:02,000\nHi\n").expect("write cues");

        let spec = spec(serde_json::json!({
            "videoId": "x",
            "titles": [{
                "text": "Заголовок",
                "fontAssetId": "ast_font0123456789",
                "x": 0.5,
                "y": 0.85
            }],
            "subtitles": { "assetId": "ast_sub0123456789a", "burnIn": true }
        }));
        let ctx = context()
            .with_asset("ast_font0123456789", &font)
            .with_asset("ast_sub0123456789a", &cues);
        let mut plan = plan();
        apply(&mut plan, &spec, &ctx).expect("stage runs");

        let rendered = plan.render();
        let titles_index = rendered.find("drawtext=").expect("titles chained");
        let subtitles_index = rendered.find("subtitles=").expect("subtitles chained");
        assert!(titles_index < subtitles_index, "{rendered}");
        assert!(rendered.starts_with("[0:v]drawtext="), "{rendered}");
        assert!(
            rendered.contains("[titles_1];[titles_1]subtitles="),
            "{rendered}"
        );
        assert_eq!(plan.video_label(), "subtitles_2");
        // The audio branch is none of this stage's business.
        assert_eq!(plan.audio_label(), Some("0:a"));
    }
}
