//! Explicit source-clock transition interval; legacy adjacent cuts resolve separately.
use super::caption_time::CaptionDelta;
use super::transition_gap::{TransitionGap,TransitionGapError};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TransitionTimeline{
 pub start_tick:CaptionDelta,
 pub duration_tick:CaptionDelta,
}
impl TransitionTimeline{
 pub fn source_span(self)->Result<(i64,i64),TransitionGapError>{
  self.start_tick.validate_wire().map_err(|_|TransitionGapError::Overflow)?;
  self.duration_tick.validate_wire().map_err(|_|TransitionGapError::Overflow)?;
  let start=i64::try_from(self.start_tick.exact_ticks()).map_err(|_|TransitionGapError::Overflow)?;
  let duration=i64::try_from(self.duration_tick.exact_ticks()).map_err(|_|TransitionGapError::Overflow)?;
  if duration<=0{return Err(TransitionGapError::InvalidDuration)}
  start.checked_add(duration).ok_or(TransitionGapError::Overflow)?;
  Ok((start,duration))
 }
 pub fn gap<T:Clone+Eq>(self,from:Option<T>,to:Option<T>)->Result<TransitionGap<T>,TransitionGapError>{
  let(start,duration)=self.source_span()?;Ok(TransitionGap{start,duration,from,to})
 }
 pub fn from_gap<T>(gap:&TransitionGap<T>)->Self{
  Self{start_tick:CaptionDelta::SourceTicks(gap.start),duration_tick:CaptionDelta::SourceTicks(gap.duration)}
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::domain::composition::ClipTransition;
 #[test]
 fn transition_timeline_wire_preserves_legacy_and_exact_preroll() {
  let legacy=serde_json::json!({"id":"tr","fromClipId":"a","toClipId":"b","durationTicks":250000,"kind":"dissolve"});
  let decoded:ClipTransition=serde_json::from_value(legacy.clone()).unwrap();
  assert_eq!(serde_json::to_value(decoded).unwrap(),legacy);
  let exact=serde_json::json!({"id":"tr","fromClipId":null,"toClipId":"b","timeline":{"startTick":"-1","durationTick":"254016000001"},"kind":"dissolve"});
  let decoded:ClipTransition=serde_json::from_value(exact.clone()).unwrap();
  assert_eq!(decoded.timeline.unwrap().source_span().unwrap(),(-1,254016000001));
  assert_eq!(serde_json::to_value(decoded).unwrap(),exact);
 }
 #[test]
 fn transition_timeline_checks_clock_conversion_and_end_overflow() {
  let span=|start,duration|TransitionTimeline{start_tick:start,duration_tick:duration}.source_span();
  assert_eq!(span(CaptionDelta::Microseconds(-1),CaptionDelta::Microseconds(2)),Ok((-254016,508032)));
  assert_eq!(span(CaptionDelta::SourceTicks(0),CaptionDelta::SourceTicks(0)),Err(TransitionGapError::InvalidDuration));
  assert_eq!(span(CaptionDelta::SourceTicks(i64::MAX),CaptionDelta::SourceTicks(1)),Err(TransitionGapError::Overflow));
  assert_eq!(span(CaptionDelta::Microseconds(i64::MAX),CaptionDelta::SourceTicks(1)),Err(TransitionGapError::Overflow));
 }
 #[test]
 fn transition_timeline_native_edit_replays_original_fixture() {
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-insert.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let id=|value:&serde_json::Value|value.as_u64().map(|n|crate::domain::composition::CompositionClipId::parse(format!("clip-{n}")).unwrap());
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),8);
  for case in cases {
   let at=case["at"].as_i64().unwrap();
   let before=&case["before"];let after=&case["after"];
   let splits:Vec<_>=before["items"].as_array().unwrap().iter().filter(|item|item["start"].as_i64().unwrap()<at&&at<item["start"].as_i64().unwrap()+item["duration"].as_i64().unwrap()).map(|item|{
    let right=after["items"].as_array().unwrap().iter().find(|candidate|candidate["id"].as_u64().is_some_and(|id|id>=2000&&id<100000)).unwrap();
    (id(&item["id"]).unwrap(),id(&right["id"]).unwrap())
   }).collect();
   let actual:Vec<_>=before["transitions"].as_array().unwrap().iter().filter_map(|tr|{
    let value=serde_json::json!({"id":"tr","fromClipId":id(&tr["from"]),"toClipId":id(&tr["to"]),"kind":"dissolve","timeline":{"startTick":tr["start"].as_i64().unwrap().to_string(),"durationTick":tr["duration"].as_i64().unwrap().to_string()}});
    serde_json::from_value::<ClipTransition>(value).unwrap().insert_explicit_gap(at,case["duration"].as_i64().unwrap(),&splits).unwrap()
   }).collect();
   let expected=after["transitions"].as_array().unwrap();assert_eq!(actual.len(),expected.len(),"{}",case["name"]);
   for (actual,expected) in actual.iter().zip(expected) {
    assert_eq!(actual.timeline.unwrap().source_span().unwrap(),(expected["start"].as_i64().unwrap(),expected["duration"].as_i64().unwrap()),"{}",case["name"]);
    assert_eq!(actual.from_clip_id,id(&expected["from"]));assert_eq!(actual.to_clip_id,id(&expected["to"]));
   }
  }
 }
}
