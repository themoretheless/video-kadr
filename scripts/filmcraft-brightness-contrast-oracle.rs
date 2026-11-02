use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for brightness in[-100.,-12.5,0.,37.25,100.]{for contrast in[-100.,-12.5,0.,37.25,100.]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="brightness_contrast").unwrap().instance();for(n,v)in[("brightness",brightness),("contrast",contrast)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"brightness":brightness,"contrast":contrast},"width":w,"height":h,"rgba":rgba,"scale":1.,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}
 let mut media=vec![];
 for (brightness,contrast) in[(-100.,100.),(37.25,-12.5),(100.,-100.)]{for tick in[0u64,250000,500000]{
 let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
 for index in 0..(1000000-tick).div_ceil(100000){
 let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="brightness_contrast").unwrap().instance();
 for(n,v)in[("brightness",t*brightness),("contrast",t*contrast)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 
 let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
 }
 media.push(json!({"brightness":brightness,"contrast":contrast,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}

println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
