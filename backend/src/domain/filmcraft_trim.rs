//! Lossless single-clip regular trim using Filmcraft source ticks and edit-context handles.
use super::filmcraft_project_file::SourceProjectDocument;
use anyhow::{ensure,Context,Result};
use serde_json::Value;
const MAX:i64=i64::MAX/4;
const MIN:i64=i64::MIN/4;
#[derive(Clone,Copy)]
pub enum Edge { In, Out }
/// Original trim-monitor edit time and adjacent clip sides. A gap keeps one side empty.
/// Missing clips preserve the requested side identity, as original `sides` does.
pub fn edit_point_context(document:&SourceProjectDocument,sequence:u64,clip:u64,edge:Edge)->Result<Value>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing trim-monitor sequence")?;
 let found=["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().into_iter().flatten().find_map(|track|track["items"].as_array().into_iter().flatten().find(|item|item["id"].as_u64()==Some(clip)).map(|item|(track,item))));
 let mut time=None;let mut pair=None;
 if let Some((track,item))=found{
  let start=integer(item,"start")?;let end=narrow(start as i128+integer(item,"duration")? as i128)?;
  time=Some(match edge{Edge::In=>start,Edge::Out=>end});
  for other in track["items"].as_array().context("invalid trim-monitor track items")?{
   let adjacent=match edge{Edge::Out=>integer(other,"start")?==end,Edge::In=>narrow(integer(other,"start")? as i128+integer(other,"duration")? as i128)?==start};
   if adjacent{let id=other["id"].as_u64().context("invalid adjacent clip ID")?;pair=Some(match edge{Edge::Out=>(clip,id),Edge::In=>(id,clip)});break;}
  }
 }
 let (outgoing,incoming)=match pair{Some((left,right))=>(Some(left),Some(right)),None=>match edge{Edge::Out=>(Some(clip),None),Edge::In=>(None,Some(clip))}};
 Ok(serde_json::json!({"editTime":time,"outgoing":outgoing,"incoming":incoming,"rollPair":pair}))
}
/// Metadata and exact source clock of the frame displayed on one trim-monitor side.
pub fn trim_monitor_side(document:&SourceProjectDocument,sequence:u64,clip:u64,outgoing:bool)->Result<Option<Value>>{
 use super::{filmcraft_project_index::ProjectIndex,filmcraft_sequence_source::SourceTrackItem};
 let index=ProjectIndex::new(document)?;let seq=index.item(sequence).and_then(|i|i.pointer("/kind/Sequence")).context("missing trim-monitor sequence")?;
 let normalized=|v:&Value|->Result<(i128,i128)>{let n=v["num"].as_i64().context("invalid monitor rate numerator")?;let d=v["den"].as_i64().context("invalid monitor rate denominator")?;Ok(if n>0&&d>0{(n as i128,d as i128)}else{(24000,1001)})};
 let rate=&seq["settings"]["frame_rate"];let (num,den)=normalized(rate)?;let fd=narrow(254016000000i128*den/num)?;
 for (key,video) in [("video_tracks",true),("audio_tracks",false)]{
  for (track_index,track) in seq[key].as_array().context("invalid monitor tracks")?.iter().enumerate(){
   let Some(raw)=track["items"].as_array().context("invalid monitor items")?.iter().find(|i|i["id"].as_u64()==Some(clip))else{continue};
   let item=SourceTrackItem::read(raw)?;let end=item.end().context("monitor end overflow")?;
   ensure!(item.timing.duration>0,"empty trim-monitor clip");
   let time=if outgoing{end.checked_sub(fd).context("monitor display clock overflow")?}else{item.timing.timeline_start};
   let media_time=if let Some(held)=item.timing.frame_hold{held}else{item.clocks(time.clamp(item.timing.timeline_start,end-1))?.frame};
   let mut moving=item.timing;moving.reverse=false;moving.frame_hold=None;moving.hold_filters=false;
   let source_out=moving.sample_source_item(end).map_err(anyhow::Error::msg)?.frame;
   let default=serde_json::json!({"num":24000,"den":1001});
   let item_rate=match index.item(item.source_id){None=>rate,Some(source)=>{let kind=&source["kind"];if let Some(media)=kind.get("Media"){media.pointer("/interpret/frame_rate").filter(|v|!v.is_null()).or_else(||media.pointer("/info/video/frame_rate")).unwrap_or(&default)}else if let Some(seq)=kind.get("Sequence"){&seq["settings"]["frame_rate"]}else if let Some(layer)=kind.get("AdjustmentLayer").or_else(||kind.get("Graphic")){&layer["rate"]}else{&default}}};
   let (n,d)=normalized(item_rate)?;let frame=narrow((media_time as i128*n).div_euclid(254016000000i128*d))?;
   return Ok(Some(serde_json::json!({"clip":clip,"item":item.source_id,"name":raw["name"],"track":format!("{}{index}",if video{"V"}else{"A"},index=track_index+1),"video":video,"time":time,"mediaTime":media_time,"mediaFrame":frame,"sourceIn":item.timing.source_in,"sourceOut":source_out})));
  }
 }
 Ok(None)
}
/// Original nearest edge per targeted track; an empty target list means all media tracks.
/// Equal distances retain the first item/edge in source order, including locked tracks.
pub fn nearest_edit_points(document:&SourceProjectDocument,sequence:u64,targeted:&[u64],time:i64,which:&str)->Result<Value>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing nearest-edit sequence")?;
 let (kind,want_out)=match which{"trimIn"=>("trim",Some(false)),"trimOut"=>("trim",Some(true)),"rippleIn"=>("ripple",Some(false)),"rippleOut"=>("ripple",Some(true)),_=>("roll",None)};
 let mut points:Vec<Value>=vec![];let mut pairs=vec![];
 for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array().context("invalid nearest-edit tracks")?{
  let track_id=track["id"].as_u64().context("invalid nearest-edit track ID")?;if !targeted.is_empty()&&!targeted.contains(&track_id){continue;}
  let mut best:Option<(i128,Value,Value)>=None;
  for item in track["items"].as_array().context("invalid nearest-edit items")?{let clip=item["id"].as_u64().context("invalid nearest-edit clip ID")?;
   for out in [false,true]{if want_out.is_some_and(|want|want!=out){continue;}
    let start=integer(item,"start")?;let tick=if out{narrow(start as i128+integer(item,"duration")? as i128)?}else{start};
    let context=if kind=="roll"{edit_point_context(document,sequence,clip,if out{Edge::Out}else{Edge::In})?}else{Value::Null};
    let pair=context["rollPair"].clone();if kind=="roll"&&pair.is_null(){continue;}
    let distance=(tick as i128-time as i128).abs();if best.as_ref().is_none_or(|(previous,_,_)|distance<*previous){best=Some((distance,serde_json::json!({"clip":clip,"out":out,"kind":kind}),pair));}
   }
  }
  if let Some((_,point,pair))=best{if kind=="roll"&&pairs.contains(&pair){continue;}pairs.push(pair);points.push(point);}
 }}
 Ok(serde_json::json!({"editPoints":points}))
}
#[derive(Clone,Copy)]
pub enum TrimKind { Trim, Ripple, Roll }
#[derive(Clone,Copy)]
pub struct TrimEditPoint { pub clip:u64, pub edge:Edge, pub kind:TrimKind }
/// Original trim_all_raw: ordered point edits; absent roll pairs are skipped.
/// A later refusal preserves earlier completed commands, matching engine behavior.
pub fn trim_edit_points(document:&mut SourceProjectDocument,sequence:u64,points:&[TrimEditPoint],delta:i64,linked:bool)->Result<Vec<i64>>{
 ensure!(!points.is_empty(),"no edit points selected");let mut applied=vec![];
 for point in points{
  let movement=match point.kind{
   TrimKind::Roll=>{let context=edit_point_context(document,sequence,point.clip,point.edge)?;let Some(pair)=context["rollPair"].as_array()else{continue};roll(document,sequence,pair[0].as_u64().unwrap(),pair[1].as_u64().unwrap(),delta)?},
   TrimKind::Ripple=>ripple_trim_linked(document,sequence,point.clip,point.edge,delta,linked)?,
   TrimKind::Trim=>trim_linked(document,sequence,point.clip,point.edge,delta,linked)?,
  };applied.push(movement);
 }
 Ok(applied)
}
/// Original trim.extendToPlayhead derives one shared movement from the primary point.
/// Later points retain their spacing; command refusals retain earlier completed edits.
pub fn extend_edit_points_to_playhead(document:&mut SourceProjectDocument,sequence:u64,points:&[TrimEditPoint],playhead:i64,linked:bool)->Result<Vec<i64>>{
 let primary=points.first().context("no edit points selected")?;
 let context=edit_point_context(document,sequence,primary.clip,primary.edge)?;
 let first=context["editTime"].as_i64().context("no edit points selected")?;
 let delta=narrow(playhead as i128-first as i128)?;
 trim_edit_points(document,sequence,points,delta,linked)
}
#[derive(Clone,Copy,Default,Debug,PartialEq,Eq)]
pub struct TrimShift { pub outgoing:i64, pub incoming:i64 }
/// Original add_shift uses the primary point kind/side and first applied movement.
/// Call only after a successful batch; original late command errors leave counters unchanged.
pub fn trim_shift_after(mut shift:TrimShift,primary:Option<TrimEditPoint>,applied:&[i64])->Result<TrimShift>{
 let Some(point)=primary else{return Ok(shift)};let movement=applied.first().copied().unwrap_or(0);
 if matches!(point.kind,TrimKind::Roll)||matches!(point.edge,Edge::Out){shift.outgoing=shift.outgoing.checked_add(movement).context("outgoing trim shift overflow")?;}
 if matches!(point.kind,TrimKind::Roll)||matches!(point.edge,Edge::In){shift.incoming=shift.incoming.checked_add(movement).context("incoming trim shift overflow")?;}
 Ok(shift)
}
pub struct TrimContext { pub media_start:i64, pub media_end:Option<i64>, pub min_duration:i64 }
/// Original engine EditCtx media limits, including seventeen-node subclip duration ancestry.
pub fn project_context(document:&SourceProjectDocument,sequence:u64,item:u64)->Result<TrimContext>{
 use super::filmcraft_project_index::ProjectIndex;
 let index=ProjectIndex::new(document)?;
 let rate=index.item(sequence).and_then(|v|v.pointer("/kind/Sequence/settings/frame_rate")).context("missing trim frame rate")?;
 let (mut num,mut den)=(rate["num"].as_i64().context("invalid frame rate numerator")?,rate["den"].as_i64().context("invalid frame rate denominator")?);
 if num<=0 || den<=0 {num=24000;den=1001;}
 let min_duration=narrow(254_016_000_000i128*den as i128/num as i128)?;
 let media_start=index.item(item).and_then(|v|v.pointer("/kind/Subclip")).filter(|v|v["restrict_trims"].as_bool()==Some(true))
     .map(|v|integer(&v["range"],"start")).transpose()?.unwrap_or(0);
 let mut current=item;let mut media_end=None;
 for _ in 0..=16 {
  let Some(kind)=index.item(current).and_then(|v|v.get("kind"))else{break};
  if let Some(media)=kind.get("Media"){
   if !matches!(media.pointer("/info/kind").and_then(Value::as_str),Some("Still"|"Synthetic")){media_end=Some(integer(&media["info"],"duration")?);}
   break;
  }
  if let Some(subclip)=kind.get("Subclip"){
   if subclip["restrict_trims"].as_bool()==Some(true){media_end=Some(narrow(integer(&subclip["range"],"start")? as i128+integer(&subclip["range"],"duration")? as i128)?);break;}
   current=subclip["parent"].as_u64().context("invalid subclip parent")?;continue;
  }
  if let Some(seq)=kind.get("Sequence"){
   let mut ends:[Option<i64>;2]=[None,None];
   for key in ["video_tracks","audio_tracks","caption_tracks"]{
    let group=usize::from(key=="caption_tracks");
    for track in seq[key].as_array().into_iter().flatten(){let mut end=None;
     for clip in track[if key=="caption_tracks" {"captions"}else{"items"}].as_array().into_iter().flatten(){let value=narrow(integer(clip,"start")? as i128+integer(clip,"duration")? as i128)?;end=Some(end.map_or(value,|previous:i64|previous.max(value)));}
     let value=end.unwrap_or(0);ends[group]=Some(ends[group].map_or(value,|previous|previous.max(value)));
    }
   }
   media_end=Some(ends[0].unwrap_or(0).max(ends[1].unwrap_or(0)));
  }
  break;
 }
 Ok(TrimContext{media_start,media_end,min_duration})
}
fn integer(v:&Value,k:&str)->Result<i64>{v[k].as_i64().with_context(||format!("missing clip {k}"))}
fn narrow(n:i128)->Result<i64>{i64::try_from(n).context("trim clock overflow")}
struct TrimPlan { key:&'static str,ti:usize,ci:usize,start:i64,duration:i64,source:i64,speed:f64,reverse:bool,hold:bool,d:i64 }
fn plan_trim(document:&SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64,context:&TrimContext,ripple:bool)->Result<TrimPlan>{
 ensure!(context.min_duration>0,"invalid trim minimum duration");
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let seq=document.envelope.pointer(&path).context("missing trim sequence")?;
 let mut found=None;
 for key in ["video_tracks","audio_tracks"] {
  for (ti,track) in seq[key].as_array().into_iter().flatten().enumerate(){
   if let Some(ci)=track["items"].as_array().and_then(|items|items.iter().position(|v|v["id"].as_u64()==Some(clip))){found=Some((key,ti,ci));break;}
  }
  if found.is_some(){break;}
 }
 let (key,ti,ci)=found.context("missing trim clip")?;
 let items=seq[key][ti]["items"].as_array().context("invalid trim track")?;
 let item=&items[ci];let start=integer(item,"start")?;let duration=integer(item,"duration")?;let source=integer(item,"source_in")?;
 let speed=item["speed"].as_f64().unwrap_or(1.).abs();ensure!(speed.is_finite(),"invalid trim speed");
 let reverse=item["reverse"].as_bool().unwrap_or(false);let hold=item.get("frame_hold").is_some_and(|v|!v.is_null());
 let previous=if ci>0 {narrow(integer(&items[ci-1],"start")? as i128+integer(&items[ci-1],"duration")? as i128)?}else{0};
 let next=items.get(ci+1).map(|v|integer(v,"start")).transpose()?.unwrap_or(MAX);
 let effective=speed.max(1e-9);let consumed=(duration as f64*speed).round() as i64;
 let d=match edge {
  Edge::In=>{
   let head=if hold {source}else{narrow((source as i128-context.media_start as i128).max(0))?};
   let ext=(head as f64/effective).floor() as i64;
   let lo=if hold {MIN}else{if ripple {narrow(-(ext as i128))?}else{narrow((previous as i128-start as i128).max(-(ext as i128)))?}};
   let hi=narrow(duration as i128-context.min_duration as i128)?;
   ensure!(lo<=hi,"invalid trim bounds");delta.clamp(lo,hi)
  }
  Edge::Out=>{
   let lo=narrow(context.min_duration as i128-duration as i128)?;
   let mut hi=if ripple {MAX}else{narrow(next as i128-start as i128-duration as i128)?};
   if !hold {if let Some(end)=context.media_end {hi=hi.min(((end as i128-source as i128-consumed as i128) as f64/effective).floor() as i64);}}
   delta.clamp(lo,hi.max(lo))
  }
 };
 Ok(TrimPlan{key,ti,ci,start,duration,source,speed,reverse,hold,d})
}
/// Original clamping without applying structural validation or mutation.
pub fn clamp_regular_trim(document:&SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64,context:&TrimContext)->Result<i64>{
 Ok(plan_trim(document,sequence,clip,edge,delta,context,false)?.d)
}
/// Atomic regular trim. Linked-selection and ripple orchestration are separate edit operations.
pub fn regular_trim(document:&mut SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64,context:&TrimContext)->Result<i64>{
 let TrimPlan{key,ti,ci,start,duration,source,speed,reverse,hold,d}=plan_trim(document,sequence,clip,edge,delta,context,false)?;
 let path=format!("/project/items/{sequence}/kind/Sequence");
 if d==0{return Ok(0);}
 let mut work=document.clone();let item=&mut work.envelope.pointer_mut(&path).unwrap()[key][ti]["items"][ci];
 let offset=(d as f64*speed).round() as i64;
 match edge {
  Edge::In=>{if !reverse && !hold {item["source_in"]=narrow(source as i128+offset as i128)?.into();}item["start"]=narrow(start as i128+d as i128)?.into();item["duration"]=narrow(duration as i128-d as i128)?.into();}
  Edge::Out=>{if reverse {item["source_in"]=narrow(source as i128-offset as i128)?.into();}item["duration"]=narrow(duration as i128+d as i128)?.into();}
 }
 let seq=work.envelope.pointer(&path).unwrap();
 validate_trim_tracks(seq)?;
 *document=work;Ok(d)
}
fn validate_trim_tracks(seq:&Value)->Result<()> {
 for key in ["video_tracks","audio_tracks","caption_tracks"] {
  for track in seq[key].as_array().into_iter().flatten(){let mut end=None;
   for item in track[if key=="caption_tracks" {"captions"} else {"items"}].as_array().into_iter().flatten(){let start=integer(item,"start")?;let duration=integer(item,"duration")?;let next=narrow(start as i128+duration as i128)?;
    ensure!((MIN..=MAX).contains(&start) && duration>0 && duration<=MAX && (MIN..=MAX).contains(&next),"trim outside supported clock bounds");
    ensure!(end.is_none_or(|end|end<=start),"trim overlaps neighbouring clip");end=Some(next);
   }
  }
 }
 Ok(())
}
/// Regular engine trim with optional linked selection and one atomic document update.
pub fn trim_linked(document:&mut SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64,linked:bool)->Result<i64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let seq=document.envelope.pointer(&path).context("missing trim sequence")?;
 let all:Vec<&Value>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).flat_map(|t|t["items"].as_array().into_iter().flatten()).collect();
 let target=all.iter().find(|v|v["id"].as_u64()==Some(clip)).context("missing trim clip")?;
 let link=target["link"].as_u64().filter(|_|linked);
 let mut clips=vec![clip];
 if let Some(link)=link {for item in &all {if item["link"].as_u64()==Some(link){let id=item["id"].as_u64().context("invalid linked clip ID")?;if !clips.contains(&id){clips.push(id);}}}}
 let contexts:Vec<_>=clips.iter().map(|id|{
  let item=all.iter().find(|v|v["id"].as_u64()==Some(*id)).context("missing linked clip")?;
  project_context(document,sequence,item["item"].as_u64().context("invalid linked media ID")?)
 }).collect::<Result<_>>()?;
 let mut d=delta;
 for (id,context) in clips.iter().zip(&contexts){let value=clamp_regular_trim(document,sequence,*id,edge,d,context)?;if value.unsigned_abs()<d.unsigned_abs(){d=value;}}
 let mut work=document.clone();
 for (id,context) in clips.iter().zip(&contexts){regular_trim(&mut work,sequence,*id,edge,d,context)?;}
 *document=work;Ok(d)
}
/// Close the gap containing time, preserving original locked/sync-lock policy.
pub fn close_gap(document:&mut SourceProjectDocument,sequence:u64,track_id:u64,time:i64)->Result<()>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing gap sequence")?;
 let track=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).find(|track|track["id"].as_u64()==Some(track_id)).context("missing gap track")?;
 let ranges=track["items"].as_array().into_iter().flatten().map(item_range).collect::<Result<Vec<_>>>()?;
 let index=ranges.partition_point(|r|r.0<=time);ensure!(index==0 || time>=ranges[index-1].1,"time is inside a clip");
 let start=ranges.iter().filter(|r|r.1<=time).map(|r|r.1).max().unwrap_or(0);
 let end=ranges.iter().filter(|r|r.0>time).map(|r|r.0).min().context("no following clip")?;
 let shift=narrow(start as i128-end as i128)?;
 let mut work=document.clone();let seq=work.envelope.pointer_mut(&path).unwrap();
 for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().into_iter().flatten(){
  if track["locked"].as_bool()==Some(true){continue;}
  if track["id"].as_u64()!=Some(track_id){
   if track["sync_lock"].as_bool()!=Some(true){continue;}
   for item in track["items"].as_array().into_iter().flatten(){let range=item_range(item)?;ensure!(!(range.0<end && range.1>start),"gap sync-lock conflict");}
  }
  shift_source_track(track,end,shift)?;
 }}
 *document=work;Ok(())
}
fn shift_source_track(track:&mut Value,at:i64,delta:i64)->Result<()>{
 let items=track["items"].as_array_mut().context("invalid shifted track")?;
 for item in items.iter_mut(){let time=integer(item,"start")?;if time>=at{item["start"]=narrow(time as i128+delta as i128)?.into();}}
 items.sort_by_key(|i|i["start"].as_i64().unwrap_or(0));
 if let Some(transitions)=track["transitions"].as_array_mut(){for transition in transitions.iter_mut(){let time=integer(transition,"start")?;let end=narrow(time as i128+integer(transition,"duration")? as i128)?;
  if time>=at || end>at && transition["from"].is_null(){transition["start"]=narrow(time as i128+delta as i128)?.into();}
 }transitions.sort_by_key(|t|t["start"].as_i64().unwrap_or(0));}
 Ok(())
}
pub struct DeleteResult { pub removed:usize,pub closed_spans:Vec<(i64,i64)> }
fn item_range(item:&Value)->Result<(i64,i64)>{let start=integer(item,"start")?;Ok((start,narrow(start as i128+integer(item,"duration")? as i128)?))}
fn merge_ranges(mut ranges:Vec<(i64,i64)>)->Vec<(i64,i64)>{
 ranges.sort_by_key(|r|r.0);let mut merged:Vec<(i64,i64)>=vec![];
 for range in ranges {if let Some(last)=merged.last_mut(){if range.0<=last.1{last.1=last.1.max(range.1);continue;}}merged.push(range);}merged
}
/// Delete selected clips, optionally closing only time free across affected tracks.
pub fn delete_selection(document:&mut SourceProjectDocument,sequence:u64,ids:&[u64],ripple:bool)->Result<DeleteResult>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing delete sequence")?;
 let tracks:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).collect();
 let mut affected=vec![];let mut spans=vec![];
 if ripple {
  for id in ids {let track=tracks.iter().find(|t|t["items"].as_array().into_iter().flatten().any(|i|i["id"].as_u64()==Some(*id))).context("missing delete clip")?;affected.push(track["id"].as_u64().context("invalid track ID")?);}
  let mut deleted=vec![];let mut kept=vec![];
  for track in &tracks {
   if track["locked"].as_bool()==Some(true) || !track["items"].as_array().into_iter().flatten().any(|i|i["id"].as_u64().is_some_and(|id|ids.contains(&id))){continue;}
   for item in track["items"].as_array().into_iter().flatten(){let range=item_range(item)?;if item["id"].as_u64().is_some_and(|id|ids.contains(&id)){deleted.push(range)}else{kept.push(range)}}
  }
  let kept=merge_ranges(kept);
  for range in merge_ranges(deleted){let mut at=range.0;
   for k in kept.iter().filter(|k|k.0<range.1 && k.1>range.0){if k.0>at{spans.push((at,k.0));}at=at.max(k.1);}
   if at<range.1{spans.push((at,range.1));}
  }
  ensure!(!spans.is_empty() || ids.is_empty(),"no unlocked gap can close");
 }
 let mut work=document.clone();let seq=work.envelope.pointer_mut(&path).unwrap();let mut removed=0;
 for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().into_iter().flatten(){
  if track["locked"].as_bool()==Some(true){continue;}
  let items=track["items"].as_array_mut().context("invalid delete track items")?;let before=items.len();items.retain(|i|!i["id"].as_u64().is_some_and(|id|ids.contains(&id)));removed+=before-items.len();
  let remaining:Vec<_>=items.iter().filter_map(|i|i["id"].as_u64()).collect();
  if let Some(transitions)=track["transitions"].as_array_mut(){transitions.retain(|t|{
   let from=t["from"].as_u64();let to=t["to"].as_u64();from.is_none_or(|id|remaining.contains(&id)) && to.is_none_or(|id|remaining.contains(&id)) && (from.is_some() || to.is_some())
  });}
 }}
 for &(start,end) in spans.iter().rev(){let shift=narrow(start as i128-end as i128)?;
  for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().into_iter().flatten(){
   if track["locked"].as_bool()==Some(true){continue;}
   let member=track["id"].as_u64().is_some_and(|id|affected.contains(&id));if !member && track["sync_lock"].as_bool()!=Some(true){continue;}
   let items=track["items"].as_array_mut().context("invalid ripple delete items")?;
   if !member {for item in items.iter(){let range=item_range(item)?;ensure!(!(range.0<end && range.1>start),"ripple delete sync-lock conflict");}}
   for item in items.iter_mut(){let time=integer(item,"start")?;if time>=end{item["start"]=narrow(time as i128+shift as i128)?.into();}}
   items.sort_by_key(|i|i["start"].as_i64().unwrap_or(0));
   if let Some(transitions)=track["transitions"].as_array_mut(){for transition in transitions.iter_mut(){let time=integer(transition,"start")?;let finish=narrow(time as i128+integer(transition,"duration")? as i128)?;
    if time>=end || finish>end && transition["from"].is_null(){transition["start"]=narrow(time as i128+shift as i128)?.into();}
   }transitions.sort_by_key(|t|t["start"].as_i64().unwrap_or(0));}
  }}
 }
 let closed_spans=spans.into_iter().map(|(start,end)|Ok((start,narrow(end as i128-start as i128)?))).collect::<Result<_>>()?;
 *document=work;Ok(DeleteResult{removed,closed_spans})
}

