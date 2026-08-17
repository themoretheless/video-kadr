//! Stage 4: advanced primary grade. Owned by the color agent.
//!
//! Reads `spec.color_advanced()` and emits white balance, exposure, highlight
//! and shadow recovery, the lift/gamma/gain wheels and per-band HSL. It runs
//! after geometry and before the existing look preset, curves and LUT stage,
//! so a creative look always sits on a corrected image.
//!
//! The whole grade is emitted inside the same explicit high-bit RGBA working
//! format `args.rs` already switches to for curves and 3D LUTs. Every filter
//! used here accepts `gbrap16le` (or promotes to `gbrapf32le` / `rgba64le`),
//! so nothing in the grade round-trips through 8 bits. `eq` is deliberately
//! avoided for exactly that reason: it is an 8-bit only filter.
//!
//! Emission order inside the stage, which is also the order a colourist
//! expects, is: white balance, exposure, highlight/shadow recovery,
//! lift/gamma/gain wheels, HSL secondaries.

use crate::domain::color_grade::{ColorAdvancedSpec, HslBand, RgbTriplet};
use crate::domain::edit::EditSpec;

use super::{ComplexPlan, RenderContext};

/// Same working format the LUT and curves stage uses. Keeping alpha here stops
/// a still-image grade from silently becoming opaque.
const WORKING_FORMAT: &str = "format=gbrap16le";

/// `colortemperature` neutral point. The filter multiplies by the RGB of the
/// requested black body, so a *lower* Kelvin makes the image warmer, which is
/// the inverse of the UI convention: our positive `temperature` means warmer.
const NEUTRAL_KELVIN: f64 = 6500.0;

/// Temperature and tint are unitless -1..1 sliders. One full stop of Kelvin per
/// unit keeps the mapping symmetric in log space: +1 -> 3250K, -1 -> 13000K.
const KELVIN_STOPS_PER_UNIT: f64 = 1.0;

/// How far one unit of `tint` pushes the green/magenta axis in `colorbalance`,
/// whose per-band offsets are themselves -1..1.
const TINT_BALANCE_SCALE: f64 = 0.5;

/// How far one unit of `highlights` / `shadows` moves its curve control point.
/// 0.2 keeps the recovery curve monotonic against the fixed 0.5 midpoint.
const TONE_RECOVERY_SCALE: f64 = 0.2;

/// x coordinates the wheel transfer function is sampled at before it is handed
/// to `curves`. Denser near black, where a gamma curve bends hardest.
const WHEEL_SAMPLES: [f64; 8] = [0.0, 0.0625, 0.125, 0.25, 0.375, 0.5, 0.75, 1.0];

/// Below this a control is treated as untouched and emits nothing at all.
const EPSILON: f64 = 1e-6;

pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, _ctx: &RenderContext) -> anyhow::Result<()> {
    let Some(grade) = spec.color_advanced() else {
        return Ok(());
    };
    let filters = grade_filters(grade);
    if filters.is_empty() {
        return Ok(());
    }
    // Only pay for the format switch once the grade actually emits something,
    // so a present-but-neutral `colorAdvanced` stays byte-identical to absent.
    let mut chain = Vec::with_capacity(filters.len() + 1);
    chain.push(WORKING_FORMAT.to_owned());
    chain.extend(filters);
    plan.chain_video(&chain, "grade")
}

/// The grade without the working-format switch, in documented stage order.
fn grade_filters(grade: &ColorAdvancedSpec) -> Vec<String> {
    let mut filters = Vec::new();
    filters.extend(white_balance_filters(grade));
    filters.extend(exposure_filter(grade));
    filters.extend(tone_recovery_filter(grade));
    filters.extend(wheels_filter(grade));
    filters.extend(hsl_filters(grade));
    filters
}

