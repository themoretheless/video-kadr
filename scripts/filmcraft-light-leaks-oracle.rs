use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
for (width,height) in [(6usize,4usize),(18,10)] {for alpha in [0u8,128,255] {for mode in 0..3 {for profile in 0..4 {for tick in [0u64,500000,2000000] {
 let (intensity,scale,direction,speed,seed)=match profile {1=>(0.,10.,-120.,20.,10000.),2=>(100.,1000.,450.,0.,17.),3=>(35.,75.,-45.,2.5,321.25),_=>(70.,100.,30.,1.,0.)};
 let(c1,c2)=if profile==3{([0.2,0.8,0.4,0.25],[0.7,0.1,1.,0.])}else{([1.,0.45,0.1,1.],[1.,0.15,0.35,1.])};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="light_leaks").unwrap().instance();
 for(name,value)in[("intensity",intensity),("scale",scale),("direction",direction),("speed",speed),("seed",seed)]{e.params.insert(name.into(),Param::new(ParamValue::Float(value)));}
 e.params.insert("c1".into(),Param::new(ParamValue::Color(c1)));e.params.insert("c2".into(),Param::new(ParamValue::Color(c2)));e.params.insert("mode".into(),Param::new(ParamValue::Choice(mode)));
 let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:tick as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"tick":tick,"parameters":{"c1":c1,"c2":c2,"intensity":intensity,"scale":scale,"direction":direction,"speed":speed,"seed":seed,"mode":mode},"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
