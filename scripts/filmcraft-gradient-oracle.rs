//! Gradient fixtures from pinned original active CPU renderer.
use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for shape in 0u32..4{for(start,end)in[([None,None],[None,None]),([Some(-1.23456789012345),Some(2.125)],[Some(3.5),Some(-0.5)]),([Some(0.5),Some(0.5)],[Some(0.5),Some(0.5)])]{for(scatter,midpoint,blend)in[(0.,0.,0.),(100.,50.,25.),(25.,100.,100.)]{for(start_color,end_color)in[([0.,0.,0.,1.],[1.,1.,1.,1.]),([-1.,2.,0.5,0.],[1.,0.,2.,0.5])]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut image=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="gradient").unwrap().instance();
for(n,p)in[("start",start),("end",end)]{e.params.insert(n.into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(p[0].unwrap_or(f64::NAN),p[1].unwrap_or(f64::NAN)))));}e.params.insert("shape".into(),Param::new(ParamValue::Choice(shape)));e.params.insert("start_color".into(),Param::new(ParamValue::Color(start_color)));e.params.insert("end_color".into(),Param::new(ParamValue::Color(end_color)));for(n,v)in[("scatter",scatter),("midpoint",midpoint),("blend",blend)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);cases.push(json!({"parameters":{"start":start,"end":end,"shape":shape,"scatter":scatter,"midpoint":midpoint,"blend":blend,"startColor":start_color,"endColor":end_color},"width":w,"height":h,"rgba":rgba,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}} let mut media=vec![];
 for shape in 0u32..4 {for tick in [0u64,250000,500000] {
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12 {255}else{32};[v,v,v,if i%16<8&&i/16<12 {128}else{0}]}).collect();
  let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000) {
   let local=(tick+index*100000).min(500000);let progress=local as f64/500000.0;
   let px:Vec<f32>=rgba.chunks_exact(4).flat_map(|p|{let a=p[3] as f32/255.0;let v=filmcraft_color::srgb_to_linear(p[0] as f32/255.0)*a;[v,v,v,a]}).collect();
   let mut image=Image{w:16,h:16,px};
   let mut effect=filmcraft_project::effect_defs().iter().find(|e|e.id=="gradient").unwrap().instance();
   effect.params.insert("start".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(8.0-progress*6.75,progress*2.125))));
   effect.params.insert("end".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(8.0-progress*4.5,16.0-progress*16.5))));
   for(name,value)in[("scatter",progress*100.0),("midpoint",progress*100.0),("blend",progress*25.0)]{effect.params.insert(name.into(),Param::new(ParamValue::Float(value)));}
   effect.params.insert("shape".into(),Param::new(ParamValue::Choice(shape)));
   effect.params.insert("start_color".into(),Param::new(ParamValue::Color([progress as f32,(1.0-progress)as f32,(progress*0.5)as f32,progress as f32])));
   effect.params.insert("end_color".into(),Param::new(ParamValue::Color([(1.0-progress)as f32,progress as f32,(1.0-progress*0.5)as f32,(1.0-progress)as f32])));
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.0,seconds:local as f64/1000000.0,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut image,&effect,&cx);frames.push(image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"shape":shape,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }
 }

println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
