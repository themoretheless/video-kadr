// Original Filmcraft transition-control oracle, MIT.
use filmcraft_project::{EffectKind,ParamKind,ParamValue,Param};
use filmcraft_geom::Vec2;
use filmcraft_render::{image::Image,transitions};
use serde_json::json;
fn changed(def: &filmcraft_project::ParamDef) -> Option<ParamValue> {
    Some(match (&def.kind, &def.default) {
        (ParamKind::Float { min, max, .. }, ParamValue::Float(v)) => {
            let hi = if *max > 1e5 { v * 2.0 + 10.0 } else { *max };
            let target = if (hi - v).abs() > (v - min).abs() { v + (hi - v) * 0.5 } else { v - (v - min) * 0.5 };
            // pixel sizes are authored for 1080p; keep them meaningful on the small test frame
            ParamValue::Float(if def.id == "border_width" {
                6.0
            } else if matches!(&def.kind, ParamKind::Float { unit: "px", .. }) && *v > 20.0 {
                v * 0.25
            } else {
                target
            })
        }
        (ParamKind::Choice(opts), ParamValue::Choice(c)) => ParamValue::Choice((c + 1) % opts.len() as u32),
        (_, ParamValue::Bool(v)) => ParamValue::Bool(!v),
        (_, ParamValue::Color(c)) => ParamValue::Color([1.0 - c[0], 0.5, c[2] * 0.3, 1.0]),
        (ParamKind::Point, _) => ParamValue::Vec2(Vec2::new(12.0, 9.0)),
        (ParamKind::Angle, ParamValue::Float(v)) => ParamValue::Float(v + 70.0),
        _ => return None,
    })
}

fn main(){
 let(w,h)=(16usize,12usize);
 let rgba=|offset:usize|->Vec<u8>{(0..w*h).flat_map(|i|[((i*17+offset)%256)as u8,((i*31+offset*3)%256)as u8,((i*7+offset*11)%256)as u8,[0,64,128,255][i%4]]).collect()};
 let(a,b)=(rgba(3),rgba(79));
 let image=|rgba:&[u8]|Image{w,h,px:rgba.chunks_exact(4).flat_map(|p|{let alpha=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*alpha,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*alpha,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*alpha,alpha]}).collect()};
 let(ai,bi)=(image(&a),image(&b));let mut cases=vec![];let mut controls=0;
 for def in filmcraft_project::effect_defs().iter().filter(|d|d.kind==EffectKind::VideoTransition){
  for parameter in &def.params{
   let Some(value)=changed(parameter)else{continue};controls+=1;
   let mut effect=def.instance();effect.params.insert(parameter.id.into(),Param::new(value.clone()));
   for reverse in [false,true]{for progress in [0.25f32,0.5,0.75]{
    let out=if reverse{transitions::apply(&effect,&bi,&ai,1.-progress)}else{transitions::apply(&effect,&ai,&bi,progress)};
    cases.push(json!({"effect":def.id,"parameter":parameter.id,"value":value,"progress":progress,"reverse":reverse,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
   }}
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"a":a,"b":b,"controls":controls,"cases":cases}));
}
