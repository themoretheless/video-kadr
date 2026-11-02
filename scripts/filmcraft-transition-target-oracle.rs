use filmcraft_project::{Project,ItemId,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,ClipId};
use filmcraft_time::{Tick,TimeRange,FrameRate};use filmcraft_media::{Generator,MediaSource,generators::GeneratorSource};use filmcraft_edit::{EditCtx,Edge};use serde_json::json;
mod original {use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
const T:i64=254016000000;
fn clip(id:u64,item:ItemId,start:i64,duration:i64,link:Option<u64>)->TrackItem {serde_json::from_value(json!({"id":id,"item":item,"name":"Linked","label":"Iris","start":start,"duration":duration,"source_in":T,"speed":1.,"reverse":false,"enabled":true,"effects":[],"link":link})).unwrap()}
fn plan(q:&filmcraft_project::Sequence,rate:FrameRate,kind:filmcraft_project::TrackKind,targeted:Vec<filmcraft_project::TrackId>,clip:Option<ClipId>,edge:Option<String>,t:Tick,dur:Tick)->Option<serde_json::Value>{
__PLAN__
}
fn main(){let mut cases=vec![];
for rate in [FrameRate::new(24,1),FrameRate::new(24000,1001)] {for edge in [Edge::In] {for delta in [-1,0,T/2,2*T-rate.tick_of(3).0,2*T-rate.tick_of(3).0+1,2*T,2*T+rate.tick_of(3).0-1,2*T+rate.tick_of(3).0,3*T,5*T] {for reverse in [false,true] {for linked in [false] {for scenario in 0..4 {for mode in 0..4 {for range_duration in [0,T/4+1,T] {
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
if scenario==2{let q=p.sequence_mut(seq).unwrap();q.video_tracks[0].items[0].duration=Tick(2*T);for track in &mut q.audio_tracks{track.items[1].start=Tick(9*T/4);track.items[1].duration=Tick(7*T/4);}}
let q=p.sequence(seq).unwrap();let kind=if scenario==1{filmcraft_project::TrackKind::Audio}else{filmcraft_project::TrackKind::Video};
let targeted=if scenario==3{vec![]}else{q.tracks(kind).iter().map(|t|t.id).collect()};
let selected=if mode==0{None}else{Some(ClipId(if mode==3{9999}else if scenario==1{1002}else{1001}))};
let edge=match mode{1=>Some("in".to_string()),2=>Some("out".to_string()),_=>None};
let expected=plan(q,rate,kind,targeted.clone(),selected,edge.clone(),Tick(delta),Tick(range_duration));
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"sequence":seq,"kind":if kind==filmcraft_project::TrackKind::Video{"video_tracks"}else{"audio_tracks"},"targeted":targeted,"clip":selected,"edge":edge,"time":delta,"duration":range_duration,"expected":expected}));

}}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
