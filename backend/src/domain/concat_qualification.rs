//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in reference qualification without exposing private working-frame methods.
use super::{
    linear_color_frame::LinearColorFrame, transition_procedural, transition_spec::TransitionSpec,
};
use anyhow::{ensure, Result};
/// Exercise the production transition adapter on bounded constant straight-HDR
/// inputs, including hidden RGB. This module is absent from default builds.
pub fn constant_transition(
    spec: &TransitionSpec,
    side: usize,
    from: [f32; 4],
    to: [f32; 4],
    progress: f32,
    reverse: bool,
) -> Result<Vec<[f32; 4]>> {
    ensure!((1..=64).contains(&side), "qualification side must be1..64");
    spec.validate().map_err(anyhow::Error::msg)?;
    let frame = |pixel: [f32; 4]| -> Result<LinearColorFrame> {
        let mut frame =
            LinearColorFrame::from_working(side, side, &vec![0.; side * side * 4], &mut || Ok(()))?;
        frame.map_concat_straight_pixels(&mut |_, _, _| pixel, &mut || Ok(()))?;
        Ok(frame)
    };
    let mut out = transition_procedural::render(
        spec,
        &frame(from)?,
        &frame(to)?,
        progress,
        reverse,
        &mut || Ok(()),
    )?;
    out.concat_straight_snapshot(&mut || Ok(()))
}

/// Bounded spatial inputs for checking displacement and source selection.
pub fn spatial_transition(
    spec: &TransitionSpec,
    width: usize,
    height: usize,
    from: &[[f32; 4]],
    to: &[[f32; 4]],
    progress: f32,
    reverse: bool,
) -> Result<Vec<[f32; 4]>> {
    ensure!(
        (1..=64).contains(&width) && (1..=64).contains(&height),
        "qualification dimensions must be1..64"
    );
    ensure!(
        from.len() == width * height && to.len() == width * height,
        "qualification pixel count mismatch"
    );
    spec.validate().map_err(anyhow::Error::msg)?;
    let frame = |pixels: &[[f32; 4]]| -> Result<LinearColorFrame> {
        let mut f = LinearColorFrame::from_working(
            width,
            height,
            &vec![0.; width * height * 4],
            &mut || Ok(()),
        )?;
        f.map_concat_straight_pixels(&mut |_, x, y| pixels[y * width + x], &mut || Ok(()))?;
        Ok(f)
    };
    let mut out = transition_procedural::render(
        spec,
        &frame(from)?,
        &frame(to)?,
        progress,
        reverse,
        &mut || Ok(()),
    )?;
    out.concat_straight_snapshot(&mut || Ok(()))
}

/// Run the unchanged production sampler at supplied diagnostic coordinates.
pub fn texture_samples(
    width: usize,
    height: usize,
    pixels: &[[f32; 4]],
    coordinates: &[[f32; 2]],
) -> Result<Vec<[f32; 4]>> {
    ensure!(
        (1..=64).contains(&width)
            && (1..=64).contains(&height)
            && pixels.len() == width * height
            && coordinates.len() <= 4096,
        "qualification texture budget mismatch"
    );
    ensure!(
        pixels
            .iter()
            .all(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 65504.)
                && (0. ..=1.).contains(&p[3]))
            && coordinates.iter().flatten().all(|v| v.is_finite()),
        "invalid qualification texels/UV"
    );
    let pixels: Vec<[f32; 4]> = pixels
        .iter()
        .map(|p| p.map(|v| half::f16::from_f32(v).to_f32()))
        .collect();
    Ok(coordinates
        .iter()
        .map(|uv| super::concat_zoom_blur::read(&pixels, width, height, *uv))
        .collect())
}
