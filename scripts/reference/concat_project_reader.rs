//! Run the pinned Concat reader and writer against a generated export; never opens media paths.
use serde_json::Value;
fn main(){
 let args:Vec<String>=std::env::args().collect();let document:Value=serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
 let project=concat_project::from_document(&document).expect("Original Concat reader rejected export");
 let expected=document["timelines"].as_array().unwrap();assert_eq!(project.timelines.len(),expected.len());
 for timeline in expected{
  let actual=project.timelines.iter().find(|actual|actual.id==timeline["id"].as_str().unwrap()).unwrap();
  let clips=timeline["clips"].as_array().unwrap();assert_eq!(actual.clips.len(),clips.len(),"Original reader dropped a clip");
  for clip in clips{let actual=actual.clips.iter().find(|actual|actual.id==clip["id"].as_str().unwrap()).unwrap();assert_eq!(actual.track_id,clip["trackId"].as_str().unwrap());assert_eq!(actual.start,clip["start"].as_f64().unwrap());assert_eq!(actual.duration,clip["duration"].as_f64().unwrap());}
 }
 let video=&expected[0]["video"];let settings=concat_project::DocumentSettings{name:"Export reader qualification".into(),width:video["width"].as_u64().unwrap() as u32,height:video["height"].as_u64().unwrap() as u32,rate_num:video["rateNum"].as_i64().unwrap(),rate_den:video["rateDen"].as_i64().unwrap()};
 let rewritten=concat_project::to_document(&settings,&project);std::fs::write(&args[2],serde_json::to_vec(&rewritten).unwrap()).unwrap();println!("Original Concat read/write passed: {} timelines, {} clips",project.timelines.len(),project.timelines.iter().map(|timeline|timeline.clips.len()).sum::<usize>());
}
