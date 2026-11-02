//! Original Filmcraft whole-graphic roll after responsive pins.
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use super::{
    graphic_pin_layers::{EvaluatedContent, EvaluatedLayer},
    graphic_roll::{Roll, RollMode, roll_offset},
};
use anyhow::{Result, ensure};
pub fn apply_roll(
    layers: &mut [EvaluatedLayer],
    scales: &[[f64; 2]],
    roll: &Roll,
    u: i64,
    d: i64,
    canvas: (u32, u32),
) -> Result<()> {
    if roll.mode == RollMode::Off {
        return Ok(());
    }
    ensure!(
        layers.len() == scales.len(),
        "graphic roll scale metadata mismatch"
    );
    let mut content: Option<[f64; 4]> = None;
    for (layer, scale) in layers.iter().zip(scales).filter(|(layer, _)| layer.enabled) {
        let mut bounds = layer.bounds()?;
        let background = match &layer.content {
            EvaluatedContent::Shape(spec) => &spec.appearance.background,
            EvaluatedContent::Text(spec) => &spec.appearance.background,
        };
        if let Some((_, pad, _)) = background {
            let k = scale[0].abs().max(scale[1].abs());
            let p = *pad as f64 * k;
            bounds = [bounds[0] - p, bounds[1] - p, bounds[2] + p, bounds[3] + p];
        }
        content = Some(match content {
            Some(a) => [
                a[0].min(bounds[0]),
                a[1].min(bounds[1]),
                a[2].max(bounds[2]),
                a[3].max(bounds[3]),
            ],
            None => bounds,
        });
    }
    let Some(content) = content else {
        return Ok(());
    };
    let (dx, dy) = roll_offset(roll, u, d, canvas, content);
    ensure!(dx.is_finite() && dy.is_finite(), "graphic roll overflow");
    for layer in layers {
        layer.position[0] += dx;
        layer.position[1] += dy;
        layer.matrix[4] = layer.matrix[0] * -layer.anchor[0]
            + layer.matrix[2] * -layer.anchor[1]
            + layer.position[0];
        layer.matrix[5] = layer.matrix[1] * -layer.anchor[0]
            + layer.matrix[3] * -layer.anchor[1]
            + layer.position[1];
        ensure!(
            layer.matrix.iter().all(|v| v.is_finite()),
            "graphic rolled transform overflow"
        );
    }
    Ok(())
}
