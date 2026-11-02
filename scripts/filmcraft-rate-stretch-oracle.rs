use filmcraft_project::{Project,ItemId,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,ClipId};
use filmcraft_time::{Tick,TimeRange,FrameRate};use filmcraft_media::{Generator,MediaSource,generators::GeneratorSource};use filmcraft_edit::{EditCtx,Edge};use serde_json::json;
mod original {use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
const T:i64=254016000000;
fn clip(id:u64,item:ItemId,start:i64,duration:i64,link:Option<u64>)->TrackItem {serde_json::from_value(json!({"id":id,"item":item,"name":"Linked","label":"Iris","start":start,"duration":duration,"source_in":T,"speed":1.,"reverse":false,"enabled":true,"effects":[],"link":link})).unwrap()}
fn main(){let mut cases=vec![];
for rate in [FrameRate::new(24,1),FrameRate::new(24000,1001)] {for edge in [Edge::In,Edge::Out] {for delta in [-3*T,-T,-1,0,1,T,3*T] {for reverse in [false,true] {for linked in [false,true] {for scenario in 0..4 {
let sync=scenario>=4;let scenario=scenario%4;
let mut p=Project::new("Linked trim");let generator=Generator::Tone{hz:440.,db:-6.};let base=GeneratorSource::new(generator.clone(),4,4,rate,Tick(8*T));let media=p.add_item("Tone",Label::Iris,ItemKind::Media(MediaClip{media:MediaRef::Generator(generator),info:base.info().clone(),interpret:Default::default(),mark_in:None,mark_out:None,markers:vec![],offline:false,proxy:None,identity:None}),None);
let restricted=p.add_item("Restricted",Label::Iris,ItemKind::Subclip{parent:media,range:TimeRange::new(Tick(T/2),Tick(3*T/2)),restrict_trims:true},None);
let seq=p.new_sequence("Sequence",SequenceSettings{frame_rate:rate,..Default::default()},1,2,None);
{let q=p.sequence_mut(seq).unwrap();let mut selected=clip(1001,media,2*T,2*T,Some(777));selected.reverse=reverse;selected.item=if linked{restricted}else{media};selected.speed=if scenario==1{1.25}else if scenario==3{0.}else{1.};if scenario==2{selected.frame_hold=Some(Tick(T));}q.video_tracks[0].items=vec![clip(1010,media,0,T,None),selected,clip(1011,media,5*T,T,None)];
for side in 0..2 {let mut partner=clip(1002+side as u64,restricted,5*T/2,5*T/4,Some(777));partner.reverse=reverse;partner.speed=if scenario==1{1.25}else if scenario==3{0.}else{1.};if scenario==2{partner.frame_hold=Some(Tick(T));}q.audio_tracks[side].sync_lock=sync;q.audio_tracks[side].locked=scenario==2 && side==0;q.audio_tracks[side].items=vec![clip(1020+side as u64,media,0,9*T/4,None),partner,clip(1030+side as u64,media,4*T,T,None)];}}
let before=p.clone();let mut ids=vec![ClipId(1001)];if linked{let q=p.sequence(seq).unwrap();for track in q.all_tracks(){for item in &track.items{if item.link==Some(777)&&!ids.contains(&item.id){ids.push(item.id);}}}}
let mut work=p.clone();let duration=|id|original::media_duration(&before,&(),id);let start=|id|original::media_start(&before,id);let mut next=before.next_id;let mut ctx=EditCtx{next_id:&mut next,media_duration:&duration,media_start:&start,min_duration:rate.frame_duration()};
let result=filmcraft_edit::rate_stretch(work.sequence_mut(seq).unwrap(),ClipId(1001),edge,Tick(delta),&mut ctx);
if result.is_ok(){p=work;}
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":before,"opaque":{"id":18446744073709551615u64}},"expected":{"format":"filmcraft.project","schema_version":12,"project":p,"opaque":{"id":18446744073709551615u64}},"sequence":seq,"edge":if edge==Edge::In{"in"}else{"out"},"delta":delta,"linked":linked,"error":result.is_err(),"applied":result.ok()}));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
