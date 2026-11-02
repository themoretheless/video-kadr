use serde_json::Value;
use video_kadr_backend::domain::{
    composition::VideoEffect, flat_surface, group_surface::GroupSurface,
};
#[test]
fn original_metal_zero_alpha_neighbors_and_retained_chain_colors() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/concat-zero-alpha.json")).unwrap();
    assert_eq!(
        fixture["reference"]["commit"],
        "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
    );
    assert_eq!(fixture["zeroAlphaColoredInput"], true);
    assert_eq!(fixture["adapter"]["backend"], "Metal");
    assert_eq!(fixture["upstreamProbes"], 6);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    let (mut visible, mut transparent, mut checked) = (0, 0, 0);
    let mut examples = vec![];
    for (index, c) in cases.iter().enumerate() {
        let side = c["side"].as_u64().unwrap() as usize;
        let rgba = serde_json::from_value::<Vec<u8>>(c["inputRgba"].clone()).unwrap();
        let mut output = GroupSurface {
            width: side,
            height: side,
            rgba,
        };
        let effects: Vec<VideoEffect> = serde_json::from_value(c["stages"].clone()).unwrap();
        flat_surface::apply_effect_stack(&mut output, &effects, 0, &[], &mut || Ok(())).unwrap();
        let expected = serde_json::from_value::<Vec<u8>>(c["expectedRgba"].clone()).unwrap();
        assert_eq!(output.rgba.len(), expected.len());
        for (pixel, (a, e)) in output
            .rgba
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
            .enumerate()
        {
            checked += 1;
            assert_eq!(a[3], e[3], "case{index} alpha{pixel}");
            for channel in 0..3 {
                if a[channel].abs_diff(e[channel]) > 1 {
                    if e[3] == 0 {
                        transparent += 1
                    } else {
                        visible += 1
                    }
                    if examples.len() < 20 {
                        examples.push((index, pixel, channel, a[channel], e[channel], e[3]));
                    }
                }
            }
        }
    }
    assert_eq!(checked, 5200);
    assert!(
        visible == 0 && transparent == 0,
        "visible mismatches{visible}, transparent mismatches{transparent}; {examples:?}"
    );
}

#[test]
fn transparent_working_colors_survive_reads_clones_and_early_cancellation() {
    use video_kadr_backend::domain::{
        concat_exposure::ConcatExposure, concat_spatial_filters::Sharpen,
        linear_color_frame::LinearColorFrame,
    };
    let (width, height) = (65, 3);
    let input = GroupSurface {
        width,
        height,
        rgba: (0..width * height)
            .flat_map(|i| {
                let zero = i % 5 == 0 || [63, 64, 127, 128, 194].contains(&i);
                if zero {
                    [if i % 2 == 0 { 255 } else { 30 }, 80, 140, 0]
                } else {
                    [
                        ((i * 31) % 256) as u8,
                        ((i * 53) % 256) as u8,
                        ((i * 79) % 256) as u8,
                        255,
                    ]
                }
            })
            .collect(),
    };
    let mut frame = LinearColorFrame::from_surface(&input, &mut || Ok(())).unwrap();
    assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), input);
    ConcatExposure { stops: 1.25 }
        .apply_linear(&mut frame, &mut || Ok(()))
        .unwrap();
    let before = frame.to_surface(&mut || Ok(())).unwrap();
    let mut fork = frame.clone();
    Sharpen { amount: 1. }
        .apply_linear(&mut fork, &mut || Ok(()))
        .unwrap();
    let expected = fork.to_surface(&mut || Ok(())).unwrap();
    assert_eq!(
        frame.to_surface(&mut || Ok(())).unwrap(),
        before,
        "clone modified original hidden colors"
    );
    assert!(Sharpen { amount: 1. }
        .apply_linear(&mut frame, &mut || anyhow::bail!(
            "cancelled before row output"
        ))
        .is_err());
    assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), before);
    Sharpen { amount: 1. }
        .apply_linear(&mut frame, &mut || Ok(()))
        .unwrap();
    assert_eq!(frame.to_surface(&mut || Ok(())).unwrap(), expected);
}
