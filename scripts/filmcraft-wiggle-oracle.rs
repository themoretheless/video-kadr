use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){let mut cases=vec![];
 for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for dimensions in 0..3{for seconds in[0.,0.123456,0.5,1.25]{for frequency in[0.,3.25]{for seed in[0.,42.75]{for(amount,rotation,sizing)in[(2.5,0.,0.),(0.,37.,0.),(0.,0.,100.),(2.5,37.,100.)]{for scale in[0.5f32,1.]{
 let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="wiggle").unwrap().instance();
 for(n,v)in[("frequency",frequency),("amount",amount),("rotation",rotation),("scale",sizing),("seed",seed)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 e.params.insert("dimensions".into(),Param::new(ParamValue::Choice(dimensions)));
 let cx=FxCtx{t:filmcraft_time::Tick((seconds*1000000.)as i64),px_scale:scale,seconds,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
 cases.push(json!({"parameters":{"frequency":frequency,"amount":amount,"rotation":rotation,"scale":sizing,"seed":seed,"dimensions":dimensions},"seconds":seconds,"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}}}
 let mut media=vec![];
 for dimensions in 0..3{for tick in[0u64,250000,500000]{
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000){
   let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
   let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
   let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="wiggle").unwrap().instance();

   for(n,v)in[("frequency",t*4.),("amount",1.+t*4.),("rotation",t*45.),("scale",t*100.),("seed",t*42.75)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
   e.params.insert("dimensions".into(),Param::new(ParamValue::Choice(dimensions)));
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"dimensions":dimensions,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