/// Clamp a wire-derived scalar and neutralise anything non-finite. The domain
/// layer already validated these, so this is defence in depth against a future
/// caller that builds a spec by hand.
fn sanitize(value: f64, fallback: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn white_balance_filters(grade: &ColorAdvancedSpec) -> Vec<String> {
    let mut filters = Vec::new();
    let temperature = sanitize(grade.temperature(), 0.0, -1.0, 1.0);
    if temperature.abs() > EPSILON {
        // Positive slider -> warmer image -> lower Kelvin, hence the negation.
        // `pl=1` keeps the correction chromatic instead of also darkening.
        let kelvin =
            (NEUTRAL_KELVIN * (-temperature * KELVIN_STOPS_PER_UNIT).exp2()).clamp(1000.0, 40000.0);
        filters.push(format!("colortemperature=temperature={kelvin:.0}:pl=1.000"));
    }
    let tint = sanitize(grade.tint(), 0.0, -1.0, 1.0);
    if tint.abs() > EPSILON {
        // `colortemperature` only walks the black body axis, so the
        // green/magenta axis goes through `colorbalance` midtones instead.
        // Positive tint is magenta: red and blue up, green down.
        let offset = tint * TINT_BALANCE_SCALE;
        filters.push(format!(
            "colorbalance=rm={offset:.3}:gm={:.3}:bm={offset:.3}:pl=1",
            -offset
        ));
    }
    filters
}

fn exposure_filter(grade: &ColorAdvancedSpec) -> Vec<String> {
    let stops = sanitize(grade.exposure_stops(), 0.0, -3.0, 3.0);
    if stops.abs() <= EPSILON {
        return Vec::new();
    }
    // The filter takes EV directly and its own range (-3..3) is wider than the
    // contract's -2..2, so this is a one-to-one mapping.
    vec![format!("exposure=exposure={stops:.3}")]
}

/// Highlight and shadow recovery. FFmpeg has no dedicated shadows/highlights
/// filter, so this is a master tone curve with the quarter-tone and
/// three-quarter-tone control points moved and the midpoint pinned. Positive
/// `highlights` brightens the highlights, negative recovers them, and the same
/// sign convention applies to `shadows`.
fn tone_recovery_filter(grade: &ColorAdvancedSpec) -> Vec<String> {
    let highlights = sanitize(grade.highlights(), 0.0, -1.0, 1.0);
    let shadows = sanitize(grade.shadows(), 0.0, -1.0, 1.0);
    if highlights.abs() <= EPSILON && shadows.abs() <= EPSILON {
        return Vec::new();
    }
    let low = (0.25 + shadows * TONE_RECOVERY_SCALE).clamp(0.02, 0.48);
    let high = (0.75 + highlights * TONE_RECOVERY_SCALE).clamp(0.52, 0.98);
    vec![format!(
        "curves=master='0/0 0.25/{low:.6} 0.5/0.5 0.75/{high:.6} 1/1':interp=pchip"
    )]
}

/// Lift, gamma and gain wheels.
///
/// Mapping rationale. `colorbalance` is a three-way *tint* control, not a
/// wheel, and `colorlevels` clamps every input and output level to -1..1, so it
/// cannot express `gain` above 1 nor gamma at all. `eq` has per-channel gamma
/// but is 8-bit only, which would break the high-bit working format. What is
/// left, and what is actually correct, is a per-channel `curves` transfer
/// function sampling the standard wheel formula
///
/// ```text
/// out_c(x) = clamp(x * gain_c + lift_c, 0, 1) ^ (1 / gamma_c)
/// ```
///
/// so lift offsets the whole channel, gain scales it, and gamma bends the
/// midtones. `interp=pchip` is monotone-preserving, and the function is
/// monotone for every value the domain layer admits, so the sampled curve never
/// overshoots between control points.
fn wheels_filter(grade: &ColorAdvancedSpec) -> Vec<String> {
    let lift = grade.lift().unwrap_or(RgbTriplet {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    });
    let gamma = grade.gamma().unwrap_or(RgbTriplet {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    });
    let gain = grade.gain().unwrap_or(RgbTriplet {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    });

    let channels = [
        ("red", lift.red(), gamma.red(), gain.red()),
        ("green", lift.green(), gamma.green(), gain.green()),
        ("blue", lift.blue(), gamma.blue(), gain.blue()),
    ];
    let mut options = Vec::new();
    for (name, lift, gamma, gain) in channels {
        let lift = sanitize(lift, 0.0, -0.5, 0.5);
        let gamma = sanitize(gamma, 1.0, 0.1, 4.0);
        let gain = sanitize(gain, 1.0, 0.0, 4.0);
        if lift.abs() <= EPSILON && (gamma - 1.0).abs() <= EPSILON && (gain - 1.0).abs() <= EPSILON
        {
            continue;
        }
        options.push(format!("{name}='{}'", wheel_points(lift, gamma, gain)));
    }
    if options.is_empty() {
        return Vec::new();
    }
    options.push("interp=pchip".into());
    vec![format!("curves={}", options.join(":"))]
}

fn wheel_points(lift: f64, gamma: f64, gain: f64) -> String {
    WHEEL_SAMPLES
        .into_iter()
        .map(|x| {
            let linear = (x * gain + lift).clamp(0.0, 1.0);
            let y = linear.powf(1.0 / gamma).clamp(0.0, 1.0);
            format!("{x:.6}/{y:.6}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Per-band HSL secondaries through `huesaturation`, which selects one of the
/// six primary/secondary hue ranges and shifts hue, saturation and intensity
/// inside it. Its saturation and intensity are *shifts* in -1..1 that the
/// filter turns into a `1 + shift` multiplier, so the contract's 0..4
/// multiplier maps as `shift = multiplier - 1` and saturates at 2x.
///
/// `huesaturation` has no orange range and neither does `selectivecolor`;
/// approximating orange by driving reds and yellows together would also move
/// pure reds and pure yellows, so the orange band is skipped rather than
/// emitted wrong.
fn hsl_filters(grade: &ColorAdvancedSpec) -> Vec<String> {
    let mut filters = Vec::new();
    // Iterate the canonical band order so the emitted chain is independent of
    // the order the bands arrived in on the wire.
    for band in HslBand::ALL {
        let Some(adjust) = grade.hsl().iter().find(|adjust| adjust.band() == band) else {
            continue;
        };
        let Some(colors) = huesaturation_colors(band) else {
            continue;
        };
        let hue = sanitize(adjust.hue_degrees(), 0.0, -180.0, 180.0);
        let saturation = sanitize(adjust.saturation(), 1.0, 0.0, 4.0) - 1.0;
        let intensity = sanitize(adjust.luminance(), 1.0, 0.0, 4.0) - 1.0;
        if hue.abs() <= EPSILON && saturation.abs() <= EPSILON && intensity.abs() <= EPSILON {
            continue;
        }
        filters.push(format!(
            "huesaturation=hue={hue:.3}:saturation={:.3}:intensity={:.3}:colors={colors}",
            saturation.clamp(-1.0, 1.0),
            intensity.clamp(-1.0, 1.0)
        ));
    }
    filters
}

/// `colors` flag for a band, or `None` when `huesaturation` has no such range.
const fn huesaturation_colors(band: HslBand) -> Option<&'static str> {
    match band {
        HslBand::Red => Some("r"),
        HslBand::Yellow => Some("y"),
        HslBand::Green => Some("g"),
        HslBand::Cyan => Some("c"),
        HslBand::Blue => Some("b"),
        HslBand::Magenta => Some("m"),
        // Not representable; see `hsl_filters`.
        HslBand::Orange => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::edit::EditExtensions;
    use crate::model;
    use crate::render::graph::InputSpec;

    fn neutral() -> model::ColorAdvanced {
        model::ColorAdvanced {
            temperature: 0.0,
            tint: 0.0,
            exposure: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            lift: None,
            gamma: None,
            gain: None,
            hsl: Vec::new(),
        }
    }

    fn spec_from(grade: model::ColorAdvanced) -> EditSpec {
        let mut request: model::EditRequest =
            serde_json::from_value(serde_json::json!({ "videoId": "x" })).unwrap();
        request.color_advanced = Some(grade);
        let extensions = EditExtensions::from_request(&request).unwrap();
        crate::services::render::EditPlan::compile(
            crate::domain::artifact_graph::Fingerprint::digest(b"source"),
            request,
            crate::services::render::SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap()
    }

    fn context() -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, true, Some(30.0), 10.0)
    }

    fn rendered(grade: model::ColorAdvanced) -> String {
        let spec = spec_from(grade);
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), true);
        apply(&mut plan, &spec, &context()).unwrap();
        plan.render()
    }

    #[test]
    fn an_absent_grade_leaves_the_plan_untouched() {
        let request: model::EditRequest =
            serde_json::from_value(serde_json::json!({ "videoId": "x" })).unwrap();
        let extensions = EditExtensions::from_request(&request).unwrap();
        let spec = crate::services::render::EditPlan::compile(
            crate::domain::artifact_graph::Fingerprint::digest(b"source"),
            request,
            crate::services::render::SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap();

        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), true);
        apply(&mut plan, &spec, &context()).unwrap();
        assert!(plan.is_trivial());
    }

    #[test]
    fn a_neutral_grade_emits_nothing_not_even_the_format_switch() {
        let mut grade = neutral();
        grade.lift = Some(model::Rgb {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        });
        grade.gamma = Some(model::Rgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        });
        grade.gain = Some(model::Rgb {
            r: 1.0,
            g: 1.0,
            b: 1.0,
        });
        grade.hsl = vec![model::HslBandAdjust {
            band: "blue".into(),
            hue: 0.0,
            saturation: 1.0,
            luminance: 1.0,
        }];
        assert_eq!(rendered(grade), "");
    }

    #[test]
    fn temperature_lowers_kelvin_for_a_warmer_image() {
        let mut warm = neutral();
        warm.temperature = 1.0;
        assert_eq!(
            rendered(warm),
            "[0:v]format=gbrap16le,colortemperature=temperature=3250:pl=1.000[grade_1]"
        );

        let mut cool = neutral();
        cool.temperature = -1.0;
        assert!(rendered(cool).contains("colortemperature=temperature=13000:pl=1.000"));
    }

    #[test]
    fn tint_walks_the_green_magenta_axis_through_colorbalance() {
        let mut magenta = neutral();
        magenta.tint = 0.4;
        assert!(
            rendered(magenta).contains("colorbalance=rm=0.200:gm=-0.200:bm=0.200:pl=1"),
            "positive tint must push midtones magenta"
        );

        let mut green = neutral();
        green.tint = -0.4;
        assert!(rendered(green).contains("colorbalance=rm=-0.200:gm=0.200:bm=-0.200:pl=1"));
    }

    #[test]
    fn exposure_maps_one_to_one_onto_the_exposure_filter() {
        let mut grade = neutral();
        grade.exposure = -1.5;
        assert!(rendered(grade).contains("exposure=exposure=-1.500"));
    }

    #[test]
    fn highlight_and_shadow_recovery_moves_only_the_quarter_tones() {
        let mut grade = neutral();
        grade.highlights = -1.0;
        grade.shadows = 0.5;
        let chain = rendered(grade);
        assert!(
            chain.contains(
                "curves=master='0/0 0.25/0.350000 0.5/0.5 0.75/0.550000 1/1':interp=pchip"
            ),
            "{chain}"
        );
    }

    #[test]
    fn wheels_become_a_per_channel_curve_and_skip_neutral_channels() {
        let mut grade = neutral();
        grade.gain = Some(model::Rgb {
            r: 2.0,
            g: 1.0,
            b: 1.0,
        });
        let chain = rendered(grade);
        // Only the red channel moved, so only `red` is emitted.
        assert!(chain.contains("curves=red='"), "{chain}");
        assert!(!chain.contains("green='"), "{chain}");
        assert!(!chain.contains("blue='"), "{chain}");
        // gain=2 clips at x=0.5 and is exactly 2x below it.
        assert!(chain.contains("0.000000/0.000000"), "{chain}");
        assert!(chain.contains("0.250000/0.500000"), "{chain}");
        assert!(chain.contains("0.500000/1.000000"), "{chain}");
        assert!(chain.contains("1.000000/1.000000"), "{chain}");
        assert!(chain.contains(":interp=pchip"), "{chain}");
    }

    #[test]
    fn lift_raises_black_and_gamma_bends_the_midtones() {
        let mut grade = neutral();
        grade.lift = Some(model::Rgb {
            r: 0.0,
            g: 0.1,
            b: 0.0,
        });
        grade.gamma = Some(model::Rgb {
            r: 1.0,
            g: 1.0,
            b: 2.0,
        });
        let chain = rendered(grade);
        // Green: out(0) = lift, out(1) = 1.
        assert!(chain.contains("green='0.000000/0.100000"), "{chain}");
        assert!(
            chain.contains("green='0.000000/0.100000 0.062500/0.162500"),
            "{chain}"
        );
        // Blue: gamma 2 means out(x) = sqrt(x), so 0.25 -> 0.5.
        assert!(chain.contains("blue='0.000000/0.000000"), "{chain}");
        assert!(chain.contains("0.250000/0.500000"), "{chain}");
    }

    #[test]
    fn hsl_bands_emit_huesaturation_in_canonical_order() {
        let mut grade = neutral();
        grade.hsl = vec![
            model::HslBandAdjust {
                band: "blue".into(),
                hue: 12.0,
                saturation: 1.0,
                luminance: 1.0,
            },
            model::HslBandAdjust {
                band: "red".into(),
                hue: 0.0,
                saturation: 1.5,
                luminance: 0.75,
            },
        ];
        let chain = rendered(grade);
        let red = chain.find("colors=r").unwrap();
        let blue = chain.find("colors=b").unwrap();
        assert!(
            red < blue,
            "wire order must not leak into the chain: {chain}"
        );
        assert!(
            chain.contains("huesaturation=hue=0.000:saturation=0.500:intensity=-0.250:colors=r"),
            "{chain}"
        );
        assert!(
            chain.contains("huesaturation=hue=12.000:saturation=0.000:intensity=0.000:colors=b"),
            "{chain}"
        );
    }

    #[test]
    fn a_saturation_multiplier_above_two_saturates_at_the_filter_maximum() {
        let mut grade = neutral();
        grade.hsl = vec![model::HslBandAdjust {
            band: "green".into(),
            hue: 0.0,
            saturation: 4.0,
            luminance: 1.0,
        }];
        assert!(rendered(grade).contains("saturation=1.000:intensity=0.000:colors=g"));
    }

    #[test]
    fn the_orange_band_is_skipped_because_ffmpeg_cannot_select_it() {
        let mut grade = neutral();
        grade.hsl = vec![
            model::HslBandAdjust {
                band: "orange".into(),
                hue: 30.0,
                saturation: 2.0,
                luminance: 1.0,
            },
            model::HslBandAdjust {
                band: "cyan".into(),
                hue: -5.0,
                saturation: 1.0,
                luminance: 1.0,
            },
        ];
        let chain = rendered(grade);
        assert!(!chain.contains("hue=30.000"), "{chain}");
        assert_eq!(chain.matches("huesaturation=").count(), 1, "{chain}");
        assert!(chain.contains("colors=c"), "{chain}");
    }

    #[test]
    fn a_combined_grade_emits_every_stage_in_the_documented_order() {
        let mut grade = neutral();
        grade.temperature = 0.5;
        grade.tint = 0.2;
        grade.exposure = 0.75;
        grade.highlights = -0.5;
        grade.shadows = 0.25;
        grade.lift = Some(model::Rgb {
            r: 0.05,
            g: 0.0,
            b: 0.0,
        });
        grade.gain = Some(model::Rgb {
            r: 1.0,
            g: 1.0,
            b: 1.2,
        });
        grade.hsl = vec![model::HslBandAdjust {
            band: "yellow".into(),
            hue: 8.0,
            saturation: 1.25,
            luminance: 1.0,
        }];
        let chain = rendered(grade);

        let stages = [
            "format=gbrap16le",
            "colortemperature=",
            "colorbalance=",
            "exposure=",
            "curves=master=",
            "curves=red=",
            "huesaturation=",
        ];
        let mut previous = 0;
        for stage in stages {
            let at = chain
                .find(stage)
                .unwrap_or_else(|| panic!("missing {stage} in {chain}"));
            assert!(at >= previous, "{stage} out of order in {chain}");
            previous = at;
        }
        // The whole grade is one chain hanging off the primary video pad.
        assert!(chain.starts_with("[0:v]format=gbrap16le,"), "{chain}");
        assert!(chain.ends_with("[grade_1]"), "{chain}");
    }

    #[test]
    fn non_finite_channels_fall_back_to_neutral_instead_of_leaking_nan() {
        // The domain layer rejects these, so build the spec directly to prove
        // the emitter itself never writes `NaN` into a filter string.
        let broken = ColorAdvancedSpec {
            temperature: f64::NAN,
            tint: f64::INFINITY,
            exposure_stops: f64::NAN,
            highlights: f64::NAN,
            shadows: f64::NAN,
            lift: Some(RgbTriplet {
                r: f64::NAN,
                g: 0.0,
                b: 0.0,
            }),
            gamma: None,
            gain: None,
            hsl: Vec::new(),
        };
        assert!(grade_filters(&broken).is_empty());
    }
}
