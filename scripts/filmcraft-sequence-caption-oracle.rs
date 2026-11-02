use filmcraft_project::{Project,SequenceSettings,CaptionTrack,CaptionFormat};
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
for origin in [-254016000000i64,0,17,9007199254740993]{for duration in [-17i64,0,254016000000]{for enabled in [false,true]{for empty in [false,true]{
let mut p=Project::new("Captions");let id=p.new_sequence("Sequence",SequenceSettings::default(),1,1,None);
let mut track=CaptionTrack::new(filmcraft_project::TrackId(100),"Captions".into(),CaptionFormat::default());track.enabled=enabled;
if !empty {for (cid,start,length) in [(101,origin,duration),(102,origin+1,duration)]{track.captions.push(serde_json::from_value(json!({"id":cid,"start":start,"duration":length,"text":"<v Name>Привет &amp; world</v>\n第二行","speaker":"Name","cue_id":"cue","settings":"line:90% align:start"})).unwrap());}}
p.sequence_mut(id).unwrap().caption_tracks.push(track);
let seq=p.sequence(id).unwrap();let at=[origin-1,origin,origin+1,origin+duration,origin+duration+1];
let lookup:Vec<_>=at.iter().map(|at|seq.caption_tracks[0].caption_at(Tick(*at)).map(|c|c.id)).collect();
let end=seq.duration().0;
cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":p},"id":id,"duration":end,"at":at,"lookup":lookup}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
