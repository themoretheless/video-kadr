//! Intrinsic source clip state for sequence rendering, sampled on the effect clock.
//! Raw effects/masks are retained; this is not admission into the pixel renderer.
use super::{filmcraft_project_file::SourceProjectDocument,filmcraft_project_index::ProjectIndex,filmcraft_sequence_source::{SourceSequence,SourceTrackItem},filmcraft_motion::FilmcraftMotion,filmcraft_effect_scalar::{self,Evaluation}};
use anyhow::{Context,Result};use serde_json::{Value,json};
#[derive(Debug)]pub struct ClipAppearance{pub matrix:[f64;6],pub opacity:f32,pub blend:u32,pub effects:Vec<Value>,pub parameter_ticks:i64}
pub fn sample(document:&SourceProjectDocument,sequence:u64,clip:u64,at:i64)->Result<ClipAppearance>{
 let index=ProjectIndex::new(document)?;let sequence=SourceSequence::read(&index,sequence)?;
 let item=sequence.video_tracks.iter().flat_map(|t|&t.items).find(|i|i.id==clip).context("missing appearance clip")?;
 sample_item(&index,&sequence,item,at)
}
pub fn sample_item(index:&ProjectIndex<'_>,sequence:&SourceSequence<'_>,item:&SourceTrackItem<'_>,at:i64)->Result<ClipAppearance>{
 let ticks=item.clocks(at)?.parameters;
 let effects=item.raw["effects"].as_array().context("missing clip effects")?;
 let effect=|name:&str|effects.iter().find(|e|e["effect"].as_str()==Some(name));
 let enabled=|effect:&Value|effect["enabled"].as_bool().context("invalid intrinsic enabled state");
 let motion=effect("motion");let motion_enabled=motion.map(enabled).transpose()?.unwrap_or(false);
 let size=index.source_size(item.source_id).context("missing appearance source size")?;
 let dimension=|name:&str|->Result<u32>{u32::try_from(sequence.raw["settings"][name].as_u64().context("missing sequence dimensions")?).context("sequence dimensions overflow")};let canvas=(dimension("width")?,dimension("height")?);
 let params=if motion_enabled{motion.unwrap().get("params").context("missing Motion parameters")?.clone()}else{json!({})};
 let matrix=FilmcraftMotion::from_source_parameters_in_geometry(&params,canvas,size)?.matrix(canvas,size,ticks,motion_enabled,item.raw["scale_to_frame"].as_bool().unwrap_or(false))?;
 let mut opacity=1.;let mut blend=0;
 if let Some(effect)=effect("opacity").filter(|e|e["enabled"].as_bool()==Some(true)){
  let value=match effect.pointer("/params/opacity"){Some(parameter)=>{let curve=filmcraft_effect_scalar::convert(parameter,Evaluation::EffectParameter)?;filmcraft_effect_scalar::sample(&curve,ticks)},None=>100.};opacity=(value/100.).clamp(0.,1.) as f32;
  blend=effect.pointer("/params/blend/value/Choice").and_then(Value::as_u64).filter(|v|*v<27).unwrap_or(0) as u32;
 }
 Ok(ClipAppearance{matrix,opacity,blend,effects:effects.clone(),parameter_ticks:ticks})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn original_intrinsic_appearance_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-appearance.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),256);
  for case in cases{let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let actual=sample(&document,case["sequence"].as_u64().unwrap(),case["clip"].as_u64().unwrap(),case["at"].as_i64().unwrap()).unwrap();assert_eq!(json!(actual.matrix.map(f64::to_bits)),case["matrixBits"],"matrix mode {} at {}",case["mode"],case["at"]);assert_eq!(json!(actual.opacity.to_bits()),case["opacityBits"]);assert_eq!(json!(actual.blend),case["blend"]);assert_eq!(json!(actual.effects),case["effects"]);assert_eq!(json!(actual.parameter_ticks),case["parameters"]);}
 }
}
