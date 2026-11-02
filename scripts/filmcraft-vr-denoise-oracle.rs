use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for amount in[0.,20.,50.,100.]{for noise_type in 0..2{for show_noise in[false,true]{for px_scale in[0.35,1.,2.]{
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_denoise").unwrap().instance();
 e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));e.params.insert("amount".into(),Param::new(ParamValue::Float(amount)));e.params.insert("noise_type".into(),Param::new(ParamValue::Choice(noise_type)));e.params.insert("show_noise".into(),Param::new(ParamValue::Bool(show_noise)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"pxScale":px_scale,"parameters":{"frameLayout":layout,"amount":amount,"noiseType":noise_type,"showNoise":show_noise},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
