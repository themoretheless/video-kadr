//! Offline render service contract: immutable edit plan plus resource policy.

use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::config::encode_budget::EncodeBudget;
use crate::domain::artifact_graph::Fingerprint;
use crate::domain::edit::{
    AspectRatio, AudioEffects, CensorColor, CensorSpec, EditSpec, GeometrySpec, LookPreset,
    OutputScale, PixelRect, Rotation, TimeRange, TimingSpec, VideoEffects,
};
use crate::domain::output::{OutputFormat, OutputSpec, VideoCodec};
use crate::model::{Crop, EditRequest, Scale, Trim};

const EDIT_PLAN_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceMediaMetadata {
    pub width: u32,
    pub height: u32,
    pub duration_seconds: f64,
}

impl SourceMediaMetadata {
    pub fn new(width: u32, height: u32, duration_seconds: f64) -> anyhow::Result<Self> {
        if !duration_seconds.is_finite() || duration_seconds < 0.0 {
            anyhow::bail!("Недопустимая длительность источника");
        }
        Ok(Self {
            width,
            height,
            duration_seconds,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceMediaSpec {
    pub width: u32,
    pub height: u32,
    pub duration_micros: u64,
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
        let source = SourceMediaSpec::from_metadata(metadata)?;
        normalize_request(&mut request, source)?;
        let (edit, output) = map_request(request)?;
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
        if supports_audio && (output.audio_codec.is_none() != edit.audio().muted) {
            anyhow::bail!("edit plan audio output does not match mute semantics");
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
            look,
            vignette: request.vignette,
            denoise: request.denoise,
            sharpen: request.sharpen,
            grain: request.grain,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportExecutionProfile {
    pub encode_budget: EncodeBudget,
    pub verify_checksums: bool,
}

#[derive(Debug, Clone)]
pub struct RenderExecution {
    plan: Arc<EditPlan>,
    pub profile: ExportExecutionProfile,
}

impl RenderExecution {
    pub fn new(plan: Arc<EditPlan>, profile: ExportExecutionProfile) -> Self {
        Self { plan, profile }
    }

    pub fn plan(&self) -> &EditPlan {
        &self.plan
    }

    pub fn output(&self) -> &OutputSpec {
        &self.plan.output
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
        let second = EditPlan::compile(source, edit.clone(), self::source()).unwrap();
        let other =
            EditPlan::compile(Fingerprint::digest(b"source-b"), edit, self::source()).unwrap();
        assert_eq!(first.plan_fingerprint, second.plan_fingerprint);
        assert_ne!(first.plan_fingerprint, other.plan_fingerprint);
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
        ] {
            let request = serde_json::from_value(value).unwrap();
            assert!(
                EditPlan::compile(Fingerprint::digest(b"source"), request, self::source()).is_err()
            );
        }
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
}
