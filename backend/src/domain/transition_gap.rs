//! Original Filmcraft transition interval editing, independent of render layout.
//! Units are the signed original sequence clock (254016000000 ticks/second).
//! Source: edit/lib.rs split_track_at and shift_track_from, Filmcraft MIT.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct TransitionGap<T>{pub start:i64,pub duration:i64,pub from:Option<T>,pub to:Option<T>}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum TransitionGapError{InvalidDuration,Overflow}
impl<T:Clone+Eq> TransitionGap<T>{
 /// Open a gap after splitting media. `splits` maps old clip IDs to right pieces.
 /// None removes a single-sided fade whose endpoint was split inside its window.
 pub fn insert(&self,at:i64,duration:i64,splits:&[(T,T)])->Result<Option<Self>,TransitionGapError>{
  if self.duration<=0||duration<=0{return Err(TransitionGapError::InvalidDuration)}
  let end=self.start.checked_add(self.duration).ok_or(TransitionGapError::Overflow)?;
  let endpoint_split=splits.iter().any(|(old,_)|self.from.as_ref()==Some(old)||self.to.as_ref()==Some(old));
  if self.start<at&&at<end&&endpoint_split&&!(self.from.is_some()&&self.to.is_some()){return Ok(None)}
  let mut next=self.clone();
  if self.start>=at{
   if let Some((_,right))=splits.iter().find(|(old,_)|self.from.as_ref()==Some(old)){next.from=Some(right.clone())}
  }
  if self.start>=at||(end>at&&self.from.is_none()){
   next.start=self.start.checked_add(duration).ok_or(TransitionGapError::Overflow)?;
   next.start.checked_add(next.duration).ok_or(TransitionGapError::Overflow)?;
  }
  Ok(Some(next))
 }
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn original_global_insert_transition_fixture(){
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-insert.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),8);
  for case in cases{
   let at=case["at"].as_i64().unwrap();let duration=case["duration"].as_i64().unwrap();
   let before=&case["before"];let after=&case["after"];
   let splits:Vec<(u64,u64)>=before["items"].as_array().unwrap().iter().filter(|item|item["start"].as_i64().unwrap()<at&&at<item["start"].as_i64().unwrap()+item["duration"].as_i64().unwrap()).map(|item|{
    let right=after["items"].as_array().unwrap().iter().find(|candidate|candidate["id"].as_u64().unwrap()>=2000&&candidate["id"].as_u64().unwrap()<100000).unwrap();
    (item["id"].as_u64().unwrap(),right["id"].as_u64().unwrap())
   }).collect();
   let actual:Vec<_>=before["transitions"].as_array().unwrap().iter().filter_map(|tr|TransitionGap{start:tr["start"].as_i64().unwrap(),duration:tr["duration"].as_i64().unwrap(),from:tr["from"].as_u64(),to:tr["to"].as_u64()}.insert(at,duration,&splits).unwrap()).collect();
   let expected:Vec<_>=after["transitions"].as_array().unwrap().iter().map(|tr|TransitionGap{start:tr["start"].as_i64().unwrap(),duration:tr["duration"].as_i64().unwrap(),from:tr["from"].as_u64(),to:tr["to"].as_u64()}).collect();
   assert_eq!(actual,expected,"{}",case["name"]);
  }
 }
 #[test]fn exact_signed_boundaries_and_refusals(){
  let tr=TransitionGap{start:-1,duration:2,from:None,to:Some(1)};
  assert_eq!(tr.insert(0,1,&[]).unwrap().unwrap().start,0);
  assert_eq!(tr.insert(1,1,&[(1,2)]).unwrap(),Some(tr.clone()));
  assert_eq!(tr.insert(0,1,&[(1,2)]).unwrap(),None);
  assert_eq!(tr.insert(0,0,&[]),Err(TransitionGapError::InvalidDuration));
  let overflow=TransitionGap{start:i64::MAX-1,duration:1,from:Some(1),to:Some(2)};
  assert_eq!(overflow.insert(0,1,&[]),Err(TransitionGapError::Overflow));
  let invalid=TransitionGap::<u64>{start:i64::MAX,duration:1,from:None,to:None};
  assert_eq!(invalid.insert(0,1,&[]),Err(TransitionGapError::Overflow));
 }
}
