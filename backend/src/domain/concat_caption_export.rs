use anyhow::{ensure, Result};
use std::collections::BTreeMap;

use super::{
    composition::{AnimatableValue, Rgba, TextAlignment, TextClip, DEFAULT_TIME_BASE},
    concat_clip_animation::{Pose, SourceAnimation},
    concat_effect_upgrade::ParamKey,
    concat_title::{self, TitleStyle},
    keyframes::{bounce_out, Extrapolation, Interpolation},
};

fn color(value: Rgba) -> String {
    let byte = |channel: f64| (channel * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(value.red),
        byte(value.green),
        byte(value.blue),
        byte(value.alpha)
    )
}

pub(super) fn title(clip: &mut TextClip, width: u32, height: u32) -> Result<()> {
    ensure!(
        clip.style.shadow.alpha == 0.0,
        "Caption shadow parameters require a Concat mapping"
    );
    let transform = &mut clip.transform;
    ensure!(
        transform.concat_animation.is_none(),
        "Retained caption animation requires a Concat anchor mapping"
    );
    let constant = |value: &AnimatableValue, expected: f64| matches!(value, AnimatableValue::Constant { value } if *value == expected);
    ensure!(
        constant(&transform.scale_x, 1.0)
            && constant(&transform.scale_y, 1.0)
            && constant(&transform.rotation_degrees, 0.0),
        "Transformed caption anchors require a Concat mapping"
    );

    let style = TitleStyle {
        font_family: clip.style.font_family.clone(),
        font_size: clip.style.font_size / f64::from(height),
        font_weight: if clip.style.bold { 700.0 } else { 400.0 },
        italic: clip.style.italic,
        color: color(clip.style.color),
        align: clip.style.align.unwrap_or(TextAlignment::Left),
        stroke_width: clip.style.stroke_width.round() / f64::from(height),
        stroke_color: color(clip.style.stroke),
        background: if clip.style.background.alpha == 0.0 {
            String::new()
        } else {
            color(clip.style.background)
        },
        background_radius: 0.0,
        background_padding_x: 0.0,
        background_padding_y: 0.0,
        shadow: false,
        max_width: f64::from(clip.text_box_width.unwrap_or(0.0)) / f64::from(width),
        ..TitleStyle::default()
    };
    style.validate()?;
    let [anchor_x, anchor_y] = clip
        .text_anchor
        .unwrap_or([transform.anchor_x, transform.anchor_y]);
    let align_anchor = match style.align {
        TextAlignment::Left => 0.0,
        TextAlignment::Center => 0.5,
        TextAlignment::Right => 1.0,
    };
    if anchor_x != align_anchor || anchor_y != 0.5 {
        let fonts = super::concat_project_fonts::current();
        let (block_width, block_height) = concat_text::measure_block(
            fonts.as_deref().unwrap_or_else(|| concat_title::fonts()),
            &style.original(&clip.text),
            width,
            height,
        )?;
        for (value, delta) in [
            (
                &mut transform.x,
                (align_anchor - anchor_x) * f64::from(block_width),
            ),
            (&mut transform.y, (0.5 - anchor_y) * f64::from(block_height)),
        ] {
            let AnimatableValue::Constant { value } = value else {
                anyhow::bail!("Animated caption position requires a Concat anchor mapping");
            };
            *value += delta;
        }
    }
    opacity_animation(clip, width, height)?;
    clip.concat_title = Some(style);
    Ok(())
}

// Concat stores the easing on each segment's incoming key.
const LINEAR: [f64; 4] = [0.0, 0.0, 1.0, 1.0];
const CUBIC_IN: [f64; 4] = [1.0 / 3.0, 0.0, 2.0 / 3.0, 0.0];
const CUBIC_OUT: [f64; 4] = [1.0 / 3.0, 1.0, 2.0 / 3.0, 1.0];
const QUADRATIC_IN: [f64; 4] = [1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0 / 3.0];
const QUADRATIC_OUT: [f64; 4] = [1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 1.0];

