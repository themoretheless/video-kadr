// Full source-sequence qualification of the procedural compositor path.

 use super::*;
 #[test]fn procedural_transition_scene_matches_original_9408_frames(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-all-transition-sequence.json")).unwrap();
  assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),9408);
  for case in cases{
   let mode=case["mode"].as_u64().unwrap();let mut composition=Composition::new(Default::default());
   composition.canvas.width=4;composition.canvas.height=4;composition.canvas.background.alpha=0.;
   composition.sources=serde_json::from_value(serde_json::json!({"red":{"id":"red","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false},"blue":{"id":"blue","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false}})).unwrap();
   let kind="procedural";
   let kernel=serde_json::json!({"effect":case["effect"],"params":{}});
   let mut transitions=vec![serde_json::json!({"id":"first","fromClipId":if mode==1{None}else{Some("a")},"toClipId":if mode==2{None}else{Some("b")},"timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind,"kernel":kernel,"reverse":case["reverse"]})];
   if mode==5{transitions.push(serde_json::json!({"id":"second","fromClipId":null,"toClipId":"b","timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind,"kernel":kernel}));}
   composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","transitions":transitions,"clips":[{"id":"a","sourceId":"red","placement":{"timelineStartTick":0,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","enabled":mode!=3},{"id":"b","sourceId":"blue","placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","enabled":mode!=4}]}])).unwrap();
   let exact=case["at"].as_str().unwrap().parse::<i128>().unwrap();let time=(exact/254016)as u64;
   let mut decode=|id:&super::super::composition::SourceId,_:u64,_:bool|Ok(GroupSurface{width:4,height:4,rgba:if id.as_str()=="red"{[255,0,0,255].repeat(16)}else{[0,0,255,255].repeat(16)}});
   let mut working=None;
   render_media_layers_context(&composition,time,&mut||Ok(()),None,Some(&mut decode),&[],0,None,Some(&mut working),Some(exact)).unwrap().expect("native transition support");
   let bits:Vec<u32>=working.unwrap().pixels().iter().flatten().map(|value|value.to_bits()).collect();
   assert_eq!(bits,serde_json::from_value::<Vec<u32>>(case["bits"].clone()).unwrap(),"{case}");
  }
 }
