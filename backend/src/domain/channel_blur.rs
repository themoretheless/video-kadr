//! Filmcraft Channel Blur; MIT attribution docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelBlur {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
    pub dimensions: u8,
    pub repeat_edge: bool,
}
impl ChannelBlur {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.dimensions <= 2, "Channel Blur dimensions out of range");
        ensure!(
            [self.red, self.green, self.blue, self.alpha]
                .into_iter()
                .all(|v| v.is_finite() && (0. ..=1000.).contains(&v)),
            "Channel Blur amount out of range"
        );
        Ok(())
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        px_scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            px_scale.is_finite() && px_scale > 0.,
            "invalid Channel Blur pixel scale"
        );
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let mut channels = Vec::with_capacity(4);
        for (c, value) in [self.red, self.green, self.blue, self.alpha]
            .into_iter()
            .enumerate()
        {
            let plane: Vec<f32> = frame.pixels().iter().map(|p| p[c]).collect();
            checkpoint()?;
            channels.push(super::gaussian_blur::blur_plane(
                &plane,
                w,
                h,
                value * px_scale * 0.5,
                self.dimensions,
                self.repeat_edge,
                checkpoint,
            )?);
        }
        let mut index = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = std::array::from_fn(|c| channels[c][index]);
                index += 1;
                p
            },
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChannelBlurChannel {
    Red,
    Green,
    Blue,
    Alpha,
}
pub type ChannelBlurAnimation =
    std::collections::BTreeMap<ChannelBlurChannel, super::composition::AnimatableValue>;
pub fn validate_animation(a: &ChannelBlurAnimation) -> Result<()> {
    for v in a.values() {
        v.validate()?;
        ensure!(
            super::composition::animatable_values_in(v, 0., 1000., true),
            "Channel Blur animation out of range"
        );
    }
    Ok(())
}
pub fn sample_animation(
    mut p: ChannelBlur,
    a: &ChannelBlurAnimation,
    t: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<ChannelBlur> {
    let t: super::animation_sample_time::AnimationSampleTime = t.into();

    p.validate()?;
    validate_animation(a)?;
    for (c, v) in a {
        let v = super::nested_composition::sample_value(v, t)? as f32;
        match c {
            ChannelBlurChannel::Red => p.red = v,
            ChannelBlurChannel::Green => p.green = v,
            ChannelBlurChannel::Blue => p.blue = v,
            ChannelBlurChannel::Alpha => p.alpha = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_blur_persisted_four_curves_and_choices() {
        let curve = |v: f64| serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":v}]}});
        let e:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"channel_blur","parameters":{"dimensions":2,"repeatEdge":true},"animation":{"red":curve(2.),"green":curve(4.),"blue":curve(6.),"alpha":curve(8.)}})).unwrap();
        assert_eq!(
            e,
            serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::ChannelBlur {
            parameters,
            animation: Some(a),
        } = e
        else {
            panic!("Channel Blur")
        };
        assert_eq!(
            sample_animation(parameters, &a, 250000).unwrap(),
            ChannelBlur {
                red: 1.,
                green: 2.,
                blue: 3.,
                alpha: 4.,
                dimensions: 2,
                repeat_edge: true
            }
        );
        assert!(ChannelBlur {
            red: 1001.,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn channel_blur_identity_independent_alpha_and_cancellation() {
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| [255, 64, 128, if i % 2 == 0 { 255 } else { 0 }])
                .collect(),
        };
        let frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut out = frame.clone();
        ChannelBlur::default()
            .apply_linear(&mut out, 1., &mut || Ok(()))
            .unwrap();
        assert_eq!(out.pixels(), frame.pixels());
        ChannelBlur {
            alpha: 4.,
            ..Default::default()
        }
        .apply_linear(&mut out, 1., &mut || Ok(()))
        .unwrap();
        for (a, b) in frame.pixels().iter().zip(out.pixels()) {
            assert_eq!(&a[..3], &b[..3]);
        }
        assert_ne!(out.pixels(), frame.pixels());
        assert!(ChannelBlur::default()
            .apply_linear(&mut out, 1., &mut || anyhow::bail!("cancelled"))
            .is_err());
    }
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn original_working_frame_bits() {
        #[derive(serde::Deserialize)]
        struct Case {
            parameters: ChannelBlur,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            scale: f32,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-channel-blur.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 192);
        for (index, c) in fixture.cases.into_iter().enumerate() {
            let source = super::super::group_surface::GroupSurface {
                width: c.width,
                height: c.height,
                rgba: c.rgba,
            };
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            c.parameters
                .apply_linear(&mut frame, c.scale, &mut || Ok(()))
                .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "Channel Blur original working bits case{index}"
            );
        }
    }
}
