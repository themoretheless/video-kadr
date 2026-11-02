use filmcraft_project::{Project,ItemId};
use filmcraft_time::FrameRate;
use serde_json::json;
mod original { use filmcraft_project::{Project,ItemId};use filmcraft_time::Tick;type MediaPool=();const MAX_SUBCLIP_CHAIN:usize=16;
__CONTEXT__
}
fn main(){let fixture:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for rate in [FrameRate::new(24,1),FrameRate::new(24000,1001),FrameRate{num:0,den:1},FrameRate::new(30,1)]{for caption_long in [false,true]{
 let mut project:Project=serde_json::from_value(fixture["document"]["project"].clone()).unwrap();project.sequence_mut(ItemId(5)).unwrap().settings.frame_rate=rate;
 let sequence=project.sequence_mut(ItemId(5)).unwrap();
 sequence.audio_tracks[0].items.push(serde_json::from_value(json!({"id":999,"item":1,"name":"Media","label":"Iris","start":9007199254740993i64,"duration":254016000000i64,"source_in":0,"speed":1.,"reverse":false,"enabled":true,"effects":[]})).unwrap());
 sequence.caption_tracks.push(serde_json::from_value(json!({"id":1000,"name":"Captions","format":"Subtitle","enabled":true,"locked":false,"captions":[{"id":1001,"start":9007199254740993i64,"duration":if caption_long {762048000000i64}else{127008000000i64},"text":"Caption"}]})).unwrap());
 for item in 1..=27 {cases.push(json!({"document":{"format":"filmcraft.project","schema_version":12,"project":project},"sequence":5,"item":item,"start":original::media_start(&project,ItemId(item)),"end":original::media_duration(&project,&(),ItemId(item)),"minimum":rate.frame_duration()}));}
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
