use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for rays in [0u32,1,2,3] {for(threshold,length,rotation,intensity,colorize,color)in [
(75.,60.,45.,100.,0.,[1f32;4]),
(0.,0.,0.,0.,0.,[1.;4]),
(0.,2000.,90.,1000.,100.,[0.25,0.75,0.125,0.]),
(100.,2000.,-37.25,1000.,0.,[1.;4]),
(0.,0.6,37.5,37.5,100.,[0.,0.,0.,1.]),
(20.,1.,180.,150.,50.,[1.,0.,0.,0.]),
(5.,3.25,-720.,75.,25.,[0.3,0.8,1.,0.]),
(0.,2000.,360.,1000.,0.,[1.;4])] {for scale in [0.5f32,1.] {
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="glint").unwrap().instance();for(id,value)in[("threshold",ParamValue::Float(threshold)),("length",ParamValue::Float(length)),("rotation",ParamValue::Float(rotation)),("intensity",ParamValue::Float(intensity)),("colorize",ParamValue::Float(colorize)),("color",ParamValue::Color(color)),("rays",ParamValue::Choice(rays))]{e.params.insert(id.into(),Param::new(value));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"threshold":threshold,"length":length,"rotation":rotation,"intensity":intensity,"colorize":colorize,"color":color,"rays":rays},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}let mut media=vec![];
for rays in [0u32,1,2,3] {for tick in [0u64,250000,500000] {
let channels=[("threshold",75.,5.),("length",60.,120.),("rotation",45.,-135.),("intensity",100.,350.),("colorize",0.,100.)];
let colors=[("colorRed",1.,0.25),("colorGreen",1.,0.75),("colorBlue",1.,0.125),("colorAlpha",1.,0.)];
let curve=|a:f64,b:f64|json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
let animation:serde_json::Map<String,serde_json::Value>=channels.iter().chain(colors.iter()).map(|(name,a,b)|(name.to_string(),curve(*a,*b))).collect();
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){let local=(tick+index*100000).min(500000);let t=local as f64/500000.;let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="glint").unwrap().instance();
e.params.insert("rays".into(),Param::new(ParamValue::Choice(rays)));
for(id,a,b)in channels {e.params.insert(id.into(),Param::new(ParamValue::Float(a+(b-a)*t)));}
let color=colors.map(|(_,a,b)|(a+(b-a)*t)as f32);e.params.insert("color".into(),Param::new(ParamValue::Color(color)));
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());}
media.push(json!({"localTick":tick,"parameters":{"rays":rays},"animation":animation,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
