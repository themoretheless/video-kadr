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
        ("hue", "hsl_hue"),
        ("hueRange", "hsl_hue_range"),
        ("saturationMin", "hsl_sat_min"),
        ("lightnessMin", "hsl_luma_min"),
        ("lightnessMax", "hsl_luma_max"),
        ("soften", "hsl_soft"),
        ("denoise", "hsl_denoise"),
        ("blur", "hsl_blur"),
        ("temperature", "hsl_temp"),
        ("tint", "hsl_tint"),
        ("saturation", "hsl_sat"),
        ("hueShift", "hsl_hue_shift"),
    ];
    let mut cases = vec![];
    for pixel_scale in [1.0, 0.5, 0.25] {
        for view in 0..4 {
            for refine in [false, true] {
                let parameters = json!({"showMask":view,"hue":350.0,"hueRange":130.0,"saturationMin":12.0,"lightnessMin":7.0,"lightnessMax":91.0,"soften":37.0,"denoise":if refine {63.0} else {0.0},"blur":if refine {11.0} else {0.0},"temperature":45.0,"tint":-32.0,"saturation":146.0,"hueShift":-77.0});
                let mut effect = filmcraft_project::effect::find_effect("lumetri")
                    .unwrap()
                    .instance();
                for name in [
                    "basic_on",
                    "creative_on",
                    "curves_on",
                    "wheels_on",
                    "vignette_on",
                ] {
                    effect
                        .params
                        .insert(name.into(), Param::new(ParamValue::Bool(false)));
                }
                effect
                    .params
                    .insert("hsl_on".into(), Param::new(ParamValue::Bool(true)));
                effect
                    .params
                    .insert("hsl_show_mask".into(), Param::new(ParamValue::Choice(view)));
                for (host, source) in mapping {
                    effect.params.insert(
                        source.into(),
                        Param::new(ParamValue::Float(parameters[host].as_f64().unwrap())),
                    );
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
                    px_scale: pixel_scale,
                    seconds: 0.0,
                    timecode: "",
                    clip_name: "",
                    project: None,
                    env: None,
                    working: filmcraft_color::WorkingSpace::Rec709,
                };
                apply(&mut image, &effect, &cx);
                cases.push(json!({"pixelScale":pixel_scale,"parameters":parameters,"expected":image.to_rgba8()}));
            }
        }
    }
    println!("{}",serde_json::to_string_pretty(&json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"input":input,"cases":cases})).unwrap());
}
