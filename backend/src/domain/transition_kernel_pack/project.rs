//! Minimal non-animated transition parameter adapter. No upstream project dependency.
use serde::Deserialize;
use std::{collections::BTreeMap,sync::OnceLock};
use super::{geom::Vec2,time::Tick};
#[derive(Clone,Debug,Deserialize)]
pub enum ParamValue {
 Float(f64),Choice(u32),Bool(bool),Color([f32;4]),
 Vec2(#[serde(deserialize_with="point_or_auto")] Vec2),
 Text(String),
}
fn point_or_auto<'de,D:serde::Deserializer<'de>>(d:D)->Result<Vec2,D::Error>{
 #[derive(Deserialize)]struct Point{x:Option<f64>,y:Option<f64>}
 let p=Point::deserialize(d)?;Ok(Vec2::new(p.x.unwrap_or(f64::NAN),p.y.unwrap_or(f64::NAN)))
}
impl ParamValue {fn as_f64(&self)->Option<f64>{match self{Self::Float(v)=>Some(*v),Self::Choice(v)=>Some(*v as f64),Self::Bool(v)=>Some(*v as u8 as f64),_=>None}}}
#[derive(Clone,Debug,Deserialize)]pub struct Param {pub value:ParamValue,#[serde(skip)]numeric_at_zero:Option<f64>}
impl Param {pub fn new(value:ParamValue)->Self{Self{value,numeric_at_zero:None}}}
#[derive(Clone,Debug,Deserialize)]pub struct EffectInstance {pub effect:String,#[serde(default)]pub params:BTreeMap<String,Param>}
#[derive(Deserialize)]pub struct ParamDef {pub id:String,pub default:ParamValue,pub kind:ParamKind}
#[derive(Deserialize)]pub struct EffectDef {pub id:String,pub params:Vec<ParamDef>}
impl EffectDef {pub fn param(&self,id:&str)->Option<&ParamDef>{self.params.iter().find(|p|p.id==id)}}
impl EffectInstance {
 /// Original transition kernels sample numeric controls at Tick::ZERO, while
 /// discrete/colour/point controls retain their authored baseline values.
 pub fn from_source(raw:&serde_json::Value)->Result<Self,String>{
  let mut effect:Self=serde_json::from_value(raw.clone()).map_err(|e|e.to_string())?;
  effect.validate()?;
  for(id,param)in &mut effect.params{
   let source=&raw["params"][id];
   let Some(keys)=source.get("keyframes")else{continue};
   let keys=keys.as_array().ok_or("invalid transition parameter keys")?;
   if keys.is_empty(){continue}
   if keys.len()>4096{return Err("transition parameter key budget exceeded".into())}
   let numeric=if param.value.as_f64().is_some(){
    let curve=crate::domain::filmcraft_effect_scalar::convert(source,crate::domain::filmcraft_effect_scalar::Evaluation::EffectParameter).map_err(|e|e.to_string())?;
    crate::domain::filmcraft_effect_scalar::sample(&curve,0)
   }else{
    // Well-typed point/colour/text keys have no numeric interpretation. The
    // transition's value-based accessors deliberately keep their base value.
    for key in keys{let value:ParamValue=serde_json::from_value(key["value"].clone()).map_err(|e|e.to_string())?;if value.as_f64().is_some(){return Err("mixed scalar/non-scalar transition keys are not connected".into())}}
    0.0
   };
   if !numeric.is_finite(){return Err("nonfinite sampled transition parameter".into())}
   param.numeric_at_zero=Some(numeric);
  }
  Ok(effect)
 }
 pub fn param(&self,id:&str)->Option<&Param>{self.params.get(id)}
 pub fn def(&self)->Option<&'static EffectDef>{
  static DEFINITIONS:OnceLock<Vec<EffectDef>>=OnceLock::new();
  let definitions=DEFINITIONS.get_or_init(||{
   #[derive(Deserialize)]struct Catalog{definitions:Vec<EffectDef>}
   serde_json::from_str::<Catalog>(include_str!("definitions.json")).expect("checked-in transition catalog must decode").definitions
  });definitions.iter().find(|d|d.id==self.effect)
 }
 pub fn f64_at(&self,id:&str,_tick:Tick)->f64{
  self.params.get(id).map(|p|p.numeric_at_zero.or_else(||p.value.as_f64()).unwrap_or(0.0)).or_else(||self.def().and_then(|d|d.param(id)).and_then(|p|p.default.as_f64())).unwrap_or(0.0)
 }
}

