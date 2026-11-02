//! Responsive pins on evaluated vector/text content before rasterisation.
//! Original Filmcraft apply_pins arithmetic; attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    graphic_pin::{Pin, PinNode, resolve_pins_checked},
    graphic_shape_render::ShapeLayer,
    graphic_text_layout::text_layout,
    graphic_text_render::TextLayer,
};
use anyhow::Result;
pub enum EvaluatedContent {
    Shape(ShapeLayer),
    Text(TextLayer),
}
pub struct EvaluatedLayer {
    pub uid: u64,
    pub enabled: bool,
    pub pin: Option<Pin>,
    pub matrix: [f64; 6],
    pub position: [f64; 2],
    pub anchor: [f64; 2],
    pub scale: [f64; 2],
    pub content: EvaluatedContent,
}
impl EvaluatedLayer {
    pub fn bounds(&self) -> Result<[f64; 4]> {
        let b = match &self.content {
            EvaluatedContent::Text(spec) => text_layout(&spec.text).bounds,
            // Pins can invert an evaluated shape's size; preserve the upstream signed path.
            EvaluatedContent::Shape(spec) => spec
                .shape
                .path_with_signed_size()?
                .bounds()
                .map(|b| [b.0, b.1, b.2, b.3])
                .unwrap_or([0.; 4]),
        };
        let m = self.matrix;
        let mut bounds = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for (x, y) in [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])] {
            let (x, y) = (x as f64, y as f64);
            let p = [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
            bounds = [
                bounds[0].min(p[0]),
                bounds[1].min(p[1]),
                bounds[2].max(p[0]),
                bounds[3].max(p[1]),
            ];
        }
        Ok(bounds)
    }
}
pub fn apply_pins(layers: &mut [EvaluatedLayer], canvas: (u32, u32)) -> Result<()> {
    if !layers.iter().any(|layer| layer.pin.is_some()) {
        return Ok(());
    }
    let nodes: Vec<_> = layers
        .iter()
        .map(|layer| {
            Ok(PinNode {
                uid: layer.uid,
                pin: layer.pin.clone(),
                bounds: layer.bounds()?,
                resizable: matches!(layer.content, EvaluatedContent::Shape(_)),
            })
        })
        .collect::<Result<_>>()?;
    let resolved = resolve_pins_checked(&nodes, [0., 0., canvas.0 as f64, canvas.1 as f64])?;
    for ((layer, node), bounds) in layers.iter_mut().zip(&nodes).zip(resolved) {
        if bounds == node.bounds {
            continue;
        }
        let (ow, oh) = (
            node.bounds[2] - node.bounds[0],
            node.bounds[3] - node.bounds[1],
        );
        let (nw, nh) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
        if let EvaluatedContent::Shape(spec) = &mut layer.content {
            let kx = if ow > 1e-9 { nw / ow } else { 1. };
            let ky = if oh > 1e-9 { nh / oh } else { 1. };
            if (kx - 1.).abs() > 1e-12 || (ky - 1.).abs() > 1e-12 {
                spec.shape.size = [
                    (spec.shape.size[0] as f64 * kx) as f32,
                    (spec.shape.size[1] as f64 * ky) as f32,
                ];
                spec.shape.points.iter_mut().for_each(|point| {
                    *point = [(point[0] as f64 * kx) as f32, (point[1] as f64 * ky) as f32]
                });
            }
        }
        let now = layer.bounds()?;
        layer.position[0] = layer.position[0] + bounds[0] - now[0];
        layer.position[1] = layer.position[1] + bounds[1] - now[1];
        // Rebuild translation from the changed position, as Affine::motion does upstream.
        layer.matrix[4] = layer.matrix[0] * -layer.anchor[0]
            + layer.matrix[2] * -layer.anchor[1]
            + layer.position[0];
        layer.matrix[5] = layer.matrix[1] * -layer.anchor[0]
            + layer.matrix[3] * -layer.anchor[1]
            + layer.position[1];
    }
    Ok(())
}
