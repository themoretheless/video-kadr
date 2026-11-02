use filmcraft_project::{Sequence,Project,ItemId,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,ClipId};
use filmcraft_time::{Tick,TimeRange,FrameRate};use filmcraft_project::{TrackKind,TrackId};use serde_json::Value;use filmcraft_media::{Generator,MediaSource,generators::GeneratorSource};use filmcraft_edit::{EditCtx,Edge};use serde_json::json;
mod original {use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
const T:i64=254016000000;
fn clip(id:u64,item:ItemId,start:i64,duration:i64,link:Option<u64>)->TrackItem {serde_json::from_value(json!({"id":id,"item":item,"name":"Linked","label":"Iris","start":start,"duration":duration,"source_in":T,"speed":1.,"reverse":false,"enabled":true,"effects":[],"link":link})).unwrap()}
fn plan(q:&filmcraft_project::Sequence,rate:FrameRate,kind:filmcraft_project::TrackKind,targeted:Vec<filmcraft_project::TrackId>,clip:Option<ClipId>,edge:Option<String>,t:Tick,dur:Tick)->Option<serde_json::Value>{
__PLAN__
}
#[derive(Debug)]enum EngineError{NoSequence,Other(String)}
impl std::fmt::Display for EngineError{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"{self:?}")}}
type Result<T>=std::result::Result<T,EngineError>;
#[derive(Clone)]struct EditPoint{clip:ClipId,out:bool}
struct State{selection:Vec<ClipId>,edit_points:Vec<EditPoint>}struct History{undo:Vec<()>}
struct Session{project:Project,sequence:ItemId,state:State,history:History,frames:i64}
impl Session{
 fn active_sequence(&self)->Option<&filmcraft_project::Sequence>{self.project.sequence(self.sequence)}
 fn execute(&mut self,cmd:&str,p:serde_json::Value)->Result<serde_json::Value>{
  let q=self.active_sequence().ok_or(EngineError::NoSequence)?;let rate=q.settings.frame_rate;let kind=if cmd=="sequence.applyVideoTransition"{filmcraft_project::TrackKind::Video}else{filmcraft_project::TrackKind::Audio};let dur=rate.tick_of(self.frames);let selected=plan(q,rate,kind,vec![],Some(ClipId(p["clip"].as_u64().unwrap())),Some(p["edge"].as_str().unwrap().into()),Tick(0),dur).ok_or_else(||EngineError::Other("no target".into()))?;
  let effect=if kind==filmcraft_project::TrackKind::Video{"cross_dissolve"}else{"constant_power"};let transition=filmcraft_project::Transition{id:filmcraft_project::TransitionId(0),effect:filmcraft_project::find_effect(effect).unwrap().instance(),start:Tick(selected["start"].as_i64().unwrap()),duration:dur,from:selected["from"].as_u64().map(ClipId),to:selected["to"].as_u64().map(ClipId),align:Default::default(),reverse:false};
  let mut work=self.project.clone();let mut next=work.next_id;let md=|_|None;let ms=|_|Tick(0);let mut ctx=EditCtx{next_id:&mut next,media_duration:&md,media_start:&ms,min_duration:rate.frame_duration()};let id=filmcraft_edit::add_transition(work.sequence_mut(self.sequence).unwrap(),filmcraft_project::TrackId(selected["track"].as_u64().unwrap()),transition,&mut ctx).map_err(|e|EngineError::Other(e.to_string()))?;work.next_id=next;self.project=work;self.history.undo.push(());Ok(json!({"transition":id}))
 }
}
mod trim{use super::*;
__TRIM__
}
mod clip_ops{pub fn collapse_history(s:&mut super::Session,n:usize,_:&str){if s.history.undo.len()>n{s.history.undo.truncate(n+1);}}}
__BATCH__
__CONTEXT_HELPERS__
fn main(){let mut cases=vec![];
for rate in [FrameRate::new(24,1),FrameRate::new(24000,1001)] {for edge in [Edge::In] {for delta in [-1,0,T/2] {for reverse in [false,true] {for linked in [false] {for scenario in 0..4 {for mode in 0..6 {for range_duration in [0,7,30] {
let track_variant=scenario/8;let scenario=scenario%8;
let sync=scenario>=2;let scenario=scenario%4;
let mut p=Project::new("Linked trim");let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,rate,Tick(8*T));let media=p.add_item("Tone",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let restricted=p.add_item("Restricted",Label::Iris,ItemKind::Subclip{parent:media,range:TimeRange::new(Tick(T/2),Tick(3*T/2)),restrict_trims:true},None);
let seq=p.new_sequence("Sequence",SequenceSettings{frame_rate:rate,..Default::default()},1,2,None);
{let q=p.sequence_mut(seq).unwrap();let mut selected=clip(1001,media,2*T,2*T,Some(777));selected.reverse=reverse;q.video_tracks[0].items=vec![clip(1010,media,0,T,None),selected];
for side in 0..2 {let mut partner=clip(1002+side as u64,restricted,5*T/2,5*T/4,Some(777));partner.reverse=reverse;partner.speed=if scenario==1{1.25}else if scenario==3{0.}else{1.};if scenario==2{partner.frame_hold=Some(Tick(T));}q.audio_tracks[side].sync_lock=sync;q.audio_tracks[side].locked=scenario==2 && side==0;q.audio_tracks[side].items=vec![clip(1020+side as u64,media,0,9*T/4,None),partner,clip(1030+side as u64,media,4*T,T,None)];}}
{let q=p.sequence_mut(seq).unwrap();for track in q.all_tracks_mut(){let first=track.items.first().unwrap();let last=track.items.last().unwrap();let make=|id,start,from,to|filmcraft_project::Transition{id:filmcraft_project::TransitionId(id),effect:filmcraft_project::find_effect("cross_dissolve").unwrap().instance(),start:Tick(start),duration:Tick(T/4),from,to,align:Default::default(),reverse:false};track.transitions=vec![make(8001,first.start.0,None,Some(first.id)),make(8002,last.start.0-T/8,Some(first.id),Some(last.id)),make(8003,last.end().0-T/4,Some(last.id),None)];}}
{let q=p.sequence_mut(seq).unwrap();for side in 0..3 {let mut track=filmcraft_project::CaptionTrack::new(filmcraft_project::TrackId(900+side),format!("Captions{side}"),Default::default());track.locked=side==1;track.sync_lock=side!=2;
for (i,(start,duration)) in [(0,T/4),(T/2,T),(2*T,2*T),(9*T/2,T)].into_iter().enumerate(){track.captions.push(filmcraft_project::Caption{id:ClipId(5000+side*10+i as u64),start:Tick(start),duration:Tick(duration),text:format!("Cue{i}"),speaker:Some("Speaker".into()),cue_id:Some(format!("cue-{i}")),settings:"position:20%".into()});}q.caption_tracks.push(track);}}
{let q=p.sequence_mut(seq).unwrap();q.video_tracks[0].locked=scenario==1||scenario==3;q.audio_tracks[0].locked=scenario==2||scenario==3;q.audio_tracks[1].locked=scenario==3;if delta!=0{for tr in q.all_tracks_mut(){tr.transitions.clear();}}if delta==T/2{q.video_tracks[0].items[0].duration=Tick(2*T);}}
let clips:Vec<u64>=match mode{0=>vec![1001],1=>vec![1002,1003],2=>vec![1001,1002,1003],3=>vec![1010,1001,1001],4=>vec![9999],_=>vec![]};
let contexts=clips.iter().enumerate().map(|(i,id)|{let ep=EditPoint{clip:ClipId(*id),out:i%2==1};let q=p.sequence(seq).unwrap();let pair=roll_pair(q,&ep);let (outgoing,incoming)=sides(q,&ep);json!({"editTime":edit_time(q,&ep),"outgoing":outgoing,"incoming":incoming,"rollPair":pair})}).collect::<Vec<_>>();let before=p.clone();let mut session=Session{project:p,sequence:seq,state:State{selection:vec![],edit_points:clips.iter().enumerate().map(|(i,id)|EditPoint{clip:ClipId(*id),out:i%2==1}).collect()},history:History{undo:vec![]},frames:range_duration};let result=trim::apply_default_transitions(&mut session);let error=result.is_err();
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":before},"expected":{"format":"filmcraft.project","schema_version":12,"project":session.project},"sequence":seq,"contexts":contexts,"points":clips.iter().enumerate().map(|(i,id)|json!({"clip":id,"out":i%2==1})).collect::<Vec<_>>(),"frames":range_duration,"error":error,"result":result.ok()}));
}}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
