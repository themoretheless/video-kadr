//! Original Concat Metal pass precision, confined to Concat compatibility stages.
use super::linear_color_frame::LinearColorFrame;
use anyhow::Result;
// Original probe upload is RNE; original Metal render-target writes round to zero.
pub(crate) fn texture_half(value: f32) -> f32 {
    let nearest = half::f16::from_f32(value);
    if nearest.to_f32().abs() > value.abs() {
        half::f16::from_bits(nearest.to_bits() - 1).to_f32()
    } else {
        nearest.to_f32()
    }
}
pub(crate) fn apply_channels(
    frame: &mut LinearColorFrame,
    mut channel: impl FnMut(f32) -> f32,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    apply_rgb(frame, |rgb| rgb.map(&mut channel), checkpoint)
}
pub(crate) fn apply_rgb(
    frame: &mut LinearColorFrame,
    mut operation: impl FnMut([f32; 3]) -> [f32; 3],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    frame.map_concat_rgb_pixels(
        &mut |mut p, _, _| {
            if p[3] > 0.0 {
                let input_alpha = p[3];
                let alpha = half::f16::from_f32(input_alpha).to_f32();
                let input =
                    std::array::from_fn(|i| half::f16::from_f32(p[i] / input_alpha).to_f32());
                let output = operation(input);
                for i in 0..3 {
                    p[i] = texture_half(output[i].clamp(-65504.0, 65504.0)) * alpha;
                }
                p[3] = alpha;
            }
            p
        },
        checkpoint,
    )
}

pub(crate) fn straight_half(p: [f32; 4]) -> [f32; 3] {
    if p[3] > 0. {
        std::array::from_fn(|i| half::f16::from_f32(p[i] / p[3]).to_f32())
    } else {
        [0.; 3]
    }
}
pub(crate) fn store_half(mut p: [f32; 4], rgb: [f32; 3]) -> [f32; 4] {
    if p[3] > 0. {
        let alpha = half::f16::from_f32(p[3]).to_f32();
        for i in 0..3 {
            p[i] = texture_half(rgb[i].clamp(-65504., 65504.)) * alpha;
        }
        p[3] = alpha;
    }
    p
}
pub(crate) fn apply_rgb_at(
    frame: &mut LinearColorFrame,
    mut operation: impl FnMut([f32; 3], usize, usize) -> [f32; 3],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    frame.map_concat_rgb_pixels(
        &mut |p, x, y| store_half(p, operation(straight_half(p), x, y)),
        checkpoint,
    )
}
