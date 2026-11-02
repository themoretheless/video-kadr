use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for profile in 0..4{for use_tint in[false,true]{for px_scale in[0.35f32,1.,2.]{
 let(threshold,radius,brightness,saturation,tint)=match profile{1=>(0.,0.,0.,0.,[1.,0.,0.,0.]),2=>(100.,1000.,10.,4.,[0.,1.,0.,1.]),3=>(37.5,12.25,1.75,0.45,[0.2,0.7,0.4,0.25]),_=>(70.,30.,1.,1.,[1.,1.,1.,1.])};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_glow").unwrap().instance();for(name,value)in[("threshold",threshold),("radius",radius),("brightness",brightness),("saturation",saturation)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));e.params.insert("use_tint".into(),Param::new(ParamValue::Bool(use_tint)));e.params.insert("tint".into(),Param::new(ParamValue::Color(tint)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"pxScale":px_scale,"parameters":{"frameLayout":layout,"threshold":threshold,"radius":radius,"brightness":brightness,"saturation":saturation,"tint":tint,"useTint":use_tint},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