fn bounce_segments(interpolation: Interpolation, mut endpoint: impl FnMut(f64, f64, [f64; 4])) {
    // Native bounce_out changes quadratic or reverses direction at these points.
    const BREAKS: [f64; 8] = [0.0, 8.0, 12.0, 16.0, 18.0, 20.0, 21.0, 22.0];
    let mut half = |offset, scale, reversed| {
        for index in 1..BREAKS.len() {
            let (time, value) = if reversed {
                let t = BREAKS[BREAKS.len() - 1 - index] / 22.0;
                (1.0 - t, 1.0 - bounce_out(t))
            } else {
                let t = BREAKS[index] / 22.0;
                (t, bounce_out(t))
            };
            let ease = if (index % 2 == 1) ^ reversed {
                QUADRATIC_IN
            } else {
                QUADRATIC_OUT
            };
            endpoint(offset + scale * time, offset + scale * value, ease);
        }
    };
    match interpolation {
        Interpolation::BounceIn => half(0.0, 1.0, true),
        Interpolation::BounceOut => half(0.0, 1.0, false),
        Interpolation::BounceInOut => {
            half(0.0, 0.5, true);
            half(0.5, 0.5, false);
        }
        _ => unreachable!("requires bounce interpolation"),
    }
}

fn opacity_animation(clip: &mut TextClip, width: u32, height: u32) -> Result<()> {
    let AnimatableValue::Keyframes { track } = &clip.opacity else {
        return Ok(());
    };
    ensure!(
        track.time_base == u64::from(DEFAULT_TIME_BASE)
            && track.sample_clock.is_none()
            && track.extrapolation.unwrap_or_default() == Extrapolation::default(),
        "Caption opacity clock or extrapolation requires a Concat mapping"
    );
    let duration = i64::try_from(
        clip.timeline_end_tick
            .checked_sub(clip.timeline_start_tick)
            .filter(|duration| *duration > 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid caption duration"))?,
    )?;
    let mut keys = Vec::with_capacity(track.keyframes.len());
    let key_at = |tick: f64, value, ease| ParamKey {
        derived: false,
        at: tick / duration as f64,
        value,
        ease,
    };
    for (index, key) in track.keyframes.iter().enumerate() {
        ensure!(
            key.tangents.is_none()
                && (0..=duration).contains(&key.tick)
                && (0.0..=1.0).contains(&key.value),
            "Caption opacity key requires a Concat mapping"
        );
        let ease = if index == 0 {
            LINEAR
        } else {
            let previous = &track.keyframes[index - 1];
            match previous.interpolation.unwrap_or(track.interpolation) {
                Interpolation::Linear => LINEAR,
                Interpolation::EaseIn => CUBIC_IN,
                Interpolation::EaseOut => CUBIC_OUT,
                Interpolation::EaseInOut | Interpolation::EaseInOutCubic => {
                    // The native curve changes cubic at its exact midpoint.
                    let tick = (previous.tick as f64 + key.tick as f64) / 2.0;
                    let value = (previous.value + key.value) / 2.0;
                    keys.push(key_at(tick, value, CUBIC_IN));
                    CUBIC_OUT
                }
                mode @ (Interpolation::BounceIn
                | Interpolation::BounceOut
                | Interpolation::BounceInOut) => {
                    bounce_segments(mode, |progress, value, ease| {
                        let tick = if progress == 1.0 {
                            key.tick as f64
                        } else {
                            previous.tick as f64 + (key.tick - previous.tick) as f64 * progress
                        };
                        let opacity = if progress == 1.0 {
                            key.value
                        } else {
                            previous.value + (key.value - previous.value) * value.clamp(0.0, 1.0)
                        };
                        keys.push(key_at(tick, opacity, ease));
                    });
                    continue;
                }
                _ => anyhow::bail!("Caption opacity key requires a Concat mapping"),
            }
        };
        keys.push(key_at(key.tick as f64, key.value, ease));
    }
    let transform = &mut clip.transform;
    let (AnimatableValue::Constant { value: x }, AnimatableValue::Constant { value: y }) =
        (&transform.x, &transform.y)
    else {
        anyhow::bail!("Animated caption position requires a Concat mapping");
    };
    let animation = SourceAnimation {
        duration_ticks: duration,
        rest: Pose {
            scale_x: 1.0,
            scale_y: 1.0,
            x: *x / f64::from(width),
            y: *y / f64::from(height),
            rotation_degrees: 0.0,
            opacity: 1.0,
        },
        keys: BTreeMap::from([("opacity".to_owned(), keys)]),
        stretch: [1.0, 1.0],
        canvas: [f64::from(width), f64::from(height)],
    };
    animation.validate()?;
    transform.concat_animation = Some(animation);
    clip.opacity = AnimatableValue::constant(1.0);
    Ok(())
}