pub(super) fn clip_location(seq:&Value,id:u64)->Result<(&'static str,usize,usize)>{
 ["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().and_then(|tracks|tracks.iter().enumerate().find_map(|(ti,t)|t["items"].as_array().and_then(|items|items.iter().position(|i|i["id"].as_u64()==Some(id))).map(|ci|(key,ti,ci))))).context("missing edit clip")
}
pub(super) fn follow_transition_cuts(seq:&Value,after:&mut Value)->Result<()>{
 for key in ["video_tracks","audio_tracks"]{for track in after[key].as_array_mut().into_iter().flatten(){
  let Some(old)=seq[key].as_array().and_then(|tracks|tracks.iter().find(|t|t["id"]==track["id"]))else{continue};let current=track.clone();
  for transition in track["transitions"].as_array_mut().into_iter().flatten(){if let(Some(a),Some(b))=(transition_anchor(old,transition)?,transition_anchor(&current,transition)?){transition["start"]=narrow(integer(transition,"start")? as i128+b as i128-a as i128)?.into();}}
 }}
 Ok(())
}
/// Roll the shared cut; original source handles and transition anchors are preserved.
pub fn roll(document:&mut SourceProjectDocument,sequence:u64,left:u64,right:u64,delta:i64)->Result<i64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing roll sequence")?;
 let (lk,lt,li)=clip_location(seq,left)?;let (rk,rt,ri)=clip_location(seq,right)?;
 let l=&seq[lk][lt]["items"][li];let r=&seq[rk][rt]["items"][ri];
 let (ls,ld,rs,rd)=(integer(l,"start")?,integer(l,"duration")?,integer(r,"start")?,integer(r,"duration")?);
 ensure!(narrow(ls as i128+ld as i128)?==rs,"items are not adjacent");
 let lc=project_context(document,sequence,l["item"].as_u64().context("invalid roll media")?)?;let rc=project_context(document,sequence,r["item"].as_u64().context("invalid roll media")?)?;
 let left_speed=l["speed"].as_f64().unwrap_or(1.).abs();let right_speed=r["speed"].as_f64().unwrap_or(1.).abs();ensure!(left_speed.is_finite() && right_speed.is_finite(),"invalid roll speed");
 let mut d=delta.max(narrow(lc.min_duration as i128-ld as i128)?).min(narrow(rd as i128-lc.min_duration as i128)?);
 if l.get("frame_hold").is_none_or(Value::is_null){if let Some(end)=lc.media_end{
  let consumed=(ld as f64*left_speed).round() as i64;let remain=end as i128-integer(l,"source_in")? as i128-consumed as i128;
  d=d.min((remain as f64/left_speed).floor() as i64);
 }}
 let source=integer(r,"source_in")?;
 let head=if r.get("frame_hold").is_some_and(|v|!v.is_null()){source}else{narrow((source as i128-rc.media_start as i128).max(0))?};
 d=d.max(narrow(-((head as f64/right_speed).floor() as i64 as i128))?);
 if d==0{return Ok(0);}
 let mut work=document.clone();let after=work.envelope.pointer_mut(&path).unwrap();
 after[lk][lt]["items"][li]["duration"]=narrow(ld as i128+d as i128)?.into();
 let r=&mut after[rk][rt]["items"][ri];r["start"]=narrow(rs as i128+d as i128)?.into();r["duration"]=narrow(rd as i128-d as i128)?.into();r["source_in"]=narrow(source as i128+(d as f64*right_speed).round() as i64 as i128)?.into();
 follow_transition_cuts(seq,after)?;
 *document=work;Ok(d)
}
/// Slide a clip, trimming abutting neighbors and carrying transition anchors.
pub fn slide(document:&mut SourceProjectDocument,sequence:u64,clip:u64,delta:i64)->Result<i64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing slide sequence")?;
 let (key,ti,ci)=clip_location(seq,clip)?;let items=seq[key][ti]["items"].as_array().context("invalid slide track")?;let me=&items[ci];
 let start=integer(me,"start")?;let end=narrow(start as i128+integer(me,"duration")? as i128)?;
 let minimum=project_context(document,sequence,me["item"].as_u64().context("invalid slide media")?)?.min_duration;
 let prev=ci.checked_sub(1).map(|i|&items[i]);let next=items.get(ci+1);
 let prev_end=prev.map(|i|narrow(integer(i,"start")? as i128+integer(i,"duration")? as i128)).transpose()?.unwrap_or(0);
 let next_start=next.map(|i|integer(i,"start")).transpose()?.unwrap_or(MAX);
 let adjacent_prev=prev.filter(|_|prev_end==start);let adjacent_next=next.filter(|_|next_start==end);
 let mut d=delta;
 if let Some(p)=adjacent_prev {
  d=d.max(narrow(minimum as i128-integer(p,"duration")? as i128)?);
  let ctx=project_context(document,sequence,p["item"].as_u64().context("invalid slide neighbor media")?)?;
  let speed=p["speed"].as_f64().unwrap_or(1.).abs();ensure!(speed.is_finite(),"invalid slide neighbor speed");
  if p.get("frame_hold").is_none_or(Value::is_null){if let Some(limit)=ctx.media_end{
   let consumed=(integer(p,"duration")? as f64*speed).round() as i64;
   d=d.min(((limit as i128-integer(p,"source_in")? as i128-consumed as i128) as f64/speed).floor() as i64);
  }}
 }else{d=d.max(narrow(prev_end as i128-start as i128)?);}
 if let Some(n)=adjacent_next {
  d=d.min(narrow(integer(n,"duration")? as i128-minimum as i128)?);
  let ctx=project_context(document,sequence,n["item"].as_u64().context("invalid slide neighbor media")?)?;
  let speed=n["speed"].as_f64().unwrap_or(1.).abs();ensure!(speed.is_finite(),"invalid slide neighbor speed");
  let source=integer(n,"source_in")?;let head=if n.get("frame_hold").is_some_and(|v|!v.is_null()){source}else{narrow((source as i128-ctx.media_start as i128).max(0))?};
  d=d.max(narrow(-((head as f64/speed).floor() as i64 as i128))?);
 }else{d=d.min(narrow(next_start as i128-end as i128)?);}
 if d==0{return Ok(0);}
 let mut work=document.clone();let after=work.envelope.pointer_mut(&path).unwrap();
 if let Some(p)=adjacent_prev{after[key][ti]["items"][ci-1]["duration"]=narrow(integer(p,"duration")? as i128+d as i128)?.into();}
 if let Some(n)=adjacent_next{
  let speed=n["speed"].as_f64().unwrap_or(1.).abs();let item=&mut after[key][ti]["items"][ci+1];
  item["start"]=narrow(integer(n,"start")? as i128+d as i128)?.into();item["duration"]=narrow(integer(n,"duration")? as i128-d as i128)?.into();item["source_in"]=narrow(integer(n,"source_in")? as i128+(d as f64*speed).round() as i64 as i128)?.into();
 }
 after[key][ti]["items"][ci]["start"]=narrow(start as i128+d as i128)?.into();follow_transition_cuts(seq,after)?;
 *document=work;Ok(d)
}
/// Stretch timeline duration while preserving the rounded consumed source span.
pub fn rate_stretch(document:&mut SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64)->Result<f64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let seq=document.envelope.pointer(&path).context("missing stretch sequence")?;
 let (key,ti,ci)=["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().and_then(|tracks|tracks.iter().enumerate().find_map(|(ti,t)|t["items"].as_array().and_then(|items|items.iter().position(|i|i["id"].as_u64()==Some(clip))).map(|ci|(key,ti,ci))))).context("missing stretch clip")?;
 let items=seq[key][ti]["items"].as_array().context("invalid stretch track")?;let item=&items[ci];
 let context=project_context(document,sequence,item["item"].as_u64().context("invalid stretch media ID")?)?;
 let start=integer(item,"start")?;let duration=integer(item,"duration")?;
 let speed=item["speed"].as_f64().unwrap_or(1.);ensure!(speed.is_finite(),"invalid stretch speed");
 let source_len=(duration as f64*speed.abs()).round() as i64;
 let previous=if ci>0{narrow(integer(&items[ci-1],"start")? as i128+integer(&items[ci-1],"duration")? as i128)?}else{0};
 let next=items.get(ci+1).map(|i|integer(i,"start")).transpose()?.unwrap_or(MAX);
 let (lo,hi)=match edge{
  Edge::Out=>(narrow(context.min_duration as i128-duration as i128)?,narrow(next as i128-start as i128-duration as i128)?),
  Edge::In=>(narrow(previous as i128-start as i128)?,narrow(duration as i128-context.min_duration as i128)?),
 };
 ensure!(lo<=hi,"invalid stretch bounds");let d=delta.clamp(lo,hi);
 let new_duration=narrow(duration as i128+match edge{Edge::In=>-(d as i128),Edge::Out=>d as i128})?;
 ensure!(new_duration>0,"invalid stretch duration");let new_speed=source_len as f64/new_duration as f64;
 let new_start=if matches!(edge,Edge::In){narrow(start as i128+d as i128)?}else{start};
 let item=&mut document.envelope.pointer_mut(&path).unwrap()[key][ti]["items"][ci];
 item["start"]=new_start.into();item["duration"]=new_duration.into();item["speed"]=new_speed.into();Ok(new_speed)
}
/// Slip source media without changing timeline placement or source-time keyframes.
pub fn slip(document:&mut SourceProjectDocument,sequence:u64,clip:u64,delta_media:i64)->Result<i64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let seq=document.envelope.pointer(&path).context("missing slip sequence")?;
 let (key,ti,ci)=["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().and_then(|tracks|tracks.iter().enumerate().find_map(|(ti,t)|t["items"].as_array().and_then(|items|items.iter().position(|i|i["id"].as_u64()==Some(clip))).map(|ci|(key,ti,ci))))).context("missing slip clip")?;
 let item=&seq[key][ti]["items"][ci];
 let context=project_context(document,sequence,item["item"].as_u64().context("invalid slip media ID")?)?;
 let source=integer(item,"source_in")?;let duration=integer(item,"duration")?;
 let speed=item["speed"].as_f64().unwrap_or(1.);ensure!(speed.is_finite(),"invalid slip speed");
 let used=(duration as f64*speed.abs()).round() as i64;
 let hold=item.get("frame_hold").is_some_and(|v|!v.is_null());
 let max_in=if hold {MAX}else{context.media_end.map(|end|narrow(end as i128-used as i128)).transpose()?.unwrap_or(MAX)};
 let lo=if hold {0}else{context.media_start};
 let new_in=narrow(source as i128+delta_media as i128)?.clamp(lo,max_in.max(lo));
 let applied=narrow(new_in as i128-source as i128)?;
 document.envelope.pointer_mut(&path).unwrap()[key][ti]["items"][ci]["source_in"]=new_in.into();Ok(applied)
}
fn transition_anchor(track:&Value,transition:&Value)->Result<Option<i64>> {
 let to=transition["to"].as_u64();let from=transition["from"].as_u64();let id=to.or(from);
 let item=track["items"].as_array().into_iter().flatten().find(|i|i["id"].as_u64()==id && id.is_some());
 item.map(|i|if to.is_some(){integer(i,"start")}else{narrow(integer(i,"start")? as i128+integer(i,"duration")? as i128)}).transpose()
}
/// Atomic source ripple group with original linked followers and cut-anchored transitions.
pub fn ripple_trim_linked(document:&mut SourceProjectDocument,sequence:u64,clip:u64,edge:Edge,delta:i64,linked:bool)->Result<i64>{
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let seq=document.envelope.pointer(&path).context("missing ripple sequence")?;
 let tracks:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).collect();
 let all:Vec<_>=tracks.iter().flat_map(|t|t["items"].as_array().into_iter().flatten()).collect();
 let target=all.iter().find(|i|i["id"].as_u64()==Some(clip)).context("missing ripple clip")?;
 let link=target["link"].as_u64().filter(|_|linked);let mut ids=vec![clip];
 if let Some(link)=link {for item in &all {if item["link"].as_u64()==Some(link){let id=item["id"].as_u64().context("invalid linked clip ID")?;if !ids.contains(&id){ids.push(id);}}}}
 let contexts:Vec<_>=ids.iter().map(|id|{let item=all.iter().find(|i|i["id"].as_u64()==Some(*id)).context("missing linked clip")?;project_context(document,sequence,item["item"].as_u64().context("invalid media ID")?)}).collect::<Result<_>>()?;
 let mut d=delta;
 for (id,ctx) in ids.iter().zip(&contexts){let candidate=plan_trim(document,sequence,*id,edge,d,ctx,true)?.d;if candidate.unsigned_abs()<d.unsigned_abs(){d=candidate;}}
 if d==0{return Ok(0);}
 let mut work=document.clone();let seq=work.envelope.pointer_mut(&path).unwrap();let mut origins=vec![];
 for (id,ctx) in ids.iter().zip(&contexts){
  let plan=plan_trim(document,sequence,*id,edge,d,ctx,true)?;
  let item=&mut seq[plan.key][plan.ti]["items"][plan.ci];
  let from=match edge{Edge::In=>narrow(plan.start as i128+1)?,Edge::Out=>narrow(plan.start as i128+plan.duration as i128)?};
  let offset=(d as f64*plan.speed).round() as i64;
  match edge {
   Edge::In=>{if !plan.reverse && !plan.hold {item["source_in"]=narrow(plan.source as i128+offset as i128)?.into();}item["duration"]=narrow(plan.duration as i128-d as i128)?.into();}
   Edge::Out=>{if plan.reverse {item["source_in"]=narrow(plan.source as i128-offset as i128)?.into();}item["duration"]=narrow(plan.duration as i128+d as i128)?.into();}
  }
  let tid=seq[plan.key][plan.ti]["id"].as_u64().context("invalid track ID")?;
  if !origins.iter().any(|(t,_)|*t==tid){origins.push((tid,from));}
 }
 let main_from=match edge {Edge::In=>origins.iter().map(|(_,from)|*from).max().unwrap_or(0),Edge::Out=>origins.first().map(|(_,from)|*from).unwrap_or(0)};
 let shift=match edge{Edge::In=>narrow(-(d as i128))?,Edge::Out=>d};
 let tracks:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).collect();
 let own_links:Vec<_>=tracks.iter().flat_map(|t|t["items"].as_array().into_iter().flatten()).filter(|i|i["id"].as_u64().is_some_and(|id|ids.contains(&id))).filter_map(|i|i["link"].as_u64()).collect();
 let rippling:Vec<_>=tracks.iter().filter(|t|t["locked"].as_bool()!=Some(true)).filter_map(|t|{
  let tid=t["id"].as_u64()?;
  if let Some((_,from))=origins.iter().find(|(id,_)|*id==tid){Some((tid,*from,true))}
  else if t["sync_lock"].as_bool()==Some(true){Some((tid,main_from,false))}else{None}
 }).collect();
 let mut shifting_links=vec![];
 for (tid,from,_) in &rippling {let track=tracks.iter().find(|t|t["id"].as_u64()==Some(*tid)).unwrap();
  for i in track["items"].as_array().into_iter().flatten(){if !ids.contains(&i["id"].as_u64().context("invalid clip ID")?) && integer(i,"start")?>=*from {if let Some(link)=i["link"].as_u64(){if !own_links.contains(&link){shifting_links.push(link);}}}}
 }
 let links:Vec<_>=tracks.iter().flat_map(|t|t["items"].as_array().into_iter().flatten()).filter_map(|i|i["link"].as_u64().map(|link|(link,i))).map(|(link,i)|Ok((link,i["id"].as_u64().context("invalid linked ID")?,narrow(integer(i,"start")? as i128+integer(i,"duration")? as i128)?))).collect::<Result<_>>()?;
 for (tid,from,member) in rippling {
  let (key,ti)=["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().and_then(|tracks|tracks.iter().position(|t|t["id"].as_u64()==Some(tid))).map(|ti|(key,ti))).context("missing ripple track")?;
  let track=&mut seq[key][ti];
  let items=track["items"].as_array_mut().context("invalid track items")?;
  let mut followers=vec![];
  for item in items.iter_mut(){
   let id=item["id"].as_u64().context("invalid clip ID")?;let start=integer(item,"start")?;let end=narrow(start as i128+integer(item,"duration")? as i128)?;let link=item["link"].as_u64();
   let follows=start<from && end>from && link.is_some_and(|l|shifting_links.contains(&l));
   if !ids.contains(&id) && follows {followers.push(id);}
   let cut=narrow(from as i128-1)?;
   let partners:Vec<_>=links.iter().filter(|(l,other,_)|Some(*l)==link && *other!=id).collect();
   let stays=matches!(edge,Edge::In) && start<cut && link.is_some_and(|l|!own_links.contains(&l)) && !partners.is_empty() && partners.iter().all(|(_,_,end)|*end<=cut);
   if !member && shift<0 {
    let closing_start=match edge{Edge::In=>cut,Edge::Out=>narrow(from as i128+shift as i128)?};let closing_end=narrow(closing_start as i128-shift as i128)?;
    let own=link.is_some_and(|l|own_links.contains(&l));
    ensure!(!(start<closing_end && end>closing_start && !follows && !own && !stays),"ripple sync-lock conflict");
   }
   if !ids.contains(&id) && (start>=from || follows){item["start"]=narrow(start as i128+shift as i128)?.into();}
  }
  items.sort_by_key(|i|i["start"].as_i64().unwrap_or(0));
  ensure!(items.first().is_none_or(|i|i["start"].as_i64().is_some_and(|s|s>=0)),"ripple before sequence start");
  let edges:Vec<_>=items.iter().map(|i|Ok((i["id"].as_u64().context("invalid clip ID")?,integer(i,"start")?,narrow(integer(i,"start")? as i128+integer(i,"duration")? as i128)?))).collect::<Result<_>>()?;
  if let Some(transitions)=track["transitions"].as_array_mut(){
   transitions.retain(|t|match(t["from"].as_u64(),t["to"].as_u64()){
    (Some(a),Some(b)) if followers.contains(&b)=>edges.iter().find(|e|e.0==a).map(|e|e.2)==edges.iter().find(|e|e.0==b).map(|e|e.1),_=>true
   });
   transitions.sort_by_key(|t|t["start"].as_i64().unwrap_or(0));
  }

 }
 for key in ["video_tracks","audio_tracks"] {for track in seq[key].as_array_mut().into_iter().flatten(){
  let old=document.envelope.pointer(&path).unwrap()[key].as_array().and_then(|tracks|tracks.iter().find(|t|t["id"]==track["id"])).context("missing original track")?;
  let current=track.clone();
  for transition in track["transitions"].as_array_mut().into_iter().flatten(){
   if let (Some(before),Some(after))=(transition_anchor(old,transition)?,transition_anchor(&current,transition)?){transition["start"]=narrow(integer(transition,"start")? as i128+after as i128-before as i128)?.into();}
   let start=integer(transition,"start")?;let duration=integer(transition,"duration")?;let end=narrow(start as i128+duration as i128)?;
   ensure!((MIN..=MAX).contains(&start) && (0..=MAX).contains(&duration) && (MIN..=MAX).contains(&end),"ripple transition outside clock bounds");
  }
 }}
 validate_trim_tracks(seq)?;*document=work;Ok(d)
}

