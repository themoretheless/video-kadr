//! Active upstream Gradient Wipe, including external layer environment.
use filmcraft_project::{Param,ParamValue,EffectInstance};use filmcraft_render::{effects::{apply,FxCtx},image::Image,vfx::FxEnv};use filmcraft_geom::Affine;use serde_json::json;
struct Env {track:Image,index:usize,matrix:Affine}
impl FxEnv for Env {
 fn frame(&self,_:f64,_:&EffectInstance)->Option<Image>{None}
 fn source_frame(&self,_:f64,_:f32)->Option<Image>{None}
 fn track(&self,i:usize)->Option<Image>{if i==self.index{Some(self.track.clone())}else{None}}
 fn layer_to_output(&self)->Affine{self.matrix}
 fn clip_seconds(&self)->f64{1.}fn clip_offset(&self)->f64{0.}fn frame_rate(&self)->f64{30.}
 fn media_timecode(&self)->String{String::new()}fn file_name(&self)->String{String::new()}fn sequence_name(&self)->String{String::new()}
 fn sequence_size(&self)->(u32,u32){(18,10)}fn source_size(&self)->(u32,u32){(18,10)}fn clip_key(&self)->u64{0}
}
fn image(w:usize,h:usize,rgba:&[u8])->Image {Image{w,h,px:rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect()}}
fn main(){let mut cases=vec![];let track_rgba:Vec<u8>=(0..15).flat_map(|i|[[0,0,0,255],[255,255,255,128],[128,64,255,255],[255,255,255,0]][i%4]).collect();let env=Env{track:image(5,3,&track_rgba),index:0,matrix:Affine::translate(1.25,-0.75)};
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for invert in[false,true]{for completion in[0.,25.,50.,100.]{for softness in[0.,25.,100.]{for placement in 0..3{for layer in 0..3{
 let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let mut out=image(w,h,&rgba);let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="gradient_wipe_legacy").unwrap().instance();
 for(n,v)in[("completion",completion),("softness",softness)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 for(n,v)in[("layer",layer),("placement",placement)]{e.params.insert(n.into(),Param::new(ParamValue::Choice(v)));}e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));
 let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
 cases.push(json!({"parameters":{"completion":completion,"softness":softness,"placement":placement,"layer":layer,"invert":invert},"width":w,"height":h,"rgba":rgba,"trackRgba":track_rgba,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}let mut media=vec![];
for invert in[false,true]{for tick in[0u64,250000,500000]{
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){let local=(tick+index*100000).min(500000);let t=local as f64/500000.;let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="gradient_wipe_legacy").unwrap().instance();e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));for(n,v)in[("completion",t*100.),("softness",t*100.)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());}
media.push(json!({"invert":invert,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}let mut composition_cases=vec![];
for control_completion in[0.,77.]{for(sx,sy,rotation,x,y)in[(1.,1.,0.,0.,0.),(0.8,0.7,30.,1.25,-0.75),(0.25,0.25,0.,-2.,1.)]{for placement in 0..3{for invert in[false,true]{for(completion,softness)in[(25.,0.),(50.,25.),(100.,100.)]{
 let control_rgba:Vec<u8>=(0..24).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/6)%5]).collect();let mut control=image(6,4,&control_rgba);
 let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id=="gradient_wipe_legacy").unwrap().instance();effect.params.insert("completion".into(),Param::new(ParamValue::Float(control_completion)));effect.params.insert("softness".into(),Param::new(ParamValue::Float(100.)));
 let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut control,&effect,&cx);
 let(sin,cos)=(rotation as f64).to_radians().sin_cos();let(a,b,c,d)=(cos*sx,sin*sx,-sin*sy,cos*sy);let matrix=Affine{a,b,c,d,e:9.+x-a*3.-c*2.,f:5.+y-b*3.-d*2.};let transformed=control.transformed(18,10,&matrix);let mut track=Image::new(18,10);filmcraft_render::blend::composite(&mut track,&transformed,0.7,filmcraft_render::blend::Blend::Normal);
 let track_bits:Vec<_>=track.px.iter().map(|v|v.to_bits()).collect();let env=Env{track,index:1,matrix:Affine::IDENTITY};let rgba:Vec<u8>=(0..180).flat_map(|i|[[255,255,255,255],[64,128,192,128],[128,64,255,64]][i%3]).collect();let mut out=image(18,10,&rgba);
 effect.params.insert("completion".into(),Param::new(ParamValue::Float(completion)));effect.params.insert("softness".into(),Param::new(ParamValue::Float(softness)));effect.params.insert("layer".into(),Param::new(ParamValue::Choice(2)));effect.params.insert("placement".into(),Param::new(ParamValue::Choice(placement)));effect.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));
 let cx=FxCtx{env:Some(&env),..cx};apply(&mut out,&effect,&cx);
 composition_cases.push(json!({"parameters":{"completion":completion,"softness":softness,"layer":2,"placement":placement,"invert":invert},"controlCompletion":control_completion,"controlRgba":control_rgba,"transform":{"scaleX":sx,"scaleY":sy,"rotation":rotation,"x":x,"y":y},"rgba":rgba,"trackBits":track_bits,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media,"compositionCases":composition_cases}));}
