use filmcraft_project::{Project,ItemId,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,ClipId};
use filmcraft_time::{Tick,TimeRange,FrameRate};use filmcraft_media::{Generator,MediaSource,generators::GeneratorSource};use filmcraft_edit::{EditCtx,Edge};use serde_json::json;
mod original {use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
const T:i64=254016000000;
fn clip(id:u64,item:ItemId,start:i64,duration:i64,link:Option<u64>)->TrackItem {serde_json::from_value(json!({"id":id,"item":item,"name":"Linked","label":"Iris","start":start,"duration":duration,"source_in":T,"speed":1.,"reverse":false,"enabled":true,"effects":[],"link":link})).unwrap()}
use filmcraft_project::{TrackKind,Sequence,Marker,MarkerId};use serde_json::Value;type Result<T>=anyhow::Result<T>;use filmcraft_edit as edit;
#[derive(Clone)]struct State{selection:Vec<ClipId>,linked_selection:bool,copy_paste_sequence_markers:bool,ripple_sequence_markers:bool,clipboard:Vec<(TrackKind,usize,TrackItem)>,clipboard_markers:Vec<Marker>}
struct Session{project:Project,state:State,seq:ItemId,at:Tick}
impl Session{fn active_sequence(&self)->Option<&Sequence>{self.project.sequence(self.seq)}fn playhead(&self)->Tick{self.at}fn set_playhead(&mut self,t:Tick){self.at=t}
fn edit_sequence<R>(&mut self,_label:&str,f:impl FnOnce(&mut Sequence,&mut EditCtx,&mut State)->Result<R>)->Result<R>{let duration=|_|None;let start=|_|Tick::ZERO;let min=self.project.sequence(self.seq).unwrap().settings.frame_rate.frame_duration();let mut next=self.project.next_id;let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:min};let result=f(self.project.sequence_mut(self.seq).unwrap(),&mut ctx,&mut self.state);self.project.next_id=next;result}}
mod sequence_tools{use filmcraft_project::Marker;use filmcraft_time::Tick;__RIPPLE__}
__WITH_LINKS__
__COPY_PASTE__
fn main(){let mut cases=vec![];
for rate in [FrameRate::new(24,1),FrameRate::new(24000,1001)] {for edge in [Edge::In] {for delta in [T/2,3*T,5*T] {for reverse in [false,true] {for linked in [false,true] {for scenario in 0..4 {for mode in 0..2 {for include in [false,true] {for ripple in [false,true] {
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
{let q=p.sequence_mut(seq).unwrap();for (i,time) in [0,T,2*T,3*T,4*T,6*T].into_iter().enumerate(){q.markers.push(filmcraft_project::Marker{id:filmcraft_project::MarkerId(7000+i as u64),start:Tick(time),duration:Tick(T/8),name:format!("Marker{i}"),comment:"Preserve".into(),kind:Default::default(),color:Label::Iris});}}
let before=p.clone();let mut session=Session{project:p,state:State{selection:vec![ClipId(1001)],linked_selection:linked,copy_paste_sequence_markers:include,ripple_sequence_markers:ripple,clipboard:vec![],clipboard_markers:vec![]},seq,at:Tick(delta)};
copy_selection(&mut session);let clipboard=json!({"format":"video-kadr.filmcraft-clipboard","version":1,"clips":session.state.clipboard,"markers":session.state.clipboard_markers});
if scenario==1{session.project.sequence_mut(seq).unwrap().audio_tracks.truncate(1);}if scenario==3{let q=session.project.sequence_mut(seq).unwrap();q.video_tracks.clear();q.audio_tracks.clear();}
let paste_before=session.project.clone();let result=paste(&mut session,mode==1);let error=result.is_err();let (expected,selected,playhead)=if error{(paste_before.clone(),vec![],Tick(delta))}else{(session.project,session.state.selection,session.at)};
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":before,"opaque":{"id":18446744073709551615u64}},"pasteDocument":{"format":"filmcraft.project","schema_version":12,"project":paste_before,"opaque":{"id":18446744073709551615u64}},"expected":{"format":"filmcraft.project","schema_version":12,"project":expected,"opaque":{"id":18446744073709551615u64}},"sequence":seq,"clipboard":clipboard,"linked":linked,"include":include,"ripple":ripple,"insert":mode==1,"time":delta,"error":error,"selected":selected,"playhead":playhead}));
}}}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
