use filmcraft_project::{Project,SequenceSettings,AudioChannels,TrackId};
use serde_json::json;
fn main(){let layouts=[AudioChannels::Mono,AudioChannels::Stereo,AudioChannels::Surround51,AudioChannels::Adaptive];let mut cases=vec![];
for master in layouts {for bus in layouts {for output in [None,Some(100u64),Some(101),Some(999),Some(1),Some(u64::MAX)] {
let mut p=Project::new("Routing");let id=p.new_sequence("Sequence",SequenceSettings{audio_master:master,..Default::default()},1,1,None);
let seq=p.sequence_mut(id).unwrap();seq.audio_tracks[0].id=TrackId(1);seq.video_tracks[0].id=TrackId(2);
seq.audio_tracks[0].mixer.output=output.map(TrackId);
let mut a=seq.audio_tracks[0].clone();a.id=TrackId(100);a.name="Submix A".into();a.channels=bus;a.mixer.output=Some(TrackId(101));
let mut b=a.clone();b.id=TrackId(101);b.name="Submix B".into();b.channels=AudioChannels::Surround51;b.mixer.output=Some(TrackId(100));seq.submix_tracks=vec![a,b];
let strips=seq.strip_ids();let queries:Vec<_>=[1,2,100,101,999,u64::MAX].into_iter().map(|strip|json!({"id":strip,"channels":seq.output_channels(TrackId(strip)),"pans51":seq.pans_51(TrackId(strip))})).collect();
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"id":id,"strips":strips,"queries":queries}));
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