/// Original razor selection: empty track selection means all unlocked media tracks.
/// Item mode splits only explicitly selected clips; callers expand links when desired.
pub fn razor(document:&mut SourceProjectDocument,sequence:u64,tracks:&[u64],items:Option<&[u64]>,time:i64)->Result<Vec<u64>>{
 let path=format!("/project/items/{sequence}/kind/Sequence");
 let mut work=document.clone();
 let mut next=work.envelope.pointer("/project/next_id").and_then(Value::as_u64).context("missing project next ID")?;
 let seq=work.envelope.pointer_mut(&path).context("missing razor sequence")?;
 let selected_tracks=items.map(|ids|ids.iter().filter_map(|id|["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).find(|track|track["items"].as_array().into_iter().flatten().any(|clip|clip["id"].as_u64()==Some(*id))).and_then(|track|track["id"].as_u64())).collect::<Vec<_>>());
 let mut links=std::collections::HashMap::new();let mut created=vec![];
 for key in ["video_tracks","audio_tracks"]{
  for track in seq[key].as_array_mut().into_iter().flatten(){
   if track["locked"].as_bool().unwrap_or(false){continue;}
   if selected_tracks.as_ref().is_some_and(|ids|!ids.contains(&track["id"].as_u64().unwrap_or(u64::MAX))){continue;}
   if items.is_none() && !tracks.is_empty() && !tracks.contains(&track["id"].as_u64().context("invalid track ID")?){continue;}
   let clips=track["items"].as_array().context("invalid razor track")?;
   if let Some(selected)=items{
    // Original item_at uses the last clip whose start is <= time, and checks its end.
    let at=clips.partition_point(|clip|clip["start"].as_i64().unwrap_or(i64::MAX)<=time);
    if at==0{continue;}let clip=&clips[at-1];
    if time>=narrow(integer(clip,"start")? as i128+integer(clip,"duration")? as i128)? || !selected.contains(&clip["id"].as_u64().context("invalid clip ID")?){continue;}
   }
   if let Some(id)=split_source_track(track,time,&mut next,&mut links)?{created.push(id);}

  }
 }
 work.envelope["project"]["next_id"]=next.into();*document=work;Ok(created)
}

fn split_source_track(track:&mut Value,time:i64,next:&mut u64,links:&mut std::collections::HashMap<u64,u64>)->Result<Option<u64>>{
 let clips=track["items"].as_array().context("invalid split track")?;
   let mut index=None;
   for (i,clip) in clips.iter().enumerate(){let start=integer(clip,"start")?;let end=narrow(start as i128+integer(clip,"duration")? as i128)?;if start<time && time<end{index=Some(i);break;}}
   let Some(index)=index else{return Ok(None)};
   let mut left=clips[index].clone();let mut right=left.clone();
   let cut=narrow(time as i128-integer(&left,"start")? as i128)?;let duration=integer(&left,"duration")?;
   let new_id=*next;*next=next.checked_add(1).context("razor ID overflow")?;
   left["duration"]=cut.into();right["id"]=new_id.into();right["start"]=time.into();right["duration"]=narrow(duration as i128-cut as i128)?.into();
   let speed=right["speed"].as_f64().unwrap_or(1.).abs();ensure!(speed.is_finite(),"invalid razor speed");
   let reverse=right["reverse"].as_bool().unwrap_or(false);let hold=right.get("frame_hold").is_some_and(|v|!v.is_null());
   let consumed=(cut as f64*speed).round() as i64;
   if !reverse && !hold{right["source_in"]=narrow(integer(&right,"source_in")? as i128+consumed as i128)?.into();}
   else if reverse{let total=(duration as f64*speed).round() as i64;left["source_in"]=narrow(integer(&left,"source_in")? as i128+total as i128-consumed as i128)?.into();}
   if let Some(link)=right["link"].as_u64(){
    let new_link=if let Some(value)=links.get(&link){*value}else{let value=*next;*next=next.checked_add(1).context("razor link ID overflow")?;links.insert(link,value);value};right["link"]=new_link.into();
   }
   let old_id=left["id"].as_u64().context("invalid razor clip ID")?;
   if let Some(transitions)=track["transitions"].as_array_mut(){
    let mut retained=vec![];
    for mut transition in std::mem::take(transitions){
     let start=integer(&transition,"start")?;let end=narrow(start as i128+integer(&transition,"duration")? as i128)?;
     let from=transition["from"].as_u64();let to=transition["to"].as_u64();
     if start<time && time<end && (from==Some(old_id)||to==Some(old_id)) && !(from.is_some()&&to.is_some()){continue;}
     if start>=time && from==Some(old_id){transition["from"]=new_id.into();}retained.push(transition);
    }*transitions=retained;
   }
   let clips=track["items"].as_array_mut().unwrap();clips[index]=left;clips.insert(index+1,right);Ok(Some(new_id))
}

/// Original trim.toPlayhead (Q/W): selection and linked partners take precedence over targets.
/// Only unlocked tracks with a clip strictly containing the playhead qualify.
pub fn trim_to_playhead(document:&mut SourceProjectDocument,sequence:u64,selected:&[u64],targeted:&[u64],playhead:i64,next:bool,ripple:bool,linked:bool)->Result<Value>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing trim-to-playhead sequence")?;
 let all:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten()).collect();
 let mut selection=selected.to_vec();
 if linked{let links:Vec<_>=all.iter().flat_map(|t|t["items"].as_array().into_iter().flatten()).filter(|i|selected.contains(&i["id"].as_u64().unwrap_or(0))).filter_map(|i|i["link"].as_u64()).collect();
  for item in all.iter().flat_map(|t|t["items"].as_array().into_iter().flatten()){if item["link"].as_u64().is_some_and(|l|links.contains(&l)){let id=item["id"].as_u64().context("invalid linked clip ID")?;if !selection.contains(&id){selection.push(id);}}}
 }
 let mut chosen=vec![];let mut fallback=vec![];
 for track in &all{if track["locked"].as_bool().unwrap_or(false){continue;}let id=track["id"].as_u64().context("invalid trim-to-playhead track ID")?;let mut under=false;let mut selected_under=false;
  for item in track["items"].as_array().context("invalid trim-to-playhead items")?{let start=integer(item,"start")?;let end=narrow(start as i128+integer(item,"duration")? as i128)?;if start<playhead&&playhead<end{under=true;if selection.contains(&item["id"].as_u64().context("invalid clip ID")?){selected_under=true;}}}
  if selected_under{chosen.push(id);}if under&&targeted.contains(&id){fallback.push(id);}
 }
 if chosen.is_empty(){chosen=fallback;}ensure!(!chosen.is_empty(),"no clip under the playhead on the targeted tracks");
 let mut boundary:Option<i64>=None;
 for track in &all{if !chosen.contains(&track["id"].as_u64().unwrap()){continue;}for item in track["items"].as_array().unwrap(){let start=integer(item,"start")?;let end=narrow(start as i128+integer(item,"duration")? as i128)?;
  for edge in [start,end]{if next&&edge>playhead{boundary=Some(boundary.map_or(edge,|b|b.min(edge)));}else if !next&&edge<playhead{boundary=Some(boundary.map_or(edge,|b|b.max(edge)));}}
 }}
 let edge=boundary.context("no edit point")?;let (start,end)=if next{(playhead,edge)}else{(edge,playhead)};let duration=narrow(end as i128-start as i128)?;
 remove_range(document,sequence,&chosen,start,duration,ripple)?;
 Ok(serde_json::json!({"range":[start,end],"playhead":if !next&&ripple{edge}else{playhead},"tracks":chosen}))
}

/// Lift or extract the exact range; extract also edits unlocked synchronized caption tracks.
pub fn remove_range(document:&mut SourceProjectDocument,sequence:u64,tracks:&[u64],start:i64,duration:i64,ripple:bool)->Result<Vec<u64>>{
 let end=narrow(start as i128+duration as i128)?;let shift=narrow(-(duration as i128))?;
 let path=format!("/project/items/{sequence}/kind/Sequence");let mut work=document.clone();
 let mut next=work.envelope.pointer("/project/next_id").and_then(Value::as_u64).context("missing project next ID")?;
 let seq=work.envelope.pointer_mut(&path).context("missing range sequence")?;
 let mut links=std::collections::HashMap::new();let mut removed=vec![];
 for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().into_iter().flatten(){
  if track["locked"].as_bool().unwrap_or(false) || !(tracks.contains(&track["id"].as_u64().context("invalid range track ID")?) || ripple && track["sync_lock"].as_bool().unwrap_or(false)){continue;}
  removed.extend(clear_source_track_range(track,start,duration,&mut next,&mut links)?);
  if ripple{shift_source_track(track,end,shift)?;}
 }}
 if ripple{for track in seq["caption_tracks"].as_array_mut().into_iter().flatten(){
  if track["locked"].as_bool().unwrap_or(false)||!track["sync_lock"].as_bool().unwrap_or(false){continue;}
  let captions=track["captions"].as_array_mut().context("invalid caption track")?;let mut kept=vec![];
  for mut caption in std::mem::take(captions){let (a,b)=item_range(&caption)?;
   if b<=start{kept.push(caption);}else if a>=end{caption["start"]=narrow(a as i128-duration as i128)?.into();kept.push(caption);}else{
    let left=(start as i128-a as i128).max(0);let right=(b as i128-end as i128).max(0);let total=narrow(left+right)?;
    if total>0{caption["start"]=a.min(start).into();caption["duration"]=total.into();kept.push(caption);}
   }
  }kept.sort_by_key(|caption|caption["start"].as_i64().unwrap_or(0));*captions=kept;
 }}
 work.envelope["project"]["next_id"]=next.into();*document=work;Ok(removed)
}

