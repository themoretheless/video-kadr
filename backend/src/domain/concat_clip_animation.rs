// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original Concat clip-property rides, distinct from native keyframe easing.
use super::{concat_animation::PreparedTrack, concat_effect_upgrade::ParamKey};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pose {
    pub scale_x: f64,
    pub scale_y: f64,
    pub x: f64,
    pub y: f64,
    pub rotation_degrees: f64,
    pub opacity: f64,
}
#[derive(Debug, Clone)]
pub struct ClipAnimation {
    tracks: BTreeMap<String, PreparedTrack>,
}
impl ClipAnimation {
    pub fn new(keys: &BTreeMap<String, Vec<ParamKey>>) -> Result<Self> {
        ensure!(keys.len() <= 5, "too many Concat visual properties");
        ensure!(
            keys.values().map(Vec::len).sum::<usize>() <= 8192,
            "too many Concat clip keys"
        );
        let mut tracks = BTreeMap::new();
        for (property, run) in keys {
            ensure!(
                matches!(
                    property.as_str(),
                    "scale" | "offsetX" | "offsetY" | "rotation" | "opacity"
                ),
                "unknown Concat visual property {property}"
            );
            ensure!(run.len() <= 4096, "too many Concat property keys");
            ensure!(
                run.iter().all(|k| k.at.is_finite()
                    && (0.0..=1.0).contains(&k.at)
                    && k.value.is_finite()
                    && k.ease.iter().all(|p| p.is_finite())),
                "invalid Concat clip key"
            );
            let normalized: Vec<_> = run
                .iter()
                .cloned()
                .map(|mut k| {
                    if !k.derived {
                        k.value = match property.as_str() {
                            "scale" => k.value.clamp(0.05, 8.),
                            "opacity" => k.value.clamp(0., 1.),
                            "offsetX" | "offsetY" => k.value.clamp(-3., 3.),
                            _ => k.value,
                        };
                    }
                    k
                })
                .collect();
            tracks.insert(property.clone(), PreparedTrack::new(&normalized)?);
        }
        Ok(Self { tracks })
    }
    /// Rest pose uses normalized source offsets and uniform source scale. Stretch
    /// remains independent, and offsets become pixels only after source sampling.
    pub fn sample(
        &self,
        local_tick: i64,
        duration_ticks: i64,
        rest: Pose,
        stretch: [f64; 2],
        canvas: [f64; 2],
    ) -> Result<Pose> {
        ensure!(
            duration_ticks > 0
                && [
                    rest.scale_x,
                    rest.scale_y,
                    rest.x,
                    rest.y,
                    rest.rotation_degrees,
                    rest.opacity,
                    stretch[0],
                    stretch[1],
                    canvas[0],
                    canvas[1]
                ]
                .iter()
                .all(|v| v.is_finite()),
            "invalid Concat pose clock or rest values"
        );
        ensure!(canvas[0] > 0. && canvas[1] > 0., "invalid Concat canvas");
        self.sample_progress(
            local_tick as f64 / duration_ticks as f64,
            rest,
            stretch,
            canvas,
        )
    }
    pub fn sample_progress(
        &self,
        progress: f64,
        rest: Pose,
        stretch: [f64; 2],
        canvas: [f64; 2],
    ) -> Result<Pose> {
        ensure!(progress.is_finite()&&canvas[0]>0.&&canvas[1]>0.&&[rest.scale_x,rest.scale_y,rest.x,rest.y,rest.rotation_degrees,rest.opacity,stretch[0],stretch[1],canvas[0],canvas[1]].iter().all(|v|v.is_finite()), "invalid Concat property progress or pose");
        let sample = |name: &str, value: f64| -> Result<f64> {
            match self.tracks.get(name) {
                Some(track) => track.value_at(progress, value),
                None => Ok(value),
            }
        };
        let scale = sample("scale", rest.scale_x)?.max(0.001);
        let pose = Pose {
            scale_x: scale * stretch[0],
            scale_y: scale * stretch[1],
            x: sample("offsetX", rest.x)? * canvas[0],
            y: sample("offsetY", rest.y)? * canvas[1],
            rotation_degrees: sample("rotation", rest.rotation_degrees)?,
            opacity: sample("opacity", rest.opacity)?.clamp(0., 1.),
        };
        ensure!(
            [
                pose.scale_x,
                pose.scale_y,
                pose.x,
                pose.y,
                pose.rotation_degrees,
                pose.opacity
            ]
            .iter()
            .all(|v| v.is_finite()),
            "Concat pose overflow"
        );
        Ok(pose)
    }
}

/// Persisted source clock and authored values. Not a native keyframe track.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceAnimation {
    pub duration_ticks: i64,
    pub keys: BTreeMap<String, Vec<ParamKey>>,
    pub rest: Pose,
    pub stretch: [f64; 2],
    pub canvas: [f64; 2],
}
impl SourceAnimation {
    pub fn validate(&self) -> Result<()> {
        self.sample(0).map(|_| ())
    }
    pub fn sample(&self, local: u64) -> Result<Pose> {
        let local =
            i64::try_from(local).map_err(|_| anyhow::anyhow!("Concat animation clock overflow"))?;
        ClipAnimation::new(&self.keys)?.sample(
            local,
            self.duration_ticks,
            self.rest,
            self.stretch,
            self.canvas,
        )
    }
}
