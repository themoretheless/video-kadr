use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for horizontal in 0..4u32{for vertical in 0..4u32{for margins in[[0.,0.],[0.5,1.5],[0.75,2.25],[4000.,4000.]]{for profile in 0..4{for scale in[0.5f32,1.]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|{let x=i%w;let y=i/w;let a=match profile{0=>0,1=>if x==w/3&&y==h/3{255}else{1},2=>if x==w/3&&y==h/3{3}else{2},_=>if x>=w/3&&x<=w*2/3&&y>=h/3&&y<=h*2/3{128}else{0}};[(i*17%256)as u8,128,192,a]}).collect();
let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="auto_align").unwrap().instance();
for(n,v)in[("horizontal",horizontal),("vertical",vertical)]{e.params.insert(n.into(),Param::new(ParamValue::Choice(v)));}for(n,v)in[("margin_x",margins[0]),("margin_y",margins[1])]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"horizontal":horizontal,"vertical":vertical,"marginX":margins[0],"marginY":margins[1]},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}
 let mut media=vec![];
 for (horizontal,vertical) in[(1u32,1u32),(3,3),(2,2),(0,3)]{for tick in[0u64,250000,500000]{
 let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
 for index in 0..(1000000-tick).div_ceil(100000){
 let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="auto_align").unwrap().instance();
 for(n,v)in[("margin_x",t*3.),("margin_y",1.+t*3.)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 for(n,v)in[("horizontal",horizontal),("vertical",vertical)]{e.params.insert(n.into(),Param::new(ParamValue::Choice(v)));}
 let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
 }
 media.push(json!({"horizontal":horizontal,"vertical":vertical,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
