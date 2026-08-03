//! Offline render service contract: immutable edit plan plus resource policy.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::encode_budget::EncodeBudget;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::color_management::{
    ColorManagementStatusV1, ColorMatrixV1, ColorPrimariesV1, ColorProvenanceV1, ColorRangeV1,
    ColorTransferV1, PixelModelV1, SdrColorDescriptorV1, SdrColorPolicyV1,
    UnsupportedColorReasonV1,
};
use crate::domain::edit::{
    AspectRatio, AudioEffects, CensorColor, CensorSpec, ColorWheel, ColorWheels, EditSpec,
    GeometrySpec, HslAdjustment, HslSelection, HslSelective, LookPreset, LutGrade, OutputScale,
    PixelRect, Rotation, TimeRange, TimingSpec, ToneCurve, ToneCurvePoint, ToneCurves,
    VideoEffects,
};
use crate::domain::output::{OutputFormat, OutputSpec, VideoCodec};
use crate::model::{Crop, EditRequest, RateControlRequest, Scale, Trim};

const EDIT_PLAN_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceMediaMetadata {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: f64,
    pub has_audio: bool,
    pub fps: Option<f64>,
    pub color_policy: Option<SdrColorPolicyV1>,
    pub color_policy_warning: bool,
}

impl SourceMediaMetadata {
    fn legacy_bt709_policy() -> SdrColorPolicyV1 {
        SdrColorPolicyV1::new(SdrColorDescriptorV1 {
            primaries: ColorPrimariesV1::Bt709,
            transfer: ColorTransferV1::Bt709,
            matrix: ColorMatrixV1::Bt709,
            range: ColorRangeV1::Limited,
            pixel_model: PixelModelV1::Yuv,
            chroma_location: None,
        })
        .expect("legacy BT.709 policy is valid")
    }

    pub fn new(width: u32, height: u32, duration_seconds: f64) -> anyhow::Result<Self> {
        Self::new_with_audio(width, height, duration_seconds, true)
    }

    pub fn new_with_audio(
        width: u32,
        height: u32,
        duration_seconds: f64,
        has_audio: bool,
    ) -> anyhow::Result<Self> {
        if !duration_seconds.is_finite() || duration_seconds < 0.0 {
            anyhow::bail!("Недопустимая длительность источника");
        }
        Ok(Self {
            width,
            height,
            duration_seconds,
            has_audio,
            fps: None,
            color_policy: (width > 0).then(Self::legacy_bt709_policy),
            color_policy_warning: width > 0,
        })
    }

    pub fn from_probe(probe: &crate::domain::media_probe::ProbeResult) -> anyhow::Result<Self> {
        let has_audio = probe
            .streams
            .iter()
            .any(|stream| stream.kind == crate::domain::media_probe::StreamKind::Audio);
        let mut value = Self::new_with_audio(probe.width, probe.height, probe.duration, has_audio)?;
        value.fps = probe.fps;
        if probe.width == 0 {
            return Ok(value);
        }
        match resolved_source_color_management(probe) {
            Some(ColorManagementStatusV1::Supported {
                descriptor,
                provenance: ColorProvenanceV1::Signaled,
            }) => {
                value.color_policy = Some(SdrColorPolicyV1::new(descriptor).map_err(|_| {
                    anyhow::anyhow!("Противоречивые параметры цветового пространства")
                })?);
                value.color_policy_warning = false;
            }
            Some(ColorManagementStatusV1::Supported {
                descriptor,
                provenance: ColorProvenanceV1::LegacyAssumedBt709,
            }) => {
                value.color_policy = Some(SdrColorPolicyV1::new(descriptor).map_err(|_| {
                    anyhow::anyhow!("Противоречивые параметры цветового пространства")
                })?);
                value.color_policy_warning = true;
            }
            Some(ColorManagementStatusV1::Unsupported { reason }) => {
                anyhow::bail!("Неподдерживаемое цветовое пространство источника: {reason:?}")
            }
            Some(ColorManagementStatusV1::NotApplicable) | None => {
                anyhow::bail!("У видео отсутствует цветовой descriptor")
            }
            Some(ColorManagementStatusV1::Supported { .. }) => {}
        }
        Ok(value)
    }

    pub fn for_audio_extraction(
        probe: &crate::domain::media_probe::ProbeResult,
    ) -> anyhow::Result<Self> {
        let has_audio = probe
            .streams
            .iter()
            .any(|stream| stream.kind == crate::domain::media_probe::StreamKind::Audio);
        let mut value = Self::new_with_audio(probe.width, probe.height, probe.duration, has_audio)?;
        value.color_policy = None;
        value.color_policy_warning = false;
        Ok(value)
    }
}

