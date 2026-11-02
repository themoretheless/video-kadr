// Full source-sequence qualification through the Filmcraft applicability adapter.
// This covers converted default parameters, not full project-file import or delivery.

 use super::*;
 #[test]fn all_default_effect_transition_scenes_match_original_1524_frames(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-side-effects.json")).unwrap();
  assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),1524);
  let adapters:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-default-adapters.json")).unwrap();
  let mut failures=std::collections::BTreeMap::<String,(usize,String)>::new();
  for case in cases{
   let mode=case["mode"].as_u64().unwrap();let mut composition=Composition::new(Default::default());
   composition.canvas.fps_milli=23976; composition.canvas.fps_rate=Some(super::super::composition::FrameRateRatio{numerator:24000,denominator:1001});composition.canvas.width=4;composition.canvas.height=4;composition.canvas.background.alpha=0.;
   composition.sources=serde_json::from_value(serde_json::json!({"red":{"id":"red","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false},"blue":{"id":"blue","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false}})).unwrap();
   let kind="procedural";
   let kernel=serde_json::json!({"effect":"cross_dissolve","params":{}});
   let effect=adapters["effects"].as_array().unwrap().iter().find(|entry|entry["source"]==case["effect"]).unwrap()["native"].clone();
   let native:super::super::composition::VideoEffect=serde_json::from_value(effect).unwrap();
   let partition=super::super::filmcraft_video_project::partition_adapted_effects(vec![native]).unwrap();
   let is_layer=matches!(case["effect"].as_str(),Some("graphic_shape"|"graphic_text"));
   assert_eq!(partition.graphic.layers.len(),usize::from(is_layer),"graphic layer must be retained");
   assert_eq!(partition.media_effects.len(),usize::from(!is_layer));
   let effects=serde_json::to_value(&partition.media_effects).unwrap();
   let transitions=vec![serde_json::json!({"id":"first","fromClipId":"a","toClipId":"b","timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind,"kernel":kernel,"reverse":case["reverse"]})];
   composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","transitions":transitions,"clips":[{"id":"a","sourceId":"red","placement":{"timelineStartTick":0,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":if mode==0{effects.clone()}else{serde_json::json!([])},"enabled":true},{"id":"b","sourceId":"blue","placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":if mode==1{effects.clone()}else{serde_json::json!([])},"enabled":true}]}])).unwrap();
   composition.metadata=Some(super::super::metadata_burnin::CompositionMetadata{
            markers: Default::default(),
            media_versions: Default::default(),
            clip_links: Default::default(),sequence_name:"s".into(),clip_names:[("a","matte"),("b","matte")].into_iter().map(|(id,name)|(super::super::composition::CompositionClipId::parse(id).unwrap(),name.into())).collect(),source_names:[("red","matte"),("blue","matte")].into_iter().map(|(id,name)|(super::super::composition::SourceId::parse(id).unwrap(),name.into())).collect(),drop_frame:false,sequence_start_frame:0});
   let exact=case["at"].as_str().unwrap().parse::<i128>().unwrap();let time=(exact/254016)as u64;
   let mut decode=|id:&super::super::composition::SourceId,_:u64,_:bool|Ok(GroupSurface{width:4,height:4,rgba:if id.as_str()=="red"{[255,0,0,255].repeat(16)}else{[0,0,255,255].repeat(16)}});
   let mut working=None;
   let result=render_media_layers_context(&composition,time,&mut||Ok(()),None,Some(&mut decode),&[],0,None,Some(&mut working),Some(exact));
   let error=match result {
    Err(error)=>Some(format!("render error: {error:#}")),
    Ok(None)=>Some("missing transition output".into()),
    Ok(Some(_))=>{
     let bits:Vec<u32>=working.unwrap().pixels().iter().flatten().map(|value|value.to_bits()).collect();
     let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
     if bits==expected{None}else{Some(format!("pixel mismatch at {} side{} reverse{}",case["at"],mode,case["reverse"]))}
    }
   };
   if let Some(error)=error {
    let entry=failures.entry(case["effect"].as_str().unwrap().to_string()).or_insert((0,error));entry.0+=1;
   }
  }
  assert!(failures.is_empty(),"127effect scene parity failures: {failures:#?}");
 }

