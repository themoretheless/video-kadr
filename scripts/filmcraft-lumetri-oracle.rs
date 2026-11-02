//! Run against the pinned upstream FilmCraft crates, not the host implementation.
use filmcraft_project::{Param, ParamValue};
use filmcraft_render::{
    effects::{apply, FxCtx},
    image::Image,
};
use serde_json::json;
fn main() {
    let (w, h) = (7, 5);
    let input: Vec<u8> = (0..w * h)
        .flat_map(|i| {
            [
                (i * 73 % 256) as u8,
                (i * 139 % 256) as u8,
                (i * 211 % 256) as u8,
                if i % 7 == 0 {
                    0
                } else if i % 3 == 0 {
                    128
                } else {
                    255
                },
            ]
        })
        .collect();
    let mut cases = vec![];
    let mut settings = vec![];
    for look in 0..9 {
        settings.push(json!({"look":look,"intensity":100.0}));
    }
    for wheel in ["shadows", "midtones", "highlights"] {
        for (channel, limit) in [("x", 1.0), ("y", 1.0), ("lightness", 100.0)] {
            for sign in [-1.0, 1.0] {
                settings.push(json!({wheel:{channel:sign*limit}}));
            }
        }
    }
    settings.push(json!({"look":1,"intensity":145.0,"shadows":{"x":0.3,"y":-0.4,"lightness":21.0},"midtones":{"x":-0.2,"y":0.7,"lightness":-17.0},"highlights":{"x":0.8,"y":0.2,"lightness":43.0}}));
    for parameters in settings {
      for refined in [false, true] {
        let mut effect = filmcraft_project::effect::find_effect("lumetri")
            .unwrap()
            .instance();
        for name in ["vignette_on", "basic_on", "hsl_on", "hsl_on"] {
            effect
                .params
                .insert(name.into(), Param::new(ParamValue::Bool(false)));
        }
        effect
            .params
            .insert("creative_on".into(), Param::new(ParamValue::Bool(true)));
        effect
            .params
            .insert("wheels_on".into(), Param::new(ParamValue::Bool(true)));
        effect.params.insert(
            "look".into(),
            Param::new(ParamValue::Choice(
                parameters["look"].as_u64().unwrap_or(0) as u32
            )),
        );
        effect.params.insert(
            "look_intensity".into(),
            Param::new(ParamValue::Float(
                parameters["intensity"].as_f64().unwrap_or(100.0),
            )),
        );
        for wheel in ["shadows", "midtones", "highlights"] {
            let p = &parameters[wheel];
            effect.params.insert(
                format!("wheel_{wheel}"),
                Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(
                    p["x"].as_f64().unwrap_or(0.0),
                    p["y"].as_f64().unwrap_or(0.0),
                ))),
            );
            effect.params.insert(
                format!("wheel_{wheel}_l"),
                Param::new(ParamValue::Float(p["lightness"].as_f64().unwrap_or(0.0))),
            );
        }
        let curves = json!({"master":[[0.0,0.05],[0.4,0.65],[1.0,0.9]],"blue":[[0.0,0.0],[1.0,0.8]],"hueVsHue":[[0.0,0.65]],"hueVsSat":[[0.0,0.35]]});
        effect.params.insert("curves_on".into(), Param::new(ParamValue::Bool(true)));
        for (host, source) in [("master","curve_luma"),("blue","curve_blue"),("hueVsHue","hue_vs_hue"),("hueVsSat","hue_vs_sat")] {
            effect.params.insert(source.into(), Param::new(ParamValue::Curve(serde_json::from_value(curves[host].clone()).unwrap())));
        }
        let basic = json!({"temperature":23.0,"tint":-17.0,"exposure":1.3,"contrast":35.0,"highlights":-42.0,"shadows":31.0,"whites":20.0,"blacks":-15.0,"saturation":145.0});
        let creative = json!({"fadedFilm":28.0,"vibrance":42.0,"saturation":135.0});
        let vignette = json!({"amount":-1.7,"midpoint":37.0,"roundness":-45.0,"feather":62.0});
        for flag in ["basic_on","vignette_on"] { effect.params.insert(flag.into(), Param::new(ParamValue::Bool(true))); }
        for name in ["temperature","tint","exposure","contrast","highlights","shadows","whites","blacks","saturation"] {
            effect.params.insert(name.into(), Param::new(ParamValue::Float(basic[name].as_f64().unwrap())));
        }
        for (host,source) in [("fadedFilm","faded_film"),("vibrance","vibrance"),("saturation","creative_sat")] {
            effect.params.insert(source.into(), Param::new(ParamValue::Float(creative[host].as_f64().unwrap())));
        }
        for (host,source) in [("amount","vignette_amount"),("midpoint","vignette_midpoint"),("roundness","vignette_roundness"),("feather","vignette_feather")] {
            effect.params.insert(source.into(), Param::new(ParamValue::Float(vignette[host].as_f64().unwrap())));
        }
        let secondary = json!({"enabled":refined,"hue":350.0,"hueRange":130.0,"saturationMin":12.0,"lightnessMin":7.0,"lightnessMax":91.0,"soften":37.0,"denoise":63.0,"blur":11.0,"temperature":45.0,"tint":-32.0,"saturation":146.0,"hueShift":-77.0});
        let sharpen = if refined {48.0} else {0.0};
        effect.params.insert("hsl_on".into(), Param::new(ParamValue::Bool(refined)));
        effect.params.insert("sharpen".into(), Param::new(ParamValue::Float(sharpen)));
        for (host,source) in [("hue","hsl_hue"),("hueRange","hsl_hue_range"),("saturationMin","hsl_sat_min"),("lightnessMin","hsl_luma_min"),("lightnessMax","hsl_luma_max"),("soften","hsl_soft"),("denoise","hsl_denoise"),("blur","hsl_blur"),("temperature","hsl_temp"),("tint","hsl_tint"),("saturation","hsl_sat"),("hueShift","hsl_hue_shift")] {
            effect.params.insert(source.into(), Param::new(ParamValue::Float(secondary[host].as_f64().unwrap())));
        }
        let mut image = Image::new(w, h);
        for (src, dst) in input.chunks_exact(4).zip(image.px.chunks_exact_mut(4)) {
            let a = src[3] as f32 / 255.0;
            for c in 0..3 {
                dst[c] = filmcraft_color::srgb_to_linear(src[c] as f32 / 255.0) * a;
            }
            dst[3] = a;
        }
        let cx = FxCtx {
            t: filmcraft_time::Tick(0),
            px_scale: 1.0,
            seconds: 0.0,
            timecode: "",
            clip_name: "",
            project: None,
            env: None,
            working: filmcraft_color::WorkingSpace::Rec709,
        };
        apply(&mut image, &effect, &cx);
        cases.push(json!({"parameters":{"basic":basic,"creative":creative,"vignette":vignette,"grade":parameters,"curves":curves,"secondary":secondary,"sharpen":{"amount":sharpen}},"expected":image.to_rgba8()}));
    }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"input":input,"cases":cases})).unwrap());
}