fn clear_source_track_range(track:&mut Value,start:i64,duration:i64,next:&mut u64,links:&mut std::collections::HashMap<u64,u64>)->Result<Vec<u64>>{
 let end=narrow(start as i128+duration as i128)?;let mut removed=vec![];
  if duration>0{
   split_source_track(track,start,next,links)?;split_source_track(track,end,next,links)?;
   let clips=track["items"].as_array_mut().context("invalid range track")?;let mut kept=vec![];
   for clip in std::mem::take(clips){let (a,b)=item_range(&clip)?;if a>=start && b<=end{removed.push(clip["id"].as_u64().context("invalid range clip ID")?);}else{kept.push(clip);}}*clips=kept;
   let ids:std::collections::HashSet<_>=clips.iter().filter_map(|clip|clip["id"].as_u64()).collect();
   if let Some(transitions)=track["transitions"].as_array_mut(){transitions.retain(|tr|{let from=tr["from"].as_u64();let to=tr["to"].as_u64();from.is_none_or(|id|ids.contains(&id)) && to.is_none_or(|id|ids.contains(&id)) && (from.is_some()||to.is_some())});}
  }
 Ok(removed)
}
/// Validate transport field types while preserving extension properties unchanged.
pub fn validate_clip_fields(clip:&Value)->Result<()>{
 ensure!(clip.is_object() && ["id","item"].iter().all(|key|clip[key].as_u64().is_some()) && ["start","duration","source_in"].iter().all(|key|clip[key].as_i64().is_some()) && clip["speed"].as_f64().is_some_and(f64::is_finite) && clip["name"].is_string() && clip["label"].is_string() && clip["enabled"].is_boolean() && clip["effects"].is_array(),"invalid placed clip fields");
 for key in ["link","group"]{if let Some(value)=clip.get(key){ensure!(value.is_null()||value.as_u64().is_some(),"invalid clip {key}");}}
 for key in ["reverse","scale_to_frame","hold_filters"]{if let Some(value)=clip.get(key){ensure!(value.is_boolean(),"invalid clip {key}");}}
 if let Some(value)=clip.get("frame_hold"){ensure!(value.is_null()||value.as_i64().is_some(),"invalid clip frame hold");}
 if let Some(value)=clip.get("gain_db"){ensure!(value.as_f64().is_some_and(f64::is_finite),"invalid clip gain");}
 if let Some(value)=clip.get("markers"){ensure!(value.is_array(),"invalid clip markers");}
 if let Some(value)=clip.get("source_channels"){ensure!(value.as_array().is_some_and(|channels|channels.iter().all(|channel|channel.as_u64().is_some_and(|channel|channel<=u16::MAX as u64))),"invalid clip source channels");}
 Ok(())
}
pub(super) fn validate_clipboard(clipboard:&Value)->Result<()>{
 ensure!(clipboard["format"]=="video-kadr.filmcraft-clipboard" && clipboard["version"]==1,"invalid source clipboard version");
 let clips=clipboard["clips"].as_array().context("invalid clipboard clips")?;let markers=clipboard["markers"].as_array().context("invalid clipboard markers")?;
 ensure!(clips.len()<=4096 && markers.len()<=4096,"clipboard selection too large");
 for entry in clips{ensure!(entry.as_array().is_some_and(|entry|entry.len()==3),"invalid clipboard entry");ensure!(matches!(entry[0].as_str(),Some("Video"|"Audio")),"invalid clipboard track kind");ensure!(entry[1].as_u64().is_some(),"invalid clipboard track index");validate_clip_fields(&entry[2])?;}
 for marker in markers{ensure!(marker.is_object() && marker["id"].as_u64().is_some() && ["start","duration"].iter().all(|key|marker[key].as_i64().is_some()) && ["name","comment","kind","color"].iter().all(|key|marker[key].is_string()),"invalid clipboard marker fields");}
 Ok(())
}
/// Original insert/overwrite primitives, with caller-supplied placed clip IDs.
pub fn place_clips(document:&mut SourceProjectDocument,sequence:u64,placements:&[(u64,Value)],insert:bool)->Result<Vec<u64>>{
 ensure!(!insert || !placements.is_empty(),"nothing to insert");
 let path=format!("/project/items/{sequence}/kind/Sequence");let mut work=document.clone();
 let mut next=work.envelope.pointer("/project/next_id").and_then(Value::as_u64).context("missing project next ID")?;
 let seq=work.envelope.pointer_mut(&path).context("missing placement sequence")?;let mut locations=vec![];
 for (id,item) in placements{
  validate_clip_fields(item)?;
  let location=["video_tracks","audio_tracks"].into_iter().find_map(|key|seq[key].as_array().and_then(|tracks|tracks.iter().position(|track|track["id"].as_u64()==Some(*id))).map(|index|(key,index))).context("missing placement track")?;
  ensure!(!seq[location.0][location.1]["locked"].as_bool().unwrap_or(false),"placement track is locked");
  item["id"].as_u64().context("invalid placed clip ID")?;item_range(item)?;locations.push(location);
 }
 let mut links=std::collections::HashMap::new();
 if insert{
  let at=placements.iter().map(|(_,item)|integer(item,"start")).collect::<Result<Vec<_>>>()?.into_iter().min().unwrap();
  let end=placements.iter().map(|(_,item)|item_range(item).map(|(_,end)|end)).collect::<Result<Vec<_>>>()?.into_iter().max().unwrap();
  let duration=narrow(end as i128-at as i128)?;
  for key in ["video_tracks","audio_tracks"]{for track in seq[key].as_array_mut().into_iter().flatten(){
   if track["locked"].as_bool().unwrap_or(false)||!(track["sync_lock"].as_bool().unwrap_or(false)||placements.iter().any(|(id,_)|track["id"].as_u64()==Some(*id))){continue;}
   split_source_track(track,at,&mut next,&mut links)?;shift_source_track(track,at,duration)?;
  }}
  for track in seq["caption_tracks"].as_array_mut().into_iter().flatten(){
   if track["locked"].as_bool().unwrap_or(false)||!track["sync_lock"].as_bool().unwrap_or(false){continue;}
   let captions=track["captions"].as_array_mut().context("invalid insertion captions")?;
   let i=captions.partition_point(|caption|caption["start"].as_i64().unwrap_or(i64::MAX)<=at);
   if i>0{let index=i-1;let (start,end)=item_range(&captions[index])?;
    if start<at && at<end{let mut right=captions[index].clone();let id=next;next=next.checked_add(1).context("caption ID overflow")?;
     right["id"]=id.into();right["start"]=at.into();right["duration"]=narrow(end as i128-at as i128)?.into();right["cue_id"]=Value::Null;
     captions[index]["duration"]=narrow(at as i128-start as i128)?.into();captions.push(right);captions.sort_by_key(|c|c["start"].as_i64().unwrap_or(0));
    }
   }
   for caption in captions{let start=integer(caption,"start")?;if start>=at{caption["start"]=narrow(start as i128+duration as i128)?.into();}}
  }
 }
 let mut placed=vec![];
 for ((_,item),(key,index)) in placements.iter().zip(locations){let track=&mut seq[key][index];
  if !insert{clear_source_track_range(track,integer(item,"start")?,integer(item,"duration")?,&mut next,&mut links)?;}
  let items=track["items"].as_array_mut().context("invalid placement track")?;let start=integer(item,"start")?;
  let index=items.partition_point(|clip|clip["start"].as_i64().unwrap_or(i64::MAX)<=start);items.insert(index,item.clone());placed.push(item["id"].as_u64().unwrap());
 }
 work.envelope["project"]["next_id"]=next.into();*document=work;Ok(placed)
}

