use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for kind in 0..3{for invert in[false,true]{for profile in 0..4{for mode in 0..5{
 let(contrast,brightness,scale,complexity,evolution,opacity)=match profile{1=>(0.,-200.,10.,1.,-720.,0.),2=>(1000.,200.,10000.,10.,720.,100.),3=>(137.5,-12.25,37.5,4.25,123.75,45.),_=>(100.,0.,100.,6.,0.,100.)};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_fractal_noise").unwrap().instance();
 for(name,value)in[("contrast",contrast),("brightness",brightness),("scale",scale),("complexity",complexity),("evolution",evolution),("opacity",opacity)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}for(name,value)in[("frame_layout",layout),("fractal_type",kind),("mode",mode)]{e.params.insert(name.into(),Param::new(ParamValue::Choice(value)));}e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"frameLayout":layout,"fractalType":kind,"invert":invert,"mode":mode,"contrast":contrast,"brightness":brightness,"scale":scale,"complexity":complexity,"evolution":evolution,"opacity":opacity},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
