//! Filmcraft VR Color Gradients. MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    basic_grade::linear,
    light_leaks::{blend_simple, lerp3},
    vr_sphere::{dir, dot, px_to_lonlat},
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct VrColorGradients {
    pub frame_layout: u32,
    pub c1: [f32; 4],
    pub c2: [f32; 4],
    pub c3: [f32; 4],
    pub p1_lon: f32,
    pub p2_lon: f32,
    pub p3_lon: f32,
    pub p1_lat: f32,
    pub p2_lat: f32,
    pub p3_lat: f32,
    pub blend: f32,
    pub opacity: f32,
    pub mode: u32,
}
impl Default for VrColorGradients {
    fn default() -> Self {
        Self {
            frame_layout: 0,
            c1: [1., 0.25, 0.25, 1.],
            c2: [0.25, 1., 0.4, 1.],
            c3: [0.3, 0.4, 1., 1.],
            p1_lon: -120.,
            p2_lon: 0.,
            p3_lon: 120.,
            p1_lat: 20.,
            p2_lat: -20.,
            p3_lat: 20.,
            blend: 50.,
            opacity: 100.,
            mode: 0,
        }
    }
}
impl VrColorGradients {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_layout <= 2 && self.mode <= 4,
            "invalid VR gradient choice"
        );
        for c in [self.c1, self.c2, self.c3] {
            ensure!(
                c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v)),
                "invalid VR gradient color"
            );
        }
        for (v, lo, hi) in [
            (self.p1_lon, -180., 180.),
            (self.p2_lon, -180., 180.),
            (self.p3_lon, -180., 180.),
            (self.p1_lat, -90., 90.),
            (self.p2_lat, -90., 90.),
            (self.p3_lat, -90., 90.),
            (self.blend, 1., 100.),
            (self.opacity, 0., 100.),
        ] {
            ensure!(
                v.is_finite() && (lo..=hi).contains(&v),
                "invalid VR gradient parameter"
            );
        }
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut super::linear_color_frame::LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        checkpoint()?;
        let pts = [
            (self.p1_lon, self.p1_lat, self.c1),
            (self.p2_lon, self.p2_lat, self.c2),
            (self.p3_lon, self.p3_lat, self.c3),
        ]
        .map(|(lon, lat, c)| {
            (
                dir((lon as f64).to_radians(), (lat as f64).to_radians()),
                [linear(c[0]), linear(c[1]), linear(c[2])],
            )
        });
        let expo = 1. + self.blend as f64 / 25.;
        let op = self.opacity / 100.;
        let (w, h) = frame.dimensions();
        let mut output = frame.pixels().to_vec();
        let views = match self.frame_layout {
            1 if h >= 2 => vec![(0, 0, w, h / 2), (0, h / 2, w, h - h / 2)],
            2 if w >= 2 => vec![(0, 0, w / 2, h), (w / 2, 0, w - w / 2, h)],
            _ => vec![(0, 0, w, h)],
        };
        for (x0, y0, vw, vh) in views {
            checkpoint()?;
            for y in 0..vh {
                checkpoint()?;
                for x in 0..vw {
                    let (lon, lat) = px_to_lonlat(x as f64 + 0.5, y as f64 + 0.5, vw, vh);
                    let d = dir(lon, lat);
                    let mut acc = [0f64; 3];
                    let mut ws = 0.;
                    for (pd, c) in &pts {
                        let ang = dot(d, *pd).clamp(-1., 1.).acos().max(1e-3);
                        let weight = 1. / ang.powf(expo);
                        for k in 0..3 {
                            acc[k] += c[k] as f64 * weight;
                        }
                        ws += weight;
                    }
                    let g = acc.map(|v| (v / ws) as f32);
                    let p = &mut output[(y0 + y) * w + x0 + x];
                    let base = if p[3] > 1e-6 {
                        [p[0] / p[3], p[1] / p[3], p[2] / p[3]]
                    } else {
                        [0.; 3]
                    };
                    let o = lerp3(base, blend_simple(self.mode, base, g), op);
                    let a = p[3] + (1. - p[3]) * op;
                    *p = [o[0] * a, o[1] * a, o[2] * a, a];
                }
            }
        }
        let mut i = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = output[i];
                i += 1;
                p
            },
            checkpoint,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
        parameters: VrColorGradients,
        bits: Vec<u32>,
    }
    #[test]
    fn original_vr_color_gradients_working_bits() {
        let f: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-vr-color-gradients.json"
        ))
        .unwrap();
        assert_eq!(f.cases.len(), 540);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            c.parameters
                .apply_linear(&mut frame, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "VR gradient case{i}"
            );
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VrColorGradientsChannel {
    C1R,
    C1G,
    C1B,
    C1A,
    C2R,
    C2G,
    C2B,
    C2A,
    C3R,
    C3G,
    C3B,
    C3A,
    P1Lon,
    P1Lat,
    P2Lon,
    P2Lat,
    P3Lon,
    P3Lat,
    Blend,
    Opacity,
}
pub type VrColorGradientsAnimation =
    std::collections::BTreeMap<VrColorGradientsChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &VrColorGradientsAnimation) -> Result<()> {
    for (c, v) in a {
        v.validate()?;
        let (lo, hi) = match c {
            VrColorGradientsChannel::C1R => (0., 1.),
            VrColorGradientsChannel::C1G => (0., 1.),
            VrColorGradientsChannel::C1B => (0., 1.),
            VrColorGradientsChannel::C1A => (0., 1.),
            VrColorGradientsChannel::C2R => (0., 1.),
            VrColorGradientsChannel::C2G => (0., 1.),
            VrColorGradientsChannel::C2B => (0., 1.),
            VrColorGradientsChannel::C2A => (0., 1.),
            VrColorGradientsChannel::C3R => (0., 1.),
            VrColorGradientsChannel::C3G => (0., 1.),
            VrColorGradientsChannel::C3B => (0., 1.),
            VrColorGradientsChannel::C3A => (0., 1.),
            VrColorGradientsChannel::P1Lon => (-180., 180.),
            VrColorGradientsChannel::P1Lat => (-90., 90.),
            VrColorGradientsChannel::P2Lon => (-180., 180.),
            VrColorGradientsChannel::P2Lat => (-90., 90.),
            VrColorGradientsChannel::P3Lon => (-180., 180.),
            VrColorGradientsChannel::P3Lat => (-90., 90.),
            VrColorGradientsChannel::Blend => (1., 100.),
            VrColorGradientsChannel::Opacity => (0., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(v, lo, hi, true),
            "VR gradient animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut p: VrColorGradients,
    a: &VrColorGradientsAnimation,
    local: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<VrColorGradients> {
        let local: super::animation_sample_time::AnimationSampleTime = local.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, local)? as f32;
        match c {
            VrColorGradientsChannel::C1R => p.c1[0] = v,
            VrColorGradientsChannel::C1G => p.c1[1] = v,
            VrColorGradientsChannel::C1B => p.c1[2] = v,
            VrColorGradientsChannel::C1A => p.c1[3] = v,
            VrColorGradientsChannel::C2R => p.c2[0] = v,
            VrColorGradientsChannel::C2G => p.c2[1] = v,
            VrColorGradientsChannel::C2B => p.c2[2] = v,
            VrColorGradientsChannel::C2A => p.c2[3] = v,
            VrColorGradientsChannel::C3R => p.c3[0] = v,
            VrColorGradientsChannel::C3G => p.c3[1] = v,
            VrColorGradientsChannel::C3B => p.c3[2] = v,
            VrColorGradientsChannel::C3A => p.c3[3] = v,
            VrColorGradientsChannel::P1Lon => p.p1_lon = v,
            VrColorGradientsChannel::P1Lat => p.p1_lat = v,
            VrColorGradientsChannel::P2Lon => p.p2_lon = v,
            VrColorGradientsChannel::P2Lat => p.p2_lat = v,
            VrColorGradientsChannel::P3Lon => p.p3_lon = v,
            VrColorGradientsChannel::P3Lat => p.p3_lat = v,
            VrColorGradientsChannel::Blend => p.blend = v,
            VrColorGradientsChannel::Opacity => p.opacity = v,
        }
    }
    p.validate()?;
    Ok(p)
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn twenty_channels_sample_and_persist() {
        let names = [
            "c1R", "c1G", "c1B", "c1A", "c2R", "c2G", "c2B", "c2A", "c3R", "c3G", "c3B", "c3A",
            "p1Lon", "p1Lat", "p2Lon", "p2Lat", "p3Lon", "p3Lat", "blend", "opacity",
        ];
        let values = [
            0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.1, 0.2, 0.3, 0.4, -120., 20., 45.25, -12.5,
            120., 47.75, 37.5, 45.,
        ];
        let mut a = serde_json::Map::new();
        for (name, v) in names.into_iter().zip(values) {
            let delta = if name.starts_with('c') { 0.05 } else { 1. };
            a.insert(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000,"interpolation":"linear","keyframes":[{"tick":250,"value":v-delta},{"tick":750,"value":v+delta}]}}));
        }
        let a: VrColorGradientsAnimation =
            serde_json::from_value(serde_json::Value::Object(a)).unwrap();
        assert_eq!(a.len(), 20);
        let p = sample(
            VrColorGradients {
                frame_layout: 2,
                mode: 4,
                ..Default::default()
            },
            &a,
            500000,
        )
        .unwrap();
        let actual = [
            p.c1[0], p.c1[1], p.c1[2], p.c1[3], p.c2[0], p.c2[1], p.c2[2], p.c2[3], p.c3[0],
            p.c3[1], p.c3[2], p.c3[3], p.p1_lon, p.p1_lat, p.p2_lon, p.p2_lat, p.p3_lon, p.p3_lat,
            p.blend, p.opacity,
        ];
        for (actual, expected) in actual.into_iter().zip(values) {
            assert!((actual as f64 - expected).abs() < 1e-6);
        }
        assert_eq!((p.frame_layout, p.mode), (2, 4));
        let e: super::super::composition::VideoEffect = serde_json::from_value(
            serde_json::json!({"kind":"vr_color_gradients","parameters":p,"animation":a}),
        )
        .unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        assert!(VrColorGradients { p1_lat: 91., ..p }.validate().is_err());
        assert!(VrColorGradients { mode: 5, ..p }.validate().is_err());
    }
    #[test]
    fn cancellation_before_output_mutation() {
        let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &super::super::group_surface::GroupSurface {
                width: 6,
                height: 4,
                rgba: vec![128; 96],
            },
            &mut || Ok(()),
        )
        .unwrap();
        let original = frame.pixels().to_vec();
        let mut n = 0;
        assert!(VrColorGradients::default()
            .apply_linear(&mut frame, &mut || {
                n += 1;
                anyhow::ensure!(n < 4, "cancelled");
                Ok(())
            })
            .is_err());
        assert_eq!(frame.pixels(), original);
    }
}
