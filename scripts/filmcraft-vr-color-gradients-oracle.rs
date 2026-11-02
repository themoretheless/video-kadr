use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for profile in 0..4{for mode in 0..5{
 let(blend,opacity,lons,lats)=match profile{1=>(1.,0.,[-180.,0.,180.],[90.,-90.,90.]),2=>(100.,100.,[0.;3],[0.;3]),3=>(37.5,45.,[-180.,45.25,180.],[-90.,12.5,90.]),_=>(50.,100.,[-120.,0.,120.],[20.,-20.,20.])};
 let colors=if profile==3{[[0.2,0.8,0.4,0.],[0.7,0.1,1.,0.25],[1.,0.5,0.1,0.5]]}else{[[1.,0.25,0.25,1.],[0.25,1.,0.4,1.],[0.3,0.4,1.,1.]]};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_color_gradients").unwrap().instance();
 for i in 0..3{e.params.insert(format!("c{}",i+1),Param::new(ParamValue::Color(colors[i])));e.params.insert(format!("p{}_lon",i+1),Param::new(ParamValue::Float(lons[i])));e.params.insert(format!("p{}_lat",i+1),Param::new(ParamValue::Float(lats[i])));}
 e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));e.params.insert("mode".into(),Param::new(ParamValue::Choice(mode)));e.params.insert("blend".into(),Param::new(ParamValue::Float(blend)));e.params.insert("opacity".into(),Param::new(ParamValue::Float(opacity)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"frameLayout":layout,"c1":colors[0],"c2":colors[1],"c3":colors[2],"p1Lon":lons[0],"p2Lon":lons[1],"p3Lon":lons[2],"p1Lat":lats[0],"p2Lat":lats[1],"p3Lat":lats[2],"blend":blend,"opacity":opacity,"mode":mode},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
