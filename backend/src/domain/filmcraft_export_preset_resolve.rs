//! Resolve portable Filmcraft settings into the host's admitted export request.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};

fn camel(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let mut upper = false;
                    let mut name = String::new();
                    for c in key.chars() {
                        if c == '_' {
                            upper = true;
                        } else if upper {
                            name.extend(c.to_uppercase());
                            upper = false;
                        } else {
                            name.push(c);
                        }
                    }
                    (name, camel(value))
                })
                .collect(),
        ),
        Value::Array(array) => Value::Array(array.iter().map(camel).collect()),
        value => value.clone(),
    }
}

pub fn request_patch(settings: &Value) -> Result<Value> {
    ensure!(settings.is_object(), "preset settings must be an object");
    let format = settings["format"]
        .as_str()
        .context("preset format is required")?;
    let profile = |field: &str, fallback: &str| {
        settings[field]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(fallback)
            .to_owned()
    };
    let preset = match format {
        "h264" => "adaptive_high",
        "hevc" => "hevc",
        "mjpeg" => "mjpeg",
        "prores" => match profile("proresProfile", "hq").as_str() {
            "hq" => "prores_hq",
            "standard" => "prores_standard",
            "lt" => "prores_lt",
            "proxy" => "prores_proxy",
            _ => bail!("unsupported saved ProRes profile"),
        },
        "dnxhr" => match profile("dnxProfile", "hq").as_str() {
            "hq" => "dnxhr_hq",
            "sq" => "dnxhr_sq",
            "lb" => "dnxhr_lb",
            _ => bail!("unsupported saved DNxHR profile"),
        },
        "apv" => match profile("apvProfile", "422-10").as_str() {
            "422-10" => "apv42210",
            "422-12" => "apv42212",
            _ => bail!("unsupported saved APV profile"),
        },
        "mxf-op1a" => match profile("mxfVideoCodec", "dnxhr").as_str() {
            "dnxhr" => {
                ensure!(
                    profile("dnxProfile", "hq") == "hq",
                    "saved MXF DNxHR profile is not supported"
                );
                "mxf_dnxhr_hq"
            }
            "proRes" => {
                ensure!(
                    profile("proresProfile", "hq") == "hq",
                    "saved MXF ProRes profile is not supported"
                );
                "mxf_prores_hq"
            }
            "h264" => "mxf_h264",
            _ => bail!("unsupported saved MXF codec"),
        },
        "mxf-opatom" => {
            ensure!(
                profile("mxfVideoCodec", "dnxhr") == "dnxhr" && profile("dnxProfile", "hq") == "hq",
                "saved OP-Atom profile is not supported"
            );
            "mxf_atom_dnxhr_hq"
        }
        "png" => "png_sequence",
        "tiff" => "tiff_sequence",
        "bmp" => "bmp_sequence",
        "gif" => "gif",
        "wav" => {
            if settings["audio"]["bits"].as_u64().unwrap_or(16) >= 24 {
                "wav24"
            } else {
                "wav16"
            }
        }
        "aiff" => "aiff16",
        _ => bail!("unsupported saved export format: {format}"),
    };
    ensure!(
        settings["fieldOrder"].is_null() || settings["fieldOrder"] == "progressive",
        "interlaced saved exports are not supported"
    );

    ensure!(
        settings["hardwareEncoding"].is_null() || settings["hardwareEncoding"] == "off",
        "saved hardware encoding is not supported"
    );
    ensure!(
        settings["sdr"].is_null() || settings["sdr"] == false,
        "saved SDR conversion is not connected"
    );
    let silent = matches!(format, "png" | "tiff" | "bmp" | "gif");
    let mut patch = json!({"preset":preset,"withAudio":!silent&&settings["includeAudio"].as_bool().unwrap_or(true)});
    if format == "mjpeg" {
        patch["mjpegQuality"] = json!(settings["quality"].as_u64().unwrap_or(90).clamp(1, 100));
    }
    for field in [
        "frameSize",
        "scale",
        "burnCaptions",
        "captionSidecar",
        "keyframeDistance",
        "h264Profile",
        "h264Level",
        "multiplexer",
        "scaling",
        "pixelAspect",
        "maxRenderQuality",
        "metadata",
    ] {
        if !settings[field].is_null() {
            patch[field] = settings[field].clone();
        }
    }
    if format=="hevc" {patch.as_object_mut().unwrap().remove("h264Level");}
    if !settings["frameRate"].is_null() {
        let rate = &settings["frameRate"];
        patch["frameRate"] = json!({"numerator":rate["num"],"denominator":rate["den"]});
    }
    if let Some(audio) = settings["audio"].as_object() {
        let mut audio = camel(&Value::Object(audio.clone()));
        if audio["sampleRate"].is_null() {
            audio["sampleRate"] = json!(0);
        }
        patch["audio"] = audio;
    }
    if matches!(format,"h264"|"hevc") || preset == "mxf_h264" {
        patch["bitrate"] = json!({"mode":settings["bitrateMode"].as_str().unwrap_or("vbr1Pass"),"targetKbps":settings["bitrateKbps"].as_u64().unwrap_or(20000)});
        if !settings["maxBitrateKbps"].is_null() {
            patch["bitrate"]["maxKbps"] = settings["maxBitrateKbps"].clone();
        }
        if !settings["adaptiveBitrate"].is_null() {
            patch["bitrate"]["adaptiveBpp"] = settings["adaptiveBitrate"].clone();
        }
    }
    let effects = camel(&settings["effects"]);
    for field in ["loudness", "nameOverlay", "timecodeOverlay", "videoLimiter"] {
        if !effects[field].is_null() {
            patch[field] = effects[field].clone();
        }
    }
    if effects["imageOverlay"]["enabled"] == true {
        let mut image = effects["imageOverlay"].clone();
        let object = image
            .as_object_mut()
            .context("invalid saved image overlay")?;
        let path = object.remove("path").unwrap_or(Value::Null);
        ensure!(
            path.is_null() || path == "",
            "saved image overlay needs an authorized library source, not a filesystem path"
        );
        patch["imageOverlay"] = image;
    }
    Ok(patch)
}
