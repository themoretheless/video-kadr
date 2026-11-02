//! Original Track Matte Key with output-track and layer-coordinate environment.
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
for control_blur in[0.,4.25]{for(sx,sy,rotation,x,y)in[(1.,1.,0.,0.,0.),(0.8,0.7,30.,1.25,-0.75),(0.25,0.25,0.,-2.,1.)]{for composite in 0..2{for reverse in[false,true]{
let control_rgba:Vec<u8>=(0..24).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/6)%5]).collect();let mut control=image(6,4,&control_rgba);
let mut blur=filmcraft_project::effect_defs().iter().find(|d|d.id=="compound_blur").unwrap().instance();blur.params.insert("max".into(),Param::new(ParamValue::Float(control_blur)));
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut control,&blur,&cx);
let(sin,cos)=(rotation as f64).to_radians().sin_cos();let(a,b,c,d)=(cos*sx,sin*sx,-sin*sy,cos*sy);let matrix=Affine{a,b,c,d,e:9.+x-a*3.-c*2.,f:5.+y-b*3.-d*2.};let transformed=control.transformed(18,10,&matrix);let mut track=Image::new(18,10);filmcraft_render::blend::composite(&mut track,&transformed,0.7,filmcraft_render::blend::Blend::Normal);
let track_bits:Vec<_>=track.px.iter().map(|v|v.to_bits()).collect();let env=Env{track,index:1,matrix:Affine::IDENTITY};let rgba:Vec<u8>=(0..180).flat_map(|i|[[255,255,255,255],[64,128,192,128],[128,64,255,64]][i%3]).collect();let mut out=image(18,10,&rgba);
let mut effect=filmcraft_project::effect_defs().iter().find(|d|d.id=="track_matte").unwrap().instance();for(n,v)in[("matte",ParamValue::Choice(2)),("composite",ParamValue::Choice(composite)),("reverse",ParamValue::Bool(reverse))]{effect.params.insert(n.into(),Param::new(v));}
let cx=FxCtx{env:Some(&env),..cx};apply(&mut out,&effect,&cx);
cases.push(json!({"parameters":{"matte":2,"composite":composite,"reverse":reverse},"controlBlur":control_blur,"controlRgba":control_rgba,"transform":{"scaleX":sx,"scaleY":sy,"rotation":rotation,"x":x,"y":y},"rgba":rgba,"trackBits":track_bits,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