/// Opaque, relative-time Filmcraft clipboard, including optional sequence markers.
pub fn copy_clips(document:&SourceProjectDocument,sequence:u64,ids:&[u64],linked:bool,markers:bool)->Result<Value>{
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing copy sequence")?;
 let all:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().into_iter().flatten().flat_map(|track|track["items"].as_array().into_iter().flatten())).collect();
 let mut selected=ids.to_vec();
 if linked{let links:Vec<_>=ids.iter().filter_map(|id|all.iter().find(|clip|clip["id"].as_u64()==Some(*id)).and_then(|clip|clip["link"].as_u64())).collect();for clip in &all{if clip["link"].as_u64().is_some_and(|link|links.contains(&link)){let id=clip["id"].as_u64().context("invalid copy clip ID")?;if !selected.contains(&id){selected.push(id);}}}}
 let mut starts=vec![];let mut ends=vec![];for id in &selected{if let Some(clip)=all.iter().find(|clip|clip["id"].as_u64()==Some(*id)){let (start,end)=item_range(clip)?;starts.push(start);ends.push(end);}}
 let start=starts.into_iter().min().unwrap_or(0);let end=ends.into_iter().max().unwrap_or(0);let mut copied=vec![];
 for (key,kind) in [("video_tracks","Video"),("audio_tracks","Audio")]{for (index,track) in seq[key].as_array().into_iter().flatten().enumerate(){for clip in track["items"].as_array().into_iter().flatten(){if clip["id"].as_u64().is_some_and(|id|selected.contains(&id)){let mut clip=clip.clone();clip["start"]=narrow(integer(&clip,"start")? as i128-start as i128)?.into();copied.push(serde_json::json!([kind,index,clip]));}}}}
 let mut copied_markers=vec![];if markers{for marker in seq["markers"].as_array().into_iter().flatten(){let time=integer(marker,"start")?;if time>=start && time<end{let mut marker=marker.clone();marker["start"]=narrow(time as i128-start as i128)?.into();copied_markers.push(marker);}}}
 Ok(serde_json::json!({"format":"video-kadr.filmcraft-clipboard","version":1,"clips":copied,"markers":copied_markers}))
}
pub struct PasteResult{pub ids:Vec<u64>,pub playhead:i64}
pub fn paste_clips(document:&mut SourceProjectDocument,sequence:u64,clipboard:&Value,at:i64,insert:bool,include_markers:bool,ripple_markers:bool)->Result<PasteResult>{
 validate_clipboard(clipboard)?;
 let clips=clipboard["clips"].as_array().unwrap();let markers=clipboard["markers"].as_array().unwrap();
 let mut work=document.clone();let path=format!("/project/items/{sequence}/kind/Sequence");let seq=work.envelope.pointer(&path).context("missing paste sequence")?;
 let mut next=work.envelope["project"]["next_id"].as_u64().context("invalid clipboard next ID")?;let mut links=std::collections::HashMap::new();let mut placements=vec![];
 for entry in clips{
  let key=match entry[0].as_str(){Some("Video")=>"video_tracks",Some("Audio")=>"audio_tracks",_=>anyhow::bail!("invalid clipboard track kind")};
  let index=usize::try_from(entry[1].as_u64().context("invalid clipboard track index")?).context("clipboard track index overflow")?;let tracks=seq[key].as_array().context("invalid paste tracks")?;
  let Some(track)=tracks.get(index).or(tracks.last())else{continue};let mut clip=entry[2].clone();ensure!(clip.is_object(),"invalid clipboard clip");
  let id=next;next=next.checked_add(1).context("paste clip ID overflow")?;clip["id"]=id.into();clip["start"]=narrow(integer(&clip,"start")? as i128+at as i128)?.into();
  if let Some(link)=clip["link"].as_u64(){let new_link=if let Some(id)=links.get(&link){*id}else{let id=next;next=next.checked_add(1).context("paste link ID overflow")?;links.insert(link,id);id};clip["link"]=new_link.into();}
  placements.push((track["id"].as_u64().context("invalid paste track ID")?,clip));
 }
 let end=placements.iter().map(|(_,clip)|item_range(clip).map(|(_,end)|end)).collect::<Result<Vec<_>>>()?.into_iter().max().unwrap_or(at);
 work.envelope["project"]["next_id"]=next.into();let ids=place_clips(&mut work,sequence,&placements,insert)?;next=work.envelope["project"]["next_id"].as_u64().unwrap();
 let seq=work.envelope.pointer_mut(&path).unwrap();let target=seq["markers"].as_array_mut().context("invalid paste markers")?;
 if insert && ripple_markers{let shift=narrow(end as i128-at as i128)?;
  if shift<0{let cutoff=narrow(at as i128+shift as i128)?;target.retain(|marker|marker["start"].as_i64().is_none_or(|time|!(time>=cutoff&&time<at)));}
  for marker in target.iter_mut(){let start=integer(marker,"start")?;if start>=at{marker["start"]=narrow(start as i128+shift as i128)?.into();}}
 }
 if include_markers{for marker in markers{let mut marker=marker.clone();ensure!(marker.is_object(),"invalid clipboard marker");let id=next;next=next.checked_add(1).context("paste marker ID overflow")?;marker["id"]=id.into();marker["start"]=narrow(integer(&marker,"start")? as i128+at as i128)?.into();target.push(marker);}}
 target.sort_by_key(|marker|marker["start"].as_i64().unwrap_or(0));work.envelope["project"]["next_id"]=next.into();*document=work;Ok(PasteResult{ids,playhead:end})
}


