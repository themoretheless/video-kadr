// Original transition pixel oracle, Filmcraft MIT.
use filmcraft_render::{image::Image,transitions};
use serde_json::json;
fn main(){let mut cases=vec![];
 let ids:Vec<_>=filmcraft_project::effect::effect_defs().iter().filter(|d|d.kind==filmcraft_project::effect::EffectKind::VideoTransition).map(|d|d.id).collect();
 for id in &ids {
 for(w,h)in[(1,1),(1,7),(7,1),(6,4)]{for side in 0..3{for reverse in [false,true]{for progress in [-0.1f32,0.,0.25,0.5,0.75,1.,1.1]{
  let rgba=|offset:usize,empty:bool|->Vec<u8>{(0..w*h).flat_map(|i|if empty{[0,0,0,0]}else{[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+offset)%5]}).collect()};
  let a=rgba(0,side==1);let b=rgba(2,side==2);
  let image=|rgba:&[u8]|Image{w,h,px:rgba.chunks_exact(4).flat_map(|p|{let alpha=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*alpha,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*alpha,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*alpha,alpha]}).collect()};
  let effect:filmcraft_project::EffectInstance=serde_json::from_value(json!({"effect":id,"enabled":true,"params":{}})).unwrap();
  let(a_image,b_image)=(image(&a),image(&b));
  let out=if reverse{transitions::apply(&effect,&b_image,&a_image,1.-progress)}else{transitions::apply(&effect,&a_image,&b_image,progress)};
  cases.push(json!({"effect":id,"width":w,"height":h,"a":a,"b":b,"progress":progress,"reverse":reverse,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}
 }
 println!("{}",json!({"effects":ids,"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
