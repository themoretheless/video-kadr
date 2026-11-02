//! Bridge source-project frame selection to decoding/effect providers. Pure,
//! borrowed authoring context: no file reads, project mutation or silent effect loss.
use super::{filmcraft_project_file::SourceProjectDocument,filmcraft_project_index::ProjectIndex,filmcraft_sequence_source::SourceSequence,filmcraft_video_plan::{self,VideoLayer,PlannedClip},filmcraft_video_appearance::{self,ClipAppearance},filmcraft_video_compositor::{self,PreparedClip,PreparedLayer},filmcraft_blend::Blend,transition_kernel_pack::{image::Image,project::EffectInstance}};
use anyhow::{ensure,Context,Result};use serde_json::Value;use std::collections::BTreeMap;
pub struct SourceClipJob<'a>{
 pub clip:u64,pub item:u64,pub frame_ticks:i64,pub parameter_ticks:i64,pub generator_ticks:i64,
 pub raw_clip:&'a Value,pub raw_source:&'a Value,pub source_size:Option<(u32,u32)>,pub appearance:Option<ClipAppearance>,
}
pub struct SourceFrameJobs<'a>{pub sequence:&'a Value,pub at:i64,pub layers:Vec<VideoLayer>,pub clips:BTreeMap<u64,SourceClipJob<'a>>}
impl<'a> SourceFrameJobs<'a>{
 pub fn build(document:&'a SourceProjectDocument,sequence:u64,at:i64)->Result<Self>{Self::build_track(document,sequence,at,None)}
 pub(crate) fn build_track(document:&'a SourceProjectDocument,sequence:u64,at:i64,only:Option<usize>)->Result<Self>{
  let index=ProjectIndex::new(document)?;let sequence=SourceSequence::read(&index,sequence)?;let layers=filmcraft_video_plan::plan_sequence_track(&sequence,at,only)?;ensure!(layers.len()<=256,"video layer budget exceeded");let mut clips=BTreeMap::new();
  let mut add=|planned:&PlannedClip|->Result<()>{
   if clips.contains_key(&planned.clip){return Ok(())}
   let item=sequence.video_tracks.iter().flat_map(|track|&track.items).find(|item|item.id==planned.clip).context("missing planned source clip")?;
   let raw_source=index.item(item.source_id).context("missing planned source item")?;let source_size=index.source_size(item.source_id);
   let appearance=if source_size.is_some(){Some(filmcraft_video_appearance::sample_item(&index,&sequence,item,at)?)}else{None};
   clips.insert(planned.clip,SourceClipJob{clip:planned.clip,item:planned.item,frame_ticks:planned.frame,parameter_ticks:planned.parameters,generator_ticks:planned.generator,raw_clip:item.raw,raw_source,source_size,appearance});Ok(())
  };
  for layer in &layers{match layer{VideoLayer::Clip{clip,..}=>add(clip)?,VideoLayer::Transition{from,to,..}=>for clip in [from,to].into_iter().flatten(){add(clip)?}}}
  Ok(Self{sequence:sequence.raw,at,layers,clips})
 }
 /// Provider returns a complete canvas after source colour, effects, masks and
 /// Motion, before global Opacity/blend. It receives all raw authoring context;
 /// unsupported content must return an error rather than disappear. Missing
 /// dimensions are retained in the job for provider-specific admission.
 pub fn prepare(&self,mut provider:impl FnMut(&SourceClipJob<'a>)->Result<Image>,effect:impl FnMut(&Value)->Result<EffectInstance>)->Result<Vec<PreparedLayer>>{
  filmcraft_video_compositor::prepare(&self.layers,|planned|{
   let job=self.clips.get(&planned.clip).context("missing video source job")?;
   let image=provider(job)?;let appearance=job.appearance.as_ref();
   Ok(PreparedClip{image,opacity:appearance.map_or(1.,|a|a.opacity),blend:Blend::from_index(appearance.map_or(0,|a|a.blend))})
  },effect)
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn source_jobs_preserve_independent_clocks_and_complete_effects(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-appearance.json")).unwrap();let mut compared=0;
  for case in fixture["cases"].as_array().unwrap(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let jobs=SourceFrameJobs::build(&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap()).unwrap();
   if let Some(job)=jobs.clips.get(&case["clip"].as_u64().unwrap()){
    compared+=1;assert_eq!(serde_json::json!(job.parameter_ticks),case["parameters"]);let appearance=job.appearance.as_ref().unwrap();assert_eq!(serde_json::json!(appearance.matrix.map(f64::to_bits)),case["matrixBits"]);assert_eq!(serde_json::json!(appearance.opacity.to_bits()),case["opacityBits"]);assert_eq!(serde_json::json!(&appearance.effects),case["effects"]);assert_eq!(&job.raw_clip["effects"],&case["effects"]);assert_eq!(job.source_size,Some((320,200)));assert!(job.raw_source.get("kind").is_some());
    let prepared=jobs.prepare(|j|{assert_eq!(j.raw_clip["id"].as_u64(),Some(j.clip));Ok(Image::new(1,1))},|raw|serde_json::from_value(raw.clone()).map_err(Into::into)).unwrap();assert_eq!(prepared.len(),jobs.layers.len());
   }
  }assert!(compared>100,"insufficient active-clip clock/appearance coverage: {compared}");
 }
}
