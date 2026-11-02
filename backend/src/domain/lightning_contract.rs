//! Cancellation/refusal contracts for the native Lightning working frame.
use crate::domain::{
    group_surface::GroupSurface, lightning::Lightning, linear_color_frame::LinearColorFrame,
};
fn source() -> LinearColorFrame {
    LinearColorFrame::from_surface(
        &GroupSurface {
            width: 12,
            height: 8,
            rgba: vec![127; 12 * 8 * 4],
        },
        &mut || Ok(()),
    )
    .unwrap()
}
#[test]
fn every_checkpoint_preserves_input_on_cancellation() {
    let original = source();
    let parameters = Lightning {
        start: [Some(1.), Some(4.)],
        end: [Some(11.), Some(4.)],
        segments: 2.,
        detail: 1.,
        branching: 0.,
        width: 0.5,
        ..Default::default()
    };
    let mut count = 0;
    let mut frame = original.clone();
    parameters
        .apply_linear(&mut frame, 0.5, 1., &mut || {
            count += 1;
            Ok(())
        })
        .unwrap();
    assert!(count > 8);
    for stop in 1..=count {
        let mut frame = original.clone();
        let mut seen = 0;
        let result = parameters.apply_linear(&mut frame, 0.5, 1., &mut || {
            seen += 1;
            anyhow::ensure!(seen != stop, "cancelled");
            Ok(())
        });
        assert!(result.is_err(), "checkpoint {stop}");
        assert_eq!(frame.pixels(), original.pixels(), "checkpoint {stop}");
    }
}
#[test]
fn invalid_parameters_and_context_refuse_without_mutation() {
    let original = source();
    for parameters in [
        Lightning {
            mode: 5,
            ..Default::default()
        },
        Lightning {
            detail: 10.1,
            ..Default::default()
        },
        Lightning {
            width: -0.1,
            ..Default::default()
        },
        Lightning {
            seed: 10000.1,
            ..Default::default()
        },
        Lightning {
            start: [Some(f64::NAN), None],
            ..Default::default()
        },
    ] {
        let mut frame = original.clone();
        assert!(parameters
            .apply_linear(&mut frame, 0.5, 1., &mut || Ok(()))
            .is_err());
        assert_eq!(frame.pixels(), original.pixels());
    }
    for (seconds, scale) in [
        (f64::NAN, 1.),
        (f64::INFINITY, 1.),
        (0., 0.),
        (0., f32::INFINITY),
    ] {
        let mut frame = original.clone();
        assert!(Lightning::default()
            .apply_linear(&mut frame, seconds, scale, &mut || Ok(()))
            .is_err());
        assert_eq!(frame.pixels(), original.pixels());
    }
}
