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
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for invert in[false,true]{for stretch in[false,true]{for max in[0.,4.25,500.]{for scale in[0.5f32,1.]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="compound_blur").unwrap().instance();e.params.insert("max".into(),Param::new(ParamValue::Float(max)));e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));e.params.insert("stretch".into(),Param::new(ParamValue::Bool(stretch)));
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"layer":0,"max":max,"stretch":stretch,"invert":invert},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}
let track_rgba:Vec<u8>=(0..15).flat_map(|i|[[0,0,0,255],[255,255,255,128],[128,64,255,255],[255,255,255,0]][i%4]).collect();let env=Env{track:image(5,3,&track_rgba),index:0,matrix:Affine::translate(1.25,-0.75)};let mut external=vec![];
for layer in 1..3u32{for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for invert in[false,true]{for stretch in[false,true]{for max in[0.,4.25,500.]{for scale in[0.5f32,1.]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="compound_blur").unwrap().instance();e.params.insert("layer".into(),Param::new(ParamValue::Choice(layer)));e.params.insert("max".into(),Param::new(ParamValue::Float(max)));e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));e.params.insert("stretch".into(),Param::new(ParamValue::Bool(stretch)));
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);external.push(json!({"parameters":{"layer":layer,"max":max,"stretch":stretch,"invert":invert},"width":w,"height":h,"rgba":rgba,"scale":scale,"trackRgba":track_rgba,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}
}
let mut media=vec![];
for invert in[false,true]{for stretch in[false,true]{for tick in[0u64,250000,500000]{
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){
let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="compound_blur").unwrap().instance();
for(n,v)in[("max",t*8.5)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
e.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));e.params.insert("stretch".into(),Param::new(ParamValue::Bool(stretch)));
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
}
media.push(json!({"invert":invert,"stretch":stretch,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
}
let mut composition_cases=vec![];
for control_max in[0.,4.25]{for(sx,sy,rotation,x,y)in[(1.,1.,0.,0.,0.),(0.8,0.7,30.,1.25,-0.75),(0.25,0.25,0.,-2.,1.)]{for stretch in[false,true]{for invert in[false,true]{for max in[0.,4.25,500.]{
let control_rgba:Vec<u8>=(0..24).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/6)%5]).collect();let mut control=image(6,4,&control_rgba);
let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id=="compound_blur").unwrap().instance();effect.params.insert("max".into(),Param::new(ParamValue::Float(control_max)));
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut control,&effect,&cx);
let(sin,cos)=(rotation as f64).to_radians().sin_cos();let(a,b,c,d)=(cos*sx,sin*sx,-sin*sy,cos*sy);let matrix=Affine{a,b,c,d,e:9.+x-a*3.-c*2.,f:5.+y-b*3.-d*2.};let transformed=control.transformed(18,10,&matrix);let mut track=Image::new(18,10);filmcraft_render::blend::composite(&mut track,&transformed,0.7,filmcraft_render::blend::Blend::Normal);
let track_bits:Vec<_>=track.px.iter().map(|v|v.to_bits()).collect();let env=Env{track,index:1,matrix:Affine::IDENTITY};let rgba:Vec<u8>=(0..180).flat_map(|i|[[255,255,255,255],[64,128,192,128],[128,64,255,64]][i%3]).collect();let mut out=image(18,10,&rgba);
effect.params.insert("max".into(),Param::new(ParamValue::Float(max)));effect.params.insert("layer".into(),Param::new(ParamValue::Choice(2)));effect.params.insert("stretch".into(),Param::new(ParamValue::Bool(stretch)));effect.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));
let cx=FxCtx{env:Some(&env),..cx};apply(&mut out,&effect,&cx);
composition_cases.push(json!({"parameters":{"max":max,"layer":2,"stretch":stretch,"invert":invert},"controlMax":control_max,"controlRgba":control_rgba,"transform":{"scaleX":sx,"scaleY":sy,"rotation":rotation,"x":x,"y":y},"rgba":rgba,"trackBits":track_bits,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}
let mut external_media=vec![];
for(sx,sy,rotation,x,y)in[(1.,1.,0.,0.,0.),(0.8,0.7,30.,1.25,-0.75),(0.25,0.25,0.,-2.,1.)]{for stretch in[false,true]{for invert in[false,true]{for tick in[0u64,250000,500000]{
let control_rgba:Vec<u8>=(0..24).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/6)%5]).collect();
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
let mut control=image(6,4,&control_rgba);let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id=="compound_blur").unwrap().instance();effect.params.insert("max".into(),Param::new(ParamValue::Float(t*4.25)));
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut control,&effect,&cx);
let(sin,cos)=(rotation as f64).to_radians().sin_cos();let(a,b,c,d)=(cos*sx,sin*sx,-sin*sy,cos*sy);let matrix=Affine{a,b,c,d,e:8.+x-a*3.-c*2.,f:8.+y-b*3.-d*2.};let transformed=control.transformed(16,16,&matrix);let mut track=Image::new(16,16);filmcraft_render::blend::composite(&mut track,&transformed,0.7,filmcraft_render::blend::Blend::Normal);
let env=Env{track,index:1,matrix:Affine::IDENTITY};let mut out=image(16,16,&rgba);effect.params.insert("max".into(),Param::new(ParamValue::Float(t*8.5)));effect.params.insert("layer".into(),Param::new(ParamValue::Choice(2)));effect.params.insert("stretch".into(),Param::new(ParamValue::Bool(stretch)));effect.params.insert("invert".into(),Param::new(ParamValue::Bool(invert)));let cx=FxCtx{env:Some(&env),..cx};apply(&mut out,&effect,&cx);frames.push(out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
}
external_media.push(json!({"stretch":stretch,"invert":invert,"localTick":tick,"transform":{"scaleX":sx,"scaleY":sy,"rotation":rotation,"x":x,"y":y},"controlRgba":control_rgba,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"external":external,"media":media,"compositionCases":composition_cases,"externalMedia":external_media}));}
