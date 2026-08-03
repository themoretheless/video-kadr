use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    schema_version: u32,
    working_space: String,
    parameter_range: [f64; 2],
    samples: Vec<Sample>,
    tolerances: Tolerances,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tolerances {
    cpu_absolute: f64,
}

#[derive(Clone, Deserialize)]
struct Wheel {
    master: f64,
    red: f64,
    green: f64,
    blue: f64,
}

impl Wheel {
    fn channel(&self, channel: usize) -> f64 {
        self.master + [self.red, self.green, self.blue][channel]
    }
}

#[derive(Deserialize)]
struct Sample {
    rgb: [f64; 3],
    lift: Wheel,
    gamma: Wheel,
    gain: Wheel,
    expected: [f64; 3],
}

fn reference(sample: &Sample) -> [f64; 3] {
    std::array::from_fn(|channel| {
        let lifted = (sample.rgb[channel] + 0.25 * sample.lift.channel(channel)).max(0.0);
        let gamma = 2_f64.powf(-sample.gamma.channel(channel));
        let gain = 2_f64.powf(sample.gain.channel(channel));
        lifted.powf(gamma).mul_add(gain, 0.0).clamp(0.0, 1.0)
    })
}

#[test]
fn shared_lift_gamma_gain_fixture_matches_reference_math() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../fixtures/color-grade/lift-gamma-gain-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.working_space, "linear-srgb-d65");
    assert_eq!(fixture.parameter_range, [-1.0, 1.0]);
    for sample in &fixture.samples {
        let actual = reference(sample);
        for channel in 0..3 {
            assert!(
                (actual[channel] - sample.expected[channel]).abs()
                    <= fixture.tolerances.cpu_absolute,
                "channel {channel}: {} != {}",
                actual[channel],
                sample.expected[channel]
            );
        }
    }
}
