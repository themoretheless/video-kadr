// Original full scene: one animated Float control per eligible effect, source/effect/local clocks, and Hold Filters.
use filmcraft_project::{Project,ItemKind,MediaClip,MediaRef,Label,SequenceSettings,TrackItem,Transition};
use filmcraft_media::{Generator,MediaSource,SharedSource,generators::GeneratorSource};
use filmcraft_time::{Tick,FrameRate};
use filmcraft_render::{render_sequence,RenderOptions};
use serde_json::json;
use std::sync::Arc;
const T:i64=254016000000;
fn main(){
 let input:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
 let generator_fixture:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(2).unwrap()).unwrap()).unwrap();let mut generators=vec![];for c in generator_fixture["cases"].as_array().unwrap(){let g=&c["generator"];if g.get("Tone").is_none()&&!generators.contains(g){generators.push(g.clone());}}assert_eq!(generators.len(),12);
 for (i,case) in input["cases"].as_array().unwrap().iter().enumerate(){if i%8!=4{continue} for generator in &generators{for interpolation in ["frameBlending","opticalFlow"]{
  let mut document=case["document"].clone();
  for item in document["project"]["items"].as_object_mut().unwrap().values_mut(){
   if item.pointer("/kind/Media").is_some(){item["kind"]["Media"]["media"]=json!({"Generator":generator});}
   if let Some(video)=item.pointer_mut("/kind/Media/info/video"){video["width"]=json!(4);video["height"]=json!(4);video["has_alpha"]=json!(true);}
   if let Some(sequence)=item.pointer_mut("/kind/Sequence"){sequence["settings"]["width"]=json!(8);sequence["settings"]["height"]=json!(4);
    for track in sequence["video_tracks"].as_array_mut().unwrap(){for clip in track["items"].as_array_mut().unwrap(){clip["time_interpolation"]=json!(interpolation);clip["source_in"]=json!(clip["source_in"].as_i64().unwrap()+T/101);}}}
  }
  let at=case["at"].as_i64().unwrap()+T/97;
  let p:Project=serde_json::from_value(document["project"].clone()).unwrap();
  let source=|id:filmcraft_project::ItemId|{let(_,media,_)=p.resolve_media(id)?;let MediaRef::Generator(generator)=&media.media else{return None};Some(Arc::new(GeneratorSource::new(generator.clone(),4,4,filmcraft_time::FrameRate::new(24,1),Tick(100*T)))as SharedSource)};
  for scale in [1.,0.5]{
   let out=render_sequence(&p,filmcraft_project::ItemId(case["sequence"].as_u64().unwrap()),Tick(at),RenderOptions{scale,working_output:true,..Default::default()},&source);
   cases.push(json!({"document":document,"sequence":case["sequence"],"at":at,"interpolation":interpolation,"mode":case["mode"],"generator":generator,"scale":scale,"width":out.w,"height":out.h,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
  }
 }
 }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sourcePattern":"original-generators","cases":cases}));
}
