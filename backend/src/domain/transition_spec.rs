//! Persisted procedural transition controls, validated against the pinned native catalog.
use std::collections::BTreeMap;
use serde::{Deserialize,Serialize};
use super::transition_kernel_pack::project::{EffectInstance,Param,ParamValue};
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionSpec {
 pub effect:String,
 #[serde(default,skip_serializing_if="BTreeMap::is_empty")]
 pub params:BTreeMap<String,serde_json::Value>,
}
impl TransitionSpec {
 /// Native Concat kernels use their straight-alpha adapter, outside the Filmcraft pack.
 pub fn concat_style(&self)->Result<Option<super::concat_transitions::Style>,String>{
  if self.effect=="concat.cube" {if !self.params.is_empty(){return Err("Concat Cube takes no parameters".into());}return Ok(Some(super::concat_transitions::Style::Cube));}
  if self.effect=="concat.zoom-transition" {
   if self.params.keys().any(|k|k!="direction"){return Err("unknown Concat Zoom parameter".into());}
   let direction=if let Some(v)=self.params.get("direction"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Zoom direction")?;o.get("Choice").and_then(serde_json::Value::as_u64).filter(|v|*v<2).ok_or("invalid Concat Zoom direction")? as u8}else{0};
   return Ok(Some(super::concat_transitions::Style::Zoom{direction}));
  }
  if self.effect=="concat.whip-pan" {
   if self.params.keys().any(|k|k!="speed"){return Err("unknown Concat Whip Pan parameter".into());}
   let speed=if let Some(v)=self.params.get("speed"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Whip Pan speed")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(20. ..=100.).contains(v)).ok_or("invalid Concat Whip Pan speed")? as f32}else{80.};
   return Ok(Some(super::concat_transitions::Style::WhipPan{speed}));
  }
  if self.effect=="concat.light-leak-transition" {
   if self.params.keys().any(|k|k!="warmth"){return Err("unknown Concat Light Leak parameter".into());}
   let warmth=if let Some(v)=self.params.get("warmth"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Light Leak warmth")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Light Leak warmth")? as f32}else{70.};
   return Ok(Some(super::concat_transitions::Style::LightLeak{warmth}));
  }
  if self.effect=="concat.zoom-blur-transition" {
   if self.params.keys().any(|k|k!="strength"){return Err("unknown Concat Zoom Blur parameter".into());}
   let strength=if let Some(v)=self.params.get("strength"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Zoom Blur strength")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Zoom Blur strength")? as f32}else{70.};
   return Ok(Some(super::concat_transitions::Style::ZoomBlur{strength}));
  }
  if self.effect=="concat.film-burn" {
   if self.params.keys().any(|k|k!="heat"){return Err("unknown Concat Film Burn parameter".into());}
   let heat=if let Some(v)=self.params.get("heat"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Film Burn heat")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Film Burn heat")? as f32}else{70.};
   return Ok(Some(super::concat_transitions::Style::FilmBurn{heat}));
  }
  if self.effect=="concat.glitch-warp" {
   if self.params.keys().any(|k|k!="amount"){return Err("unknown Concat Glitch Warp parameter".into());}
   let amount=if let Some(v)=self.params.get("amount"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Glitch Warp amount")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Glitch Warp amount")? as f32}else{75.};
   return Ok(Some(super::concat_transitions::Style::GlitchWarp{amount}));
  }
  if self.effect=="concat.glitch-transition" {
   if self.params.keys().any(|k|k!="amount"){return Err("unknown Concat Glitch parameter".into());}
   let amount=if let Some(v)=self.params.get("amount"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Glitch amount")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Glitch amount")? as f32}else{70.};
   return Ok(Some(super::concat_transitions::Style::Glitch{amount}));
  }
  if self.effect=="concat.light-leak-flash" {
   if self.params.keys().any(|k|k!="strength"){return Err("unknown Concat Light Leak Flash parameter".into());}
   let strength=if let Some(v)=self.params.get("strength"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Light Leak Flash strength")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Light Leak Flash strength")? as f32}else{80.};
   return Ok(Some(super::concat_transitions::Style::LightLeakFlash{strength}));
  }
  if self.effect=="concat.blur-dissolve" {
   if self.params.keys().any(|k|k!="strength"){return Err("unknown Concat Blur Dissolve parameter".into());}
   let strength=if let Some(v)=self.params.get("strength"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Blur Dissolve strength")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat Blur Dissolve strength")? as f32}else{60.};
   return Ok(Some(super::concat_transitions::Style::BlurDissolve{strength}));
  }
  if matches!(self.effect.as_str(),"concat.spin"|"concat.swirl"){
   let(key,default,low,high)=if self.effect=="concat.spin"{("turns",1.,0.5,2.)}else{("twist",70.,0.,100.)};
   if self.params.keys().any(|k|k!=key){return Err("unknown Concat rotation parameter".into());}
   let value=if let Some(v)=self.params.get(key){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat rotation parameter")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(low..=high).contains(v)).ok_or("invalid Concat rotation parameter")? as f32}else{default};
   return Ok(Some(if self.effect=="concat.spin"{super::concat_transitions::Style::Spin{turns:value}}else{super::concat_transitions::Style::Swirl{twist:value}}));
  }
  if self.effect=="concat.rgb-split-wipe" {
   if self.params.keys().any(|k|k!="spread"){return Err("unknown Concat RGB Split parameter".into());}
   let spread=if let Some(v)=self.params.get("spread"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat spread")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat spread")? as f32}else{60.};
   return Ok(Some(super::concat_transitions::Style::RgbSplitWipe{spread}));
  }
  if matches!(self.effect.as_str(),"concat.push"|"concat.slide") {
   if self.params.keys().any(|k|k!="direction"){return Err("unknown Concat movement parameter".into());}
   let direction=if let Some(v)=self.params.get("direction"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat direction")?;o.get("Choice").and_then(serde_json::Value::as_u64).filter(|v|*v<4).ok_or("invalid Concat direction")? as u8}else{0};
   return Ok(Some(if self.effect=="concat.push" {super::concat_transitions::Style::Push{direction}}else{super::concat_transitions::Style::Slide{direction}}));
  }
  if self.effect=="concat.shape-wipe" {
   if self.params.keys().any(|k|k!="shape"&&k!="softness"){return Err("unknown Concat Shape Wipe parameter".into());}
   let shape=if let Some(v)=self.params.get("shape"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat shape")?;o.get("Choice").and_then(serde_json::Value::as_u64).filter(|v|*v<3).ok_or("invalid Concat shape")? as u8}else{0};
   let softness=if let Some(v)=self.params.get("softness"){let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat softness")?;o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat softness")? as f32}else{15.};
   return Ok(Some(super::concat_transitions::Style::ShapeWipe{shape,softness}));
  }
  if self.effect=="concat.iris" {
   if self.params.keys().any(|k|k!="softness"){return Err("unknown Concat Iris parameter".into());}
   let value=if let Some(v)=self.params.get("softness") {let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat Iris softness")?;o.get("Float").and_then(serde_json::Value::as_f64).ok_or("invalid Concat Iris softness")?}else{15.};
   if !value.is_finite()||!(0. ..=100.).contains(&value){return Err("invalid Concat Iris softness".into());}
   return Ok(Some(super::concat_transitions::Style::Iris{softness:value as f32}));
  }
  if self.effect=="concat.clock-wipe" {if !self.params.is_empty(){return Err("Concat Clock Wipe takes no parameters".into());}return Ok(Some(super::concat_transitions::Style::ClockWipe));}
  if self.effect!="concat.linear-wipe" {return Ok(None);}
  if self.params.keys().any(|k|k!="direction"&&k!="softness"){return Err("unknown Concat wipe parameter".into());}
  let mut direction=0;let mut softness=20.;
  if let Some(v)=self.params.get("direction") {
   let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat wipe direction")?;
   direction=o.get("Choice").and_then(serde_json::Value::as_u64).filter(|v|*v<4).ok_or("invalid Concat wipe direction")? as u8;
  }
  if let Some(v)=self.params.get("softness") {
   let o=v.as_object().filter(|o|o.len()==1).ok_or("invalid Concat wipe softness")?;
   let value=o.get("Float").and_then(serde_json::Value::as_f64).filter(|v|v.is_finite()&&(0. ..=100.).contains(v)).ok_or("invalid Concat wipe softness")?;
   softness=value as f32;
  }
  Ok(Some(super::concat_transitions::Style::LinearWipe{direction,softness}))
 }
 pub fn validate(&self)->Result<(),String>{
  if self.concat_style()?.is_some(){Ok(())}else{self.compile().map(|_|())}
 }

 pub fn compile(&self)->Result<EffectInstance,String>{
  if self.effect.len()>128||self.params.len()>32{return Err("transition control budget exceeded".into());}
  let params=self.params.iter().map(|(key,value)|{
   if key.len()>128{return Err("transition parameter id too long".into());}
   let value:ParamValue=serde_json::from_value(value.clone()).map_err(|e|e.to_string())?;
   Ok((key.clone(),Param::new(value)))
  }).collect::<Result<_,String>>()?;
  let effect=EffectInstance{effect:self.effect.clone(),params};effect.validate()?;Ok(effect)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn all_catalog_controls_roundtrip_and_reject_invalid_settings(){
  let catalog:serde_json::Value=serde_json::from_str(include_str!("transition_kernel_pack/definitions.json")).unwrap();
  let definitions=catalog["definitions"].as_array().unwrap();assert_eq!(definitions.len(),112);
  for d in definitions{
   let spec=TransitionSpec{effect:d["id"].as_str().unwrap().into(),params:d["params"].as_array().unwrap().iter().map(|p|(p["id"].as_str().unwrap().into(),p["default"].clone())).collect()};
   assert!(spec.compile().is_ok(),"{}",spec.effect);
   assert_eq!(serde_json::from_value::<TransitionSpec>(serde_json::to_value(&spec).unwrap()).unwrap(),spec);
  }
  for value in [serde_json::json!({"effect":"unknown"}),serde_json::json!({"effect":"block_motion","params":{"direction":{"Choice":99}}}),serde_json::json!({"effect":"block_motion","params":{"blocks":{"Bool":true}}})]{
   assert!(serde_json::from_value::<TransitionSpec>(value).unwrap().compile().is_err());
  }
 }
}

#[cfg(test)]mod concat_tests{
 use super::*;
 #[test]fn typed_concat_wipe_persistence_rejects_wrong_types_and_ranges(){
  let spec:TransitionSpec=serde_json::from_value(serde_json::json!({"effect":"concat.linear-wipe","params":{"direction":{"Choice":3},"softness":{"Float":70.5}}})).unwrap();
  spec.validate().unwrap();assert!(matches!(spec.concat_style().unwrap(),Some(super::super::concat_transitions::Style::LinearWipe{direction:3,softness})if softness==70.5));
  assert_eq!(serde_json::from_value::<TransitionSpec>(serde_json::to_value(&spec).unwrap()).unwrap(),spec);
  for params in [serde_json::json!({"direction":{"Choice":4}}),serde_json::json!({"direction":{"Float":1}}),serde_json::json!({"softness":{"Float":101}}),serde_json::json!({"softness":{"Float":-1}}),serde_json::json!({"extra":{"Float":1}})] {assert!(TransitionSpec{effect:"concat.linear-wipe".into(),params:serde_json::from_value(params).unwrap()}.validate().is_err());}
 }
}
