use serde_json::{json,Value};
fn main(){let mut cases=vec![];
 for name in ["split","merge","text","times","trim-out","trim-in","move-right","move-left","delete","ripple-delete","add-default","add-overlap"]{
  let mut session=filmcraft_engine::Session::default();session.execute("file.openDemoProject",json!({})).unwrap();
  let mut ids=vec![];
  for (seconds,text)in [(600," First "),(604,"Second"),(608,"Third")]{let result=session.execute("captions.add",json!({"seconds":seconds,"durationSeconds":2,"text":text})).unwrap();ids.push(result["caption"].as_u64().unwrap());}
  let before=session.execute("captions.list",json!({})).unwrap();
  let (command,params)=match name{
   "split"=>{let cue=&session.active_sequence().unwrap().caption_tracks[0].captions[1];("captions.split",json!({"caption":ids[1],"time":cue.start.0+cue.duration.0/2}))},
   "merge"=>("captions.merge",json!({"captions":[ids[0],ids[1]]})),
   "text"=>("captions.setText",json!({"caption":ids[1],"text":"Line one\r\nLine two","speaker":"  Ann  "})),
   "times"=>("captions.setTimes",json!({"caption":ids[1],"startSeconds":604.5,"endSeconds":606.5})),
   "trim-out"=>("captions.trim",json!({"caption":ids[1],"edge":"out","deltaFrames":500})),
   "trim-in"=>("captions.trim",json!({"caption":ids[1],"edge":"in","deltaFrames":-500})),
   "move-right"=>("captions.move",json!({"captions":[ids[1]],"deltaFrames":500})),
   "move-left"=>("captions.move",json!({"captions":[ids[1]],"deltaFrames":-500})),
   "delete"=>("captions.delete",json!({"captions":[ids[1]]})),
   "ripple-delete"=>("captions.delete",json!({"captions":[ids[0],ids[1]],"ripple":true})),
   "add-default"=>("captions.add",json!({"seconds":612,"text":"Default duration"})),
   _=>("captions.add",json!({"seconds":609,"text":"Overlapping"})),
  };
  let outcome=session.execute(command,params.clone());let success=outcome.is_ok();let result=match outcome{Ok(value)=>json!({"ok":value}),Err(error)=>json!({"error":error.to_string()})};
  let after=session.execute("captions.list",json!({})).unwrap();
  if success{session.execute("edit.undo",json!({})).unwrap();let restored=session.execute("captions.list",json!({})).unwrap();assert_eq!(restored["tracks"],before["tracks"],"undo{name}");}else{assert_eq!(after["tracks"],before["tracks"],"atomic{name}");}
  cases.push(json!({"name":name,"frameRate":session.active_sequence().unwrap().settings.frame_rate,"command":command,"params":params,"result":result,"before":before,"after":after,"undoRestored":success}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
