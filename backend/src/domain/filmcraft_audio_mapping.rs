//! Direct project-to-DSP mappings copied from Filmcraft render/audio_fx.rs at
//! 5231852443363f001c3f6b396dd9b1e6461ae2be (MIT). Non-direct mappings remain separate.
use super::filmcraft_dsp::{EffectSpec, validate_specs};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
pub const DIRECT_PROJECT_EFFECTS: &[(&str, &str, f64)] = &[
    ("channel_mixer_a", "channel_mixer", 0.0),
    ("channel_volume_a", "channel_volume", 0.0),
    ("dynamics_rack", "dynamics_rack", 1.0),
    ("multiband_compressor", "multiband_compressor", 1.0),
    ("tube_compressor", "tube_compressor", 1.5),
    ("fft_filter", "fft_filter", 0.1),
    ("graphic_eq", "graphic_eq_10", 0.1),
    ("graphic_eq_20", "graphic_eq_20", 0.1),
    ("graphic_eq_30", "graphic_eq_30", 0.1),
    ("notch", "notch_filter", 0.1),
    ("parametric_eq", "parametric_eq_full", 0.1),
    ("scientific_filter", "scientific_filter", 0.2),
    ("chorus_flanger", "chorus_flanger", 0.1),
    ("flanger", "flanger", 0.2),
    ("phaser", "phaser", 0.1),
    ("declicker", "click_remover", 0.05),
    ("binauralizer", "binauralizer", 0.01),
    ("distortion", "distortion", 0.05),
    ("fill_left", "fill_left", 0.0),
    ("fill_right", "fill_right", 0.0),
    ("swap_channels", "swap_channels", 0.0),
    ("guitar_suite", "guitar_suite", 0.2),
    ("loudness_radar", "loudness_meter", 0.0),
    ("mastering", "mastering", 2.0),
    ("panner_ambisonics", "ambisonics_panner", 0.0),
    ("vocal_enhancer", "vocal_enhancer", 0.3),
    ("stereo_expander", "stereo_expander", 0.0),
    ("balance_a", "balance", 0.0),
    ("mute", "mute", 0.0),
    ("analog_delay", "analog_delay", 0.0),
    ("multitap_delay", "multitap_delay", 0.0),
    ("convolution_reverb", "convolution_reverb", 0.0),
    ("surround_reverb", "surround_reverb", 0.0),
];
/// Convert a direct project effect's native scalar parameters to its DSP factory id.
/// The caller must resolve animation at the processing clock before calling this function.
pub fn direct_project_effect(
    id: &str,
    params: BTreeMap<String, f32>,
    bypass: bool,
    rate: u32,
) -> Result<(EffectSpec, usize)> {
    ensure!(
        (8000..=192000).contains(&rate),
        "invalid DSP mapping sample rate"
    );
    let (_, dsp, seconds) = DIRECT_PROJECT_EFFECTS
        .iter()
        .find(|(project, _, _)| *project == id)
        .context("not a direct Filmcraft project effect")?;
    let spec = EffectSpec { concat_source_keys: Default::default(),
        id: (*dsp).into(),
        params,
        bypass,
        animation: BTreeMap::new(),
    };
    validate_specs(std::slice::from_ref(&spec))?;
    // Exact source direct_preroll: at least 120ms for native parameter smoothing.
    let preroll = if bypass {
        0
    } else {
        (seconds.max(0.12) * f64::from(rate)).ceil() as usize
    };
    Ok((spec, preroll))
}
/// Evaluate preroll-controlling authored parameters at the seek output clock.
pub fn native_export_preroll_at(
    specs: &[EffectSpec],
    rate: u32,
    local_seconds: f64,
) -> Result<usize> {
    ensure!((8000..=192000).contains(&rate), "invalid DSP export rate");
    validate_specs(specs)?;
    let concat_pre = super::concat_audio::preroll(specs, rate)?;
    let pre = specs.iter().filter(|spec| !spec.id.starts_with("concat.")).try_fold(0usize, |pre, spec| -> Result<usize> {
        let next = if let Some(id) = spec
            .id
            .strip_prefix(super::filmcraft_audio_project::PROJECT_PREFIX)
        {
            super::filmcraft_audio_project::resolve(
                id,
                &spec.authored_params_at_seconds(local_seconds)?,
                spec.bypass,
                rate,
            )?
            .1
        } else {
            native_export_preroll(&[spec.native_at_seconds(local_seconds, rate)?], rate)?
        };
        Ok(pre.max(next))
    })?;
    let history=pre.checked_add(concat_pre).context("DSP preroll overflow")?;
    if super::concat_audio::requires_full_history(specs,rate){
        ensure!(local_seconds.is_finite(),"invalid DSP source clock");
        let age=(local_seconds.max(0.)*f64::from(rate)).ceil();
        ensure!(age<=f64::from(rate)*6.*60.*60.,"full-history DSP seek exceeds six-hour budget");
        Ok(history.max(age as usize))
    }else{Ok(history)}
}
/// Export policy for raw DSP slots with source fixed or parameter-dependent preroll.
/// Missing authored parameters resolve to native DSP defaults.
pub fn native_export_preroll(specs: &[EffectSpec], rate: u32) -> Result<usize> {
    ensure!((8000..=192000).contains(&rate), "invalid DSP export rate");
    validate_specs(specs)?;
    ensure!(
        specs.iter().all(|s| s.animation.is_empty()),
        "DSP authored automation export binding pending"
    );
    let mut seconds = 0.0f64;
    let mut project_pre = 0usize;
    let concat_pre = super::concat_audio::preroll(specs, rate)?;
    for spec in specs.iter().filter(|e| !e.bypass && !e.id.starts_with("concat.")) {
        if let Some(id) = spec
            .id
            .strip_prefix(super::filmcraft_audio_project::PROJECT_PREFIX)
        {
            let pre = super::filmcraft_audio_project::resolve(
                id,
                &spec.authored_params_at_seconds(0.0)?,
                false,
                rate,
            )?
            .1;
            project_pre = project_pre.max(pre);
            continue;
        }
        let info = super::filmcraft_dsp::effect_info(&spec.id).context("unknown DSP effect")?;
        let value = |id: &str| -> Result<f64> {
            Ok(f64::from(
                spec.params
                    .get(id)
                    .copied()
                    .or_else(|| info.params.iter().find(|p| p.id == id).map(|p| p.default))
                    .context("missing DSP preroll parameter")?,
            ))
        };
        let echo_tail = |time: f64, feedback: f64| {
            let fb = (feedback / 100.0).clamp(0.0, 0.95);
            let repeats = if fb > 0.001 {
                (-4.0 / fb.log10()).clamp(1.0, 60.0)
            } else {
                1.0
            };
            time.max(1.0) / 1000.0 * repeats
        };
        let pre = match spec.id.as_str() {
            "amplify" | "invert" | "stereo_width" => 0.0,
            "parametric_eq" | "simple_eq" => 0.05,
            "compressor" => (value("release")? / 1000.0 * 5.0).clamp(0.05, 3.0),
            "limiter" => (value("release")? / 1000.0 * 5.0).clamp(0.05, 2.0),
            "delay" => echo_tail(value("time")?, value("feedback")?).min(8.0),
            "denoise" => 1.0,
            "dehum" => 0.2,
            "pitch_shifter" | "deesser" => 0.1,
            "dereverb" => (value("rt60")? + 0.3).clamp(0.5, 3.0),
            "speech_enhance" => 0.3,
            // Source studio_reverb applies min(8) to the exponential before *0.3.
            // Raw DSP decay is already the converted seconds, so its cap is2.4s.
            "reverb" => value("decay")?.min(2.4),
            "analog_delay" => echo_tail(value("delay")?, value("feedback")?).clamp(0.12, 8.0),
            "multitap_delay" => {
                let mut worst = 0.0f64;
                for k in 1..=4 {
                    worst = worst.max(echo_tail(
                        value(&format!("delay{k}"))?,
                        value(&format!("feedback{k}"))?,
                    ));
                }
                worst.clamp(0.12, 8.0)
            }
            "convolution_reverb" => {
                let rt = [0.45, 0.8, 1.9, 3.2, 1.6, 0.3, 0.2][(value("impulse")? as usize).min(6)];
                (rt * 1.1 * value("room_size")? / 100.0 + value("predelay")? / 1000.0)
                    .clamp(0.12, 4.0)
            }
            "surround_reverb" => (value("decay")? + value("predelay")? / 1000.0).clamp(0.12, 8.0),
            id => DIRECT_PROJECT_EFFECTS
                .iter()
                .find(|(_, dsp, _)| *dsp == id)
                .map(|(_, _, pre)| pre.max(0.12))
                .context("DSP preroll/export binding not yet available")?,
        };
        seconds = seconds.max(pre);
    }
    ((seconds * f64::from(rate)).ceil() as usize).max(project_pre).checked_add(concat_pre).context("DSP preroll overflow")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn special_dsp_policies_use_native_units_and_source_constants() {
        let make = |id: &str, key: &str, value: f32| EffectSpec { concat_source_keys: Default::default(),
            id: id.into(),
            params: BTreeMap::from([(key.into(), value)]),
            bypass: false,
            animation: BTreeMap::new(),
        };
        assert_eq!(
            native_export_preroll(&[make("compressor", "release", 200.0)], 48000).unwrap(),
            48000
        );
        assert_eq!(
            native_export_preroll(&[make("limiter", "release", 1000.0)], 48000).unwrap(),
            96000
        );
        assert_eq!(
            native_export_preroll(&[make("reverb", "decay", 10.0)], 48000).unwrap(),
            115200
        );
        assert_eq!(
            native_export_preroll(&[make("dereverb", "rt60", 1.0)], 48000).unwrap(),
            62400
        );
        for (id, pre) in [
            ("parametric_eq", 2400),
            ("simple_eq", 2400),
            ("denoise", 48000),
            ("dehum", 9600),
            ("pitch_shifter", 4800),
            ("deesser", 4800),
            ("speech_enhance", 14400),
        ] {
            assert_eq!(
                native_export_preroll(
                    &[EffectSpec { concat_source_keys: Default::default(),
                        id: id.into(),
                        ..Default::default()
                    }],
                    48000
                )
                .unwrap(),
                pre
            );
        }
        assert!(
            native_export_preroll(
                &[EffectSpec { concat_source_keys: Default::default(),
                    id: "gate".into(),
                    ..Default::default()
                }],
                48000
            )
            .is_err()
        );
    }
    #[test]
    fn dynamic_preroll_uses_authored_values_defaults_caps_and_bypass() {
        let spec = |id: &str, values: &[(&str, f32)]| EffectSpec { concat_source_keys: Default::default(),
            id: id.into(),
            params: values.iter().map(|(k, v)| ((*k).into(), *v)).collect(),
            bypass: false,
            animation: BTreeMap::new(),
        };
        for (effect, expected) in [
            (
                spec("analog_delay", &[("delay", 1000.0), ("feedback", 50.0)]),
                384000,
            ),
            (
                spec("analog_delay", &[("delay", 5.0), ("feedback", 0.0)]),
                5760,
            ),
            (
                spec(
                    "convolution_reverb",
                    &[("impulse", 3.0), ("room_size", 100.0), ("predelay", 500.0)],
                ),
                192000,
            ),
            (
                spec("surround_reverb", &[("decay", 4.0), ("predelay", 100.0)]),
                196800,
            ),
        ] {
            assert_eq!(
                native_export_preroll(&[effect.clone()], 48000).unwrap(),
                expected
            );
            assert_eq!(
                native_export_preroll(
                    &[EffectSpec { concat_source_keys: Default::default(),
                        bypass: true,
                        animation: BTreeMap::new(),
                        ..effect
                    }],
                    48000
                )
                .unwrap(),
                0
            );
        }
        for id in [
            "analog_delay",
            "multitap_delay",
            "convolution_reverb",
            "surround_reverb",
        ] {
            let pre = native_export_preroll(&[spec(id, &[])], 48000).unwrap();
            assert!((5760..=384000).contains(&pre));
        }
    }
    #[test]
    fn every_direct_mapping_has_native_parameters_and_source_preroll() {
        for (project, dsp, seconds) in DIRECT_PROJECT_EFFECTS {
            let info = filmcraft_audio_dsp::effect_info(dsp).unwrap();
            let params = info
                .params
                .iter()
                .map(|p| (p.id.into(), p.default))
                .collect();
            let (spec, pre) = direct_project_effect(project, params, false, 48000).unwrap();
            assert_eq!(spec.id, *dsp);
            assert_eq!(pre, (seconds.max(0.12) * 48000.0).ceil() as usize);
            assert_eq!(
                direct_project_effect(project, BTreeMap::new(), true, 48000)
                    .unwrap()
                    .1,
                0
            );
        }
        assert!(direct_project_effect("unknown", BTreeMap::new(), false, 48000).is_err());
        assert!(
            direct_project_effect(
                "mastering",
                BTreeMap::from([("unknown".into(), 1.0)]),
                false,
                48000
            )
            .is_err()
        );
    }
}
