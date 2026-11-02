//! Filmcraft Metadata & Timecode Burn-in, source text and layout semantics.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{linear_color_frame::LinearColorFrame, text_burnin::draw_text};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug)]
pub struct BurninContext<'a> {
    pub sequence_timecode: &'a str,
    pub media_timecode: Option<&'a str>,
    pub clip_name: &'a str,
    pub file_name: Option<&'a str>,
    pub sequence_name: Option<&'a str>,
    pub frame_rate: Option<f64>,
    pub seconds: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataBurnin {
    pub source: u32,
    pub position: [Option<f64>; 2],
    pub alignment: u32,
    pub size: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub prefix: String,
}
impl Default for MetadataBurnin {
    fn default() -> Self {
        Self {
            source: 0,
            position: [None, None],
            alignment: 0,
            size: 6.,
            color: [1.; 4],
            opacity: 60.,
            prefix: String::new(),
        }
    }
}
impl MetadataBurnin {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.source <= 5 && self.alignment <= 6,
            "invalid metadata burn-in choice"
        );
        ensure!(
            self.size.is_finite() && (1.0..=100.0).contains(&self.size),
            "invalid metadata burn-in size"
        );
        ensure!(
            self.opacity.is_finite() && (0.0..=100.0).contains(&self.opacity),
            "invalid metadata burn-in opacity"
        );
        ensure!(
            self.color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "invalid metadata burn-in color"
        );
        ensure!(
            self.position.iter().flatten().all(|v| v.is_finite()),
            "invalid metadata burn-in position"
        );
        Ok(())
    }
    pub fn text(&self, context: BurninContext<'_>) -> Result<String> {
        self.validate()?;
        ensure!(
            context.seconds.is_finite()
                && context.frame_rate.is_none_or(|v| v.is_finite() && v > 0.0),
            "invalid metadata burn-in time context"
        );
        let body = match self.source {
            0 => context.sequence_timecode.to_owned(),
            1 => context
                .media_timecode
                .unwrap_or(context.sequence_timecode)
                .to_owned(),
            2 => context.clip_name.to_owned(),
            3 => context.file_name.unwrap_or(context.clip_name).to_owned(),
            4 => format!(
                "{}",
                (context.seconds * context.frame_rate.unwrap_or(30.0)).round() as i64
            ),
            _ => context.sequence_name.unwrap_or_default().to_owned(),
        };
        Ok(if self.prefix.is_empty() {
            body
        } else {
            format!("{} {}", self.prefix, body)
        })
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        context: BurninContext<'_>,
        pixel_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        let text = self.text(context)?;
        ensure!(
            pixel_scale.is_finite() && pixel_scale > 0.0,
            "invalid metadata burn-in pixel scale"
        );
        let (w, h) = frame.dimensions();
        let size = (self.size / 100.0 * h as f32).max(6.0);
        let margin = size as f64 * 1.2;
        let (position, alignment) = match self.alignment {
            0 => ([w as f64 / 2.0, h as f64 - margin], 1),
            1 => ([margin, h as f64 - margin], 0),
            2 => ([w as f64 - margin, h as f64 - margin], 2),
            3 => ([w as f64 / 2.0, margin], 1),
            4 => ([margin, margin], 0),
            5 => ([w as f64 - margin, margin], 2),
            _ => (
                [
                    self.position[0].map_or(w as f64 / 2.0, |v| v * pixel_scale as f64),
                    self.position[1].map_or(h as f64 / 2.0, |v| v * pixel_scale as f64),
                ],
                1,
            ),
        };
        let family = if matches!(self.source, 0 | 1 | 4) {
            "JetBrains Mono"
        } else {
            "Inter"
        };
        draw_text(
            frame,
            &text,
            family,
            position,
            size,
            alignment,
            self.color,
            self.opacity / 100.0,
            checkpoint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: MetadataBurnin,
        has_env: bool,
        pixel_scale: f32,
        seconds: f64,
        bits: Vec<u32>,
    }
    #[test]
    fn original_metadata_burnin_working_bits() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-metadata-burnin.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 252);
        for (index, case) in fixture.cases.into_iter().enumerate() {
            let mut frame = LinearColorFrame::from_surface(
                &GroupSurface {
                    width: case.width,
                    height: case.height,
                    rgba: case.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            let context = BurninContext {
                sequence_timecode: "01:00:00:00",
                media_timecode: case.has_env.then_some("00:00:10:00"),
                clip_name: "Clip Привет",
                file_name: case.has_env.then_some("A001.mov"),
                sequence_name: case.has_env.then_some("Seq Привет"),
                frame_rate: case.has_env.then_some(24.),
                seconds: case.seconds,
            };
            case.parameters
                .apply_linear(&mut frame, context, case.pixel_scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                case.bits,
                "Metadata Burn-in case{index}"
            );
        }
    }
    #[test]
    fn contextual_text_fallbacks_and_prefix() {
        let context = BurninContext {
            sequence_timecode: "01:00:00:00",
            media_timecode: None,
            clip_name: "Clip",
            file_name: None,
            sequence_name: None,
            frame_rate: None,
            seconds: -0.05,
        };
        let expected = ["01:00:00:00", "01:00:00:00", "Clip", "Clip", "-2", ""];
        for (source, body) in expected.into_iter().enumerate() {
            let p = MetadataBurnin {
                source: source as u32,
                ..Default::default()
            };
            assert_eq!(p.text(context).unwrap(), body);
            assert_eq!(
                MetadataBurnin {
                    prefix: "PREFIX".into(),
                    ..p
                }
                .text(context)
                .unwrap(),
                format!("PREFIX {body}")
            );
        }
        assert!(MetadataBurnin {
            source: 6,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(MetadataBurnin {
            alignment: 7,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MetadataBurninChannel {
    PositionX,
    PositionY,
    Size,
    ColorR,
    ColorG,
    ColorB,
    ColorA,
    Opacity,
}
pub type MetadataBurninAnimation =
    std::collections::BTreeMap<MetadataBurninChannel, super::composition::AnimatableValue>;
pub fn validate_animation(animation: &MetadataBurninAnimation) -> Result<()> {
    for (channel, value) in animation {
        value.validate()?;
        let (lo, hi) = match channel {
            MetadataBurninChannel::PositionX | MetadataBurninChannel::PositionY => {
                (-(f32::MAX as f64), f32::MAX as f64)
            }
            MetadataBurninChannel::Size => (1., 100.),
            MetadataBurninChannel::Opacity => (0., 100.),
            _ => (0., 1.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Metadata Burn-in animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut parameters: MetadataBurnin,
    animation: &MetadataBurninAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<MetadataBurnin> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    parameters.validate()?;
    validate_animation(animation)?;
    for (channel, curve) in animation {
        let value = super::nested_composition::sample_value(curve, local)?;
        match channel {
            MetadataBurninChannel::PositionX => parameters.position[0] = Some(value),
            MetadataBurninChannel::PositionY => parameters.position[1] = Some(value),
            MetadataBurninChannel::Size => parameters.size = value as f32,
            MetadataBurninChannel::ColorR => parameters.color[0] = value as f32,
            MetadataBurninChannel::ColorG => parameters.color[1] = value as f32,
            MetadataBurninChannel::ColorB => parameters.color[2] = value as f32,
            MetadataBurninChannel::ColorA => parameters.color[3] = value as f32,
            MetadataBurninChannel::Opacity => parameters.opacity = value as f32,
        }
    }
    parameters.validate()?;
    Ok(parameters)
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn eight_channels_preserve_metadata_choices_and_prefix() {
        let mut curves = serde_json::Map::new();
        for (name, value) in [
            ("positionX", 12.25),
            ("positionY", -7.75),
            ("size", 20.25),
            ("colorR", 0.2),
            ("colorG", 0.4),
            ("colorB", 0.6),
            ("colorA", 0.8),
            ("opacity", 37.5),
        ] {
            curves.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":value-0.1},{"tick":750,"value":value+0.1}]}}));
        }
        let animation: MetadataBurninAnimation =
            serde_json::from_value(serde_json::Value::Object(curves)).unwrap();
        let base = MetadataBurnin {
            source: 5,
            alignment: 6,
            prefix: "Seq".into(),
            ..Default::default()
        };
        let result = sample(base.clone(), &animation, 500000).unwrap();
        assert_eq!(result.position, [Some(12.25), Some(-7.75)]);
        assert_eq!(result.size, 20.25);
        assert_eq!(result.color, [0.2, 0.4, 0.6, 0.8]);
        assert_eq!(result.opacity, 37.5);
        assert_eq!(
            (result.source, result.alignment, result.prefix.as_str()),
            (5, 6, "Seq")
        );
        assert_eq!(base.position, [None, None]);
        assert_eq!(
            result,
            serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap()
        );
        assert_eq!(
            animation,
            serde_json::from_value(serde_json::to_value(&animation).unwrap()).unwrap()
        );
        let invalid: MetadataBurninAnimation =
            serde_json::from_value(serde_json::json!({"size":{"mode":"constant","value":101}}))
                .unwrap();
        assert!(validate_animation(&invalid).is_err());
    }
}

/// Optional authored metadata; absent in older v1 composition documents.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionMetadata {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<CompositionMarker>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub caption_text_tracks: Vec<String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub clip_links: std::collections::BTreeMap<String, Vec<super::composition::CompositionClipId>>,
    /// Typed native tracks representing one original Concat lane.
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub concat_lane_groups: std::collections::BTreeMap<super::composition::TrackId, Vec<super::composition::TrackId>>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub media_versions: std::collections::BTreeMap<super::composition::CompositionClipId, MediaVersions>,
    pub sequence_name: String,
    pub clip_names: std::collections::BTreeMap<super::composition::CompositionClipId, String>,
    pub source_names: std::collections::BTreeMap<super::composition::SourceId, String>,
    pub drop_frame: bool,
    pub sequence_start_frame: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaVersions {
    pub original_source_id: super::composition::SourceId,
    pub copy_source_id: super::composition::SourceId,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub cutout:Option<super::cutout::Attachment>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub video_cutout:Option<super::video_cutout::Attachment>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionMarker {
    pub id: String,
    pub tick: i64,
    pub label: String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub color: Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub origin: Option<MarkerOrigin>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub enum MarkerOrigin { Manual, AutoBeat }
pub fn validate_markers(markers:&[CompositionMarker])->Result<(),String>{
    if markers.len()>256{return Err("Marker limit is 256".into());}
    let mut ids=std::collections::BTreeSet::new();
    for marker in markers {
        if super::composition::CompositionClipId::parse(marker.id.clone()).is_err()||!ids.insert(&marker.id)||!(0..=86_400_000_000).contains(&marker.tick)||marker.label.trim().is_empty()||marker.label.chars().count()>128{return Err("Invalid composition marker".into());}
        if let Some(color)=&marker.color {if color.len()!=7||!color.starts_with('#')||!color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit){return Err("Invalid marker color".into());}}
    }
    Ok(())
}

/// Owned values supplied by the compositor; no source IDs are presented as authored names.
#[derive(Clone, Debug, PartialEq)]
pub struct BurninValues {
    pub has_environment: bool,
    pub sequence_timecode: String,
    pub media_timecode: String,
    pub clip_name: String,
    pub file_name: String,
    pub sequence_name: String,
    pub frame_rate: f64,
    pub seconds: f64,
}
impl BurninValues {
    pub fn context(&self) -> BurninContext<'_> {
        BurninContext {
            sequence_timecode: &self.sequence_timecode,
            media_timecode: self.has_environment.then_some(self.media_timecode.as_str()),
            clip_name: &self.clip_name,
            file_name: self.has_environment.then_some(self.file_name.as_str()),
            sequence_name: self.has_environment.then_some(self.sequence_name.as_str()),
            frame_rate: self.has_environment.then_some(self.frame_rate),
            seconds: self.seconds,
        }
    }
    pub fn from_composition(
        composition: &super::composition::Composition,
        clip_id: &super::composition::CompositionClipId,
        source_id: &super::composition::SourceId,
        timeline_us: u64,
        source_us: u64,
        local_us: u64,
    ) -> Result<Self> {
        let rate = composition.canvas.frame_rate();
        let rate = filmcraft_time::FrameRate::new(rate.numerator as i64, rate.denominator as i64);
        let ticks = |us: u64| -> Result<filmcraft_time::Tick> {
            let us = i64::try_from(us)
                .map_err(|_| anyhow::anyhow!("metadata time exceeds signed tick range"))?;
            ensure!(
                us as i128 * filmcraft_time::TICKS_PER_SECOND as i128 / 1_000_000
                    <= i64::MAX as i128,
                "metadata time exceeds Filmcraft tick range"
            );
            Ok(filmcraft_time::Tick::from_units(us, 1_000_000))
        };
        let sequence_frame = rate
            .frame_at(ticks(timeline_us)?)
            .checked_add(
                composition
                    .metadata
                    .as_ref()
                    .map_or(0, |m| m.sequence_start_frame),
            )
            .ok_or_else(|| anyhow::anyhow!("sequence timecode frame overflow"))?;
        let source_frame = rate.frame_at(ticks(source_us)?);
        let drop = composition.metadata.as_ref().is_some_and(|m| m.drop_frame);
        Ok(Self {
            has_environment: true,
            sequence_timecode: filmcraft_time::format_timecode_frames(sequence_frame, rate, drop),
            media_timecode: filmcraft_time::format_timecode_frames(source_frame, rate, drop),
            clip_name: composition
                .metadata
                .as_ref()
                .and_then(|m| m.clip_names.get(clip_id))
                .cloned()
                .unwrap_or_default(),
            file_name: composition
                .metadata
                .as_ref()
                .and_then(|m| m.source_names.get(source_id))
                .cloned()
                .unwrap_or_default(),
            sequence_name: composition
                .metadata
                .as_ref()
                .map(|m| m.sequence_name.clone())
                .unwrap_or_default(),
            frame_rate: rate.num as f64 / rate.den as f64,
            seconds: local_us as f64 / 1_000_000.0,
        })
    }
}

#[cfg(test)]
mod composition_metadata_tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec, Composition, CompositionClipId, SourceId};
    #[test]
    fn metadata_optional_roundtrip_and_exact_clock_names() {
        let mut canvas = CanvasSpec::default();
        canvas.fps_milli = 30000;
        let mut c = Composition::new(canvas);
        let old = serde_json::to_value(&c).unwrap();
        assert!(old.get("metadata").is_none());
        assert_eq!(c, serde_json::from_value(old).unwrap());
        let clip = CompositionClipId::parse("clip").unwrap();
        let source = SourceId::parse("source").unwrap();
        c.metadata = Some(CompositionMetadata {
            markers: Default::default(),
            media_versions: Default::default(),
            clip_links: Default::default(),
            concat_lane_groups: Default::default(),
            sequence_name: "Seq Привет".into(),
            clip_names: std::collections::BTreeMap::from([(clip.clone(), "Clip".into())]),
            source_names: std::collections::BTreeMap::from([(source.clone(), "A001.mov".into())]),
            sequence_start_frame: 108000,
            drop_frame: false,
        });
        let values =
            BurninValues::from_composition(&c, &clip, &source, 1_500_000, 500_000, 250_000)
                .unwrap();
        assert_eq!(values.sequence_timecode, "01:00:01:15");
        assert_eq!(values.media_timecode, "00:00:00:15");
        assert_eq!(
            (&values.clip_name, &values.file_name, &values.sequence_name),
            (
                &"Clip".to_owned(),
                &"A001.mov".to_owned(),
                &"Seq Привет".to_owned()
            )
        );
        assert_eq!(values.seconds, 0.25);
        assert_eq!(
            c,
            serde_json::from_value(serde_json::to_value(&c).unwrap()).unwrap()
        );
        c.metadata = None;
        let values = BurninValues::from_composition(&c, &clip, &source, 0, 0, 0).unwrap();
        assert!(
            values.clip_name.is_empty()
                && values.file_name.is_empty()
                && values.sequence_name.is_empty()
        );
        assert!(BurninValues::from_composition(&c, &clip, &source, u64::MAX, 0, 0).is_err());
    }
}

include!("metadata_burnin_exact_time.rs");

#[cfg(test)]
mod media_versions_tests {
    use super::CompositionMetadata;
    #[test]
    fn media_versions_preserve_original_and_copy_in_native_wire_roundtrip() {
        let input = serde_json::json!({"mediaVersions":{"clip":{"originalSourceId":"original","copySourceId":"processed"}}});
        let metadata: CompositionMetadata = serde_json::from_value(input.clone()).unwrap();
        let encoded = serde_json::to_value(&metadata).unwrap();
        assert_eq!(encoded["mediaVersions"], input["mediaVersions"]);
        assert_eq!(serde_json::from_value::<CompositionMetadata>(encoded).unwrap(), metadata);
        let legacy: CompositionMetadata = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(legacy.media_versions.is_empty());
        assert!(serde_json::to_value(legacy).unwrap().get("mediaVersions").is_none());
        assert!(serde_json::from_value::<CompositionMetadata>(serde_json::json!({"mediaVersions":{"clip":{"originalSourceId":"original","copySourceId":"processed","unexpected":true}}})).is_err());
    }
}
