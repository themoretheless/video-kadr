use filmcraft_project::{Param,ParamValue,EffectInstance};
use filmcraft_render::{effects::{apply,FxCtx},image::Image,vfx::FxEnv};
use filmcraft_geom::Affine;use serde_json::json;
struct Env(f64);
impl FxEnv for Env {
 fn frame(&self,_:f64,_:&EffectInstance)->Option<Image>{None}
 fn source_frame(&self,_:f64,_:f32)->Option<Image>{None}
 fn track(&self,_:usize)->Option<Image>{None}
 fn layer_to_output(&self)->Affine{Affine::IDENTITY}
 fn clip_seconds(&self)->f64{1.}
 fn clip_offset(&self)->f64{0.}
 fn frame_rate(&self)->f64{self.0}
 fn media_timecode(&self)->String{String::new()}
 fn file_name(&self)->String{String::new()}
 fn sequence_name(&self)->String{String::new()}
 fn sequence_size(&self)->(u32,u32){(18,10)}
 fn source_size(&self)->(u32,u32){(18,10)}
 fn clip_key(&self)->u64{0}
}
fn main(){let mut cases=vec![];
 for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for seconds in[0.,0.123456,1.25]{for complexity in[1.,3.75,6.]{for blur in[0.,50.,100.]{for fps in[10.,30000./1001.]{for scale in[0.5f32,1.]{for seed in[0.,42.75]{for frequency in[0.1,4.]{for(amount,rotation,zoom)in[(0.,0.,0.),(2.5,3.75,25.)]{
 let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="camera_shake").unwrap().instance();
 for(n,v)in[("frequency",frequency),("amount",amount),("rotation",rotation),("zoom",zoom),("seed",seed),("complexity",complexity),("motion_blur",blur)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 let env=Env(fps);let cx=FxCtx{t:filmcraft_time::Tick((seconds*1000000.)as i64),px_scale:scale,seconds,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
 cases.push(json!({"parameters":{"frequency":frequency,"amount":amount,"rotation":rotation,"zoom":zoom,"seed":seed,"complexity":complexity,"motionBlur":blur},"seconds":seconds,"fps":fps,"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}}}}}
 let mut media=vec![];
 for complexity in[1.,3.75,6.]{for blur in[0.,100.]{for tick in[0u64,250000,500000]{
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000){
   let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
   let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
   let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="camera_shake").unwrap().instance();

   for(n,v)in[("frequency",1.+t*3.),("amount",1.+t*4.),("rotation",t*4.),("zoom",t*25.),("complexity",1.+t*(complexity-1.)),("motion_blur",t*blur),("seed",t*42.75)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:Some(&Env(10.)),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"complexity":complexity,"motionBlur":blur,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
