use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for(width,height)in[(6usize,4usize),(17,9),(18,10)]{for alpha in[0u8,128,255]{for layout in 0..3{for profile in 0..4{for tick in[0i64,100000,500000,1250000]{
 let(amplitude,distortion,rate,color,scanlines,seed)=match profile{1=>(100.,1.,60.,100.,100.,10000.),2=>(50.,100.,0.,0.,50.,0.),3=>(37.5,12.25,7.5,45.,12.5,17.75),_=>(0.,20.,10.,50.,0.,0.)};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="vr_digital_glitch").unwrap().instance();
 for(name,value)in[("amplitude",amplitude),("distortion",distortion),("rate",rate),("color",color),("scanlines",scanlines),("seed",seed)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}e.params.insert("frame_layout".into(),Param::new(ParamValue::Choice(layout)));
 let cx=FxCtx{t:filmcraft_time::Tick(tick),px_scale:1.,seconds:tick as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"tick":tick,"parameters":{"frameLayout":layout,"amplitude":amplitude,"distortion":distortion,"rate":rate,"color":color,"scanlines":scanlines,"seed":seed},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
