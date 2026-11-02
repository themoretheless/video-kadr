use filmcraft_project::{Param,ParamValue,EffectInstance};
use filmcraft_render::{effects::{apply,FxCtx},image::Image,vfx::FxEnv};
use filmcraft_geom::{Affine,Vec2};use serde_json::json;
struct Env(f64,f64);
impl FxEnv for Env {
 fn frame(&self,_:f64,_:&EffectInstance)->Option<Image>{None}
 fn source_frame(&self,_:f64,_:f32)->Option<Image>{None}
 fn track(&self,_:usize)->Option<Image>{None}
 fn layer_to_output(&self)->Affine{Affine::IDENTITY}
 fn clip_seconds(&self)->f64{self.1}
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
 for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for easing in 0..4{for progress in[0.,0.25,0.5,0.75,1.]{for blur in[0.,50.,100.]{for fps in[10.,30000./1001.]{for scale in[0.5f32,1.]{
 let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="move").unwrap().instance();
 e.params.insert("from".into(),Param::new(ParamValue::Vec2(Vec2::new(-2.123456789012,1.25))));e.params.insert("to".into(),Param::new(ParamValue::Vec2(Vec2::new(3.5,-0.75))));
 e.params.insert("motion_blur".into(),Param::new(ParamValue::Float(blur)));e.params.insert("easing".into(),Param::new(ParamValue::Choice(easing)));
 let env=Env(fps,1.);let cx=FxCtx{t:filmcraft_time::Tick((progress*1000000.)as i64),px_scale:scale,seconds:progress,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
 cases.push(json!({"parameters":{"from":[-2.123456789012,1.25],"to":[3.5,-0.75],"motionBlur":blur,"easing":easing},"progress":progress,"span":1./fps,"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}}}}

 for q in [250u64,290]{for blur in [0.,0.5,(11./14.)as f32 as f64]{for progress in [0.9,1.]{
 let (w,h)=(4,4);let rgba:Vec<u8>=(0..16u64).flat_map(|i|{let(x,y)=(i%4,i/4);[((11+x*37+y*19+q*13)%256)as u8,((29+x*17+y*43+q*23)%256)as u8,((47+x*53+y*7+q*41)%256)as u8,[64,128,192,255][((x+y+q)%4)as usize]]}).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="move").unwrap().instance();e.params.insert("motion_blur".into(),Param::new(ParamValue::Float(blur)));
 let env=Env(24.,10.);let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:progress*10.,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
 cases.push(json!({"parameters":{"motionBlur":blur},"progress":progress,"span":1./240.,"width":w,"height":h,"rgba":rgba,"scale":1.,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}
 let mut media=vec![];
 for easing in 0..4{for tick in[0u64,250000,500000]{
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000){
   let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
   let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
   let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="move").unwrap().instance();

   e.params.insert("motion_blur".into(),Param::new(ParamValue::Float(t*100.)));
   e.params.insert("easing".into(),Param::new(ParamValue::Choice(easing)));
   e.params.insert("from".into(),Param::new(ParamValue::Vec2(Vec2::new(-4.+t*3.,2.-t*3.))));
   e.params.insert("to".into(),Param::new(ParamValue::Vec2(Vec2::new(2.+t*4.,-2.+t*3.))));
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:Some(&Env(10.,1.)),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"easing":easing,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
