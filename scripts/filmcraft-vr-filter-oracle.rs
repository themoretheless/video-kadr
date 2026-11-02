use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for effect in ["vr_blur","vr_sharpen"]{for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for strength in[0.,0.5,50.,500.,if effect=="vr_blur"{1000.}else{250.}]{for px_scale in[0.35,0.5,1.,2.]{
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id==effect).unwrap().instance();
 e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));e.params.insert(if effect=="vr_blur"{"blurriness"}else{"amount"}.into(),Param::new(ParamValue::Float(strength)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"effect":effect,"width":width,"height":height,"rgba":rgba,"layout":layout,"strength":strength,"pxScale":px_scale,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
