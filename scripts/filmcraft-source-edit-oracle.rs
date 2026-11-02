// Independent expected Insert/Overwrite behavior from pinned MIT Session.execute.
use filmcraft_project::*;
use filmcraft_time::{Tick,TimeRange};
use serde_json::json;
use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;let sequence=s.state.active_sequence.unwrap();
 let p=std::sync::Arc::make_mut(&mut s.project);let media=p.items.values().find(|i|i.as_media().is_some_and(|m|m.info.has_audio()&&m.info.has_video())).unwrap().clone();
 if let Some(m)=p.item_mut(media.id).and_then(|i|i.as_media_mut()){m.mark_in=Some(Tick(254016000000));m.mark_out=Some(Tick(762048000000));if let Some(v)=&mut m.info.video{v.width=640;v.height=360;}}
 p.add_item("Subclip",media.label,ItemKind::Subclip{parent:media.id,range:TimeRange::new(Tick(127008000000),Tick(508032000000)),restrict_trims:true},None);
 let graphic=p.add_item("Graphic",media.label,ItemKind::Graphic{width:320,height:240,rate:filmcraft_time::FrameRate::FPS_25},None);
 p.source_graphics.insert(graphic,SourceGraphic{layers:vec![find_effect("simple_text").unwrap().instance()],meta:None});
 p.add_item("Adjustment",media.label,ItemKind::AdjustmentLayer{width:800,height:600,rate:filmcraft_time::FrameRate::FPS_25,duration:Tick(508032000000)},None);
 let source_seq=p.add_item("Source sequence",media.label,p.item(sequence).unwrap().kind.clone(),None);
 let mut still=media.kind.clone();if let ItemKind::Media(m)=&mut still{m.info.kind=filmcraft_media::MediaKind::Still;m.info.duration=Tick(0);m.mark_in=None;m.mark_out=None;}p.add_item("Still",media.label,still,None);
 let base=s.project.clone();let mut profiles=vec![];let mut cases=vec![];let mut sources=vec![None,Some(ItemId(u64::MAX)),Some(media.id),Some(sequence),Some(source_seq),Some(graphic)];
 sources.extend(base.items.values().filter(|i|matches!(i.kind,ItemKind::Subclip{..}|ItemKind::AdjustmentLayer{..})||i.as_media().is_some_and(|m|m.info.kind==filmcraft_media::MediaKind::Still||!m.info.has_video()||!m.info.has_audio())).map(|i|Some(i.id)));
 for profile in 0..12{
  s.project=base.clone();let p=std::sync::Arc::make_mut(&mut s.project);let rate=p.sequence(sequence).unwrap().settings.frame_rate;let fd=rate.frame_duration();
  if profile==1{p.sequence_mut(sequence).unwrap().mark_in=Some(rate.tick_of(48));}
  if profile==2||profile==3{for item in p.items.values_mut(){item.split=if profile==2{SplitMarks{video_in:Some(Tick(0)),video_out:Some(rate.tick_of(9)),audio_in:Some(rate.tick_of(3)),audio_out:Some(rate.tick_of(12))}}else{SplitMarks{video_in:Some(rate.tick_of(2)),video_out:Some(rate.tick_of(10)),audio_in:Some(rate.tick_of(2)),audio_out:Some(rate.tick_of(10))}};}}
  if profile==4{p.item_mut(media.id).unwrap().as_media_mut().unwrap().interpret.audio_channels=Some(AudioChannelMap{format:AudioChannels::Mono,clips:vec![vec![0],vec![1],vec![0,1]]});}
  if profile==6{p.sequence_mut(sequence).unwrap().video_tracks[0].locked=true;}
  if profile==7{
   let next=p.alloc_id();let q=p.sequence_mut(source_seq).unwrap();let clip=&mut q.video_tracks[0].items[0];clip.reverse=true;clip.speed=1.5;
   let from=clip.id;let at=clip.start;q.video_tracks[0].transitions.push(Transition{id:TransitionId(next),effect:find_effect("opacity").unwrap().instance(),start:at,duration:fd,from:Some(from),to:None,align:TransitionAlign::default(),reverse:true});
  }
  if profile==8{p.sequence_mut(sequence).unwrap().video_tracks[0].items[0].item=sequence;}
  if profile==9{let q=p.sequence_mut(source_seq).unwrap();q.multicam=Some(MulticamSource{cameras:vec![Camera{name:"Audio only".into(),video_track:None,audio_tracks:vec![],enabled:true,source:None},Camera{name:"Hidden first angle".into(),video_track:Some(q.video_tracks[0].id),audio_tracks:vec![],enabled:false,source:None}],..Default::default()});}
  if profile==10{p.source_graphics.get_mut(&graphic).unwrap().meta=Some(GraphicMeta{intro:Tick(33),outro:Tick(44),design_in:Tick(55),design_duration:Tick(1000),..Default::default()});let clip=&mut p.sequence_mut(source_seq).unwrap().video_tracks[0].items[0];clip.frame_hold=Some(Tick(77));clip.hold_filters=true;}
  if profile==11{let tid=TrackId(p.alloc_id());let cid=ClipId(p.alloc_id());let mut track=CaptionTrack::new(tid,"Captions".into(),CaptionFormat::default());track.captions.push(Caption{id:cid,start:rate.tick_of(24),duration:rate.tick_of(240),text:"Cross the edit".into(),speaker:Some("Narrator".into()),cue_id:Some("cue".into()),settings:String::new()});p.sequence_mut(sequence).unwrap().caption_tracks.push(track);}
  let project=s.project.clone();profiles.push(serde_json::to_value(project.as_ref())?);
  for source in &sources{for patch in 0..7{for as_clips in [false,true]{for command in ["source.insert","source.overwrite"]{
   s.project=project.clone();s.history.undo.clear();s.history.redo.clear();s.state.active_sequence=Some(sequence);s.state.source_item=*source;s.state.source_playhead=Tick(333);s.state.playheads.clear();s.state.playheads.insert(sequence,rate.tick_of(120));s.state.selection=vec![];s.state.selection_follows_playhead=profile==4;s.state.linked_selection=profile!=4;s.state.sequences_as_clips=as_clips;s.state.ripple_sequence_markers=profile%2==1;
   s.prefs.media.default_media_scaling=if profile%3==0{"none"}else if profile%3==1{"setToFrameSize"}else{"scaleToFrameSize"}.into();s.prefs.timeline.still_image_duration=if profile==5{17.5}else{5.};s.prefs.timeline.still_image_unit=if profile==5{"frames"}else{"seconds"}.into();
   let q=project.sequence(sequence).unwrap();let video=match patch{1|3=>None,4=>Some(TrackId(u64::MAX)),6=>q.audio_tracks.first().map(|t|t.id),5=>q.video_tracks.get(1).map(|t|t.id),_=>q.video_tracks.first().map(|t|t.id)};let audio=match patch{2|3=>None,5=>q.audio_tracks.get(1).map(|t|t.id),_=>q.audio_tracks.first().map(|t|t.id)};
   let targeted=if profile==4{vec![]}else{q.all_tracks().map(|t|t.id).collect::<Vec<_>>()};s.state.targeting.clear();s.state.targeting.insert(sequence,filmcraft_engine::Targeting{targeted:targeted.clone(),video_dest:video,audio_dest:audio});
   let context=json!({"activeSequence":sequence,"sourceItem":source,"sourcePlayhead":333,"playhead":rate.tick_of(120),"targeting":{"videoDest":video,"audioDest":audio},"sequencesAsClips":as_clips,"rippleSequenceMarkers":s.state.ripple_sequence_markers,"selectionFollowsPlayhead":s.state.selection_follows_playhead,"linkedSelection":s.state.linked_selection,"targetedTrackIds":targeted,"defaultMediaScaling":s.prefs.media.default_media_scaling,"stillImageDuration":s.prefs.timeline.still_image_duration,"stillImageUnit":s.prefs.timeline.still_image_unit});
   let result=s.execute(command,json!({}));let after=serde_json::to_value(s.project.as_ref())?;
   cases.push(json!({"profile":profile,"command":command,"context":context,"ok":result.is_ok(),"result":result.ok(),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"afterContext":{"activeSequence":s.state.active_sequence,"sourceItem":s.state.source_item,"sourcePlayhead":s.state.source_playhead,"playhead":s.playhead(),"playheadFrames":s.sequence_rate().frame_at(s.playhead()),"selection":s.state.selection},"undo":s.history.undo.len()}));
  }}}}
 }
 std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","profiles":profiles,"cases":cases}))?)?;Ok(())
}
