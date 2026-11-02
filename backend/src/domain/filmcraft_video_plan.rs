//! Source renderer track selection and clocks. A plan does not admit pixels/effects.
use super::{filmcraft_project_file::SourceProjectDocument,filmcraft_project_index::ProjectIndex,filmcraft_sequence_source::{SourceSequence,SourceTrackItem}};
use anyhow::{Context,Result};
use serde::Serialize;
use serde_json::Value;
#[derive(Debug,Serialize)]
pub struct PlannedClip { pub clip:u64,pub item:u64,pub frame:i64,pub parameters:i64,pub generator:i64 }
#[derive(Debug,Serialize)]
#[serde(tag="kind",rename_all="camelCase")]
pub enum VideoLayer {
 Clip {track:u64,clip:PlannedClip},
 Transition {track:u64,id:u64,from:Option<PlannedClip>,to:Option<PlannedClip>,progress:f64,reverse:bool,effect:Value},
}
fn clip(item:&SourceTrackItem<'_>,at:i64)->Result<PlannedClip>{let clocks=item.clocks(at)?;Ok(PlannedClip{clip:item.id,item:item.source_id,frame:clocks.frame,parameters:clocks.parameters,generator:clocks.generator})}
/// Bottom track first. Transitions take precedence over ordinary item lookup and
/// may sample their sides outside the ordinary clip range (transition handles).
pub fn plan(document:&SourceProjectDocument,sequence:u64,at:i64)->Result<Vec<VideoLayer>>{
 let index=ProjectIndex::new(document)?;let sequence=SourceSequence::read(&index,sequence)?;plan_sequence(&sequence,at)
}
pub fn plan_sequence(sequence:&SourceSequence<'_>,at:i64)->Result<Vec<VideoLayer>>{plan_sequence_track(sequence,at,None)}
pub(crate) fn plan_sequence_track(sequence:&SourceSequence<'_>,at:i64,only:Option<usize>)->Result<Vec<VideoLayer>>{
 let mut layers=Vec::new();
 for(index,track)in sequence.video_tracks.iter().enumerate(){
  if only.is_some_and(|selected|selected!=index){continue}
  if only.is_none()&&!track.raw["enabled"].as_bool().context("missing video track enabled state")?{continue}
  let transitions=track.raw["transitions"].as_array().context("missing video transitions")?;
  let mut active=None;
  for transition in transitions {
   let start=transition["start"].as_i64().context("invalid video transition start")?;
   let duration=transition["duration"].as_i64().context("invalid video transition duration")?;
   let end=start.checked_add(duration).context("video transition end overflow")?;
   if start<=at&&at<end{active=Some((transition,start,duration));break}
  }
  if let Some((transition,start,duration))=active {
   let side=|key:&str|->Result<Option<PlannedClip>>{
    let id=match &transition[key]{Value::Null=>return Ok(None),value=>value.as_u64().context("invalid transition side ID")?};
    track.items.iter().find(|item|item.id==id&&item.enabled).map(|item|clip(item,at)).transpose()
   };
   let delta=at.checked_sub(start).context("transition progress overflow")?;
   layers.push(VideoLayer::Transition{track:track.id,id:transition["id"].as_u64().context("invalid video transition ID")?,from:side("from")?,to:side("to")?,progress:(delta as f64/duration as f64).clamp(0.,1.),reverse:transition.get("reverse").map(|v|v.as_bool().context("invalid transition reverse")).transpose()?.unwrap_or(false),effect:transition.get("effect").context("missing transition effect")?.clone()});
   continue
  }
  if let Some(item)=track.item_at(at).filter(|item|item.enabled){layers.push(VideoLayer::Clip{track:track.id,clip:clip(item,at)?})}
 }
 Ok(layers)
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn original_track_transition_and_source_clocks_match(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-plan.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),512);
  for case in cases {let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let actual=plan(&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap()).unwrap();assert_eq!(serde_json::to_value(actual).unwrap(),case["layers"],"mode {} at {}",case["mode"],case["at"]);}
 }
}
