use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_version: u32,
    working_space: String,
    samples: Vec<Sample>,
}

#[derive(Deserialize)]
struct Sample {
    rgb: [f64; 3],
    temperature: f64,
    tint: f64,
    highlights: f64,
    shadows: f64,
    expected: [f64; 3],
}

fn smoothstep(low: f64, high: f64, value: f64) -> f64 {
    let t = ((value - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn reference(sample: &Sample) -> [f64; 3] {
    let mut rgb = sample.rgb;
    rgb[0] *= 2_f64.powf(0.25 * sample.temperature - 0.10 * sample.tint);
    rgb[1] *= 2_f64.powf(0.20 * sample.tint);
    rgb[2] *= 2_f64.powf(-0.25 * sample.temperature - 0.10 * sample.tint);
    let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let shadows = 1.0 - smoothstep(0.0, 0.5, luma);
    let highlights = smoothstep(0.5, 1.0, luma);
    let gain = 2_f64.powf(0.75 * (sample.shadows * shadows + sample.highlights * highlights));
    rgb.map(|channel| (channel * gain).clamp(0.0, 1.0))
}

#[test]
fn shared_primary_correction_fixture_matches_reference_math() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../fixtures/color-grade/primary-corrections-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.working_space, "linear-srgb-d65");
    for sample in &fixture.samples {
        let actual = reference(sample);
        for channel in 0..3 {
            assert!(
                (actual[channel] - sample.expected[channel]).abs() <= 1e-8,
                "channel {channel}: {} != {}",
                actual[channel],
                sample.expected[channel]
            );
        }
    }
}
