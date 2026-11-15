use anyhow::{ensure, Result};

use super::{
    composition::{AnimatableValue, Rgba, TextAlignment, TextClip},
    concat_title::{self, TitleStyle},
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
    clip.concat_title = Some(style);
    Ok(())
}
