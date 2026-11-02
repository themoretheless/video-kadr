use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(96usize,48usize),(128,64)]{for alpha in[0u8,128,255]{for font in 0..2{for alignment in 0..3{for profile in 0..4{
let(text,size,position,color,bg)=match profile{1=>("Привет עולם",12.25,[width as f64/2.,height as f64/2.],[0.2,0.7,0.4,0.5],60.),2=>("Line 1\nLine 2",9.,[3.5,8.25],[1.,0.,0.,1.],100.),3=>("",64.,[width as f64/2.,height as f64/2.],[1.,1.,1.,1.],0.),_=>("FilmCraft",14.,[width as f64/2.,height as f64/2.],[1.,1.,1.,1.],0.)};
let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="simple_text").unwrap().instance();
for(n,v)in[("size",ParamValue::Float(size)),("position",ParamValue::Vec2(filmcraft_geom::Vec2::new(position[0],position[1]))),("color",ParamValue::Color(color)),("bg_opacity",ParamValue::Float(bg)),("font",ParamValue::Choice(font)),("alignment",ParamValue::Choice(alignment)),("text",ParamValue::Text(text.into()))]{e.params.insert(n.into(),Param::new(v));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
cases.push(json!({"width":width,"height":height,"rgba":rgba,"text":text,"size":size,"position":position,"color":color,"background":bg/100.,"font":font,"alignment":alignment,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
