//! Flat Filmcraft Lumetri controls into the existing native SDR grading contract.
use super::{filmcraft_effect_scalar as scalar, lumetri::Lumetri};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
fn number(param: &Value, tick: i64) -> Result<f64> {
    Ok(scalar::sample(
        &scalar::convert(param, scalar::Evaluation::EffectParameter)?,
        tick,
    ))
}
fn lut(
    reference: &str,
    project: &Value,
) -> Result<(Option<super::color_lut::ColorLut>, Option<String>)> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Ok((None, None));
    }
    if let Some(id) = reference.strip_prefix("builtin:") {
        return Ok((
            None,
            super::builtin_luts::catalog()?
                .iter()
                .any(|entry| entry.id == id)
                .then(|| id.into()),
        ));
    }
    let Some(id) = reference.strip_prefix("lib:") else {
        return Ok((None, None));
    };
    let entries = project.get("luts").and_then(Value::as_array);
    let mut matches = entries
        .into_iter()
        .flatten()
        .filter(|entry| entry["id"] == id);
    let Some(entry) = matches.next() else {
        return Ok((None, None));
    };
    ensure!(matches.next().is_none(), "ambiguous source LUT");
    let format = match entry["format"].as_str() {
        Some("cube") => super::color_lut::TableFormat::Cube,
        Some("3dl") => super::color_lut::TableFormat::ThreeDl,
        _ => anyhow::bail!("unsupported source LUT format"),
    };
    let text = entry["text"].as_str().context("missing source LUT text")?;
    ensure!(text.len() <= 16 * 1024 * 1024, "source LUT text too large");
    let table = super::color_lut::ColorLut {
        format,
        text: text.into(),
        intensity: 100.,
    };
    if table.validate().is_err() {
        return Ok((None, None));
    }
    Ok((Some(table), None))
}
/// Reserve before converting/parsing tables or curves. Cached LUT storage has its own bounded cache.
pub(super) fn scratch(raw: &Value, project: &Value) -> Result<usize> {
    let mut bytes = 64usize * 1024;
    for name in ["input_lut", "look_lut"] {
        if let Some(id) = raw["params"][name]["value"]["Text"]
            .as_str()
            .and_then(|s| s.trim().strip_prefix("lib:"))
        {
            for entry in project
                .get("luts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|entry| entry["id"] == id)
            {
                let size = entry["text"]
                    .as_str()
                    .context("missing source LUT text")?
                    .len();
                ensure!(size <= 16 * 1024 * 1024, "source LUT text too large");
                bytes = bytes
                    .checked_add(size.checked_mul(8).context("LUT scratch overflow")?)
                    .and_then(|n| n.checked_add(1024 * 1024))
                    .context("LUT scratch overflow")?;
            }
        } else if raw["params"][name]["value"]["Text"]
            .as_str()
            .is_some_and(|s| s.trim().starts_with("builtin:"))
        {
            bytes = bytes
                .checked_add(include_bytes!("assets/filmcraft-builtin-luts.bin").len() + 64 * 1024)
                .context("builtin LUT scratch overflow")?;
        }
    }
    Ok(bytes)
}
pub(super) fn compile(raw: &Value, tick: i64, project: &Value) -> Result<Lumetri> {
    ensure!(raw["effect"] == "lumetri", "not a Lumetri source effect");
    let mut out = serde_json::to_value(Lumetri::default())?;
    for (name, param) in raw["params"]
        .as_object()
        .context("missing Lumetri controls")?
    {
        let path = match name.as_str() {
            "basic_on" => "/basic/enabled",
            "creative_on" => "/creative/enabled",
            "vignette_on" => "/vignette/enabled",
            "curves_on" => "/curvesOn",
            "wheels_on" => "/wheelsOn",
            "hsl_on" => "/secondary/enabled",
            "temperature" => "/basic/temperature",
            "tint" => "/basic/tint",
            "exposure" => "/basic/exposure",
            "contrast" => "/basic/contrast",
            "highlights" => "/basic/highlights",
            "shadows" => "/basic/shadows",
            "whites" => "/basic/whites",
            "blacks" => "/basic/blacks",
            "saturation" => "/basic/saturation",
            "faded_film" => "/creative/fadedFilm",
            "vibrance" => "/creative/vibrance",
            "creative_sat" => "/creative/saturation",
            "look" => "/grade/look",
            "look_intensity" => "/grade/intensity",
            "sharpen" => "/sharpen/amount",
            "wheel_shadows_l" => "/grade/shadows/lightness",
            "wheel_midtones_l" => "/grade/midtones/lightness",
            "wheel_highlights_l" => "/grade/highlights/lightness",
            "vignette_amount" => "/vignette/amount",
            "vignette_midpoint" => "/vignette/midpoint",
            "vignette_roundness" => "/vignette/roundness",
            "vignette_feather" => "/vignette/feather",
            "hsl_show_mask" => "/secondary/showMask",
            "hsl_hue" => "/secondary/hue",
            "hsl_hue_range" => "/secondary/hueRange",
            "hsl_sat_min" => "/secondary/saturationMin",
            "hsl_luma_min" => "/secondary/lightnessMin",
            "hsl_luma_max" => "/secondary/lightnessMax",
            "hsl_soft" => "/secondary/soften",
            "hsl_denoise" => "/secondary/denoise",
            "hsl_blur" => "/secondary/blur",
            "hsl_temp" => "/secondary/temperature",
            "hsl_tint" => "/secondary/tint",
            "hsl_sat" => "/secondary/saturation",
            "hsl_hue_shift" => "/secondary/hueShift",
            "curve_luma" => "/curves/master",
            "curve_red" => "/curves/red",
            "curve_green" => "/curves/green",
            "curve_blue" => "/curves/blue",
            "hue_vs_sat" => "/curves/hueVsSat",
            "hue_vs_hue" => "/curves/hueVsHue",
            "hue_vs_luma" => "/curves/hueVsLuma",
            "luma_vs_sat" => "/curves/lumaVsSat",
            "sat_vs_sat" => "/curves/satVsSat",
            "hdr_white" | "hdr_specular" | "curves_hdr_range" => {
                ensure!(
                    number(param, tick)?.is_finite(),
                    "invalid SDR-inactive HDR control"
                );
                continue;
            }
            "input_lut" | "look_lut" => {
                let reference = param["value"]["Text"]
                    .as_str()
                    .context("invalid source LUT reference")?;
                let (inline, builtin) = lut(reference, project)?;
                let (a, b) = if name == "input_lut" {
                    ("inputLut", "inputLutBuiltin")
                } else {
                    ("lookLut", "lookLutBuiltin")
                };
                out[a] = serde_json::to_value(inline)?;
                out[b] = serde_json::to_value(builtin)?;
                continue;
            }
            "wheel_shadows" | "wheel_midtones" | "wheel_highlights" => {
                let target = match name.as_str() {
                    "wheel_shadows" => "shadows",
                    "wheel_midtones" => "midtones",
                    _ => "highlights",
                };
                let point =
                    super::filmcraft_component_curve::point_with_runtime_fallback(param, [0., 0.])?;
                out["grade"][target]["x"] = json!(scalar::sample(&point[0], tick));
                out["grade"][target]["y"] = json!(scalar::sample(&point[1], tick));
                continue;
            }
            "shadow_tint" | "highlight_tint" => {
                let curves = super::filmcraft_component_curve::color(param)?;
                let prefix = if name == "shadow_tint" {
                    "shadow"
                } else {
                    "highlight"
                };
                for (curve, suffix) in curves.iter().zip(["Red", "Green", "Blue", "Alpha"]) {
                    out["creative"][format!("{prefix}{suffix}")] =
                        json!(scalar::sample(curve, tick) as f32);
                }
                continue;
            }
            _ => anyhow::bail!("unsupported source Lumetri control: {name}"),
        };
        let target = out
            .pointer_mut(path)
            .context("missing native Lumetri section")?;
        *target = if target.is_boolean() {
            json!(param["value"]["Bool"]
                .as_bool()
                .context("invalid Lumetri section switch")?)
        } else if target.is_array() {
            let curve = param["value"]["Curve"]
                .as_array()
                .context("invalid Lumetri curve")?;
            ensure!(curve.len() <= 256, "Lumetri curve too large");
            json!(curve)
        } else if target.is_u64() {
            json!(param["value"]["Choice"]
                .as_u64()
                .context("invalid Lumetri choice")?)
        } else {
            json!(number(param, tick)?)
        };
    }
    let out: Lumetri = serde_json::from_value(out)?;
    out.validate()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_orphan_and_invalid_lut_references_bypass_without_authoring_mutation() {
        for reference in ["lib:removed", "builtin:removed", "unknown:entry"] {
            let raw =
                json!({"effect":"lumetri","params":{"look_lut":{"value":{"Text":reference}}}});
            let before = raw.clone();
            let result = compile(&raw, 0, &Value::Null).unwrap();
            assert!(result.look_lut.is_none() && result.look_lut_builtin.is_none());
            assert_eq!(raw, before);
        }
        let raw = json!({"effect":"lumetri","params":{"look_lut":{"value":{"Text":"lib:broken"}}}});
        let project = json!({"luts":[{"id":"broken","format":"cube","text":"not a table"}]});
        assert!(compile(&raw, 0, &project).unwrap().look_lut.is_none());
        assert!(compile(
            &json!({"effect":"lumetri","params":{"unrecognized":{"value":{"Float":0.}}}}),
            0,
            &Value::Null
        )
        .is_err());
    }
}
