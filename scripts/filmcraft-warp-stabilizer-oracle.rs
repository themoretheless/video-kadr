use filmcraft_project::{Param,ParamValue,EffectInstance};
use filmcraft_render::{effects::{apply,FxCtx},image::Image,vfx::FxEnv};
use serde_json::json;
struct Env{key:u64,alpha:u8}
fn rgba(k:i64,alpha:u8)->Vec<u8>{(0..96*64).flat_map(|i|{let x=(i%96) as i64-2*k;let y=(i/96) as i64-k;let u=x.rem_euclid(96) as usize;let v=y.rem_euclid(64) as usize;let a=((u*17+v*31+(u/7)*(v/5)*53)%256)as u8;[a,((u*3+v*11)%256)as u8,((u*29+v*7)%256)as u8,alpha]}).collect()}
fn picture(k:i64,alpha:u8)->Image{let px=rgba(k,alpha).chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();Image{w:96,h:64,px}}
impl FxEnv for Env {
fn source_frame(&self,dt:f64,_scale:f32)->Option<Image>{let t=0.5+dt;if !(0. ..1.).contains(&t){None}else{Some(picture((t*6.).round()as i64,self.alpha))}}
fn frame(&self,_:f64,_:&EffectInstance)->Option<Image>{None}fn track(&self,_:usize)->Option<Image>{None}
fn layer_to_output(&self)->filmcraft_geom::Affine{filmcraft_geom::Affine::IDENTITY}
fn clip_seconds(&self)->f64{1.}fn clip_offset(&self)->f64{0.}fn frame_rate(&self)->f64{6.}
fn media_timecode(&self)->String{String::new()}fn file_name(&self)->String{String::new()}fn sequence_name(&self)->String{String::new()}
fn sequence_size(&self)->(u32,u32){(96,64)}fn source_size(&self)->(u32,u32){(96,64)}fn clip_key(&self)->u64{self.key}
}
fn main(){let mut cases=vec![];for method in 0..4{for framing in 0..4{for result in 0..2{
let preserve=(method+framing)%2==0;let detailed=result==1;let alpha=if framing%2==0{255}else{128};
let env=Env{key:9000000+cases.len()as u64,alpha};let mut img=picture(3,alpha);
let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="warp_stabilizer").unwrap().instance();
for(name,value)in[("method",method),("framing",framing),("result",result)]{e.params.insert(name.into(),Param::new(ParamValue::Choice(value)));}
for(name,value)in[("preserve_scale",preserve),("detailed",detailed)]{e.params.insert(name.into(),Param::new(ParamValue::Bool(value)));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:1.,seconds:0.5,timecode:"",clip_name:"",project:None,env:Some(&env),working:filmcraft_color::WorkingSpace::Rec709};apply(&mut img,&e,&cx);
cases.push(json!({"alpha":alpha,"parameters":{"method":method,"framing":framing,"result":result,"preserveScale":preserve,"detailed":detailed},"bits":img.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
