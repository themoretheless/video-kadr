//! Write-on cancellation and refusal contracts, independent of the rendering oracle.
use crate::domain::{
    group_surface::GroupSurface,
    linear_color_frame::LinearColorFrame,
    write_on::WriteOn,
    write_on_animation::{self, WriteOnAnimation},
};
fn source() -> LinearColorFrame {
    LinearColorFrame::from_surface(
        &GroupSurface {
            width: 32,
            height: 18,
            rgba: vec![127; 32 * 18 * 4],
        },
        &mut || Ok(()),
    )
    .unwrap()
}
#[test]
fn cancellation_at_every_checkpoint_preserves_input() {
    let original = source();
    let parameters = WriteOn {
        spacing: 0.05,
        ..Default::default()
    };
    let mut count = 0;
    let mut frame = original.clone();
    parameters
        .apply_linear(
            &mut frame,
            filmcraft_time::Tick::from_seconds_f64(0.125),
            0.125,
            1.,
            &mut |tick| Ok([Some(3. + tick.seconds() * 100.), Some(9.)]),
            &mut || {
                count += 1;
                Ok(())
            },
        )
        .unwrap();
    assert!(count > 18);
    for stop in 1..=count {
        let mut frame = original.clone();
        let mut seen = 0;
        let result = parameters.apply_linear(
            &mut frame,
            filmcraft_time::Tick::from_seconds_f64(0.125),
            0.125,
            1.,
            &mut |tick| Ok([Some(3. + tick.seconds() * 100.), Some(9.)]),
            &mut || {
                seen += 1;
                anyhow::ensure!(seen != stop, "cancelled");
                Ok(())
            },
        );
        assert!(result.is_err(), "checkpoint {stop}");
        assert_eq!(frame.pixels(), original.pixels(), "checkpoint {stop}");
    }
    let mut frame = original.clone();
    let mut count = 0;
    assert!(parameters
        .apply_linear(
            &mut frame,
            filmcraft_time::Tick::from_seconds_f64(0.125),
            0.125,
            1.,
            &mut |_| {
                count += 1;
                anyhow::ensure!(count < 2, "history unavailable");
                Ok([Some(3.), Some(9.)])
            },
            &mut || Ok(())
        )
        .is_err());
    assert_eq!(frame.pixels(), original.pixels());
}
#[test]
fn clock_key_overflow_and_invalid_channels_are_refused_without_mutation() {
    let original = source();
    let parameters = WriteOn::default();
    let mut frame = original.clone();
    assert!(
        write_on_animation::apply(&parameters, None, &mut frame, u64::MAX, 1., &mut || Ok(()))
            .is_err()
    );
    assert_eq!(frame.pixels(), original.pixels());
    let animation:WriteOnAnimation=serde_json::from_value(serde_json::json!({"brushX":{"mode":"keyframes","track":{"timeBase":1,"interpolation":"linear","keyframes":[{"tick":0,"value":3.},{"tick":1000000000,"value":20.}]}}})).unwrap();
    let mut frame = original.clone();
    assert!(write_on_animation::apply(
        &parameters,
        Some(&animation),
        &mut frame,
        0,
        1.,
        &mut || Ok(())
    )
    .is_err());
    assert_eq!(frame.pixels(), original.pixels());
    for value in [
        serde_json::json!({"spacing":{"mode":"constant","value":0.}}),
        serde_json::json!({"size":{"mode":"constant","value":500.1}}),
        serde_json::json!({"opacity":{"mode":"constant","value":100.1}}),
    ] {
        let animation: WriteOnAnimation = serde_json::from_value(value).unwrap();
        assert!(write_on_animation::validate_animation(&animation).is_err());
    }
    assert!(serde_json::from_value::<WriteOnAnimation>(
        serde_json::json!({"unknown":{"mode":"constant","value":1.}})
    )
    .is_err());
}
