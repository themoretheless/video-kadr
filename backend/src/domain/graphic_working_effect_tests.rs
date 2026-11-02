use super::{
    composition::VideoEffect, flat_surface::apply_effect_stack_working,
    group_surface::GroupSurface, linear_color_frame::LinearColorFrame,
};
fn input() -> LinearColorFrame {
    let mut frame =
        LinearColorFrame::from_surface(&GroupSurface::transparent(4, 2).unwrap(), &mut || Ok(()))
            .unwrap();
    let mut i = 0;
    frame
        .map_premultiplied(
            &mut |_| {
                i += 1;
                [0.12345679 + i as f32 * 0.001, 0.2345679, 0.3456789, 0.75]
            },
            &mut || Ok(()),
        )
        .unwrap();
    frame
}
#[test]
fn retained_graphic_effect_bridge_preserves_float_bits_and_flip_order() {
    let original = input();
    let mut empty = original.clone();
    apply_effect_stack_working(&mut empty, &[], 0, &[], None, &mut || Ok(())).unwrap();
    assert_eq!(empty.pixels(), original.pixels());
    let mut flipped = original.clone();
    apply_effect_stack_working(
        &mut flipped,
        &[VideoEffect::HorizontalFlip],
        0,
        &[],
        None,
        &mut || Ok(()),
    )
    .unwrap();
    let expected: Vec<_> = original
        .pixels()
        .chunks(4)
        .flat_map(|row| row.iter().rev().copied())
        .collect();
    assert_eq!(flipped.pixels(), expected);
}
#[test]
fn every_retained_effect_checkpoint_preserves_original_on_error() {
    let original = input();
    let mut count = 0;
    apply_effect_stack_working(
        &mut original.clone(),
        &[VideoEffect::HorizontalFlip],
        0,
        &[],
        None,
        &mut || {
            count += 1;
            Ok(())
        },
    )
    .unwrap();
    assert!(count > 0);
    for stop in 1..=count {
        let mut frame = original.clone();
        let mut seen = 0;
        let result = apply_effect_stack_working(
            &mut frame,
            &[VideoEffect::HorizontalFlip],
            0,
            &[],
            None,
            &mut || {
                seen += 1;
                anyhow::ensure!(seen != stop, "cancelled");
                Ok(())
            },
        );
        assert!(result.is_err(), "checkpoint{stop}");
        assert_eq!(frame.pixels(), original.pixels(), "checkpoint{stop}");
    }
}
