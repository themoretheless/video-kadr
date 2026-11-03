//! Portable per-clip selection of a verified automatic video mask family.
use anyhow::{ensure,Result};
use serde::{Serialize,Deserialize};
/// A reference to a saved owner, never an authorization grant.
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CacheOrigin {
 pub project_id:String,
 pub expected_revision:u64,
 pub source_id:String,
 #[serde(default,skip_serializing_if="std::collections::BTreeMap::is_empty")]
 pub region_sources:std::collections::BTreeMap<String,String>,
}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RegionTrack {pub stroke:usize,pub snapshot:super::video_region::Attachment}

#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields,try_from="Wire")]
pub struct Binding { #[serde(default,skip_serializing_if="Option::is_none")] pub origin:Option<CacheOrigin>, pub source_hash:String, pub model_hash:String, pub runtime_hash:String, pub subject:String, #[serde(default,skip_serializing_if="Option::is_none")] pub feather:Option<f64>, #[serde(default,skip_serializing_if="Option::is_none")] pub correction:Option<super::cutout::Correction>, #[serde(default,skip_serializing_if="Vec::is_empty")] pub region_tracks:Vec<RegionTrack> }
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Wire { #[serde(default)] origin:Option<CacheOrigin>, source_hash:String, model_hash:String, runtime_hash:String, subject:String, #[serde(default)] feather:Option<f64>, #[serde(default)] correction:Option<super::cutout::Correction>, #[serde(default)] region_tracks:Vec<RegionTrack> }
impl Binding {
 pub fn settings(&self)->super::cutout::Correction {self.correction.clone().unwrap_or(super::cutout::Correction{custom:false,strokes:vec![],feather:self.feather.unwrap_or(0.)})}
 pub fn validate(&self)->Result<()> {
  if let Some(origin)=&self.origin {
   super::composition::SourceId::parse(&origin.project_id)?;
   super::composition::SourceId::parse(&origin.source_id)?;
   ensure!((1..=9_007_199_254_740_991).contains(&origin.expected_revision),"invalid video cache owner revision");
   ensure!(origin.region_sources.len()<=1024,"too many video region owner bindings");
   for(target,source)in &origin.region_sources {
    super::composition::SourceId::parse(target)?;super::composition::SourceId::parse(source)?;
    ensure!(self.region_tracks.iter().any(|track|&track.snapshot.mask_source_id==target),"unused video region owner binding");
   }
  }

  if let Some(correction)=&self.correction{validate_tracks(correction,&self.region_tracks,&self.source_hash)?;ensure!(self.feather.is_none_or(|f|f==correction.feather),"conflicting video correction feather");}
  ensure!(self.correction.is_some()||self.region_tracks.is_empty(),"video regions require correction settings");
  ensure!(self.feather.is_none_or(|value|value.is_finite()&&(0.0..=0.1).contains(&value)),"invalid video cutout feather");
  for hash in [&self.source_hash,&self.model_hash,&self.runtime_hash]{ensure!(hash.len()==64&&hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"invalid video cutout identity");}
  let person=matches!(self.model_hash.as_str(),super::cutout::PERSON_MODEL_HASH|super::cutout::SELFIE_MODEL_HASH);
  let object=self.model_hash=="60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a";
  ensure!(match self.subject.as_str(){"auto"=>person||object,"person"=>person,"object"=>object,_=>false},"unsupported bound video cutout family");Ok(())
 }
}
impl TryFrom<Wire> for Binding { type Error=anyhow::Error;fn try_from(w:Wire)->Result<Self>{let value=Self{origin:w.origin,source_hash:w.source_hash,model_hash:w.model_hash,runtime_hash:w.runtime_hash,subject:w.subject,feather:w.feather,correction:w.correction,region_tracks:w.region_tracks};value.validate()?;Ok(value)} }

pub fn validate_correction(correction:&super::cutout::Correction)->Result<()> {
 correction.validate()?;
 ensure!(correction.strokes.iter().all(|stroke|stroke.region.is_none() && !(matches!(stroke.tool,super::cutout::Tool::SmartBrush|super::cutout::Tool::SmartEraser)&&stroke.at.is_some())),"video correction requires source-time learned regions");Ok(())
}

pub fn validate_tracks(correction:&super::cutout::Correction,tracks:&[RegionTrack],source_hash:&str)->Result<()> {
 correction.validate()?;ensure!(tracks.len()<=correction.strokes.len(),"too many video region tracks");
 let mut indices=std::collections::BTreeSet::new();
 for track in tracks{
  ensure!(indices.insert(track.stroke),"duplicate video region stroke");track.snapshot.validate()?;
  let stroke=correction.strokes.get(track.stroke).ok_or_else(||anyhow::anyhow!("video region stroke missing"))?;
  ensure!(matches!(stroke.tool,super::cutout::Tool::SmartBrush|super::cutout::Tool::SmartEraser)&&stroke.region.is_none(),"video track requires a smart stroke");
  let at=stroke.at.ok_or_else(||anyhow::anyhow!("video smart stroke requires seed time"))?;
  let micros=(at*1_000_000.).round();ensure!(micros.is_finite()&&(0.0..=9_007_199_254_740_991.).contains(&micros),"video region seed exceeds source clock");
  let seed=super::video_region::frame_ordinal(micros as u64,track.snapshot.frames,track.snapshot.frame_rate,track.snapshot.frame_times_us.as_deref())?;
  ensure!(seed==track.snapshot.seed_frame&&track.snapshot.points==stroke.points&&track.snapshot.source_hash==source_hash,"video region source/seed/prompt mismatch");
 }
 for(index,stroke)in correction.strokes.iter().enumerate(){ensure!(stroke.region.is_none(),"still region cannot bind a video stroke");if matches!(stroke.tool,super::cutout::Tool::SmartBrush|super::cutout::Tool::SmartEraser)&&stroke.at.is_some(){ensure!(indices.contains(&index),"timed smart video stroke requires a region track");}}
 Ok(())
}
