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
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for matte in 0..5{for composite in 0..2{for reverse in[false,true]{for environment in 0..3{for matrix in[Affine::IDENTITY,Affine{a:0.8,b:0.2,c:-0.3,d:1.2,e:1.25,f:-0.75},Affine{a:1.,b:0.,c:0.,d:1.,e:100.,f:-100.}]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();
let track_rgba:Vec<u8>=(0..24).flat_map(|i|[[255,0,128,255],[255,255,255,128],[0,255,0,64],[128,64,255,1],[0,0,0,0]][i%5]).collect();
let mut out=image(w,h,&rgba);let env=Env{track:image(6,4,&track_rgba),index:if environment==2{99}else{(matte as usize).saturating_sub(1)},matrix};
let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="track_matte").unwrap().instance();
for(n,v)in[("matte",ParamValue::Choice(matte)),("composite",ParamValue::Choice(composite)),("reverse",ParamValue::Bool(reverse))]{e.params.insert(n.into(),Param::new(v));}
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:if environment==0{None}else{Some(&env)},working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);
cases.push(json!({"width":w,"height":h,"rgba":rgba,"parameters":{"matte":matte,"composite":composite,"reverse":reverse},"environment":environment,"matrix":[matrix.a,matrix.b,matrix.c,matrix.d,matrix.e,matrix.f],"trackWidth":6,"trackHeight":4,"trackRgba":track_rgba,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
