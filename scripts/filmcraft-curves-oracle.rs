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
    let mapping = [
        ("master", "curve_luma"),
        ("red", "curve_red"),
        ("green", "curve_green"),
        ("blue", "curve_blue"),
        ("hueVsSat", "hue_vs_sat"),
        ("hueVsHue", "hue_vs_hue"),
        ("hueVsLuma", "hue_vs_luma"),
        ("lumaVsSat", "luma_vs_sat"),
        ("satVsSat", "sat_vs_sat"),
    ];
    let mut cases = vec![];
    let mut settings = vec![json!({})];
    for (name, _) in mapping {
        for points in [
            json!([[0.25, 0.8]]),
            json!([[0.0, 0.1], [0.3, 0.7], [0.8, 0.4], [1.0, 0.9]]),
        ] {
            settings.push(json!({name:points}));
        }
    }
    settings.push(json!({"master":[[0.0,0.0],[0.4,0.65],[1.0,0.95]],"blue":[[0.0,0.0],[1.0,0.8]],"hueVsHue":[[0.0,0.65]],"hueVsSat":[[0.0,0.35]],"lumaVsSat":[[0.0,0.6]],"satVsSat":[[0.0,0.7]]}));
    for parameters in settings {
        let mut effect = filmcraft_project::effect::find_effect("lumetri")
            .unwrap()
            .instance();
        for name in [
            "vignette_on",
            "creative_on",
            "basic_on",
            "wheels_on",
            "hsl_on",
        ] {
            effect
                .params
                .insert(name.into(), Param::new(ParamValue::Bool(false)));
        }
        effect
            .params
            .insert("curves_on".into(), Param::new(ParamValue::Bool(true)));
        for (host, source) in mapping {
            if let Some(value) = parameters.get(host) {
                let points = serde_json::from_value(value.clone()).unwrap();
                effect
                    .params
                    .insert(source.into(), Param::new(ParamValue::Curve(points)));
            }
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
        cases.push(json!({"parameters":parameters,"expected":image.to_rgba8()}));
    }
    println!("{}",serde_json::to_string_pretty(&json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"input":input,"cases":cases})).unwrap());
}
