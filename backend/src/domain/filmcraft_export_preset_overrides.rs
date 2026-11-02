//! Flat export overrides and aliases from pinned Filmcraft engine/export_tools.rs.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

pub fn format_id(name: &str) -> Option<&'static str> {
    Some(
        match name
            .to_ascii_lowercase()
            .replace([' ', '-', '_', '.'], "")
            .as_str()
        {
            "h264" | "mp4" | "avc" | "m4v" => "h264",
            "hevc" | "h265" | "hvc1" | "hev1" | "x265" => "hevc",
            "prores" | "mov" | "appleprores" => "prores",
            "dnxhr" | "dnxhd" | "dnx" | "avid" | "aviddnxhr" | "aviddnxhd" | "vc3" => "dnxhr",
            "apv" | "apv1" => "apv",
            "mxf" | "mxfop1a" | "op1a" => "mxf-op1a",
            "mxfopatom" | "opatom" | "mxfatom" | "avidmxf" => "mxf-opatom",
            "mjpeg" | "motionjpeg" | "jpeg" => "mjpeg",
            "png" | "pngsequence" => "png",
            "tif" | "tiff" | "tiffsequence" => "tiff",
            "bmp" | "bmpsequence" => "bmp",
            "gif" | "animatedgif" => "gif",
            "wav" | "waveform" | "waveformaudio" => "wav",
            "aif" | "aiff" | "aifc" => "aiff",
            _ => return None,
        },
    )
}

fn integer(value: &Value) -> Result<Option<u32>> {
    if value.is_null() {
        return Ok(None);
    }
    let number = value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| {
            value
                .as_f64()
                .filter(|n| {
                    n.is_finite() && n.fract() == 0. && (0.0..=f64::from(u32::MAX)).contains(n)
                })
                .map(|n| n as u32)
        });
    Ok(Some(number.context(
        "export override must be an unsigned 32-bit integer",
    )?))
}

fn rate(value: &Value) -> Option<Value> {
    if let Some(text) = value.as_str() {
        if let Some((num, den)) = text.split_once('/') {
            return Some(
                json!({"num":num.trim().parse::<i64>().ok()?,"den":den.trim().parse::<i64>().ok()?}),
            );
        }
        return rate(&json!(text.trim().parse::<f64>().ok()?));
    }
    let fps = value.as_f64().filter(|n| *n > 0. && *n <= 1000.)?;
    for (rounded, num) in [
        (23.976, 24000),
        (29.97, 30000),
        (59.94, 60000),
        (119.88, 120000),
    ] {
        if (fps - rounded).abs() < 0.01 {
            return Some(json!({"num":num,"den":1001}));
        }
    }
    let (n, d) = if (fps - fps.round()).abs() < 1e-6 {
        (fps.round() as i64, 1)
    } else {
        ((fps * 1000.).round() as i64, 1000)
    };
    let rate = filmcraft_time::FrameRate::new(n, d);
    Some(json!({"num":rate.num,"den":rate.den}))
}

pub fn apply(settings: &mut Value, flat: &Value) -> Result<()> {
    if let Some(format) = flat["format"].as_str() {
        settings["format"] = json!(format_id(format).context("unknown export format")?);
    }
    if let Some(scale) = flat["scale"].as_f64() {
        let scale = scale as f32;
        ensure!(
            scale.is_finite() && scale > 0.,
            "output scale must be finite and positive"
        );
        settings["scale"] = json!(scale);
    }
    if let Some(audio) = flat["audio"].as_bool() {
        settings["includeAudio"] = json!(audio);
    }
    if let Some(quality) = flat["quality"].as_u64() {
        settings["quality"] = json!(quality.min(100));
    }
    for field in ["bitrateKbps", "maxBitrateKbps", "keyframeDistance"] {
        if let Some(number) = integer(&flat[field])? {
            settings[field] = json!(number);
            if field == "bitrateKbps" {
                settings["adaptiveBitrate"] = Value::Null;
            }
        }
    }
    for (field, allowed) in [
        ("bitrateMode", &["cbr", "vbr1Pass", "vbr2Pass"][..]),
        ("mxfVideoCodec", &["dnxhr", "proRes", "h264"][..]),
    ] {
        if let Some(value) = flat[field].as_str() {
            ensure!(allowed.contains(&value), "invalid {field}");
            settings[field] = json!(value);
        }
    }
    if !flat["hardwareEncoding"].is_null() {
        let hardware = match &flat["hardwareEncoding"] {
            Value::Bool(on) => {
                if *on {
                    "auto"
                } else {
                    "off"
                }
            }
            Value::String(value) if value == "off" || value == "auto" => value.as_str(),
            _ => anyhow::bail!("hardwareEncoding must be off or auto"),
        };
        settings["hardwareEncoding"] = json!(hardware);
    }
    for field in ["burnCaptions", "sdr"] {
        if let Some(value) = flat[field].as_bool() {
            settings[field] = json!(value);
        }
    }
    for field in ["proresProfile", "dnxProfile", "apvProfile"] {
        if let Some(value) = flat[field].as_str() {
            settings[field] = json!(value);
        }
    }
    match (integer(&flat["width"])?, integer(&flat["height"])?) {
        (Some(w), Some(h)) => {
            ensure!(w > 0 && h > 0, "frame size must be positive");
            settings["frameSize"] = json!([w, h]);
        }
        (None, None) => {}
        _ => anyhow::bail!("provide both width and height"),
    }
    if !flat["fps"].is_null() {
        let rate = rate(&flat["fps"]).context("invalid export fps")?;
        let num = rate["num"].as_i64().unwrap_or(0);
        let den = rate["den"].as_i64().unwrap_or(0);
        ensure!(
            num > 0
                && den > 0
                && num <= i64::from(u32::MAX)
                && den <= i64::from(u32::MAX)
                && (num as f64 / den as f64) <= 1000.,
            "invalid export fps"
        );
        settings["frameRate"] = rate;
    }
    if let Some(sidecar) = flat["captionSidecar"].as_str() {
        settings["captionSidecar"] = if sidecar.is_empty() || sidecar == "none" {
            Value::Null
        } else {
            json!(sidecar.to_ascii_lowercase())
        };
    }
    if let Some(lufs) = flat["loudnessLufs"].as_f64() {
        settings["effects"]["loudness"]["enabled"] = json!(true);
        settings["effects"]["loudness"]["target_lufs"] = json!(lufs);
    }
    Ok(())
}
