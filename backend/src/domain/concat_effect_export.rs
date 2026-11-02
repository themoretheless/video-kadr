//! Reverse source effect links only when the existing native adapter proves exact reconstruction.
use super::{composition::VideoEffect,concat_effect_upgrade::{AppliedFilter,native_effect}};
use anyhow::{Context,Result,bail,ensure};
use serde_json::Value;

pub fn export_effect(effect:&VideoEffect)->Result<AppliedFilter>{
 if let VideoEffect::ConcatBypass{link}=effect{return Ok(link.clone())}
 let (effect,enabled)=match effect{
  VideoEffect::NativeBypass{effect}=>{ensure!(!matches!(effect.as_ref(),VideoEffect::NativeBypass{..}|VideoEffect::ConcatBypass{..}),"Nested bypass ownership cannot be represented by one Concat effect link");(effect.as_ref(),false)},
  effect=>(effect,true)
 };
 let wire=serde_json::to_value(effect)?;
 let kind=wire["kind"].as_str().context("Effect kind missing")?;
 let name=kind.strip_prefix("concat_").context("Native effect has no Concat package representation")?;
 let mut link=AppliedFilter{id:format!("concat.{}",name.replace('_',"-")),params:Default::default(),enabled:true,keys:Default::default()};
 if let Some(params)=wire.get("parameters").and_then(Value::as_object){for (name,value) in params{
  if let Some(values)=value.as_array(){
   if matches!(kind,"concat_adjust"|"concat_color_wheels")&&matches!(name.as_str(),"lift"|"gamma"|"gain"){
    ensure!(values.len()==3,"Invalid source wheel vector");
    for (axis,value) in ["x","y","m"].iter().zip(values){link.params.insert(format!("{name}.{axis}"),value.as_f64().context("Invalid source wheel coordinate")?);}
   }else if matches!(kind,"concat_adjust"|"concat_curves")&&matches!(name.as_str(),"luma"|"red"|"green"|"blue"){
    ensure!(values.len()<=8,"Source curve exceeds eight points");
    for (index,point) in values.iter().enumerate(){let point=point.as_array().context("Invalid source curve point")?;ensure!(point.len()==2,"Invalid source curve dimensions");for (axis,value) in ["x","y"].iter().zip(point){link.params.insert(format!("{name}.{index}.{axis}"),value.as_f64().context("Invalid source curve coordinate")?);}}
   }else{bail!("{}: non-scalar source parameter {name}",link.id)}
  }else{link.params.insert(name.clone(),value.as_f64().with_context(||format!("{}: non-scalar source parameter {name}",link.id))?);}
 }}
 if kind=="concat_vibrance"{ensure!(wire["parameters"]["intensity"].as_f64()==Some(100.),"Native vibrance intensity has no source package field");link.params.remove("intensity");}
 if let Some(keys)=wire.get("concatKeys").filter(|keys|!keys.is_null()){
  if keys.is_array(){
   let channel=match kind{"concat_exposure"=>"stops","concat_contrast"|"concat_saturation"|"concat_glow"|"concat_film_grain"=>"amount","concat_gaussian_blur"=>"radius","concat_halftone"=>"size","concat_fisheye"=>"strength","concat_chromatic_aberration"=>"shift",_=>bail!("{}: scalar source key channel needs a reverse mapping",link.id)};
   link.keys.insert(channel.into(),serde_json::from_value(keys.clone())?);
  }else{link.keys=serde_json::from_value(keys.clone())?;}
 }
 let reconstructed=native_effect(&link)?.with_context(||format!("{}: source package unavailable",link.id))?;
 ensure!(serde_json::to_value(reconstructed)?==wire,"{}: native animation or parameter representation cannot be exported losslessly",link.id);
 link.enabled=enabled;
 Ok(link)
}
