//! Filmcraft project audio scalar dispatch at pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! MIT source: crates/render/src/audio_fx.rs. Callers resolve media-time curves first.
use super::filmcraft_audio_mapping::{DIRECT_PROJECT_EFFECTS, native_export_preroll};
use super::filmcraft_dsp::EffectSpec;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

pub const PROJECT_PREFIX: &str = "filmcraft:";

pub fn validate_authored(spec: &EffectSpec) -> Result<()> {
    let id = spec
        .id
        .strip_prefix(PROJECT_PREFIX)
        .context("not a project audio slot")?;
    let def = catalog()["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .context("unknown project audio slot")?;
    ensure!(
        def["intrinsic"] == false,
        "intrinsic audio slot requires mixer"
    );
    let check = |key: &str, value: f64| -> Result<()> {
        let param = def["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == key)
            .context("unknown project audio parameter")?;
        let kind = &param["kind"];
        let (min, max, discrete) = if let Some(float) = kind.get("Float") {
            (
                float["min"].as_f64().unwrap(),
                float["max"].as_f64().unwrap(),
                false,
            )
        } else if let Some(choices) = kind.get("Choice") {
            (
                0.0,
                choices.as_array().unwrap().len().saturating_sub(1) as f64,
                true,
            )
        } else {
            (0.0, 1.0, true)
        };
        ensure!(
            value.is_finite()
                && value >= min
                && value <= max
                && (!discrete || value.fract() == 0.0),
            "project audio parameter out of range"
        );
        Ok(())
    };
    for (key, value) in &spec.params {
        // Static storage is f32: validate against bounds rounded to that same domain.
        let parameter = def["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|parameter| parameter["id"] == key.as_str())
            .context("unknown project audio parameter")?;
        let mut scalar = f64::from(*value);
        if let Some(bounds) = parameter["kind"].get("Float") {
            for bound in ["min", "max"] {
                if let Some(exact) = bounds[bound].as_f64() {
                    if *value == exact as f32 {
                        scalar = exact;
                    }
                }
            }
        }
        check(key, scalar)?;
    }
    ensure!(
        spec.animation.len() <= 128,
        "project audio animation budget exceeded"
    );
    let mut keys = 0usize;
    for (key, curve) in &spec.animation {
        curve
            .validate()
            .map_err(|e| anyhow::anyhow!("invalid project audio curve: {e:?}"))?;
        match curve {
            super::composition::AnimatableValue::Constant { value } => check(key, *value)?,
            super::composition::AnimatableValue::Keyframes { track } => {
                keys = keys
                    .checked_add(track.keyframes.len())
                    .context("project audio key overflow")?;
                ensure!(keys <= 8192, "project audio key budget exceeded");
                for point in &track.keyframes {
                    check(key, point.value)?;
                }
            }
        }
    }
    Ok(())
}

pub fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("filmcraft_audio_project_catalog.json"))
            .expect("checked-in project audio catalog")
    })
}