#[derive(Deserialize)]
pub enum ParamKind {Float{min:f64,max:f64},Choice(Vec<String>),Angle,Bool,Color,Point}
impl EffectInstance {
 /// Validate the typed native boundary; the kernel's unknown-ID dissolve fallback is not admission.
 pub fn validate(&self)->Result<(),String>{
  let def=self.def().ok_or_else(||format!("unknown transition {}",self.effect))?;
  if self.params.len()>def.params.len(){return Err("too many transition parameters".into());}
  for(id,param)in &self.params{
   let definition=def.param(id).ok_or_else(||format!("unknown transition parameter {id}"))?;
   let valid=match(&definition.kind,&param.value){
    (ParamKind::Float{min,max},ParamValue::Float(v))=>v.is_finite()&&v>=min&&v<=max,
    (ParamKind::Angle,ParamValue::Float(v))=>v.is_finite()&&v.abs()<=f32::MAX as f64,
    (ParamKind::Choice(options),ParamValue::Choice(v))=>(*v as usize)<options.len(),
    (ParamKind::Bool,ParamValue::Bool(_))=>true,
    (ParamKind::Color,ParamValue::Color(c))=>c.iter().all(|v|v.is_finite()&&(0.0..=1.0).contains(v)),
    (ParamKind::Point,ParamValue::Vec2(v))=>[v.x,v.y].iter().all(|v|v.is_nan()||(v.is_finite()&&v.abs()<=f32::MAX as f64)),
    _=>false,
   };
   if !valid{return Err(format!("invalid transition parameter {id}"));}
  }
  Ok(())
 }
}

#[cfg(test)]mod authored_control_tests{
 use super::*;use crate::domain::{filmcraft_video_compositor::{compose,PreparedClip,PreparedLayer},filmcraft_blend::Blend,transition_kernel_pack::{context::RenderContext,image::Image}};use std::sync::{Arc,atomic::AtomicBool};
 #[test]fn original_keyed_controls_and_frames_match_every_bit(){
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/filmcraft-transition-keyed-controls.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),2016);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,32,64*1024).unwrap();
  let image=|key:&str|Image{w:8,h:4,px:fixture[key].as_array().unwrap().iter().map(|v|f32::from_bits(v.as_u64().unwrap()as u32)).collect()};
  for(index,case)in cases.iter().enumerate(){let effect=EffectInstance::from_source(&case["effect"]).unwrap();for(id,bits)in case["numericBits"].as_object().unwrap(){assert_eq!(effect.f64_at(id,super::super::time::Tick::ZERO).to_bits(),bits.as_u64().unwrap(),"numeric {index}/{id}");}for(id,param)in &effect.params{let base:ParamValue=serde_json::from_value(case["effect"]["params"][id]["value"].clone()).unwrap();assert_eq!(format!("{:?}",param.value),format!("{:?}",base),"baseline {index}/{id}");}
   let layers=[PreparedLayer::Clip(PreparedClip{image:image("base"),opacity:1.,blend:Blend::Normal}),PreparedLayer::Transition{effect,from:Some(PreparedClip{image:image("a"),opacity:0.37,blend:Blend::Multiply}),to:Some(PreparedClip{image:image("b"),opacity:0.73,blend:Blend::Screen}),progress:case["progress"].as_f64().unwrap()as f32,reverse:case["reverse"].as_bool().unwrap()}];
   let output=compose(&context,&layers,8,4,1.).unwrap();assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"transition case {index}, {} profile {}",case["effect"]["effect"],case["profile"]);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}
