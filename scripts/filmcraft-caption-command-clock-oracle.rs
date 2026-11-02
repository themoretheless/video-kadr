// Captures command-specific clock coercion by executing original engine commands.
use filmcraft_time::FrameRate;
use serde_json::json;
fn main(){let mut cases=vec![];
 for rate in [FrameRate::FPS_24,FrameRate::FPS_23_976,FrameRate::FPS_29_97]{
  for name in ["add","times","split","move","trim"]{
   let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({})).unwrap();
   let sid=s.state.active_sequence.unwrap();std::sync::Arc::make_mut(&mut s.project).sequence_mut(sid).unwrap().settings.frame_rate=rate;
   let start=rate.tick_of(13).0+1;
   let added=s.execute("captions.add",json!({"time":start,"durationSeconds":3.0*rate.den as f64/rate.num as f64,"text":"Exact"})).unwrap();
   let id=added["caption"].as_u64().unwrap();
   let before=s.execute("captions.list",json!({})).unwrap();
   let (command,params)=match name{
    "add"=>("captions.add",json!({"time":rate.tick_of(20).0+1,"durationSeconds":3.0*rate.den as f64/rate.num as f64,"text":"Added"})),
    "times"=>("captions.setTimes",json!({"caption":id,"startTime":rate.tick_of(14).0+1,"endTime":rate.tick_of(17).0+1})),
    "split"=>("captions.split",json!({"caption":id,"time":rate.tick_of(13).0+rate.frame_duration().0*3/2})),
    "move"=>("captions.move",json!({"captions":[id],"deltaFrames":-1})),
    _=>("captions.trim",json!({"caption":id,"edge":"in","deltaFrames":1})),
   };
   let result=s.execute(command,params.clone()).unwrap();let after=s.execute("captions.list",json!({})).unwrap();
   s.execute("edit.undo",json!({})).unwrap();let undo=s.execute("captions.list",json!({})).unwrap();assert_eq!(undo["tracks"],before["tracks"]);
   cases.push(json!({"name":name,"rate":rate,"initialAddTime":start.to_string(),"command":command,"params":params,"result":result,"before":before,"after":after}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