/// Translate resolved source project parameters, including omitted source defaults.
/// Intrinsic volume/panner entries belong to the host mixer and are rejected here.
pub fn resolve(
    id: &str,
    values: &BTreeMap<String, f64>,
    bypass: bool,
    rate: u32,
) -> Result<(EffectSpec, usize)> {
    let def = catalog()["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .context("unknown Filmcraft project audio effect")?;
    ensure!(
        def["intrinsic"] == false,
        "intrinsic audio effect requires mixer binding"
    );
    let definitions = def["params"].as_array().unwrap();
    ensure!(
        values
            .iter()
            .all(|(key, value)| value.is_finite()
                && definitions.iter().any(|p| p["id"] == key.as_str())),
        "unknown or non-finite project audio parameter"
    );
    let mut source_scalars = BTreeMap::new();
    let mut resolved = BTreeMap::new();
    for p in definitions {
        let key = p["id"].as_str().unwrap();
        let default = p["default"].as_object().unwrap().values().next().unwrap();
        let default = default.as_f64().unwrap_or_else(|| {
            if default.as_bool() == Some(true) {
                1.0
            } else {
                0.0
            }
        });
        source_scalars.insert(key.to_owned(), values.get(key).copied().unwrap_or(default));
        resolved.insert(
            key.to_owned(),
            values.get(key).copied().unwrap_or(default) as f32,
        );
    }
    ensure!(
        resolved.values().all(|v| v.is_finite()),
        "project audio scalar overflow"
    );
    let f = |key: &str| resolved.get(key).copied().unwrap_or(0.0);
    let mut params = BTreeMap::new();
    let dsp = match id {
        "amplify" => {
            params.insert("gain".into(), f("gain"));
            "amplify"
        }
        "dynamics" | "single_band_compressor" => {
            for key in ["threshold", "ratio", "attack", "release"] {
                params.insert(key.into(), f(key));
            }
            if id == "single_band_compressor" {
                params.insert("knee".into(), 0.0);
                params.insert("makeup".into(), f("output"));
            }
            "compressor"
        }
        "hard_limiter" => {
            params.insert("ceiling".into(), f("max"));
            params.insert("input_gain".into(), f("boost").max(0.0));
            params.insert("release".into(), f("release"));
            "limiter"
        }
        "delay" => {
            params.insert("time".into(), f("delay") * 1000.0);
            params.insert("feedback".into(), f("feedback"));
            params.insert("mix".into(), f("mix"));
            "delay"
        }
        // Preserve dispatcher precedence over the upstream DIRECT table's same id.
        "parametric_eq" => {
            for (dst, src) in [
                ("low_freq", "low_freq"),
                ("low", "low_gain"),
                ("mid_freq", "mid_freq"),
                ("mid", "mid_gain"),
                ("mid_q", "mid_q"),
                ("high_freq", "high_freq"),
                ("high", "high_gain"),
            ] {
                params.insert(dst.into(), f(src));
            }
            "simple_eq"
        }
        "highpass" | "lowpass" | "bandpass" | "bass" | "treble" | "simple_notch" | "simple_eq" => {
            let (kind, freq, q) = match id {
                "highpass" => (4.0, f("cutoff"), 0.707),
                "lowpass" => (3.0, f("cutoff"), 0.707),
                "bandpass" => (6.0, f("center"), f("q")),
                "bass" => (1.0, 200.0, std::f32::consts::FRAC_1_SQRT_2),
                "treble" => (2.0, 4000.0, std::f32::consts::FRAC_1_SQRT_2),
                "simple_notch" => (5.0, f("center"), f("q")),
                _ => (0.0, f("center"), f("q")),
            };
            for (key, value) in [
                ("b1.on", 1.0),
                ("b1.type", kind),
                ("b1.freq", freq),
                ("b1.q", q),
            ] {
                params.insert(key.into(), value);
            }
            if matches!(id, "bass" | "treble" | "simple_eq") {
                params.insert("b1.gain".into(), f("boost"));
            }
            "parametric_eq"
        }
        "denoise" => {
            params.insert("reduction".into(), f("amount") * 0.4);
            "denoise"
        }
        "dehummer" => {
            params.insert("frequency".into(), f("freq"));
            params.insert(
                "amount".into(),
                (1.0 - 10f32.powf(f("gain").min(0.0) / 20.0)) * 100.0,
            );
            params.insert("harmonics".into(), f("harmonics"));
            params.insert("q".into(), f("q"));
            "dehum"
        }
        "deesser" | "dereverb" | "speech_enhance" | "stereo_width" => {
            params = resolved.clone();
            id
        }
        "studio_reverb" => {
            params.insert("size".into(), f("room"));
            params.insert(
                "decay".into(),
                (0.3 * 30f32.powf(f("decay") / 100.0)).clamp(0.1, 20.0),
            );
            params.insert("damping".into(), f("damping"));
            let (dry, wet) = (f("dry").max(0.0), f("wet").max(0.0));
            params.insert(
                "mix".into(),
                if dry + wet > 0.0 {
                    wet / (dry + wet) * 100.0
                } else {
                    0.0
                },
            );
            "reverb"
        }
        "invert_a" => "invert",
        "pitch_shifter" => {
            params.insert(
                "semitones".into(),
                (f("semitones") + f("cents") / 100.0).clamp(-12.0, 12.0),
            );
            "pitch_shifter"
        }
        "volume_a" => {
            params.insert(
                "gain".into(),
                if f("bypass") >= 0.5 {
                    0.0
                } else {
                    f("level").clamp(-96.0, 24.0)
                },
            );
            "amplify"
        }
        _ => {
            params = resolved;
            DIRECT_PROJECT_EFFECTS
                .iter()
                .find(|(project, _, _)| *project == id)
                .context("project audio DSP mapping missing")?
                .1
        }
    };
    let info =
        filmcraft_audio_dsp::effect_info(dsp).context("project audio DSP factory missing")?;
    // Match upstream set_param sanitization, including choice rounding and clamp.
    for (key, value) in &mut params {
        let p = info
            .params
            .iter()
            .find(|p| p.id == key)
            .context("mapped DSP parameter missing")?;
        *value = p.sanitize(*value);
    }
    let spec = EffectSpec { concat_source_keys: Default::default(),
        id: dsp.into(),
        params,
        bypass,
        animation: BTreeMap::new(),
    };
    native_export_preroll(std::slice::from_ref(&spec), rate)?;
    let pre = if bypass {
        0
    } else {
        (project_preroll_seconds(id, &source_scalars)? * f64::from(rate)).ceil() as usize
    };
    Ok((spec, pre))
}

/// Source preroll is evaluated before DSP sanitization. Preserve f32/f64 casts per branch.
fn project_preroll_seconds(id: &str, values: &BTreeMap<String, f64>) -> Result<f64> {
    let double = |key: &str| values.get(key).copied().unwrap_or(0.0);
    let float = |key: &str| double(key) as f32;
    let echo = |time: f64, feedback: f64| {
        let fb = (feedback / 100.0).clamp(0.0, 0.95);
        let repeats = if fb > 0.001 {
            (-4.0 / fb.log10()).clamp(1.0, 60.0)
        } else {
            1.0
        };
        time * repeats
    };
    Ok(match id {
        "amplify" | "invert_a" | "stereo_width" | "volume_a" => 0.0,
        "dynamics" | "single_band_compressor" => {
            (f64::from(float("release")) / 1000.0 * 5.0).clamp(0.05, 3.0)
        }
        "hard_limiter" => (f64::from(float("release")) / 1000.0 * 5.0).clamp(0.05, 2.0),
        "delay" => echo(
            f64::from(float("delay").max(0.001)),
            f64::from(float("feedback")),
        )
        .min(8.0),
        "parametric_eq" | "highpass" | "lowpass" | "bandpass" | "bass" | "treble" | "simple_eq" => {
            0.05
        }
        "simple_notch" | "deesser" | "pitch_shifter" => 0.1,
        "denoise" => 1.0,
        "dehummer" => 0.2,
        "dereverb" => (f64::from(float("rt60")) + 0.3).clamp(0.5, 3.0),
        "speech_enhance" => 0.3,
        "studio_reverb" => 0.3 * 30f64.powf(f64::from(float("decay")) / 100.0).min(8.0),
        "analog_delay" => echo(
            f64::from(float("delay").max(1.0)) / 1000.0,
            f64::from(float("feedback")),
        )
        .clamp(0.12, 8.0),
        "multitap_delay" => (1..=4)
            .map(|n| {
                echo(
                    double(&format!("delay{n}")).max(1.0) / 1000.0,
                    double(&format!("feedback{n}")),
                )
            })
            .fold(0.0, f64::max)
            .clamp(0.12, 8.0),
        "convolution_reverb" => {
            let rt = [0.45, 0.8, 1.9, 3.2, 1.6, 0.3, 0.2][(double("impulse") as usize).min(6)];
            (rt * 1.1 * double("room_size") / 100.0 + double("predelay") / 1000.0).clamp(0.12, 4.0)
        }
        "surround_reverb" => (double("decay") + double("predelay") / 1000.0).clamp(0.12, 8.0),
        _ => DIRECT_PROJECT_EFFECTS
            .iter()
            .find(|(project, _, _)| *project == id)
            .context("missing source preroll mapping")?
            .2
            .max(0.12),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_audio_pcm_matches_source_seek_and_contiguous_requests() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-audio-pcm.json"
        ))
        .unwrap();
        assert_eq!(
            oracle["sourceCommit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 165);
        let mut chains = BTreeMap::new();
        for case in cases {
            let id = case["effect"].as_str().unwrap();
            if !chains.contains_key(id) {
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: format!("filmcraft:{id}"),
                    ..Default::default()
                };
                let pre = resolve(id, &BTreeMap::new(), false, 48000).unwrap().1;
                chains.insert(
                    id.to_owned(),
                    super::super::filmcraft_dsp::TimelineChain::new(48000, 2, &[spec], pre)
                        .unwrap(),
                );
            }
            let start = case["start"].as_i64().unwrap();
            let frames = case["frames"].as_u64().unwrap() as usize;
            let pcm = chains
                .get_mut(id)
                .unwrap()
                .render(
                    start,
                    frames,
                    &mut |start, count| {
                        let left: Vec<f32> = (0..count)
                            .map(|i| {
                                let frame = start + i as i64;
                                if !(0..48000).contains(&frame) {
                                    return 0.0;
                                }
                                ((frame * 7919 % 1009) as f32 / 1009.0 - 0.5) * 0.1
                            })
                            .collect();
                        let right = left.iter().map(|value| -*value * 0.75).collect();
                        Ok(vec![left, right])
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            for (channel, samples) in pcm.iter().enumerate() {
                assert_eq!(samples.len(), frames);
                for (index, value) in samples.iter().enumerate() {
                    assert_eq!(
                        value.to_bits(),
                        case["bits"][channel][index].as_u64().unwrap() as u32,
                        "{id} start{start} channel{channel} sample{index}"
                    );
                }
            }
        }
    }
    #[test]
    fn nondefault_audio_pcm_matches_source_seek_and_contiguous_requests() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-audio-pcm-nondefault.json"
        ))
        .unwrap();
        assert_eq!(
            oracle["sourceCommit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 495);
        let mut chains = BTreeMap::new();
        for case in cases {
            let id = case["effect"].as_str().unwrap();
            let key = format!("{id}:{}", case["scenario"].as_str().unwrap());
            let values: BTreeMap<String, f64> =
                serde_json::from_value(case["values"].clone()).unwrap();
            if !chains.contains_key(&key) {
                let spec = EffectSpec { concat_source_keys: Default::default(),
                    id: format!("filmcraft:{id}"),
                    params: values
                        .iter()
                        .map(|(key, value)| (key.clone(), *value as f32))
                        .collect(),
                    ..Default::default()
                };
                let pre = resolve(id, &values, false, 48000).unwrap().1;
                chains.insert(
                    key.clone(),
                    super::super::filmcraft_dsp::TimelineChain::new(48000, 2, &[spec], pre)
                        .unwrap(),
                );
            }
            let start = case["start"].as_i64().unwrap();
            let frames = case["frames"].as_u64().unwrap() as usize;
            let pcm = chains
                .get_mut(&key)
                .unwrap()
                .render(
                    start,
                    frames,
                    &mut |start, count| {
                        let left: Vec<f32> = (0..count)
                            .map(|i| {
                                let frame = start + i as i64;
                                if !(0..48000).contains(&frame) {
                                    return 0.0;
                                }
                                ((frame * 7919 % 1009) as f32 / 1009.0 - 0.5) * 0.1
                            })
                            .collect();
                        let right = left.iter().map(|value| -*value * 0.75).collect();
                        Ok(vec![left, right])
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            for (channel, samples) in pcm.iter().enumerate() {
                assert_eq!(samples.len(), frames);
                for (index, value) in samples.iter().enumerate() {
                    assert_eq!(
                        value.to_bits(),
                        case["bits"][channel][index].as_u64().unwrap() as u32,
                        "{key} start{start} channel{channel} sample{index}"
                    );
                }
            }
        }
    }
    #[test]
    fn all_source_parameter_targets_match_independently_compiled_dispatcher() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-audio-mapping.json"
        ))
        .unwrap();
        assert_eq!(
            oracle["sourceCommit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 220);
        for case in cases {
            let id = case["effect"].as_str().unwrap();
            let values: BTreeMap<String, f64> =
                serde_json::from_value(case["values"].clone()).unwrap();
            let (mapped, pre) = resolve(id, &values, false, 48000).unwrap();
            assert_eq!(
                pre,
                (case["prerollSeconds"].as_f64().unwrap() * 48000.0).ceil() as usize,
                "{id} {} preroll",
                case["scenario"]
            );
            assert_eq!(mapped.id, case["dsp"].as_str().unwrap(), "{id}");
            let mut native = filmcraft_audio_dsp::create_effect(&mapped.id, 48000.0, 2).unwrap();
            for (key, value) in &mapped.params {
                assert!(native.set_param(key, *value));
            }
            assert_eq!(
                native.params().len(),
                case["targets"].as_object().unwrap().len(),
                "{id}"
            );
            for parameter in native.params() {
                let expected = case["targets"][parameter.id].as_f64().unwrap() as f32;
                assert_eq!(
                    native.param(parameter.id).unwrap().to_bits(),
                    expected.to_bits(),
                    "{id} {} {}",
                    case["scenario"],
                    parameter.id
                );
            }
        }
    }
    #[test]
    fn authored_project_slot_preserves_units_curves_and_native_dispatch() {
        let spec: EffectSpec = serde_json::from_value(serde_json::json!({
            "id":"filmcraft:delay", "params":{"delay":0.25},
            "animation":{"delay":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.25},{"tick":1000000,"value":0.75}]}}}
        })).unwrap();
        super::super::filmcraft_dsp::validate_specs(&[spec.clone()]).unwrap();
        assert_eq!(spec.params["delay"], 0.25);
        assert_eq!(spec.params_at_seconds(0.5).unwrap()["time"], 500.0);
        assert_eq!(spec.native_at_seconds(0.5, 44100).unwrap().id, "delay");
        assert_eq!(
            super::super::filmcraft_audio_mapping::native_export_preroll_at(
                &[spec.clone()],
                48000,
                0.5
            )
            .unwrap(),
            24000
        );
        let value = serde_json::to_value(&spec).unwrap();
        assert_eq!(value["id"], "filmcraft:delay");
        assert_eq!(
            value["animation"]["delay"]["track"]["keyframes"][1]["value"],
            0.75
        );
        let mut invalid = spec;
        invalid.params.insert("time".into(), 1000.0);
        assert!(super::super::filmcraft_dsp::validate_specs(&[invalid]).is_err());
    }
    #[test]
    fn project_preroll_uses_unsanitized_source_clock_parameters() {
        let values = BTreeMap::from([("decay".into(), 37.123456789)]);
        let expected = 0.3 * 30f64.powf(f64::from(37.123456789f32) / 100.0).min(8.0);
        let (_, pre) = resolve("studio_reverb", &values, false, 44100).unwrap();
        assert_eq!(pre, (expected * 44100.0).ceil() as usize);
        let values = BTreeMap::from([
            ("delay".into(), 0.123456789),
            ("feedback".into(), 35.123456789),
        ]);
        let fb = f64::from(35.123456789f32) / 100.0;
        let expected = f64::from(0.123456789f32) * (-4.0 / fb.log10()).clamp(1.0, 60.0);
        assert_eq!(
            resolve("delay", &values, false, 44100).unwrap().1,
            (expected * 44100.0).ceil() as usize
        );
        assert_eq!(
            resolve("simple_notch", &BTreeMap::new(), false, 48000)
                .unwrap()
                .1,
            4800
        );
        assert_eq!(
            resolve("studio_reverb", &BTreeMap::new(), true, 48000)
                .unwrap()
                .1,
            0
        );
    }
    #[test]
    fn every_nonintrinsic_source_effect_resolves_with_source_defaults() {
        let effects = catalog()["effects"].as_array().unwrap();
        assert_eq!(effects.len(), 58);
        for def in effects {
            let id = def["id"].as_str().unwrap();
            let result = resolve(id, &BTreeMap::new(), false, 48000);
            if def["intrinsic"] == true {
                assert!(result.is_err());
                continue;
            }
            let (spec, _) = result.unwrap_or_else(|e| panic!("{id}: {e}"));
            let mut chain = super::super::filmcraft_dsp::Chain::new(48000, 2, &[spec]).unwrap();
            let mut samples = vec![vec![0.1f32; 256], vec![-0.1f32; 256]];
            let mut channels: Vec<&mut [f32]> =
                samples.iter_mut().map(|c| c.as_mut_slice()).collect();
            chain.process(&mut channels, &mut || Ok(())).unwrap();
            assert!(samples.iter().flatten().all(|v| v.is_finite()), "{id}");
        }
    }
    #[test]
    fn source_units_defaults_and_dispatcher_precedence() {
        let empty = BTreeMap::new();
        let (delay, _) = resolve("delay", &empty, false, 48000).unwrap();
        assert_eq!(delay.params["time"], 1000.0);
        assert_eq!(
            resolve("parametric_eq", &empty, false, 48000).unwrap().0.id,
            "simple_eq"
        );
        assert_eq!(
            resolve("denoise", &empty, false, 48000).unwrap().0.params["reduction"],
            16.0
        );
        let (reverb, _) = resolve("studio_reverb", &empty, false, 48000).unwrap();
        assert_eq!(reverb.params["mix"], 35.0 / 125.0 * 100.0);
        let values = BTreeMap::from([("semitones".into(), 4.0), ("cents".into(), 50.0)]);
        assert_eq!(
            resolve("pitch_shifter", &values, false, 48000)
                .unwrap()
                .0
                .params["semitones"],
            4.5
        );
        assert!(
            resolve(
                "delay",
                &BTreeMap::from([("time".into(), 10.0)]),
                false,
                48000
            )
            .is_err()
        );
    }
}
