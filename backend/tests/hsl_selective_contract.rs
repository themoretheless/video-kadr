use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selection {
    center_degrees: f64,
    half_width_degrees: f64,
    feather_degrees: f64,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Adjustment {
    hue_degrees: f64,
    saturation: f64,
    lightness: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    rgb: [f64; 3],
    selection: Selection,
    adjustment: Adjustment,
    expected_hsl: [f64; 3],
    expected_mask: f64,
    expected_rgb: [f64; 3],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_version: u32,
    working_space: String,
    order: Vec<String>,
    samples: Vec<Sample>,
    tolerances: Tolerances,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tolerances {
    cpu_absolute: f64,
}

fn rgb_to_hsl([r, g, b]: [f64; 3]) -> ([f64; 3], bool) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let lightness = (max + min) / 2.0;
    if chroma == 0.0 {
        return ([0.0, 0.0, lightness], true);
    }
    let hue_sector = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    let saturation = chroma / (1.0 - (2.0 * lightness - 1.0).abs());
    ([hue_sector * 60.0, saturation, lightness], false)
}

fn hsl_to_rgb([hue, saturation, lightness]: [f64; 3]) -> [f64; 3] {
    let h = hue.rem_euclid(360.0) / 60.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let [r, g, b] = match h.floor() as usize {
        0 => [chroma, x, 0.0],
        1 => [x, chroma, 0.0],
        2 => [0.0, chroma, x],
        3 => [0.0, x, chroma],
        4 => [x, 0.0, chroma],
        _ => [chroma, 0.0, x],
    };
    let m = lightness - chroma / 2.0;
    [r + m, g + m, b + m].map(|value| value.clamp(0.0, 1.0))
}

fn reference(sample: &Sample) -> ([f64; 3], f64, [f64; 3]) {
    let (hsl, achromatic) = rgb_to_hsl(sample.rgb);
    let distance = (hsl[0] - sample.selection.center_degrees + 540.0).rem_euclid(360.0) - 180.0;
    let distance = distance.abs();
    let mask = if achromatic
        || distance >= sample.selection.half_width_degrees + sample.selection.feather_degrees
    {
        if !achromatic && distance <= sample.selection.half_width_degrees {
            1.0
        } else {
            0.0
        }
    } else if distance <= sample.selection.half_width_degrees {
        1.0
    } else {
        let t = (distance - sample.selection.half_width_degrees) / sample.selection.feather_degrees;
        1.0 - t * t * (3.0 - 2.0 * t)
    };
    let adjusted = [
        (hsl[0] + mask * sample.adjustment.hue_degrees).rem_euclid(360.0),
        (hsl[1] + mask * sample.adjustment.saturation).clamp(0.0, 1.0),
        (hsl[2] + 0.25 * mask * sample.adjustment.lightness).clamp(0.0, 1.0),
    ];
    (hsl, mask, hsl_to_rgb(adjusted))
}

#[test]
fn shared_selective_hsl_fixture_matches_reference_math() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../fixtures/color-grade/hsl-selective-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.working_space, "encoded-srgb-iec61966-2-1");
    assert_eq!(fixture.order[6], "hsl-selective-v1");
    for sample in &fixture.samples {
        let (hsl, mask, rgb) = reference(sample);
        for (actual, expected) in hsl.into_iter().zip(sample.expected_hsl) {
            assert!((actual - expected).abs() <= fixture.tolerances.cpu_absolute);
        }
        assert!((mask - sample.expected_mask).abs() <= fixture.tolerances.cpu_absolute);
        for (actual, expected) in rgb.into_iter().zip(sample.expected_rgb) {
            assert!((actual - expected).abs() <= fixture.tolerances.cpu_absolute);
        }
    }
}
