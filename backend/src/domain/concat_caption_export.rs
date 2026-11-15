use anyhow::{ensure, Result};
use std::collections::BTreeMap;

use super::{
    composition::{AnimatableValue, Rgba, TextAlignment, TextClip, DEFAULT_TIME_BASE},
    concat_clip_animation::{Pose, SourceAnimation},
    concat_effect_upgrade::ParamKey,
    concat_title::{self, TitleStyle},
    elastic::Elastic,
    keyframes::{
        bounce_out, segment_interpolation, Extrapolation, Interpolation, KeyframeTrack,
        ProceduralInterpolation,
    },
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
    let transform = &mut clip.transform;
    let centered = clip.style.align == Some(TextAlignment::Center)
        && transform.anchor_x == 0.5
        && transform.anchor_y == 0.5
        && clip.text_anchor.is_none_or(|anchor| anchor == [0.5, 0.5]);
    ensure!(
        transform.concat_animation.as_ref().is_none_or(|animation| {
            animation.keys.keys().all(|key| {
                matches!(key.as_str(), "opacity" | "offsetX" | "offsetY")
                    || centered && matches!(key.as_str(), "scale" | "rotation")
            }) && (centered
                || (animation.rest.scale_x == 1.0
                    && animation.rest.scale_y == 1.0
                    && animation.rest.rotation_degrees == 0.0
                    && animation.stretch == [1.0, 1.0]))
        }),
        "Retained caption animation requires a Concat anchor mapping"
    );
    let constant = |value: &AnimatableValue, expected: f64| matches!(value, AnimatableValue::Constant { value } if *value == expected);
    ensure!(
        centered
            || (constant(&transform.scale_x, 1.0)
                && constant(&transform.scale_y, 1.0)
                && constant(&transform.rotation_degrees, 0.0)),
        "Transformed caption anchors require a Concat mapping"
    );

    let style = TitleStyle {
        font_family: clip.style.font_family.clone(),
        font_size: clip.style.font_size / f64::from(height),
        font_weight: clip.style.font_weight.unwrap_or(if clip.style.bold { 700.0 } else { 400.0 }),
        italic: clip.style.italic,
        opacity: clip.opacity_multiplier.unwrap_or(1.0),
        color: color(clip.style.color),
        align: clip.style.align.unwrap_or(TextAlignment::Left),
        stroke_width: clip.style.stroke_width.round() / f64::from(height),
        stroke_color: color(clip.style.stroke),
        background: if clip.style.background.alpha == 0.0 {
            String::new()
        } else {
            color(clip.style.background)
        },
        background_radius: f64::from(clip.style.background_radius.unwrap_or_default()) / f64::from(height),
        background_padding_x: f64::from(clip.style.background_padding.unwrap_or_default()[0]) / f64::from(height),
        background_padding_y: f64::from(clip.style.background_padding.unwrap_or_default()[1]) / f64::from(height),
        // Concat stores one shadow flag; its renderer owns the color, blur, and offset.
        shadow: clip.style.concat_shadow || clip.style.shadow.alpha > 0.0,
        tracking: f64::from(clip.style.tracking.unwrap_or_default()) / f64::from(height),
        line_height: clip.style.line_height.map_or(1.2, f64::from),
        max_width: f64::from(clip.text_box_width.unwrap_or(0.0)) / f64::from(width),
        max_height: f64::from(clip.text_box_height.unwrap_or(0.0)) / f64::from(height),
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
    let mut anchor_delta = [0.0, 0.0];
    if anchor_x != align_anchor || anchor_y != 0.5 {
        let fonts = super::concat_project_fonts::current();
        let (block_width, block_height) = concat_text::measure_block(
            fonts.as_deref().unwrap_or_else(|| concat_title::fonts()),
            &style.original(&clip.text),
            width,
            height,
        )?;
        anchor_delta = [
            (align_anchor - anchor_x) * f64::from(block_width),
            (0.5 - anchor_y) * f64::from(block_height),
        ];
        for (value, delta) in [&mut transform.x, &mut transform.y]
            .into_iter()
            .zip(anchor_delta)
        {
            let AnimatableValue::Constant { value } = value else {
                anyhow::bail!("Animated caption position requires a Concat anchor mapping");
            };
            *value += delta;
        }
    }
    if let Some(animation) = &mut transform.concat_animation {
        let dx = anchor_delta[0] / f64::from(width);
        let dy = anchor_delta[1] / f64::from(height);
        animation.rest.x += dx;
        animation.rest.y += dy;
        for (property, delta) in [("offsetX", dx), ("offsetY", dy)] {
            if let Some(keys) = animation.keys.get_mut(property) {
                for key in keys {
                    key.value += delta;
                    ensure!(
                        key.derived || (-3.0..=3.0).contains(&key.value),
                        "Caption position exceeds Concat key range"
                    );
                }
            }
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

const ELASTIC_OPACITY_ERROR: f64 = 0.0001;
const CONCAT_KEY_LIMIT: usize = 4096;

fn elastic_keys(
    curve: Elastic,
    left: f64,
    right: f64,
    ticks: i64,
    mut emit: impl FnMut(f64, f64) -> Result<()>,
) -> Result<()> {
    if ticks <= 1 || left == right {
        return Ok(());
    }
    let first = 1.0 / ticks as f64;
    let last = 1.0 - first;
    let delta = right - left;
    let raw = |position| left + delta * curve.sample(position);
    let mut emit_value = |position, value: f64| emit(position, value.clamp(0.0, 1.0));
    emit_value(first, raw(first))?;
    let mut intervals = vec![(first, last, 0)];
    if first < 0.5 && last > 0.5 {
        intervals = vec![(0.5, last, 0), (first, 0.5, 0)];
    }
    while let Some((start, end, depth)) = intervals.pop() {
        if start == end {
            continue;
        }
        let span = end - start;
        // Linear interpolation error is bounded by max |f''| times span² / 8.
        let error = delta.abs() * curve.curvature_bound(start, end) * span * span / 8.0;
        if error > ELASTIC_OPACITY_ERROR {
            ensure!(
                depth < 24 && error.is_finite(),
                "Elastic caption curve exceeds the Concat key budget"
            );
            let middle = start + span / 2.0;
            intervals.push((middle, end, depth + 1));
            intervals.push((start, middle, depth + 1));
            continue;
        }
        let a = raw(start);
        let b = raw(end);
        ensure!(a.is_finite() && b.is_finite(), "Invalid Elastic opacity");
        // Preserve the clipped chord exactly by placing a key at each crossing.
        let limits = if b < a { [1.0, 0.0] } else { [0.0, 1.0] };
        for limit in limits {
            if a.min(b) < limit && limit < a.max(b) {
                emit_value(start + span * (limit - a) / (b - a), limit)?;
            }
        }
        emit_value(end, b)?;
    }
    Ok(())
}

fn opacity_keys(track: &KeyframeTrack<f64>, duration: i64) -> Result<Vec<ParamKey>> {
    ensure!(
        track.time_base == u64::from(DEFAULT_TIME_BASE)
            && track.sample_clock.is_none()
            && track.extrapolation.unwrap_or_default() == Extrapolation::default(),
        "Caption opacity clock or extrapolation requires a Concat mapping"
    );
    let mut keys = Vec::with_capacity(track.keyframes.len());
    let key_at = |tick: f64, value, ease| ParamKey {
        derived: false,
        at: tick / duration as f64,
        value,
        ease,
    };
    for (index, key) in track.keyframes.iter().enumerate() {
        ensure!(
            (0..=duration).contains(&key.tick) && (0.0..=1.0).contains(&key.value),
            "Caption opacity key requires a Concat mapping"
        );
        let ease = if index == 0 {
            LINEAR
        } else {
            let previous = &track.keyframes[index - 1];
            match segment_interpolation(previous, key, track.interpolation) {
                Interpolation::Hold => {
                    if previous.value != key.value {
                        // Native time rounds to an integer microsecond. Bracket its
                        // half-tick boundary with distinct, serializable Concat keys.
                        let transition = key.tick as f64 - 0.5;
                        let margin = 1.0 / 2048.0;
                        keys.push(key_at(transition - margin, previous.value, LINEAR));
                        keys.push(key_at(transition + margin, key.value, LINEAR));
                    }
                    LINEAR
                }
                Interpolation::Linear => LINEAR,
                Interpolation::Procedural(ProceduralInterpolation::Bezier { p1, p2 }) => {
                    ensure!(
                        super::bezier::TimingBezier::new(p1, p2).is_some(),
                        "Invalid caption easing control points"
                    );
                    [p1[0], p1[1], p2[0], p2[1]]
                }
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
                Interpolation::Procedural(ProceduralInterpolation::Elastic {
                    dir,
                    amplitude,
                    period,
                }) => {
                    let curve = Elastic::new(dir, amplitude, period)
                        .ok_or_else(|| anyhow::anyhow!("Invalid Elastic opacity"))?;
                    let span = key.tick - previous.tick;
                    elastic_keys(
                        curve,
                        previous.value,
                        key.value,
                        span,
                        |progress, opacity| {
                            ensure!(
                                keys.len() < CONCAT_KEY_LIMIT - 1,
                                "Elastic caption curve exceeds the Concat key budget"
                            );
                            keys.push(key_at(
                                previous.tick as f64 + span as f64 * progress,
                                opacity,
                                LINEAR,
                            ));
                            Ok(())
                        },
                    )?;
                    LINEAR
                }
            }
        };
        keys.push(key_at(key.tick as f64, key.value, ease));
    }
    Ok(keys)
}

fn opacity_animation(clip: &mut TextClip, width: u32, height: u32) -> Result<()> {
    let transform = &mut clip.transform;
    if transform.concat_animation.is_some()
        && matches!(clip.opacity, AnimatableValue::Constant { .. })
    {
        return Ok(());
    }
    let (AnimatableValue::Constant { value: x }, AnimatableValue::Constant { value: y }) =
        (&transform.x, &transform.y)
    else {
        anyhow::bail!("Animated caption position requires a Concat mapping");
    };
    let (
        AnimatableValue::Constant { value: sx },
        AnimatableValue::Constant { value: sy },
        AnimatableValue::Constant { value: rotation },
    ) = (
        &transform.scale_x,
        &transform.scale_y,
        &transform.rotation_degrees,
    )
    else {
        anyhow::bail!("Animated caption pose requires a Concat mapping");
    };
    // A single shared scale must keep both independent stretches in range.
    let minimum = (sx.max(*sy) / 10.0).max(0.05);
    let maximum = (sx.min(*sy) / 0.1).min(8.0);
    let scale = 1.0_f64.max(minimum).min(maximum).clamp(0.05, 8.0);
    let stretch = [*sx / scale, *sy / scale];
    // Products at the source's inclusive limits can round by one ulp.
    ensure!(
        stretch
            .iter()
            .all(|value| (0.1 - 1e-14..=10.0 + 1e-14).contains(value)),
        "Caption scale and stretch cannot be represented in Concat"
    );
    let opacity = match &clip.opacity {
        AnimatableValue::Constant { .. } if scale == 1.0 => return Ok(()),
        AnimatableValue::Constant { value } => *value,
        AnimatableValue::Keyframes { .. } => 1.0,
    };
    let duration = i64::try_from(
        clip.timeline_end_tick
            .checked_sub(clip.timeline_start_tick)
            .filter(|duration| *duration > 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid caption duration"))?,
    )?;
    let keys = match &clip.opacity {
        AnimatableValue::Keyframes { track } => {
            BTreeMap::from([("opacity".to_owned(), opacity_keys(track, duration)?)])
        }
        AnimatableValue::Constant { .. } => BTreeMap::new(),
    };
    let animation = SourceAnimation {
        duration_ticks: duration,
        rest: Pose {
            scale_x: scale,
            scale_y: scale,
            x: *x / f64::from(width),
            y: *y / f64::from(height),
            rotation_degrees: *rotation,
            opacity,
        },
        keys,
        stretch: stretch.map(|value| value.clamp(0.1, 10.0)),
        canvas: [f64::from(width), f64::from(height)],
    };
    animation.validate()?;
    transform.concat_animation = Some(animation);
    clip.opacity = AnimatableValue::constant(1.0);
    Ok(())
}
