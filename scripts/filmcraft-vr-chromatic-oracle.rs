use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for profile in 0..6{for inverse in[false,true]{
 let(red,green,blue,center_x,center_y,falloff)=match profile{1=>(0.00001,0.,0.,0.,0.,50.),2=>(100.,-100.,50.,-180.,90.,0.),3=>(-100.,100.,-50.,180.,-90.,100.),4=>(25.,-37.5,12.25,37.25,-47.75,50.),5=>(0.,25.,-25.,0.,0.,75.),_=>(0.,0.,0.,0.,0.,50.)};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_chromatic_aberrations").unwrap().instance();
 for(name,value)in[("red",red),("green",green),("blue",blue),("center_x",center_x),("center_y",center_y),("falloff",falloff)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}
 e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));e.params.insert("inverse".into(),Param::new(ParamValue::Bool(inverse)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"frameLayout":layout,"red":red,"green":green,"blue":blue,"centerX":center_x,"centerY":center_y,"falloff":falloff,"inverse":inverse},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
