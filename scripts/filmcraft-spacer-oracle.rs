use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
 for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for margins in[[0.,0.,0.,0.],[0.75,1.25,2.5,0.5],[50.,3.,1.,2.]]{for uniform in[false,true]{for radius in[0.,0.75,2.,2000.]{for fill in[false,true]{for color in[[0.25f32,0.5,1.,0.],[0.25,0.5,1.,1.],[-0.5,1.25,0.1,0.25]]{for scale in[0.5f32,1.]{
 let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="spacer").unwrap().instance();
 for(n,v)in[("left",margins[0]),("top",margins[1]),("right",margins[2]),("bottom",margins[3]),("radius",radius)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}e.params.insert("uniform".into(),Param::new(ParamValue::Bool(uniform)));e.params.insert("fill".into(),Param::new(ParamValue::Bool(fill)));e.params.insert("color".into(),Param::new(ParamValue::Color(color)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"left":margins[0],"top":margins[1],"right":margins[2],"bottom":margins[3],"radius":radius,"uniform":uniform,"fill":fill,"color":color},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}}}
 let mut media=vec![];
 for uniform in[false,true]{for fill in[false,true]{for tick in[0u64,250000,500000]{
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000){
   let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
   let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
   let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="spacer").unwrap().instance();

   for(n,v)in[("left",t*3.),("top",1.+t*3.),("right",2.-t),("bottom",0.5+t*1.5),("radius",t*4.)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
   e.params.insert("uniform".into(),Param::new(ParamValue::Bool(uniform)));e.params.insert("fill".into(),Param::new(ParamValue::Bool(fill)));e.params.insert("color".into(),Param::new(ParamValue::Color([t as f32,(1.-t)as f32,(0.25+t*0.5)as f32,t as f32])));
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"uniform":uniform,"fill":fill,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