/// Add/replace an overlapping transition exactly as the original track primitive.
/// Original engine track-order edit targeting and floor-to-frame transition placement.
pub fn plan_transition(document:&SourceProjectDocument,sequence:u64,kind:&str,targeted:&[u64],clip:Option<u64>,edge:Option<Edge>,time:i64,duration:i64)->Result<Value>{
 ensure!(matches!(kind,"video_tracks"|"audio_tracks"),"invalid transition track kind");
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=document.envelope.pointer(&path).context("missing transition sequence")?;
 let rate=&seq["settings"]["frame_rate"];let mut num=integer(rate,"num")? as i128;let mut den=integer(rate,"den")? as i128;if num<=0||den<=0{num=24000;den=1001;}
 let unit=254_016_000_000i128*den;let near=(3*unit).div_euclid(num);
 for track in seq[kind].as_array().context("invalid transition tracks")?{
  let track_id=track["id"].as_u64().context("invalid transition track ID")?;if clip.is_none()&&!targeted.contains(&track_id){continue;}
  let items=track["items"].as_array().context("invalid transition items")?;
  for (i,item) in items.iter().enumerate(){let id=item["id"].as_u64().context("invalid transition clip ID")?;if clip.is_some_and(|clip|clip!=id){continue;}
   let start=integer(item,"start")? as i128;let end=start+integer(item,"duration")? as i128;
   let from=if let Some(previous)=i.checked_sub(1).and_then(|j|items.get(j)){if integer(previous,"start")? as i128+integer(previous,"duration")? as i128==start{previous["id"].as_u64()}else{None}}else{None};
   let to=items.get(i+1).filter(|next|next["start"].as_i64().map(i128::from)==Some(end)).and_then(|next|next["id"].as_u64());
   let use_in=match edge{Some(Edge::In)=>true,Some(Edge::Out)=>false,None=>(time as i128-start).abs()<=(time as i128-end).abs()};
   if clip.is_none()&&start!=time as i128&&end!=time as i128&&(start-time as i128).abs()>=near&&(end-time as i128).abs()>=near{continue;}
   let (from,to,cut)=if use_in{(from,Some(id),start)}else{(Some(id),to,end)};
   let start=if from.is_some()&&to.is_some(){cut-(duration as i128).div_euclid(2)}else if to.is_some(){cut}else{cut-duration as i128};
   let frame=(start*num).div_euclid(unit);let snapped=narrow((frame*unit).div_euclid(num))?;
   return Ok(serde_json::json!({"track":track_id,"from":from,"to":to,"start":snapped}));
  }
 }
 anyhow::bail!("no edit point at the playhead on targeted tracks")
}

pub fn add_transition(document:&mut SourceProjectDocument,sequence:u64,track_id:u64,transition:&Value)->Result<u64>{
 ensure!(transition.is_object() && transition["effect"].is_object(),"invalid transition");
 let start=integer(transition,"start")?;let end=narrow(start as i128+integer(transition,"duration")? as i128)?;
 for key in ["from","to"]{ensure!(transition[key].is_null()||transition[key].as_u64().is_some(),"invalid transition {key}");}
 let mut work=document.clone();let next=work.envelope["project"]["next_id"].as_u64().context("invalid transition next ID")?;
 let path=format!("/project/items/{sequence}/kind/Sequence");let seq=work.envelope.pointer_mut(&path).context("missing transition sequence")?;
 let mut location=None;for key in ["video_tracks","audio_tracks"]{if let Some(index)=seq[key].as_array().and_then(|tracks|tracks.iter().position(|track|track["id"].as_u64()==Some(track_id))){location=Some((key,index));break;}}
 let (key,index)=location.context("missing transition track")?;let track=&mut seq[key][index];ensure!(!track["locked"].as_bool().unwrap_or(false),"transition track is locked");
 let following=next.checked_add(1).context("transition ID overflow")?;let mut added=transition.clone();added["id"]=next.into();
 let transitions=track["transitions"].as_array_mut().context("invalid transition track")?;let mut kept=vec![];
 for old in std::mem::take(transitions){let old_start=integer(&old,"start")?;let old_end=narrow(old_start as i128+integer(&old,"duration")? as i128)?;
  if !(old_start<end && start<old_end && (old["from"]==added["from"]||old["to"]==added["to"])){kept.push(old);}
 }kept.push(added);kept.sort_by_key(|transition|transition["start"].as_i64().unwrap_or(0));*transitions=kept;
 track["items"].as_array_mut().context("invalid transition track items")?.sort_by_key(|item|item["start"].as_i64().unwrap_or(0));
 work.envelope["project"]["next_id"]=following.into();*document=work;Ok(next)
}

