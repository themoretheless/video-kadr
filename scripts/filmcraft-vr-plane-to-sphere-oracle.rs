use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for profile in 0..6{
 let(scale,pan,tilt,roll,feather)=match profile{1=>(1.,-180.,-90.,-720.,0.),2=>(180.,180.,90.,720.,100.),3=>(120.25,37.5,-47.75,12.25,37.5),4=>(90.,0.,0.,0.,50.),5=>(180.,90.,0.,0.,0.),_=>(60.,0.,0.,0.,0.)};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_plane_to_sphere").unwrap().instance();for(name,value)in[("scale",scale),("pan",pan),("tilt",tilt),("roll",roll),("feather",feather)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"frameLayout":layout,"scale":scale,"pan":pan,"tilt":tilt,"roll":roll,"feather":feather},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
