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
    let mut cases=vec![];
    for builtin in filmcraft_render::luts::builtins(){
      for (role,intensity) in [("input",100.0),("look",0.0),("look",100.0),("look",175.0)] {
        let mut effect=filmcraft_project::effect::find_effect("lumetri").unwrap().instance();
        for flag in ["curves_on","wheels_on","vignette_on","hsl_on"] {effect.params.insert(flag.into(),Param::new(ParamValue::Bool(false)));}
        effect.params.insert("basic_on".into(),Param::new(ParamValue::Bool(role=="input")));
        effect.params.insert("creative_on".into(),Param::new(ParamValue::Bool(role=="look")));
        effect.params.insert("look".into(),Param::new(ParamValue::Choice(8)));
        effect.params.insert("look_intensity".into(),Param::new(ParamValue::Float(intensity)));
        effect.params.insert(format!("{role}_lut"),Param::new(ParamValue::Text(format!("builtin:{}",builtin.id))));
        let parameters=if role=="input" {json!({"inputLutBuiltin":builtin.id,"creative":{"enabled":false},"grade":{"look":8},"curvesOn":false,"wheelsOn":false})}else{json!({"lookLutBuiltin":builtin.id,"basic":{"enabled":false},"grade":{"look":8,"intensity":intensity},"curvesOn":false,"wheelsOn":false})};
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
    }
    println!("{}",serde_json::to_string_pretty(&json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"input":input,"cases":cases})).unwrap());
}