#[cfg(test)]
mod tests {
 #[test]
 fn original_trim_shift_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-shift.json")).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-points.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1296);
  for (i,case) in cases.iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let initial=TrimShift{outgoing:case["initial"]["outShift"].as_i64().unwrap(),incoming:case["initial"]["inShift"].as_i64().unwrap()};let p=&base["points"][0];let primary=TrimEditPoint{clip:p["clip"].as_u64().unwrap(),edge:if p["out"].as_bool().unwrap(){Edge::Out}else{Edge::In},kind:match p["kind"].as_str().unwrap(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,_=>TrimKind::Roll}};
   let result=if base["error"]==true{initial}else{let applied:Vec<_>=base["applied"].as_array().unwrap().iter().map(|v|v.as_i64().unwrap()).collect();trim_shift_after(initial,Some(primary),&applied).unwrap()};
   assert_eq!(serde_json::json!({"outShift":result.outgoing,"inShift":result.incoming}),case["expected"],"trim shift {i}");
  }
  let roll=TrimEditPoint{clip:1,edge:Edge::Out,kind:TrimKind::Roll};let maximum=TrimShift{outgoing:0,incoming:i64::MAX};assert!(trim_shift_after(maximum,Some(roll),&[1]).is_err());assert_eq!(trim_shift_after(maximum,None,&[1]).unwrap(),maximum);assert_eq!(trim_shift_after(maximum,Some(roll),&[]).unwrap(),maximum);
 }

 #[test]
 fn original_trim_point_batches_match(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-points.json")).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),432);
  for (i,case) in cases.iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let points:Vec<_>=case["points"].as_array().unwrap().iter().map(|p|TrimEditPoint{clip:p["clip"].as_u64().unwrap(),edge:if p["out"].as_bool().unwrap(){Edge::Out}else{Edge::In},kind:match p["kind"].as_str().unwrap(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,_=>TrimKind::Roll}}).collect();let result=trim_edit_points(&mut doc,base["sequence"].as_u64().unwrap(),&points,case["delta"].as_i64().unwrap(),case["linked"].as_bool().unwrap());if case["error"]==true{assert!(result.is_err(),"batch trim refusal {i}");}else{assert_eq!(serde_json::to_value(result.unwrap()).unwrap(),case["applied"],"batch trim movement {i}");}assert_eq!(doc.envelope["project"],case["expectedProject"],"batch trim project {i}");
  }
 }

 #[test]
 fn original_trim_to_playhead_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-to-playhead.json")).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1536);
  for (i,case) in cases.iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let selected:Vec<_>=case["selected"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();let targeted:Vec<_>=case["targeted"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();
   let result=trim_to_playhead(&mut doc,base["sequence"].as_u64().unwrap(),&selected,&targeted,case["playhead"].as_i64().unwrap(),case["next"].as_bool().unwrap(),case["ripple"].as_bool().unwrap(),case["linked"].as_bool().unwrap());
   if case["error"]==true{assert!(result.is_err(),"Q/W refusal {i}");}else{let result=result.unwrap();assert_eq!(result["range"],case["result"]["range"],"Q/W range {i}");assert_eq!(result["playhead"],case["expectedPlayhead"],"Q/W playhead {i}");}
   assert_eq!(doc.envelope["project"],case["expectedProject"],"Q/W project {i}");
  }
 }

 #[test]
 fn trim_to_playhead_selection_targets_and_boundaries(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();let base=&fixture["cases"][0];let original=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let sequence=base["sequence"].as_u64().unwrap();let ph=762048000000;
  for next in [false,true]{for ripple in [false,true]{for linked in [false,true]{let mut doc=original.clone();let result=trim_to_playhead(&mut doc,sequence,&[1001],&[4],ph,next,ripple,linked).unwrap();assert_eq!(result["tracks"],if linked{serde_json::json!([3,4,5])}else{serde_json::json!([3])});let edge=if linked{if next{952560000000}else{635040000000}}else if next{1016064000000}else{508032000000};assert_eq!(result["range"],if next{serde_json::json!([ph,edge])}else{serde_json::json!([edge,ph])});assert_eq!(result["playhead"],if !next&&ripple{edge}else{ph});}}}
  let mut doc=original.clone();assert!(trim_to_playhead(&mut doc,sequence,&[],&[],ph,false,true,true).is_err());assert_eq!(doc.envelope,original.envelope);
  let result=trim_to_playhead(&mut doc,sequence,&[9999],&[4],ph,false,false,true).unwrap();assert_eq!(result["tracks"],serde_json::json!([4]));
  let mut doc=original.clone();assert!(trim_to_playhead(&mut doc,sequence,&[],&[3],508032000000,false,true,false).is_err());assert_eq!(doc.envelope,original.envelope);
 }

 #[test]
 fn original_extend_to_playhead_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-extend-playhead.json")).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(fixture["cases"].as_array().unwrap().len(),432);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
   let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();
   let points:Vec<_>=case["points"].as_array().unwrap().iter().map(|p|TrimEditPoint{clip:p["clip"].as_u64().unwrap(),edge:if p["out"]==true{Edge::Out}else{Edge::In},kind:match p["kind"].as_str().unwrap(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,_=>TrimKind::Roll}}).collect();
   let result=extend_edit_points_to_playhead(&mut doc,base["sequence"].as_u64().unwrap(),&points,case["playhead"].as_i64().unwrap(),case["linked"].as_bool().unwrap());
   let mut shift=TrimShift::default();if case["error"]==true{assert!(result.is_err(),"extend refusal {i}");}else{let applied=result.unwrap();assert_eq!(serde_json::to_value(&applied).unwrap(),case["applied"],"extend applied {i}");shift=trim_shift_after(shift,points.first().copied(),&applied).unwrap();}
   assert_eq!(serde_json::json!({"out_shift":shift.outgoing,"in_shift":shift.incoming}),case["shift"],"extend shift {i}");assert_eq!(doc.envelope["project"],case["expectedProject"],"extend project {i}");
  }
 }

 #[test]
 fn extend_to_playhead_preserves_original_batch_outcomes(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-points.json")).unwrap();
  let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();
  let mut checked=0;
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){
   let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];
   let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();
   let points:Vec<_>=case["points"].as_array().unwrap().iter().map(|p|TrimEditPoint{clip:p["clip"].as_u64().unwrap(),edge:if p["out"].as_bool().unwrap(){Edge::Out}else{Edge::In},kind:match p["kind"].as_str().unwrap(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,_=>TrimKind::Roll}}).collect();
   let primary=points[0];let context=edit_point_context(&doc,base["sequence"].as_u64().unwrap(),primary.clip,primary.edge).unwrap();
   let Some(first)=context["editTime"].as_i64()else{continue};
   let playhead=first.checked_add(case["delta"].as_i64().unwrap()).unwrap();
   let result=extend_edit_points_to_playhead(&mut doc,base["sequence"].as_u64().unwrap(),&points,playhead,case["linked"].as_bool().unwrap());
   if case["error"]==true{assert!(result.is_err(),"extend refusal {i}");}else{assert_eq!(serde_json::to_value(result.unwrap()).unwrap(),case["applied"],"extend movement {i}");}
   assert_eq!(doc.envelope["project"],case["expectedProject"],"extend project {i}");checked+=1;
  }
  assert_eq!(checked,288,"existing-point corpus coverage");
 }

 #[test]
 fn original_nearest_edit_points_match(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-nearest-edit-point.json")).unwrap();let source:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),2880);
  for (i,case) in cases.iter().enumerate(){let base=&source["cases"][case["sourceCase"].as_u64().unwrap() as usize];let doc=SourceProjectDocument::read(&serde_json::to_vec(&base["document"]).unwrap()).unwrap();let targets:Vec<_>=case["targeted"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();assert_eq!(nearest_edit_points(&doc,base["sequence"].as_u64().unwrap(),&targets,case["time"].as_i64().unwrap(),case["kind"].as_str().unwrap()).unwrap(),case["expected"],"nearest edit {i}");}
 }

 #[test]
 fn original_trim_monitor_sides_match(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-side.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),864);
  for (i,case) in cases.iter().enumerate(){let doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();for (j,context) in case["contexts"].as_array().unwrap().iter().enumerate(){for (side,outgoing) in [("outgoing",true),("incoming",false)]{let value=context[side].as_u64().map(|id|trim_monitor_side(&doc,case["sequence"].as_u64().unwrap(),id,outgoing).unwrap()).flatten();assert_eq!(serde_json::to_value(value).unwrap(),context[format!("{side}Frame")],"trim frame {i}/{j}/{side}");}}}
 }
 #[test]
 fn original_trim_monitor_custom_rates_match(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-rate.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),432);
  for (i,case) in cases.iter().enumerate(){let doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();for (j,context) in case["contexts"].as_array().unwrap().iter().enumerate(){for (side,outgoing) in [("outgoing",true),("incoming",false)]{let value=context[side].as_u64().map(|id|trim_monitor_side(&doc,case["sequence"].as_u64().unwrap(),id,outgoing).unwrap()).flatten();assert_eq!(serde_json::to_value(value).unwrap(),context[format!("{side}Frame")],"trim frame {i}/{j}/{side}");}}}
 }

 #[test]
 fn original_edit_point_context_matches(){
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edit-point-context.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),864);
  for (i,case) in cases.iter().enumerate(){let doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();for (j,point) in case["points"].as_array().unwrap().iter().enumerate(){assert_eq!(edit_point_context(&doc,case["sequence"].as_u64().unwrap(),point["clip"].as_u64().unwrap(),if point["out"].as_bool().unwrap(){Edge::Out}else{Edge::In}).unwrap(),case["contexts"][j],"edit context {i}/{j}");}assert_eq!(doc,before);}
 }

 #[test]
 fn original_transition_target_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-target.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1920);
  for (i,case) in cases.iter().enumerate(){let doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let targeted:Vec<_>=case["targeted"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();let edge=match case["edge"].as_str(){Some("in")=>Some(Edge::In),Some("out")=>Some(Edge::Out),_=>None};
   let result=plan_transition(&doc,case["sequence"].as_u64().unwrap(),case["kind"].as_str().unwrap(),&targeted,case["clip"].as_u64(),edge,case["time"].as_i64().unwrap(),case["duration"].as_i64().unwrap());if case["expected"].is_null(){assert!(result.is_err(),"target refusal {i}");}else{assert_eq!(result.unwrap(),case["expected"],"transition target {i}");}
  }
 }

 #[test]
 fn original_add_transition_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-add-transition.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1152);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let result=add_transition(&mut doc,case["sequence"].as_u64().unwrap(),case["track"].as_u64().unwrap(),&case["transition"]);
   if case["error"]==true{assert!(result.is_err(),"transition refusal {i}");assert_eq!(doc,before,"atomic transition {i}");}else{assert_eq!(result.unwrap(),case["id"].as_u64().unwrap(),"transition ID {i}");assert_eq!(doc.envelope,case["expected"],"transition document {i}");}
  }
  let case=cases.iter().find(|case|case["error"]==false).unwrap();let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();doc.envelope["project"]["next_id"]=u64::MAX.into();let before=doc.clone();assert!(add_transition(&mut doc,case["sequence"].as_u64().unwrap(),case["track"].as_u64().unwrap(),&case["transition"]).is_err());assert_eq!(doc,before);
 }
 #[test]
 fn malformed_clipboard_refuses_atomically_and_preserves_extensions(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-clipboard.json")).unwrap();let case=fixture["cases"].as_array().unwrap().iter().find(|case|case["error"]==false && case["include"]==true && !case["clipboard"]["clips"].as_array().unwrap().is_empty()).unwrap();
  let clipboard=&case["clipboard"];let mut malformed=vec![];
  for (field,value) in [("version",serde_json::json!(2)),("format",serde_json::json!("unknown")),("clips",Value::Null),("markers",Value::Null)]{let mut value_clipboard=clipboard.clone();value_clipboard[field]=value;malformed.push(value_clipboard);}
  for entry in [Value::Null,serde_json::json!(["Video",0]),serde_json::json!(["Video",0,{},0]),serde_json::json!(["Unknown",0,{}]),serde_json::json!(["Video",-1,{}])]{let mut value=clipboard.clone();value["clips"]=serde_json::json!([entry]);malformed.push(value);}
  for (field,value) in [("id",serde_json::json!("1")),("item",serde_json::json!(-1)),("start",serde_json::json!(0.5)),("source_in",Value::Null),("duration",serde_json::json!("1")),("speed",serde_json::json!("1")),("name",Value::Null),("label",serde_json::json!(0)),("enabled",serde_json::json!(0)),("effects",serde_json::json!({})),("link",serde_json::json!("18446744073709551615")),("group",serde_json::json!(-1)),("reverse",serde_json::json!(0)),("frame_hold",serde_json::json!(0.5)),("gain_db",serde_json::json!("0")),("markers",serde_json::json!({})),("source_channels",serde_json::json!([65536])),("hold_filters",Value::Null),("scale_to_frame",serde_json::json!(1))]{let mut value_clipboard=clipboard.clone();value_clipboard["clips"][0][2][field]=value;malformed.push(value_clipboard);}
  for (field,value) in [("id",serde_json::json!("1")),("start",serde_json::json!(0.5)),("duration",Value::Null),("comment",serde_json::json!(0)),("kind",Value::Null),("color",serde_json::json!(1)),("name",serde_json::json!([]))]{let mut value_clipboard=clipboard.clone();value_clipboard["markers"][0][field]=value;malformed.push(value_clipboard);}
  let mut value=clipboard.clone();value["clips"]=serde_json::json!(vec![clipboard["clips"][0].clone();4097]);malformed.push(value);
  let mut value=clipboard.clone();value["markers"]=serde_json::json!(vec![clipboard["markers"][0].clone();4097]);malformed.push(value);
  let before=SourceProjectDocument::read(&serde_json::to_vec(&case["pasteDocument"]).unwrap()).unwrap();
  for (i,clipboard) in malformed.iter().enumerate(){let mut doc=before.clone();assert!(paste_clips(&mut doc,case["sequence"].as_u64().unwrap(),clipboard,0,false,false,false).is_err(),"malformed clipboard {i}");assert_eq!(doc,before,"atomic malformed clipboard {i}");}
  let mut clipboard=clipboard.clone();clipboard["clips"][0][2]["opaque"]=serde_json::json!({"id":u64::MAX,"tick":9007199254740993_i64});clipboard["markers"][0]["opaque"]=serde_json::json!({"id":u64::MAX});
  let mut doc=before;let result=paste_clips(&mut doc,case["sequence"].as_u64().unwrap(),&clipboard,0,false,true,false).unwrap();
  let seq=&doc.envelope["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"];
  let pasted=["video_tracks","audio_tracks"].into_iter().flat_map(|key|seq[key].as_array().unwrap().iter()).flat_map(|track|track["items"].as_array().unwrap().iter()).find(|clip|clip["id"].as_u64()==Some(result.ids[0])).unwrap();assert_eq!(pasted["opaque"],clipboard["clips"][0][2]["opaque"]);
  assert!(seq["markers"].as_array().unwrap().iter().any(|marker|marker["opaque"]==clipboard["markers"][0]["opaque"]));
 }

 #[test]
 fn original_clipboard_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-clipboard.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),768);
  for (i,case) in cases.iter().enumerate(){
   let source=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let sequence=case["sequence"].as_u64().unwrap();
   let clipboard=copy_clips(&source,sequence,&[1001],case["linked"].as_bool().unwrap(),case["include"].as_bool().unwrap()).unwrap();assert_eq!(clipboard,case["clipboard"],"clipboard copy {i}");
   let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["pasteDocument"]).unwrap()).unwrap();let before=doc.clone();
   let result=paste_clips(&mut doc,sequence,&clipboard,case["time"].as_i64().unwrap(),case["insert"].as_bool().unwrap(),case["include"].as_bool().unwrap(),case["ripple"].as_bool().unwrap());
   if case["error"]==true{assert!(result.is_err(),"clipboard refusal {i}");assert_eq!(doc,before,"atomic clipboard {i}");}else{let result=result.unwrap();assert_eq!(serde_json::to_value(result.ids).unwrap(),case["selected"],"clipboard selection {i}");assert_eq!(result.playhead,case["playhead"].as_i64().unwrap(),"clipboard playhead {i}");assert_eq!(doc.envelope,case["expected"],"clipboard document {i}");}
  }
  let case=cases.iter().find(|c|c["error"]==false && !c["selected"].as_array().unwrap().is_empty()).unwrap();
  let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["pasteDocument"]).unwrap()).unwrap();doc.envelope["project"]["next_id"]=u64::MAX.into();let before=doc.clone();
  assert!(paste_clips(&mut doc,case["sequence"].as_u64().unwrap(),&case["clipboard"],case["time"].as_i64().unwrap(),false,true,true).is_err());assert_eq!(doc,before);
 }

 #[test]
 fn original_placement_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-placement.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1152);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let placements:Vec<_>=case["placements"].as_array().unwrap().iter().map(|v|(v[0].as_u64().unwrap(),v[1].clone())).collect();
   let result=place_clips(&mut doc,case["sequence"].as_u64().unwrap(),&placements,case["mode"]==1);
   if case["error"]==true{assert!(result.is_err(),"placement refusal {i}");assert_eq!(doc,before,"atomic placement {i}");}else{assert_eq!(serde_json::to_value(result.unwrap()).unwrap(),case["placed"],"placed IDs {i}");assert_eq!(doc.envelope,case["expected"],"placement document {i}");}
  }
  let case=cases.iter().find(|c|c["error"]==false && c["document"]["project"]["next_id"]!=c["expected"]["project"]["next_id"]).unwrap();
  let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();doc.envelope["project"]["next_id"]=u64::MAX.into();let before=doc.clone();
  let placements:Vec<_>=case["placements"].as_array().unwrap().iter().map(|v|(v[0].as_u64().unwrap(),v[1].clone())).collect();
  assert!(place_clips(&mut doc,case["sequence"].as_u64().unwrap(),&placements,case["mode"]==1).is_err());assert_eq!(doc,before);
 }

 #[test]
 fn original_range_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-range.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1536);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let tracks:Vec<_>=case["tracks"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap()).collect();
   let removed=remove_range(&mut doc,case["sequence"].as_u64().unwrap(),&tracks,case["time"].as_i64().unwrap(),case["rangeDuration"].as_i64().unwrap(),case["mode"]==1).unwrap();
   assert_eq!(serde_json::to_value(removed).unwrap(),case["removed"],"range removed IDs {i}");assert_eq!(doc.envelope,case["expected"],"range document {i}");
  }
  let case=cases.iter().find(|c|c["document"]["project"]["next_id"]!=c["expected"]["project"]["next_id"]).unwrap();
  let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();doc.envelope["project"]["next_id"]=u64::MAX.into();let before=doc.clone();
  let tracks:Vec<_>=case["tracks"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap()).collect();
  assert!(remove_range(&mut doc,case["sequence"].as_u64().unwrap(),&tracks,case["time"].as_i64().unwrap(),case["rangeDuration"].as_i64().unwrap(),case["mode"]==1).is_err());assert_eq!(doc,before);
 }

 #[test]
 fn original_razor_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-razor.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1920);
  for (i,case) in cases.iter().enumerate(){
   let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let ids:Vec<_>=case["ids"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();
   let tracks:Vec<_>=case["tracks"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();
   let created=razor(&mut doc,case["sequence"].as_u64().unwrap(),&tracks,if case["mode"]==2{Some(&ids)}else{None},case["time"].as_i64().unwrap()).unwrap();
   assert_eq!(serde_json::to_value(created).unwrap(),case["created"],"razor IDs {i}");assert_eq!(doc.envelope,case["expected"],"razor document {i}");
  }
  let case=cases.iter().find(|c|!c["created"].as_array().unwrap().is_empty()).unwrap();
  let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();doc.envelope["project"]["next_id"]=u64::MAX.into();let before=doc.clone();
  assert!(razor(&mut doc,case["sequence"].as_u64().unwrap(),&[],None,case["time"].as_i64().unwrap()).is_err());assert_eq!(doc,before);
 }

 use super::*;
 #[test]
 fn original_close_gap_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-close-gap.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(fixture["cases"].as_array().unwrap().len(),640);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let result=close_gap(&mut doc,case["sequence"].as_u64().unwrap(),case["track"].as_u64().unwrap(),case["time"].as_i64().unwrap());
   if case["error"]==true{assert!(result.is_err(),"gap {i}");assert_eq!(doc,before,"atomic gap {i}");}else{result.unwrap();assert_eq!(doc.envelope,case["expected"],"gap {i}");}
  }
 }
 #[test]
 fn original_delete_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-delete.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(fixture["cases"].as_array().unwrap().len(),384);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();let ids:Vec<_>=case["ids"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).collect();
   let result=delete_selection(&mut doc,case["sequence"].as_u64().unwrap(),&ids,case["ripple"].as_bool().unwrap());
   if case["error"]==true{assert!(result.is_err(),"delete {i}");assert_eq!(doc,before);}else{let result=result.unwrap();assert_eq!(result.removed,case["removed"].as_u64().unwrap() as usize,"delete {i}");assert_eq!(serde_json::to_value(result.closed_spans).unwrap(),case["closedSpans"],"delete {i}");assert_eq!(doc.envelope,case["expected"],"delete {i}");}
  }
 }
 #[test]
 fn original_slide_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-slide.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(fixture["cases"].as_array().unwrap().len(),1568);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   assert_eq!(slide(&mut doc,case["sequence"].as_u64().unwrap(),1001,case["delta"].as_i64().unwrap()).unwrap(),case["applied"].as_i64().unwrap(),"slide {i}");assert_eq!(doc.envelope,case["expected"],"slide {i}");
  }
 }
 #[test]
 fn original_roll_matches(){
  for (text,count) in [(include_str!("../../tests/fixtures/filmcraft-roll.json"),224),(include_str!("../../tests/fixtures/filmcraft-roll-right.json"),1120)] {
   let fixture:Value=serde_json::from_str(text).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(fixture["cases"].as_array().unwrap().len(),count);
   for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
    let result=roll(&mut doc,case["sequence"].as_u64().unwrap(),1001,1011,case["delta"].as_i64().unwrap());
    if case["error"]==true{assert!(result.is_err(),"roll {i}");assert_eq!(doc,before,"atomic roll {i}");}else{assert_eq!(result.unwrap(),case["applied"].as_i64().unwrap(),"roll {i}");assert_eq!(doc.envelope,case["expected"],"roll {i}");}
   }
  }
 }
 #[test]
 fn original_rate_stretch_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-rate-stretch.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  assert_eq!(fixture["cases"].as_array().unwrap().len(),448);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let result=rate_stretch(&mut doc,case["sequence"].as_u64().unwrap(),1001,if case["edge"]=="in"{Edge::In}else{Edge::Out},case["delta"].as_i64().unwrap()).unwrap();
   assert_eq!(result.to_bits(),case["applied"].as_f64().unwrap().to_bits(),"stretch {i}");assert_eq!(doc.envelope,case["expected"],"stretch {i}");
  }
 }
 #[test]
 fn original_slip_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-slip.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  assert_eq!(fixture["cases"].as_array().unwrap().len(),224);
  for (i,case) in fixture["cases"].as_array().unwrap().iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let result=slip(&mut doc,case["sequence"].as_u64().unwrap(),1001,case["delta"].as_i64().unwrap()).unwrap();
   assert_eq!(result,case["applied"].as_i64().unwrap(),"slip {i}");assert_eq!(doc.envelope,case["expected"],"slip {i}");
  }
 }
 #[test]
 fn original_ripple_group_matches(){
  for bytes in [include_str!("../../tests/fixtures/filmcraft-ripple-trim.json"),include_str!("../../tests/fixtures/filmcraft-ripple-transition.json"),include_str!("../../tests/fixtures/filmcraft-ripple-split.json")] {
  let fixture:Value=serde_json::from_str(bytes).unwrap();
  let cases=fixture["cases"].as_array().unwrap();assert!(matches!(cases.len(),896|224));
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let result=ripple_trim_linked(&mut doc,case["sequence"].as_u64().unwrap(),1001,if case["edge"]=="in"{Edge::In}else{Edge::Out},case["delta"].as_i64().unwrap(),case["linked"].as_bool().unwrap());
   if case["error"]==true {assert!(result.is_err(),"ripple {i}");assert_eq!(doc,before);}else{assert_eq!(result.unwrap(),case["applied"].as_i64().unwrap(),"ripple {i}");assert_eq!(doc.envelope,case["expected"],"ripple {i}");}
  }
 }
 }
 #[test]
 fn original_linked_regular_trim_project_matches(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-linked-trim.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),448);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let result=trim_linked(&mut doc,case["sequence"].as_u64().unwrap(),1001,if case["edge"]=="in" {Edge::In}else{Edge::Out},case["delta"].as_i64().unwrap(),case["linked"].as_bool().unwrap());
   if case["error"].as_bool()==Some(true){assert!(result.is_err(),"linked {i}");assert_eq!(doc,before);}else{assert_eq!(result.unwrap(),case["applied"].as_i64().unwrap(),"linked {i}");assert_eq!(doc.envelope,case["expected"],"linked {i}");}
  }
 }
 #[test]
 fn original_project_trim_media_context(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-trim-context.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  assert_eq!(fixture["cases"].as_array().unwrap().len(),216);
  for case in fixture["cases"].as_array().unwrap(){
   let doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
   let context=project_context(&doc,case["sequence"].as_u64().unwrap(),case["item"].as_u64().unwrap()).unwrap();
   assert_eq!(context.media_start,case["start"].as_i64().unwrap());assert_eq!(context.media_end,case["end"].as_i64());assert_eq!(context.min_duration,case["minimum"].as_i64().unwrap());
  }
 }
 #[test]
 fn original_regular_trim_handles_and_atomicity(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-regular-trim.json")).unwrap();
  assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1344);
  for (i,case) in cases.iter().enumerate(){let mut doc=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let before=doc.clone();
   let result=regular_trim(&mut doc,case["sequence"].as_u64().unwrap(),12,if case["edge"]=="in"{Edge::In}else{Edge::Out},case["delta"].as_i64().unwrap(),&TrimContext{media_start:case["media_start"].as_i64().unwrap(),media_end:case["media_end"].as_i64(),min_duration:case["minimum"].as_i64().unwrap()});
   if case["error"].as_bool()==Some(true){assert!(result.is_err(),"case {i}");assert_eq!(doc,before);}else{assert_eq!(result.unwrap(),case["applied"].as_i64().unwrap(),"case {i}");assert_eq!(doc.envelope["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["audio_tracks"][0]["items"][1],case["clip"],"case {i}");let mut expected=before;expected.envelope["project"]["items"][case["sequence"].as_u64().unwrap().to_string()]["kind"]["Sequence"]["audio_tracks"][0]["items"][1]=case["clip"].clone();assert_eq!(doc,expected,"preservation {i}");}
  }
 }
}
