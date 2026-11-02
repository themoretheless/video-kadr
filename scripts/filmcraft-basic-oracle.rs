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
        ("temperature", "temperature"),
        ("tint", "tint"),
        ("exposure", "exposure"),
        ("contrast", "contrast"),
        ("highlights", "highlights"),
        ("shadows", "shadows"),
        ("whites", "whites"),
        ("blacks", "blacks"),
        ("saturation", "saturation"),
    ];
    let mut cases = vec![];
    let mut settings = vec![json!({})];
    for (name, _) in mapping {
        for value in if name == "exposure" {
            vec![-5.0, -1.3, 2.2, 5.0]
        } else if name == "saturation" {
            vec![0.0, 50.0, 150.0, 200.0]
        } else {
            vec![-100.0, -35.0, 48.0, 100.0]
        } {
            settings.push(json!({name:value}));
        }
    }
    settings.push(json!({"temperature":45.0,"tint":-32.0,"exposure":1.2,"contrast":37.0,"highlights":-42.0,"shadows":28.0,"whites":19.0,"blacks":-21.0,"saturation":146.0}));
    for parameters in settings {
        let mut effect = filmcraft_project::effect::find_effect("lumetri")
            .unwrap()
            .instance();
        for name in [
            "vignette_on",
            "creative_on",
            "curves_on",
            "wheels_on",
            "hsl_on",
        ] {
            effect
                .params
                .insert(name.into(), Param::new(ParamValue::Bool(false)));
        }
        effect
            .params
            .insert("basic_on".into(), Param::new(ParamValue::Bool(true)));
        for (host, source) in mapping {
            effect.params.insert(
                source.into(),
                Param::new(ParamValue::Float(
                    parameters[host]
                        .as_f64()
                        .unwrap_or(if host == "saturation" { 100.0 } else { 0.0 }),
                )),
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
