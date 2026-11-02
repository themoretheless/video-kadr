use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for(threshold,width,radius,intensity,color,only)in [
(20.,2.,10.,150.,[0.3f32,0.8,1.,1.],false),
(0.,0.,0.,0.,[1.;4],false),
(0.,1.,0.,1000.,[0.25,0.75,0.125,0.],true),
(100.,50.,500.,1000.,[1.;4],false),
(0.,50.,500.,1000.,[1.;4],true),
(20.,0.6,0.6,150.,[0.3,0.8,1.,0.],false),
(5.,1.25,3.25,37.5,[0.,0.,0.,1.],true),
(50.,2.,12.5,75.,[1.,0.,0.,0.],false)] {for scale in[0.5f32,1.] {
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="edge_glow").unwrap().instance();for(id,value)in[("threshold",ParamValue::Float(threshold)),("width",ParamValue::Float(width)),("radius",ParamValue::Float(radius)),("intensity",ParamValue::Float(intensity)),("color",ParamValue::Color(color)),("only",ParamValue::Bool(only))]{e.params.insert(id.into(),Param::new(value));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"threshold":threshold,"width":width,"radius":radius,"intensity":intensity,"color":color,"only":only},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}let mut media=vec![];
for only in [false,true] {for tick in [0u64,250000,500000] {
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){let local=(tick+index*100000).min(500000);let t=local as f64/500000.;let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="edge_glow").unwrap().instance();
for(id,value)in [("only",ParamValue::Bool(only)),("threshold",ParamValue::Float(20.-15.*t)),("width",ParamValue::Float(2.+10.*t)),("radius",ParamValue::Float(10.+20.*t)),("intensity",ParamValue::Float(150.+200.*t)),("color",ParamValue::Color([(0.3+0.4*t)as f32,(0.8-0.6*t)as f32,(1.-t)as f32,(1.-t)as f32]))]{e.params.insert(id.into(),Param::new(value));}
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());}
media.push(json!({"localTick":tick,"parameters":{"only":only},"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