/// Public/source metadata must describe the exact policy that rendering will
/// consume. Legacy files with absent signalling therefore expose the explicit
/// warned Rec.709 assumption instead of the raw missing-field probe result.
pub fn resolved_source_color_management(
    probe: &crate::domain::media_probe::ProbeResult,
) -> Option<ColorManagementStatusV1> {
    if probe.width == 0 {
        return Some(ColorManagementStatusV1::NotApplicable);
    }
    let legacy_yuv = probe
        .streams
        .iter()
        .find(|stream| stream.kind == crate::domain::media_probe::StreamKind::Video)
        .and_then(|stream| stream.color.pixel_format.as_deref())
        .is_some_and(|format| format.starts_with("yuv") || format.starts_with("nv"));
    match probe.primary_video_color_management() {
        Some(ColorManagementStatusV1::Unsupported { reason })
            if legacy_yuv
                && matches!(
                    reason,
                    UnsupportedColorReasonV1::MissingRange
                        | UnsupportedColorReasonV1::MissingMatrix
                        | UnsupportedColorReasonV1::MissingTransfer
                        | UnsupportedColorReasonV1::MissingPrimaries
                ) =>
        {
            Some(ColorManagementStatusV1::Supported {
                descriptor: SourceMediaMetadata::legacy_bt709_policy().source,
                provenance: ColorProvenanceV1::LegacyAssumedBt709,
            })
        }
        Some(status) => Some(status.clone()),
        None => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMediaSpec {
    pub width: u32,
    pub height: u32,
    pub duration_micros: u64,
    pub has_audio: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_policy: Option<SdrColorPolicyV1>,
    #[serde(default)]
    pub color_policy_warning: bool,
}

impl SourceMediaSpec {
    fn from_metadata(value: SourceMediaMetadata) -> anyhow::Result<Self> {
        let duration_micros = value.duration_seconds * 1_000_000.0;
        if duration_micros > u64::MAX as f64 {
            anyhow::bail!("Длительность источника слишком велика");
        }
        Ok(Self {
            width: value.width,
            height: value.height,
            duration_micros: duration_micros.round() as u64,
            has_audio: value.has_audio,
            color_policy: value.color_policy,
            color_policy_warning: value.color_policy_warning,
        })
    }

    pub fn duration_seconds(self) -> f64 {
        self.duration_micros as f64 / 1_000_000.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditPlan {
    pub schema_version: u32,
    pub source_fingerprint: Fingerprint,
    pub source: SourceMediaSpec,
    pub plan_fingerprint: Fingerprint,
    pub output: OutputSpec,
    pub edit: EditSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EditPlanWire {
    schema_version: u32,
    source_fingerprint: Fingerprint,
    source: SourceMediaSpec,
    plan_fingerprint: Fingerprint,
    output: OutputSpec,
    edit: EditSpec,
}

impl<'de> Deserialize<'de> for EditPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = EditPlanWire::deserialize(deserializer)?;
        if wire.schema_version != EDIT_PLAN_SCHEMA_VERSION {
            return Err(D::Error::custom("unsupported edit plan schema"));
        }
        let compiled =
            Self::from_domain(wire.source_fingerprint, wire.source, wire.edit, wire.output)
                .map_err(D::Error::custom)?;
        if wire.plan_fingerprint != compiled.plan_fingerprint {
            return Err(D::Error::custom(
                "edit plan identity does not match its source, edit, and output",
            ));
        }
        Ok(compiled)
    }
}

impl EditPlan {
    pub fn compile(
        source_fingerprint: Fingerprint,
        mut request: EditRequest,
        metadata: SourceMediaMetadata,
    ) -> anyhow::Result<Self> {
        let source_fps = metadata.fps.unwrap_or(30.0);
        let source = SourceMediaSpec::from_metadata(metadata)?;
        let rate_control = request.rate_control.clone();
        if request.quality.is_some() && rate_control.is_some() {
            anyhow::bail!("quality and rateControl are mutually exclusive");
        }
        if let Some(RateControlRequest::Quality { crf }) = rate_control.as_ref() {
            request.quality = Some(*crf);
        }
        normalize_request(&mut request, source)?;
        let (edit, mut output) = map_request(request)?;
        if matches!(rate_control, Some(RateControlRequest::Quality { .. })) {
            anyhow::ensure!(
                matches!(
                    output.video_codec,
                    Some(VideoCodec::H264 | VideoCodec::H265 | VideoCodec::Vp9 | VideoCodec::Av1)
                ),
                "quality rateControl is unavailable for the selected format"
            );
        }
        if !source.has_audio {
            if output.format == OutputFormat::Mp3 {
                anyhow::bail!("Источник не содержит аудиодорожку");
            }
            output.audio_codec = None;
            output.audio_bitrate_bps = None;
        }
        if let Some(RateControlRequest::TargetSize {
            target_bytes,
            video_bitrate_bps,
            audio_bitrate_bps,
            estimator_version,
        }) = rate_control
        {
            anyhow::ensure!(
                estimator_version == "size-v1",
                "unsupported target-size estimator"
            );
            let duration = crate::tools::expected_output_secs(&edit, source.duration_seconds());
            let geometry = edit.geometry();
            let (mut width, mut height) = (source.width, source.height);
            if let Some(scale) = geometry.scale {
                match (scale.width, scale.height) {
                    (w, h) if w > 0 && h > 0 => (width, height) = (w as u32, h as u32),
                    (w, _) if w > 0 => {
                        width = w as u32;
                        height = (f64::from(source.height) * f64::from(width)
                            / f64::from(source.width))
                        .round() as u32;
                    }
                    (_, h) if h > 0 => {
                        height = h as u32;
                        width = (f64::from(source.width) * f64::from(height)
                            / f64::from(source.height))
                        .round() as u32;
                    }
                    _ => {}
                }
            }
            if matches!(
                geometry.rotation,
                crate::domain::edit::Rotation::Clockwise90
                    | crate::domain::edit::Rotation::Clockwise270
            ) {
                std::mem::swap(&mut width, &mut height);
            }
            if let Some(aspect) = geometry.pad_aspect {
                width = width.max(
                    (f64::from(height) * f64::from(aspect.width) / f64::from(aspect.height)).ceil()
                        as u32,
                );
                height = height.max(
                    (f64::from(width) * f64::from(aspect.height) / f64::from(aspect.width)).ceil()
                        as u32,
                );
            }
            output.apply_target_size(
                target_bytes,
                Some(audio_bitrate_bps),
                duration,
                width,
                height,
                output.fps().unwrap_or(source_fps),
            )?;
            anyhow::ensure!(
                output.video_bitrate_bps == Some(video_bitrate_bps),
                "target-size derived video bitrate mismatch"
            );
        }
        Self::from_domain(source_fingerprint, source, edit, output)
    }

    fn from_domain(
        source_fingerprint: Fingerprint,
        source: SourceMediaSpec,
        edit: EditSpec,
        output: OutputSpec,
    ) -> anyhow::Result<Self> {
        edit.validate()?;
        output.validate()?;
        let expected_dimensions = edit
            .geometry()
            .scale
            .map(|value| (Some(value.width), Some(value.height)))
            .unwrap_or((None, None));
        if (output.width, output.height) != expected_dimensions {
            anyhow::bail!("edit plan output dimensions do not match geometry");
        }
        let supports_audio = matches!(
            output.format,
            OutputFormat::Mp4
                | OutputFormat::Webm
                | OutputFormat::Mp3
                | OutputFormat::Av1
                | OutputFormat::Prores
        );
        let expects_audio = supports_audio && source.has_audio && !edit.audio().muted;
        if output.audio_codec.is_some() != expects_audio {
            anyhow::bail!("edit plan audio output does not match mute semantics");
        }
        if output.format != OutputFormat::Mp3 && source.width > 0 && source.color_policy.is_none() {
            anyhow::bail!("visual edit plan is missing its SDR color policy");
        }
        let canonical = serde_json::to_vec(&edit).expect("EditSpec serialization cannot fail");
        let canonical_output =
            serde_json::to_vec(&output).expect("OutputSpec serialization cannot fail");
        let canonical_source =
            serde_json::to_vec(&source).expect("SourceMediaSpec serialization cannot fail");
        let schema = EDIT_PLAN_SCHEMA_VERSION.to_be_bytes();
        let plan_fingerprint = Fingerprint::combine([
            b"edit-plan".as_slice(),
            schema.as_slice(),
            source_fingerprint.as_str().as_bytes(),
            canonical_source.as_slice(),
            canonical.as_slice(),
            canonical_output.as_slice(),
        ]);
        Ok(Self {
            schema_version: EDIT_PLAN_SCHEMA_VERSION,
            source_fingerprint,
            source,
            plan_fingerprint,
            output,
            edit,
        })
    }
}

fn normalize_request(edit: &mut EditRequest, source: SourceMediaSpec) -> anyhow::Result<()> {
    let duration = source.duration_seconds();
    edit.speed = finite_positive(edit.speed, "Недопустимая скорость")?.clamp(0.5, 2.0);
    edit.volume = finite_non_negative(edit.volume, "Недопустимая громкость")?.clamp(0.0, 4.0);
    edit.fade_in = finite_non_negative(edit.fade_in, "Недопустимое появление")?.min(duration);
    edit.fade_out = finite_non_negative(edit.fade_out, "Недопустимое затухание")?.min(duration);
    edit.brightness = finite_number(edit.brightness, "Недопустимая яркость")?.clamp(-1.0, 1.0);
    edit.contrast = finite_non_negative(edit.contrast, "Недопустимый контраст")?.clamp(0.0, 3.0);
    edit.saturation =
        finite_non_negative(edit.saturation, "Недопустимая насыщенность")?.clamp(0.0, 3.0);
    edit.temperature = finite_number(edit.temperature, "Недопустимая температура")?;
    edit.tint = finite_number(edit.tint, "Недопустимый оттенок")?;
    edit.highlights = finite_number(edit.highlights, "Недопустимые света")?;
    edit.shadows = finite_number(edit.shadows, "Недопустимые тени")?;
    if edit.color_wheels.is_some_and(|value| value.is_neutral()) {
        edit.color_wheels = None;
    }
    if let Some(mut selective) = edit.hsl_selective {
        let selection = selective.selection;
        let adjustment = selective.adjustment;
        anyhow::ensure!(
            selection.center_degrees.is_finite()
                && (0.0..=360.0).contains(&selection.center_degrees)
                && selection.half_width_degrees.is_finite()
                && (0.0..=180.0).contains(&selection.half_width_degrees)
                && selection.feather_degrees.is_finite()
                && (0.0..=90.0).contains(&selection.feather_degrees)
                && selection.half_width_degrees + selection.feather_degrees <= 180.0
                && adjustment.hue_degrees.is_finite()
                && (-180.0..=180.0).contains(&adjustment.hue_degrees)
                && adjustment.saturation.is_finite()
                && (-1.0..=1.0).contains(&adjustment.saturation)
                && adjustment.lightness.is_finite()
                && (-1.0..=1.0).contains(&adjustment.lightness),
            "Недопустимые параметры selective HSL"
        );
        if selective.selection.center_degrees == 360.0 {
            selective.selection.center_degrees = 0.0;
            edit.hsl_selective = Some(selective);
        }
        if selective.is_neutral() {
            edit.hsl_selective = None;
        }
    }
    edit.sharpen = finite_non_negative(edit.sharpen, "Недопустимая резкость")?.clamp(0.0, 5.0);
    edit.grain = finite_non_negative(edit.grain, "Недопустимое зерно")?.clamp(0.0, 100.0);
    normalize_trim(&mut edit.trim, duration)?;
    normalize_segments(&mut edit.segments, duration)?;

    if let Some(crop) = edit.crop.as_mut() {
        clamp_rect_to_source(crop, source.width, source.height);
    }
    if let Some(censor) = edit.censor.as_mut() {
        clamp_rect_to_source(censor, source.width, source.height);
    }
    if let Some(fps) = edit.fps {
        if !fps.is_finite() || fps <= 0.0 {
            anyhow::bail!("Недопустимый fps");
        }
        edit.fps = Some(fps.clamp(1.0, 240.0));
    }
    if let Some(scale) = &edit.scale {
        validate_scale(scale)?;
    }
    Ok(())
}

fn finite_number(value: f64, message: &str) -> anyhow::Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        anyhow::bail!(message.to_owned())
    }
}

fn finite_non_negative(value: f64, message: &str) -> anyhow::Result<f64> {
    let value = finite_number(value, message)?;
    if value >= 0.0 {
        Ok(value)
    } else {
        anyhow::bail!(message.to_owned())
    }
}

fn finite_positive(value: f64, message: &str) -> anyhow::Result<f64> {
    let value = finite_number(value, message)?;
    if value > 0.0 {
        Ok(value)
    } else {
        anyhow::bail!(message.to_owned())
    }
}

fn normalize_trim(trim: &mut Option<Trim>, duration: f64) -> anyhow::Result<()> {
    let Some(trim) = trim.as_mut() else {
        return Ok(());
    };
    trim.start = finite_non_negative(trim.start, "Недопустимое начало обрезки")?.min(duration);
    trim.end = finite_non_negative(trim.end, "Недопустимый конец обрезки")?.min(duration);
    if trim.end - trim.start <= 0.01 {
        anyhow::bail!("Недопустимый диапазон обрезки");
    }
    Ok(())
}

fn normalize_segments(segments: &mut Option<Vec<Trim>>, duration: f64) -> anyhow::Result<()> {
    let Some(current) = segments.as_mut() else {
        return Ok(());
    };
    let mut normalized = Vec::with_capacity(current.len());
    for segment in current.iter() {
        let start =
            finite_non_negative(segment.start, "Недопустимое начало сегмента")?.min(duration);
        let end = finite_non_negative(segment.end, "Недопустимый конец сегмента")?.min(duration);
        if end - start > 0.01 {
            normalized.push(Trim { start, end });
        }
    }
    normalized.sort_by(|left, right| left.start.total_cmp(&right.start));
    for pair in normalized.windows(2) {
        if pair[1].start < pair[0].end {
            anyhow::bail!("Сегменты не должны пересекаться");
        }
    }
    *segments = (!normalized.is_empty()).then_some(normalized);
    Ok(())
}

fn clamp_rect_to_source(rect: &mut Crop, source_width: u32, source_height: u32) {
    if source_width == 0 || source_height == 0 {
        return;
    }
    let min_width = if source_width >= 2 { 2 } else { 1 };
    let min_height = if source_height >= 2 { 2 } else { 1 };
    rect.x = rect.x.min(source_width - min_width);
    rect.y = rect.y.min(source_height - min_height);
    rect.w = rect.w.clamp(min_width, source_width - rect.x);
    rect.h = rect.h.clamp(min_height, source_height - rect.y);
    if min_width == 2 {
        rect.w = (rect.w & !1).max(2);
    }
    if min_height == 2 {
        rect.h = (rect.h & !1).max(2);
    }
}

fn validate_scale(scale: &Scale) -> anyhow::Result<()> {
    let valid_dimension = |value| matches!(value, -2 | -1) || (2..=7680).contains(&value);
    if !valid_dimension(scale.w) || !valid_dimension(scale.h) || (scale.w < 0 && scale.h < 0) {
        anyhow::bail!("Недопустимый размер экспорта");
    }
    Ok(())
}

fn map_request(request: EditRequest) -> anyhow::Result<(EditSpec, OutputSpec)> {
    let map_wheel = |wheel: crate::model::ColorWheelRequest| ColorWheel {
        master: wheel.master,
        red: wheel.red,
        green: wheel.green,
        blue: wheel.blue,
    };
    let color_wheels = request.color_wheels.map(|wheels| ColorWheels {
        lift: map_wheel(wheels.lift),
        gamma: map_wheel(wheels.gamma),
        gain: map_wheel(wheels.gain),
    });
    let hsl_selective = request.hsl_selective.map(|selective| HslSelective {
        selection: HslSelection {
            center_degrees: selective.selection.center_degrees,
            half_width_degrees: selective.selection.half_width_degrees,
            feather_degrees: selective.selection.feather_degrees,
        },
        adjustment: HslAdjustment {
            hue_degrees: selective.adjustment.hue_degrees,
            saturation: selective.adjustment.saturation,
            lightness: selective.adjustment.lightness,
        },
    });
    let trim = request
        .trim
        .map(|value| TimeRange::new(value.start, value.end))
        .transpose()?;
    let segments = request
        .segments
        .unwrap_or_default()
        .into_iter()
        .map(|value| TimeRange::new(value.start, value.end))
        .collect::<Result<Vec<_>, _>>()?;
    let scale = request.scale.map(|value| OutputScale {
        width: value.w,
        height: value.h,
    });
    let crop = request.crop.map(|value| PixelRect {
        x: value.x,
        y: value.y,
        width: value.w,
        height: value.h,
    });
    let censor_color = CensorColor::parse(request.censor_color.as_deref())?;
    let censor = request.censor.map(|value| CensorSpec {
        rect: PixelRect {
            x: value.x,
            y: value.y,
            width: value.w,
            height: value.h,
        },
        color: censor_color,
    });
    let look = request
        .filter
        .as_deref()
        .map(LookPreset::parse)
        .transpose()?;
    let curves = request
        .curves
        .map(|value| {
            Ok::<_, anyhow::Error>(ToneCurves::new(
                value.master.map(map_curve).transpose()?,
                value.red.map(map_curve).transpose()?,
                value.green.map(map_curve).transpose()?,
                value.blue.map(map_curve).transpose()?,
            ))
        })
        .transpose()?
        .flatten();
    let lut = request
        .lut
        .map(|value| LutGrade::new(value.id, value.intensity))
        .transpose()?
        .flatten();
    let pad_aspect = request.pad.as_deref().map(AspectRatio::parse).transpose()?;
    let edit = EditSpec::new(
        TimingSpec {
            trim,
            segments,
            speed: request.speed,
            reverse: request.reverse,
            fade_in_seconds: request.fade_in,
            fade_out_seconds: request.fade_out,
        },
        GeometrySpec {
            crop,
            scale,
            rotation: Rotation::from_degrees(request.rotate)?,
            flip_horizontal: request.flip_h,
            flip_vertical: request.flip_v,
            pad_aspect,
            censor,
        },
        VideoEffects {
            brightness: request.brightness,
            contrast: request.contrast,
            saturation: request.saturation,
            temperature: request.temperature,
            tint: request.tint,
            highlights: request.highlights,
            shadows: request.shadows,
            color_wheels,
            hsl_selective,
            look,
            vignette: request.vignette,
            denoise: request.denoise,
            sharpen: request.sharpen,
            grain: request.grain,
            curves,
            lut,
        },
        AudioEffects {
            muted: request.mute,
            volume: request.volume,
            normalize: request.normalize_audio,
            highpass: request.highpass,
        },
    )?;
    let format = OutputFormat::parse(request.format.as_deref())?;
    let codec = match (format, request.codec.as_deref()) {
        (OutputFormat::Mp4, value) => Some(VideoCodec::parse_mp4(value)?),
        (_, Some(_)) => anyhow::bail!("Кодек можно задавать только для MP4"),
        (_, None) => None,
    };
    let output = OutputSpec::new(
        format,
        codec,
        request.mute,
        request.quality,
        request.fps,
        scale,
    )?;
    Ok((edit, output))
}

fn map_curve(points: Vec<crate::model::CurvePoint>) -> anyhow::Result<ToneCurve> {
    ToneCurve::new(
        points
            .into_iter()
            .map(|point| ToneCurvePoint::new(point.x, point.y))
            .collect(),
    )
    .map_err(Into::into)
}

/// Validate the wire-level colour payload before it is persisted as durable
/// work. Source-dependent geometry is still validated when the full edit plan
/// is compiled, but LUT/curve bounds do not need media metadata.
pub fn validate_color_grade_request(request: &EditRequest) -> anyhow::Result<()> {
    if let Some(selective) = request.hsl_selective {
        let selection = selective.selection;
        let adjustment = selective.adjustment;
        anyhow::ensure!(
            selection.center_degrees.is_finite()
                && (0.0..=360.0).contains(&selection.center_degrees)
                && selection.half_width_degrees.is_finite()
                && (0.0..=180.0).contains(&selection.half_width_degrees)
                && selection.feather_degrees.is_finite()
                && (0.0..=90.0).contains(&selection.feather_degrees)
                && selection.half_width_degrees + selection.feather_degrees <= 180.0
                && adjustment.hue_degrees.is_finite()
                && (-180.0..=180.0).contains(&adjustment.hue_degrees)
                && adjustment.saturation.is_finite()
                && (-1.0..=1.0).contains(&adjustment.saturation)
                && adjustment.lightness.is_finite()
                && (-1.0..=1.0).contains(&adjustment.lightness),
            "Недопустимые параметры selective HSL"
        );
    }
    if let Some(curves) = &request.curves {
        let validated = ToneCurves::new(
            curves.master.clone().map(map_curve).transpose()?,
            curves.red.clone().map(map_curve).transpose()?,
            curves.green.clone().map(map_curve).transpose()?,
            curves.blue.clone().map(map_curve).transpose()?,
        );
        anyhow::ensure!(validated.is_some(), "Кривые не содержат ни одного канала");
    }
    if let Some(lut) = &request.lut {
        LutGrade::new(lut.id.clone(), lut.intensity)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportExecutionProfile {
    pub encode_budget: EncodeBudget,
    pub verify_checksums: bool,
}

/// Private filesystem resources resolved by an HTTP/application adapter. Asset
/// ids stay in the immutable edit plan; only this execution envelope owns paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderResources {
    lut_path: Option<PathBuf>,
    lut_sha256: Option<String>,
}

impl RenderResources {
    pub fn with_lut_path(path: impl Into<PathBuf>) -> Self {
        Self {
            lut_path: Some(path.into()),
            lut_sha256: None,
        }
    }

    pub fn with_verified_lut(path: impl Into<PathBuf>, sha256: String) -> Self {
        Self {
            lut_path: Some(path.into()),
            lut_sha256: Some(sha256),
        }
    }

    pub fn lut_path(&self) -> Option<&Path> {
        self.lut_path.as_deref()
    }

    pub fn lut_sha256(&self) -> Option<&str> {
        self.lut_sha256.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct RenderExecution {
    plan: Arc<EditPlan>,
    pub profile: ExportExecutionProfile,
    resources: RenderResources,
}

impl RenderExecution {
    pub fn new(plan: Arc<EditPlan>, profile: ExportExecutionProfile) -> Self {
        Self {
            plan,
            profile,
            resources: RenderResources::default(),
        }
    }

    pub fn new_with_resources(
        plan: Arc<EditPlan>,
        profile: ExportExecutionProfile,
        resources: RenderResources,
    ) -> Self {
        Self {
            plan,
            profile,
            resources,
        }
    }

    pub fn plan(&self) -> &EditPlan {
        &self.plan
    }

    pub fn output(&self) -> &OutputSpec {
        &self.plan.output
    }

    pub fn resources(&self) -> &RenderResources {
        &self.resources
    }
}

#[cfg(test)]
mod tests {
    use crate::config::encode_budget::{EncodeProfile, RuntimeLimits};

    use super::*;

    fn source() -> SourceMediaMetadata {
        SourceMediaMetadata::new(1920, 1080, 10.0).unwrap()
    }

    fn compile(value: serde_json::Value) -> anyhow::Result<EditPlan> {
        let request = serde_json::from_value(value)?;
        EditPlan::compile(Fingerprint::digest(b"source"), request, source())
    }

    #[test]
    fn compiler_clamps_source_dependent_geometry_and_fps() {
        let plan = compile(serde_json::json!({
            "videoId": "x",
            "crop": { "x": 9999, "y": 9999, "w": 0, "h": 9999 },
            "censor": { "x": 1919, "y": 1079, "w": 20, "h": 20 },
            "fps": 500.0,
            "scale": { "w": 1280, "h": -2 }
        }))
        .unwrap();

        let crop = plan.edit.geometry().crop.unwrap();
        assert_eq!(
            (crop.x, crop.y, crop.width, crop.height),
            (1918, 1078, 2, 2)
        );
        let censor = plan.edit.geometry().censor.as_ref().unwrap().rect;
        assert_eq!(
            (censor.x, censor.y, censor.width, censor.height),
            (1918, 1078, 2, 2)
        );
        assert_eq!(plan.output.fps_milli, Some(240_000));
    }

    #[test]
    fn compiler_recomputes_size_v1_and_rejects_client_drift() {
        let valid = serde_json::json!({
            "videoId": "x",
            "rateControl": { "mode": "target_size", "targetBytes": 10_000_000,
                "videoBitrateBps": 7_472_000, "audioBitrateBps": 128_000,
                "estimatorVersion": "size-v1" }
        });
        let plan = compile(valid.clone()).unwrap();
        assert_eq!(plan.output.video_bitrate_bps, Some(7_472_000));
        let args =
            crate::tools::build_ffmpeg_args(Path::new("in.mp4"), Path::new("out.mp4"), &plan);
        assert!(args.windows(2).any(|pair| pair == ["-b:v", "7472000"]));
        assert!(!args.iter().any(|arg| arg == "-crf"));
        let mut drifted = valid;
        drifted["rateControl"]["videoBitrateBps"] = serde_json::json!(7_471_000);
        assert!(compile(drifted)
            .unwrap_err()
            .to_string()
            .contains("mismatch"));
    }

    #[test]
    fn video_only_sources_disable_audio_and_reject_audio_only_exports() {
        let source = SourceMediaMetadata::new_with_audio(1920, 1080, 10.0, false).unwrap();
        let video = EditPlan::compile(
            Fingerprint::digest(b"video-only"),
            serde_json::from_value(serde_json::json!({
                "videoId": "x",
                "normalizeAudio": true
            }))
            .unwrap(),
            source,
        )
        .unwrap();
        assert!(!video.source.has_audio);
        assert!(video.output.audio_codec.is_none());

        let audio_only = EditPlan::compile(
            Fingerprint::digest(b"video-only"),
            serde_json::from_value(serde_json::json!({
                "videoId": "x",
                "format": "mp3"
            }))
            .unwrap(),
            source,
        );
        assert!(audio_only.is_err());
    }

    #[test]
    fn compiler_normalizes_timing_segments_and_effect_ranges() {
        let plan = compile(serde_json::json!({
            "videoId": "x",
            "speed": 3.5,
            "volume": 9.0,
            "fadeIn": 99.0,
            "fadeOut": 99.0,
            "brightness": 2.0,
            "contrast": 9.0,
            "saturation": 9.0,
            "temperature": 1.0,
            "tint": -1.0,
            "highlights": 1.0,
            "shadows": -1.0,
            "sharpen": 9.0,
            "grain": 999.0,
            "trim": { "start": 2.0, "end": 99.0 },
            "segments": [
                { "start": 9.5, "end": 99.0 },
                { "start": 0.0, "end": 1.0 },
                { "start": 2.0, "end": 2.0 }
            ]
        }))
        .unwrap();

        assert_eq!(plan.edit.timing().speed, 2.0);
        assert_eq!(plan.edit.audio().volume, 4.0);
        assert_eq!(plan.edit.timing().fade_in_seconds, 10.0);
        assert_eq!(plan.edit.timing().fade_out_seconds, 10.0);
        assert_eq!(plan.edit.video().brightness, 1.0);
        assert_eq!(plan.edit.video().contrast, 3.0);
        assert_eq!(plan.edit.video().saturation, 3.0);
        assert_eq!(plan.edit.video().temperature, 1.0);
        assert_eq!(plan.edit.video().tint, -1.0);
        assert_eq!(plan.edit.video().highlights, 1.0);
        assert_eq!(plan.edit.video().shadows, -1.0);
        assert_eq!(plan.edit.video().sharpen, 5.0);
        assert_eq!(plan.edit.video().grain, 100.0);
        assert_eq!(
            plan.edit.timing().trim.unwrap(),
            TimeRange::new(2.0, 10.0).unwrap()
        );
        assert_eq!(
            plan.edit.timing().segments,
            [
                TimeRange::new(0.0, 1.0).unwrap(),
                TimeRange::new(9.5, 10.0).unwrap()
            ]
        );
    }

    #[test]
    fn compiler_rejects_primary_corrections_outside_wire_range() {
        for (field, value) in [
            ("temperature", 1.000_001),
            ("tint", -1.000_001),
            ("highlights", 2.0),
            ("shadows", -2.0),
        ] {
            let mut request = serde_json::json!({"videoId": "x"});
            request[field] = serde_json::json!(value);
            assert!(
                compile(request).is_err(),
                "{field}={value} must be rejected"
            );
        }
    }

    #[test]
    fn compiler_canonicalizes_neutral_wheels_and_rejects_out_of_range_components() {
        let neutral = compile(serde_json::json!({
            "videoId": "x",
            "colorWheels": {"lift": {}, "gamma": {}, "gain": {}}
        }))
        .unwrap();
        assert!(neutral.edit.video().color_wheels.is_none());

        let extremes = compile(serde_json::json!({
            "videoId": "x",
            "colorWheels": {
                "lift": {"master": 1, "red": -1},
                "gamma": {"green": 1},
                "gain": {"blue": -1}
            }
        }))
        .unwrap();
        assert!(extremes.edit.video().color_wheels.is_some());

        for (wheel, channel, value) in [
            ("lift", "master", 1.000_001),
            ("gamma", "red", -1.000_001),
            ("gain", "green", 2.0),
            ("gain", "blue", -2.0),
        ] {
            let mut request = serde_json::json!({
                "videoId": "x",
                "colorWheels": {"lift": {}, "gamma": {}, "gain": {}}
            });
            request["colorWheels"][wheel][channel] = serde_json::json!(value);
            assert!(compile(request).is_err(), "{wheel}.{channel}={value}");
        }
    }

    #[test]
    fn compiler_validates_and_canonicalizes_selective_hsl() {
        let neutral = compile(serde_json::json!({
            "videoId": "x",
            "hslSelective": {
                "selection": {"centerDegrees": 120, "halfWidthDegrees": 45, "featherDegrees": 20},
                "adjustment": {}
            }
        }))
        .unwrap();
        assert!(neutral.edit.video().hsl_selective.is_none());

        let active = compile(serde_json::json!({
            "videoId": "x",
            "hslSelective": {
                "selection": {"centerDegrees": 359.999, "halfWidthDegrees": 90, "featherDegrees": 90},
                "adjustment": {"hueDegrees": -180, "saturation": 1, "lightness": -1}
            }
        }))
        .unwrap();
        assert!(active.edit.video().hsl_selective.is_some());
        let center_zero = compile(serde_json::json!({
            "videoId":"x", "hslSelective":{"selection":{"centerDegrees":0},"adjustment":{"hueDegrees":1}}
        }))
        .unwrap();
        let center_wrap = compile(serde_json::json!({
            "videoId":"x", "hslSelective":{"selection":{"centerDegrees":360},"adjustment":{"hueDegrees":1}}
        }))
        .unwrap();
        assert_eq!(center_zero.plan_fingerprint, center_wrap.plan_fingerprint);

        for request in [
            serde_json::json!({"videoId":"x","hslSelective":{"selection":{"centerDegrees":360.001},"adjustment":{"hueDegrees":1}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"selection":{"halfWidthDegrees":181},"adjustment":{"hueDegrees":1}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"selection":{"featherDegrees":91},"adjustment":{"hueDegrees":1}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"selection":{"halfWidthDegrees":100,"featherDegrees":81},"adjustment":{"hueDegrees":1}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"adjustment":{"hueDegrees":181}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"adjustment":{"saturation":1.001}}}),
            serde_json::json!({"videoId":"x","hslSelective":{"adjustment":{"lightness":-1.001}}}),
        ] {
            assert!(compile(request).is_err());
        }
    }

    #[test]
    fn compiler_rejects_bad_timing_fps_scale_and_non_finite_values() {
        for value in [
            serde_json::json!({"videoId": "x", "speed": 0.0}),
            serde_json::json!({"videoId": "x", "fps": -1.0}),
            serde_json::json!({"videoId": "x", "scale": {"w": -1, "h": -2}}),
            serde_json::json!({"videoId": "x", "trim": {"start": 5.0, "end": 5.0}}),
            serde_json::json!({
                "videoId": "x",
                "segments": [
                    {"start": 0.0, "end": 2.0},
                    {"start": 1.0, "end": 3.0}
                ]
            }),
        ] {
            assert!(compile(value).is_err());
        }

        let mut request: EditRequest =
            serde_json::from_value(serde_json::json!({"videoId": "x"})).unwrap();
        request.volume = f64::INFINITY;
        assert!(EditPlan::compile(Fingerprint::digest(b"source"), request, source()).is_err());
    }

    #[test]
    fn edit_plan_fingerprint_is_deterministic_and_source_sensitive() {
        let edit: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0}
        }))
        .unwrap();
        let source = Fingerprint::digest(b"source-a");
        let first = EditPlan::compile(source.clone(), edit.clone(), self::source()).unwrap();
        let second = EditPlan::compile(source.clone(), edit.clone(), self::source()).unwrap();
        let other = EditPlan::compile(
            Fingerprint::digest(b"source-b"),
            edit.clone(),
            self::source(),
        )
        .unwrap();
        assert_eq!(first.plan_fingerprint, second.plan_fingerprint);
        assert_ne!(first.plan_fingerprint, other.plan_fingerprint);

        let neutral: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0},
            "colorWheels": {"lift": {}, "gamma": {}, "gain": {}}
        }))
        .unwrap();
        let active: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0},
            "colorWheels": {"lift": {"red": 0.25}, "gamma": {}, "gain": {}}
        }))
        .unwrap();
        let baseline = EditPlan::compile(source.clone(), edit.clone(), self::source()).unwrap();
        let neutral = EditPlan::compile(source.clone(), neutral, self::source()).unwrap();
        let active = EditPlan::compile(source.clone(), active, self::source()).unwrap();
        assert_eq!(baseline.plan_fingerprint, neutral.plan_fingerprint);
        assert_ne!(baseline.plan_fingerprint, active.plan_fingerprint);

        let negative_zero: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0},
            "colorWheels": { "lift": { "red": -0.0 } }
        }))
        .unwrap();
        let tiny_nonzero: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "trim": {"start": 1.0, "end": 2.0},
            "colorWheels": { "lift": { "red": 5e-10 } }
        }))
        .unwrap();
        let negative_zero =
            EditPlan::compile(source.clone(), negative_zero, self::source()).unwrap();
        let tiny_nonzero = EditPlan::compile(source.clone(), tiny_nonzero, self::source()).unwrap();
        assert_eq!(baseline.plan_fingerprint, negative_zero.plan_fingerprint);
        assert_ne!(baseline.plan_fingerprint, tiny_nonzero.plan_fingerprint);
        assert!(tiny_nonzero.edit.video().color_wheels.is_some());

        let selective_neutral: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId":"source", "trim":{"start":1.0,"end":2.0},
            "hslSelective":{"selection":{"centerDegrees":240},"adjustment":{}}
        }))
        .unwrap();
        let selective_tiny: EditRequest = serde_json::from_value(serde_json::json!({
            "videoId":"source", "trim":{"start":1.0,"end":2.0},
            "hslSelective":{"selection":{"centerDegrees":240},"adjustment":{"saturation":5e-10}}
        }))
        .unwrap();
        let selective_neutral =
            EditPlan::compile(source.clone(), selective_neutral, self::source()).unwrap();
        let selective_tiny = EditPlan::compile(source, selective_tiny, self::source()).unwrap();
        assert_eq!(
            baseline.plan_fingerprint,
            selective_neutral.plan_fingerprint
        );
        assert_ne!(baseline.plan_fingerprint, selective_tiny.plan_fingerprint);
        assert!(selective_tiny.edit.video().hsl_selective.is_some());
    }

    #[test]
    fn source_metadata_is_immutable_plan_identity() {
        let request = || {
            serde_json::from_value(serde_json::json!({
                "videoId": "source",
                "fadeOut": 1.0
            }))
            .unwrap()
        };
        let fingerprint = Fingerprint::digest(b"same-source");
        let first = EditPlan::compile(
            fingerprint.clone(),
            request(),
            SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap();
        let changed = EditPlan::compile(
            fingerprint,
            request(),
            SourceMediaMetadata::new(1280, 720, 12.0).unwrap(),
        )
        .unwrap();

        assert_eq!(first.source.duration_seconds(), 10.0);
        assert_ne!(first.plan_fingerprint, changed.plan_fingerprint);

        let mut tampered = serde_json::to_value(first).unwrap();
        tampered["source"]["durationMicros"] = serde_json::json!(11_000_000_u64);
        assert!(serde_json::from_value::<EditPlan>(tampered).is_err());
    }

    #[test]
    fn transport_source_id_is_not_part_of_domain_plan() {
        let request = |video_id| {
            serde_json::from_value(serde_json::json!({
                "videoId": video_id,
                "trim": {"start": 1.0, "end": 2.0}
            }))
            .unwrap()
        };
        let source = Fingerprint::digest(b"same-immutable-source");
        let first = EditPlan::compile(source.clone(), request("wire-a"), self::source()).unwrap();
        let second = EditPlan::compile(source, request("wire-b"), self::source()).unwrap();

        assert_eq!(first.plan_fingerprint, second.plan_fingerprint);
        assert!(!serde_json::to_string(&first).unwrap().contains("videoId"));
    }

    #[test]
    fn compiler_rejects_unknown_or_out_of_range_semantics() {
        for value in [
            serde_json::json!({"videoId": "source", "format": "unknown"}),
            serde_json::json!({"videoId": "source", "filter": "unknown"}),
            serde_json::json!({"videoId": "source", "censorColor": "transparent"}),
            serde_json::json!({"videoId": "source", "format": "webm", "codec": "h265"}),
            serde_json::json!({"videoId": "source", "rotate": 45}),
            serde_json::json!({"videoId": "source", "quality": 99}),
            serde_json::json!({"videoId": "source", "lut": {"id": "../look", "intensity": 1.0}}),
            serde_json::json!({"videoId": "source", "lut": {"id": "look", "intensity": 1.1}}),
            serde_json::json!({"videoId": "source", "curves": {"red": [{"x": 0.1, "y": 0.0}, {"x": 1.0, "y": 1.0}]}}),
        ] {
            let request = serde_json::from_value(value).unwrap();
            assert!(
                EditPlan::compile(Fingerprint::digest(b"source"), request, self::source()).is_err()
            );
        }
    }

    #[test]
    fn compiler_maps_valid_lut_and_all_curve_channels() {
        let plan = compile(serde_json::json!({
            "videoId": "source",
            "lut": {"id": "look-1", "intensity": 0.4},
            "curves": {
                "master": [{"x": 0.0, "y": 0.05}, {"x": 1.0, "y": 0.95}],
                "red": [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 1.0}],
                "green": [{"x": 0.0, "y": 0.0}, {"x": 0.5, "y": 0.55}, {"x": 1.0, "y": 1.0}],
                "blue": [{"x": 0.0, "y": 0.1}, {"x": 1.0, "y": 0.9}]
            }
        }))
        .unwrap();

        let video = plan.edit.video();
        assert_eq!(video.lut.as_ref().unwrap().id(), "look-1");
        assert_eq!(video.lut.as_ref().unwrap().intensity(), 0.4);
        let curves = video.curves.as_ref().unwrap();
        assert_eq!(curves.master().unwrap().points().len(), 2);
        assert_eq!(curves.green().unwrap().points().len(), 3);
    }

    #[test]
    fn zero_intensity_lut_is_canonicalized_to_bypass() {
        let plan = compile(serde_json::json!({
            "videoId": "source",
            "lut": {"id": "look-1", "intensity": 0.0}
        }))
        .unwrap();
        assert!(plan.edit.video().lut.is_none());
    }

    #[test]
    fn export_policy_does_not_own_output_semantics() {
        let edit = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "quality": 18
        }))
        .unwrap();
        let plan =
            Arc::new(EditPlan::compile(Fingerprint::digest(b"source"), edit, source()).unwrap());
        let profile = ExportExecutionProfile {
            encode_budget: EncodeBudget::for_profile(
                EncodeProfile::Balanced,
                RuntimeLimits {
                    logical_cpus: 4,
                    memory_mib: Some(2048),
                },
            )
            .unwrap(),
            verify_checksums: true,
        };
        let execution = RenderExecution::new(plan.clone(), profile);
        assert_eq!(execution.output(), &plan.output);
    }

    #[test]
    fn deserialization_rejects_tampered_plan_identity_and_output() {
        let edit = serde_json::from_value(serde_json::json!({
            "videoId": "source",
            "format": "av1",
            "quality": 28
        }))
        .unwrap();
        let plan = EditPlan::compile(Fingerprint::digest(b"source"), edit, source()).unwrap();
        let mut value = serde_json::to_value(&plan).unwrap();
        value["output"]["crf"] = serde_json::json!(1);
        assert!(serde_json::from_value::<EditPlan>(value).is_err());

        let mut value = serde_json::to_value(&plan).unwrap();
        value["planFingerprint"] = serde_json::json!(Fingerprint::digest(b"tampered"));
        assert!(serde_json::from_value::<EditPlan>(value).is_err());

        let mut output = plan.output.clone();
        output.audio_codec = None;
        assert!(
            EditPlan::from_domain(plan.source_fingerprint, plan.source, plan.edit, output).is_err()
        );
    }

    fn color_probe(
        range: Option<&str>,
        matrix: &str,
        transfer: &str,
        primaries: &str,
    ) -> crate::domain::media_probe::ProbeResult {
        let mut stream = serde_json::json!({
            "index":0,"codec_type":"video","width":1920,"height":1080,"pix_fmt":"yuv420p",
            "color_space":matrix,"color_transfer":transfer,"color_primaries":primaries
        });
        if let Some(range) = range {
            stream["color_range"] = serde_json::json!(range);
        }
        crate::domain::media_probe::ProbeResult::from_ffprobe_json(&serde_json::json!({
            "streams":[stream,{"index":1,"codec_type":"audio"}],
            "format":{"duration":"10.0"}
        }))
        .unwrap()
    }

    #[test]
    fn source_color_policy_resolves_signaled_assumed_and_hard_errors() {
        let exact =
            SourceMediaMetadata::from_probe(&color_probe(Some("tv"), "bt709", "bt709", "bt709"))
                .unwrap();
        assert!(!exact.color_policy_warning);
        let missing =
            SourceMediaMetadata::from_probe(&color_probe(None, "bt709", "bt709", "bt709")).unwrap();
        assert!(missing.color_policy_warning);
        assert!(matches!(
            resolved_source_color_management(&color_probe(None, "bt709", "bt709", "bt709")),
            Some(ColorManagementStatusV1::Supported {
                provenance: ColorProvenanceV1::LegacyAssumedBt709,
                ..
            })
        ));
        assert!(SourceMediaMetadata::from_probe(&color_probe(
            Some("tv"),
            "bt709",
            "smpte2084",
            "bt709"
        ))
        .is_err());
        assert!(
            SourceMediaMetadata::from_probe(&color_probe(Some("tv"), "rgb", "bt709", "bt709"))
                .is_err()
        );
        // Build from ffprobe JSON because normalized ProbeResult intentionally
        // does not retain the original raw object.
        let rgb_missing_matrix = serde_json::json!({
            "streams":[{"index":0,"codec_type":"video","width":1920,"height":1080,
                "pix_fmt":"rgb24","color_range":"pc","color_transfer":"srgb","color_primaries":"bt709"}],
            "format":{"duration":"10.0"}
        });
        let rgb_missing_matrix =
            crate::domain::media_probe::ProbeResult::from_ffprobe_json(&rgb_missing_matrix)
                .unwrap();
        assert!(SourceMediaMetadata::from_probe(&rgb_missing_matrix).is_err());
    }

    #[test]
    fn audio_only_color_status_is_not_applicable_and_mp3_bypasses_video_policy() {
        let probe =
            crate::domain::media_probe::ProbeResult::from_ffprobe_json(&serde_json::json!({
                "streams":[{"index":0,"codec_type":"audio","codec_name":"mp3"}],
                "format":{"duration":"10.0"}
            }))
            .unwrap();
        assert_eq!(
            resolved_source_color_management(&probe),
            Some(ColorManagementStatusV1::NotApplicable)
        );
        let metadata = SourceMediaMetadata::for_audio_extraction(&probe).unwrap();
        assert!(metadata.color_policy.is_none());
        assert!(!metadata.color_policy_warning);
    }

    #[test]
    fn source_color_policy_is_part_of_plan_fingerprint() {
        let request = || serde_json::from_value(serde_json::json!({"videoId":"source"})).unwrap();
        let bt709 =
            SourceMediaMetadata::from_probe(&color_probe(Some("tv"), "bt709", "bt709", "bt709"))
                .unwrap();
        let srgb =
            SourceMediaMetadata::from_probe(&color_probe(Some("tv"), "bt709", "srgb", "bt709"))
                .unwrap();
        let first = EditPlan::compile(Fingerprint::digest(b"source"), request(), bt709).unwrap();
        let second = EditPlan::compile(Fingerprint::digest(b"source"), request(), srgb).unwrap();
        assert_ne!(first.plan_fingerprint, second.plan_fingerprint);
        assert_eq!(first.schema_version, 6);
    }
}
