//! Source-free Filmcraft graphic layer stack; native vector rasterisation in paint order.
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::graphic_pin::Pin;
use super::graphic_pin_layers::{apply_pins, EvaluatedContent, EvaluatedLayer};
use super::{
    graphic_shape::GraphicShape,
    graphic_shape_animation::{
        sample_signed as shape_sample, validate_animation as validate_shape, GraphicShapeAnimation,
    },
    graphic_shape_render::{raster_layer_signed as shape_raster, LayerRaster},
    graphic_source_text_animation::SourceTextAnimation,
    graphic_text::GraphicTextParameters,
    graphic_text_animation::{
        sample_signed as text_sample, validate_animation as validate_text, GraphicTextAnimation,
    },
    graphic_text_render::raster_layer as text_raster,
    group_surface::GroupSurface,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphicContent {
    Text {
        parameters: GraphicTextParameters,
        #[serde(default)]
        animation: Option<GraphicTextAnimation>,
        #[serde(default, rename = "sourceTextAnimation")]
        source_text_animation: Option<SourceTextAnimation>,
    },
    Shape {
        parameters: GraphicShape,
        #[serde(default)]
        animation: Option<GraphicShapeAnimation>,
    },
}
fn enabled() -> bool {
    true
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_parameters: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub uid: u64,
    #[serde(default = "enabled")]
    pub enabled: bool,
    pub content: GraphicContent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<Pin>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicClip {
    pub layers: Vec<GraphicLayer>,
}
fn multiply(a: [f64; 6], b: [f64; 6]) -> [f64; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}
fn matrix(
    position: Option<[f64; 2]>,
    anchor: [f64; 2],
    scale: f32,
    scale_width: f32,
    uniform: bool,
    rotation: f32,
    canvas: (u32, u32),
    prefix: [f64; 6],
) -> Result<[f64; 6]> {
    let p = position.unwrap_or([canvas.0 as f64 / 2., canvas.1 as f64 / 2.]);
    let sy = scale as f64 / 100.;
    let sx = if uniform {
        sy
    } else {
        scale_width as f64 / 100.
    };
    let (sin, cos) = (rotation as f64).to_radians().sin_cos();
    let local = multiply(
        multiply(
            multiply([1., 0., 0., 1., p[0], p[1]], [cos, sin, -sin, cos, 0., 0.]),
            [sx, 0., 0., sy, 0., 0.],
        ),
        [1., 0., 0., 1., -anchor[0], -anchor[1]],
    );
    let m = multiply(prefix, local);
    ensure!(
        m.iter().all(|v| v.is_finite()),
        "graphic layer transform overflow"
    );
    Ok(m)
}
impl GraphicClip {
    pub fn validate(&self) -> Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        for layer in &self.layers {
            ensure!(
                layer.uid == 0 || ids.insert(layer.uid),
                "duplicate graphic layer uid"
            );
            if let Some(params) = &layer.original_parameters {
                super::graphic_template_animation::validate_parameters(params)?;
            }
            if let Some(pin) = &layer.pin {
                pin.validate()?;
            }
            match &layer.content {
                GraphicContent::Shape {
                    parameters,
                    animation,
                } => {
                    parameters.validate()?;
                    if let Some(a) = animation {
                        validate_shape(a)?;
                    }
                }
                GraphicContent::Text {
                    parameters,
                    animation,
                    source_text_animation,
                } => {
                    parameters.validate()?;
                    if let Some(a) = animation {
                        validate_text(a)?;
                    }
                    if let Some(a) = source_text_animation {
                        a.validate()?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn evaluated_layers(
        &self,
        canvas: (u32, u32),
        local: u64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<EvaluatedLayer>> {
        self.evaluated_layers_signed(canvas, i128::from(local), checkpoint)
    }
    pub(crate) fn evaluated_layers_signed(
        &self,
        canvas: (u32, u32),
        local: i128,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<EvaluatedLayer>> {
        self.validate()?;
        let mut layers = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            checkpoint()?;
            let sampled_content = if let Some(params) = &layer.original_parameters {
                let mut content = serde_json::to_value(&layer.content)?;
                content["parameters"] = super::graphic_template_animation::native_parameters(
                    &content["parameters"],
                    params,
                    local,
                )?;
                serde_json::from_value(content)?
            } else {
                layer.content.clone()
            };
            let content = match &sampled_content {
                GraphicContent::Shape {
                    parameters,
                    animation,
                } => {
                    let p = if let Some(a) = animation {
                        shape_sample(parameters.clone(), a, local, canvas)?
                    } else {
                        parameters.clone()
                    };
                    let (spec, _) = p.evaluate(layer.enabled, canvas)?;
                    let m = matrix(
                        p.position,
                        p.anchor,
                        p.scale,
                        p.scale_width,
                        p.uniform_scale,
                        p.rotation,
                        canvas,
                        [1., 0., 0., 1., 0., 0.],
                    )?;
                    (
                        EvaluatedContent::Shape(spec),
                        m,
                        p.position
                            .unwrap_or([canvas.0 as f64 / 2., canvas.1 as f64 / 2.]),
                        p.anchor,
                        [
                            if p.uniform_scale {
                                p.scale as f64 / 100.
                            } else {
                                p.scale_width as f64 / 100.
                            },
                            p.scale as f64 / 100.,
                        ],
                    )
                }
                GraphicContent::Text {
                    parameters,
                    animation,
                    source_text_animation,
                } => {
                    let mut p = if let Some(a) = animation {
                        text_sample(parameters.clone(), a, local, canvas)?
                    } else {
                        parameters.clone()
                    };
                    if let Some(a) = source_text_animation {
                        p.text = a.sample_signed(local)?.into();
                    }
                    let (spec, _) = p.evaluate(layer.enabled, canvas)?;
                    let m = matrix(
                        p.position,
                        p.anchor,
                        p.scale,
                        p.scale_width,
                        p.uniform_scale,
                        p.rotation,
                        canvas,
                        [1., 0., 0., 1., 0., 0.],
                    )?;
                    (
                        EvaluatedContent::Text(spec),
                        m,
                        p.position
                            .unwrap_or([canvas.0 as f64 / 2., canvas.1 as f64 / 2.]),
                        p.anchor,
                        [
                            if p.uniform_scale {
                                p.scale as f64 / 100.
                            } else {
                                p.scale_width as f64 / 100.
                            },
                            p.scale as f64 / 100.,
                        ],
                    )
                }
            };
            layers.push(EvaluatedLayer {
                uid: layer.uid,
                enabled: layer.enabled,
                pin: layer.pin.clone(),
                matrix: content.1,
                position: content.2,
                anchor: content.3,
                scale: content.4,
                content: content.0,
            });
        }
        apply_pins(&mut layers, canvas)?;
        checkpoint()?;
        Ok(layers)
    }
    /// Render directly into a transparent working frame without a media source or intermediate encoded pixels.
    pub fn render(
        &self,
        canvas: (u32, u32),
        output: (u32, u32),
        local: u64,
        prefix: [f64; 6],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<LinearColorFrame> {
        self.render_signed(canvas, output, i128::from(local), prefix, checkpoint)
    }
    /// Freeze imported and host-authored animation at exact Filmcraft ticks before rasterisation.
    pub fn snapshot_original_ticks(&self, canvas: (u32,u32), ticks: i128) -> Result<Self> {
        self.validate()?;
        let mut snapshot = self.clone();
        let time = super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(i64::try_from(ticks)?);
        for layer in &mut snapshot.layers {
            if let Some(params) = layer.original_parameters.take() {
                let mut content = serde_json::to_value(&layer.content)?;
                content["parameters"] = super::graphic_template_animation::native_parameters_at_ticks(&content["parameters"], &params, ticks)?;
                layer.content = serde_json::from_value(content)?;
            }
            match &mut layer.content {
                GraphicContent::Shape {parameters,animation} => {
                    if let Some(curve) = animation.take() { *parameters = super::graphic_shape_animation::sample(parameters.clone(), &curve, time, canvas)?; }
                }
                GraphicContent::Text {parameters,animation,source_text_animation} => {
                    if let Some(curve) = animation.take() { *parameters = super::graphic_text_animation::sample(parameters.clone(), &curve, time, canvas)?; }
                    if let Some(curve) = source_text_animation.take() { parameters.text = curve.sample_original_ticks(ticks)?.into(); }
                }
            }
        }
        Ok(snapshot)
    }
    pub fn render_signed(
        &self,
        canvas: (u32, u32),
        output: (u32, u32),
        local: i128,
        prefix: [f64; 6],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<LinearColorFrame> {
        self.render_with_roll(canvas, output, local, prefix, None, checkpoint)
    }
    pub fn render_with_roll(
        &self,
        canvas: (u32, u32),
        output: (u32, u32),
        local: i128,
        prefix: [f64; 6],
        roll: Option<(&super::graphic_roll::Roll, i64, i64)>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<LinearColorFrame> {
        self.validate()?;
        ensure!(
            canvas.0 > 0 && canvas.1 > 0 && output.0 > 0 && output.1 > 0,
            "empty graphic canvas"
        );
        ensure!(
            prefix.iter().all(|v| v.is_finite()),
            "invalid graphic canvas transform"
        );
        let (w, h) = (output.0 as usize, output.1 as usize);
        let bytes = w
            .checked_mul(h)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| anyhow::anyhow!("graphic canvas overflow"))?;
        ensure!(bytes <= 256 << 20, "graphic canvas too large");
        let mut frame = LinearColorFrame::from_surface(
            &GroupSurface {
                width: w,
                height: h,
                rgba: vec![0; bytes],
            },
            checkpoint,
        )?;
        let mut layers = self.evaluated_layers_signed(canvas, local, checkpoint)?;
        if let Some((roll, u, d)) = roll {
            let scales: Vec<_> = layers.iter().map(|layer| layer.scale).collect();
            super::graphic_roll_layers::apply_roll(&mut layers, &scales, roll, u, d, canvas)?;
        }
        for layer in layers {
            checkpoint()?;
            if !layer.enabled {
                continue;
            }
            let m = multiply(prefix, layer.matrix).map(|value| value as f32);
            ensure!(
                m.iter().all(|value| value.is_finite()),
                "graphic layer transform overflow"
            );
            let raster: Option<LayerRaster> = match &layer.content {
                EvaluatedContent::Shape(spec) => shape_raster(spec, &m, w, h),
                EvaluatedContent::Text(spec) => text_raster(spec, &m, w, h),
            };
            checkpoint()?;
            let Some(r) = raster else {
                continue;
            };
            let mut index = 0;
            frame.map_premultiplied(
                &mut |pixel| {
                    let (x, y) = ((index % w) as i64, (index / w) as i64);
                    index += 1;
                    let (sx, sy) = (x - r.x as i64, y - r.y as i64);
                    if sx < 0 || sy < 0 || sx >= r.img.w as i64 || sy >= r.img.h as i64 {
                        return pixel;
                    }
                    let offset = (sy as usize * r.img.w + sx as usize) * 4;
                    let s = &r.img.px[offset..offset + 4];
                    if s[3] <= 0. {
                        return pixel;
                    }
                    let k = 1. - s[3];
                    std::array::from_fn(|i| s[i] + pixel[i] * k)
                },
                checkpoint,
            )?;
        }
        checkpoint()?;
        Ok(frame)
    }
}
