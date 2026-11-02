//! Pinned original active Brush Strokes fixtures.
use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for surface in 0u32..4{for angle in[-45.,135.,720.]{for(size,length,density,randomness,blend)in[(0.,0.,0.,0.,0.),(2.,10.,1.,1.,0.),(5.,40.,2.,2.,50.),(0.5,2.5,0.,0.5,100.)]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut image=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="brush_strokes").unwrap().instance();e.params.insert("surface".into(),Param::new(ParamValue::Choice(surface)));
for(n,v)in[("angle",angle),("size",size),("length",length),("density",density),("randomness",randomness),("blend",blend)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);cases.push(json!({"parameters":{"surface":surface,"angle":angle,"size":size,"length":length,"density":density,"randomness":randomness,"blend":blend},"width":w,"height":h,"rgba":rgba,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}let mut media=vec![];
for surface in 0u32..4{for tick in[0u64,250000,500000]{
 let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
 for index in 0..(1000000-tick).div_ceil(100000){let local=(tick+index*100000).min(500000);let t=local as f64/500000.;let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="brush_strokes").unwrap().instance();e.params.insert("surface".into(),Param::new(ParamValue::Choice(surface)));
 for(n,v)in[("angle",-45.+t*180.),("size",t*5.),("length",t*40.),("density",t*2.),("randomness",t*2.),("blend",t*50.)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
 }
 media.push(json!({"surface":surface,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
