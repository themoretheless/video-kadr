use serde_json::{json,Value};
use video_kadr_backend::domain::{concat_project_import::{import,ImportRequest},concat_project_export::export,flat_surface,composition::Composition};
fn convert(document:Value)->video_kadr_backend::domain::nested_composition::NestedCompositionProject{let report=import(serde_json::from_value::<ImportRequest>(json!({"schemaVersion":1,"document":document,"mediaBindings":{}})).unwrap()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);report.project.unwrap()}
fn frame(composition:&Composition,time:u64)->Vec<u8>{flat_surface::render_generated(composition,time,&mut||Ok(())).unwrap().unwrap().rgba}
fn source()->Value{json!({"version":1,"media":[],"activeTimelineId":"TL","timelines":[{"id":"TL","name":"Interchange","video":{"width":160,"height":120,"rateNum":30000,"rateDen":1001},"tracks":[{"id":"bottom","name":"Ground"},{"id":"look","name":"Treatment"},{"id":"top","name":"Words"}],"clips":[{"id":"ground","trackId":"bottom","kind":"shape","start":0,"duration":4,"shape":{"kind":"arrow","size":0.8,"fill":"#804020","strokeWidth":0},"flipH":true,"crop":{"left":0.05},"keys":[{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":90}]},{"id":"look","trackId":"look","kind":"layer","start":1,"duration":2,"opacity":0.5,"fadeIn":0.5,"fadeOut":0.5,"videoEffects":[{"id":"concat.exposure","params":{"stops":1},"keys":{"stops":[{"at":0,"value":0},{"at":1,"value":2}]}}]},{"id":"words","trackId":"top","kind":"text","start":0,"duration":4,"text":{"content":"Round trip","fontSize":0.2},"offsetY":0.2,"rotation":12}]}]})}
#[test]
fn retained_three_kind_project_exports_actual_source_json_and_reimports_exact_frames(){
 let project=convert(source());let before=serde_json::to_value(&project).unwrap();let document=export(&project).unwrap();if let Ok(path)=std::env::var("CONCAT_EXPORTED_PROJECT_OUTPUT"){std::fs::write(path,serde_json::to_vec(&document).unwrap()).unwrap();}let restored=convert(document.clone());
 assert_eq!(document["activeTimelineId"],project.root);assert_eq!(document["timelines"][0]["video"]["rateNum"],30000);
 for time in [0,500000,1000000,1250000,1750000,2500000,2999999,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"time {time}");}
 if let Ok(path)=std::env::var("CONCAT_REWRITTEN_PROJECT_INPUT"){
  let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=convert(rewritten);
  for time in [0,500000,1250000,1750000,2500000,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"Original reader/writer at {time}");}
  println!("Original Concat reader/writer roundtrip: 6 exact RGBA frames passed");
 }
 assert_eq!(serde_json::to_value(&project).unwrap(),before);
}
#[test]
fn native_only_animation_refuses_without_altering_project(){
 let project=convert(source());let mut value=serde_json::to_value(&project).unwrap();value["compositions"][&project.root]["content"]["tracks"][1]["clips"][0]["effects"][0].as_object_mut().unwrap().remove("concatKeys");value["compositions"][&project.root]["content"]["tracks"][1]["clips"][0]["effects"][0]["animation"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":1000000,"value":0},{"tick":3000000,"value":2}]}});
 let project=serde_json::from_value(value.clone()).unwrap();let before=serde_json::to_value(&project).unwrap();let error=export(&project).unwrap_err();assert!(format!("{error:#}").contains("cannot be exported losslessly"),"{error:#}");assert_eq!(serde_json::to_value(project).unwrap(),before);
}
#[test]
fn scalar_source_packages_and_disabled_owners_have_exact_reverse_links(){
 use video_kadr_backend::domain::{concat_effect_upgrade::{AppliedFilter,native_effect},concat_effect_export::export_effect,composition::VideoEffect};
 for name in ["exposure","contrast","saturation","white-balance","glow","fisheye","motion-blur","gaussian-blur","halftone","pixelate","vibrance","posterize","film-grain","mono","film","digicam","midnight","vhs"]{
  let link:AppliedFilter=serde_json::from_value(json!({"id":format!("concat.{name}")})).unwrap();let effect=native_effect(&link).unwrap().unwrap_or_else(||panic!("missing source adapter: {name}"));let exported=export_effect(&effect).unwrap_or_else(|error|panic!("{name}: {error:#}"));let restored=native_effect(&exported).unwrap().unwrap();assert_eq!(serde_json::to_value(&effect).unwrap(),serde_json::to_value(restored).unwrap(),"{name}");
  let disabled=export_effect(&VideoEffect::NativeBypass{effect:Box::new(effect)}).unwrap();assert!(!disabled.enabled);assert_eq!(disabled.id,link.id);
 }
}
#[test]
fn all_seven_shapes_and_portable_scalar_keys_survive_source_roundtrip(){
 for kind in ["square","circle","triangle","parallelogram","trapezoid","line","arrow"]{
  let mut document=source();document["timelines"][0]["clips"][0]["shape"]["kind"]=json!(kind);
  let project=convert(document);let restored=convert(export(&project).unwrap());
  for time in [250000,1750000,3500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"{kind} at {time}");}
 }
}
#[test]
fn incompatible_native_workspace_state_is_never_silently_exported(){
 let original=convert(source());let before=serde_json::to_value(&original).unwrap();
 for mode in ["lock","tail","background"]{
  let mut value=before.clone();let node=&mut value["compositions"][&original.root];
  match mode{"lock"=>node["content"]["tracks"][0]["locked"]=json!(true),"tail"=>node["durationUs"]=json!(4500000),_=>node["content"]["canvas"]["background"]["red"]=json!(0.2)}
  let project=serde_json::from_value(value.clone()).unwrap();assert!(export(&project).is_err(),"{mode}");assert_eq!(serde_json::to_value(project).unwrap(),value);
 }
}
#[test]
fn disabled_plugin_parameter_names_are_not_confused_with_key_metadata(){
 let mut document=source();document["timelines"][0]["clips"][1]["videoEffects"]=json!([{"id":"custom.plugin","enabled":false,"params":{"at":0.1,"value":0.2,"derived":0.7},"keys":{"derived":[{"at":0.5,"value":0.8}]}}]);
 let project=convert(document);let output=export(&project).unwrap();let link=&output["timelines"][0]["clips"].as_array().unwrap().iter().find(|clip|clip["id"]=="look").unwrap()["videoEffects"][0];
 assert_eq!(link["params"]["derived"].as_f64(),Some(0.7));assert_eq!(link["keys"]["derived"][0]["value"].as_f64(),Some(0.8));let restored=convert(output);assert_eq!(frame(&project.compositions[&project.root].content,1750000),frame(&restored.compositions[&restored.root].content,1750000));
}
#[test]
fn mixed_native_and_source_pose_animation_refuses_instead_of_dropping_native_keys(){
 let project=convert(source());let mut value=serde_json::to_value(&project).unwrap();value["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["transform"]["scaleX"]=json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.0},{"tick":4000000,"value":2.0}]}});
 let project=serde_json::from_value(value).unwrap();let before=serde_json::to_value(&project).unwrap();assert!(format!("{:#}",export(&project).unwrap_err()).contains("Native animation needs source-key conversion"));assert_eq!(serde_json::to_value(&project).unwrap(),before);
}
#[test]
fn composite_color_parameters_and_source_wheel_keys_roundtrip_structurally_and_in_pixels(){
 use video_kadr_backend::domain::{concat_effect_upgrade::{AppliedFilter,native_effect},concat_effect_export::export_effect};
 for id in ["concat.adjust","concat.color-wheels","concat.curves"]{
  let mut params=json!({});
  if id!="concat.curves"{params["lift.x"]=json!(0.2);params["lift.y"]=json!(-0.1);params["gamma.m"]=json!(0.15);params["gain.x"]=json!(-0.2);params["gain.y"]=json!(0.1);}
  if id!="concat.color-wheels"{for (index,point) in [[0.0,0.0],[0.4,0.5],[1.0,1.0]].iter().enumerate(){params[format!("luma.{index}.x")]=json!(point[0]);params[format!("luma.{index}.y")]=json!(point[1]);}}
  if id=="concat.adjust"{params["brightness"]=json!(12.0);params["temperature"]=json!(5600.0);}
  let mut link=json!({"id":id,"params":params});if id!="concat.curves"{link["keys"]=json!({"lift.x":[{"at":0.0,"value":-0.2,"ease":"inOut"},{"at":1.0,"value":0.3}],"gamma.m":[{"at":0.0,"value":0.0},{"at":1.0,"value":0.2}]});}
  let typed:AppliedFilter=serde_json::from_value(link.clone()).unwrap();let effect=native_effect(&typed).unwrap().unwrap();let exported=export_effect(&effect).unwrap();let restored=native_effect(&exported).unwrap().unwrap();assert_eq!(serde_json::to_value(&effect).unwrap(),serde_json::to_value(restored).unwrap(),"{id}");
  let mut document=source();document["timelines"][0]["clips"][1]["videoEffects"]=json!([link]);let project=convert(document);let output=export(&project).unwrap();if id=="concat.adjust"{if let Ok(path)=std::env::var("CONCAT_EXPORTED_COMPOSITE_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}}
  if id=="concat.adjust"{if let Ok(path)=std::env::var("CONCAT_REWRITTEN_COMPOSITE_INPUT"){
   let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=convert(rewritten);
   for time in [1250000,1750000,2500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"Original composite reader/writer at {time}");}
   println!("Original composite reader/writer roundtrip: 3 exact RGBA frames passed");
  }}
  let restored=convert(output);for time in [1250000,1750000,2500000]{assert_eq!(frame(&project.compositions[&project.root].content,time),frame(&restored.compositions[&restored.root].content,time),"{id} at {time}");}
 }
}
#[test]
fn noncanonical_native_curve_ownership_is_not_silently_rewritten(){
 use video_kadr_backend::domain::{composition::VideoEffect,concat_effect_export::export_effect};
 let effect:VideoEffect=serde_json::from_value(json!({"kind":"concat_curves","parameters":{"luma":[[0.2,0.3]],"red":[],"green":[],"blue":[]}})).unwrap();let before=serde_json::to_value(&effect).unwrap();assert!(export_effect(&effect).is_err());assert_eq!(serde_json::to_value(effect).unwrap(),before);
}
#[test]
fn explicit_media_paths_roundtrip_audio_video_and_image_without_opening_paths(){
 use video_kadr_backend::domain::{concat_project_export::export_with_paths,composition::CompositionSource};
 use std::collections::BTreeMap;
 let mut document=source();document["media"]=json!([{"id":"snd","kind":"audio","path":"/missing/snd.wav"},{"id":"vid","kind":"video","path":"/missing/vid.mov"},{"id":"pic","kind":"image","path":"/missing/pic.png"}]);
 let tracks=document["timelines"][0]["tracks"].as_array_mut().unwrap();for id in ["sound","picture","movie"]{tracks.push(json!({"id":id,"name":id}));}
 let clips=document["timelines"][0]["clips"].as_array_mut().unwrap();
 clips.push(json!({"id":"sound","trackId":"sound","kind":"audio","mediaId":"snd","start":0.25,"duration":2,"sourceStart":0.5,"speed":1.5,"volume":0.7,"fadeIn":0.1,"fadeOut":0.2,"keys":[{"property":"volume","at":0,"value":0.2},{"property":"volume","at":1,"value":0.8}],"filters":[{"id":"concat.echo","params":{"delay":0.25,"decay":0.4},"keys":{"delay":[{"at":0.5,"value":0.4}]}},{"id":"concat.hall","enabled":false}]}));
 clips.push(json!({"id":"picture","trackId":"picture","kind":"image","mediaId":"pic","start":0,"duration":4,"flipH":true,"offsetX":0.2,"keys":[{"property":"rotation","at":0,"value":0},{"property":"rotation","at":1,"value":45}]}));
 clips.push(json!({"id":"movie","trackId":"movie","kind":"video","mediaId":"vid","start":0.5,"duration":3,"sourceStart":0.75,"speed":1.25,"volume":0.5,"audioStream":1,"fadeIn":0.2,"fadeOut":0.3,"crop":{"left":0.1},"videoEffects":[{"id":"concat.exposure","params":{"stops":0.5}}]}));
 let bindings:BTreeMap<String,CompositionSource>=serde_json::from_value(json!({"snd":{"id":"snd","kind":"audio","durationTicks":10000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2},"vid":{"id":"vid","kind":"video","durationTicks":10000000,"width":160,"height":120,"hasAudio":true,"audioChannels":2},"pic":{"id":"pic","kind":"image","durationTicks":0,"width":160,"height":120,"hasAudio":false}})).unwrap();
 let report=import(serde_json::from_value(json!({"schemaVersion":1,"document":document,"mediaBindings":bindings})).unwrap()).unwrap();assert_eq!(report.status,"converted","{:?}",report.diagnostics);let project=report.project.unwrap();let before=serde_json::to_value(&project).unwrap();
 assert!(export(&project).is_err());let paths=BTreeMap::from([("snd".into(),"/unopened/snd.wav".into()),("vid".into(),"/unopened/vid.mov".into()),("pic".into(),"/unopened/pic.png".into())]);let output=export_with_paths(&project,&paths).unwrap();
 if let Ok(path)=std::env::var("CONCAT_EXPORTED_MEDIA_OUTPUT"){std::fs::write(path,serde_json::to_vec(&output).unwrap()).unwrap();}
 let restored=import(serde_json::from_value(json!({"schemaVersion":1,"document":output,"mediaBindings":bindings})).unwrap()).unwrap().project.unwrap();
 assert_eq!(project.compositions[&project.root].content.tracks,restored.compositions[&restored.root].content.tracks);
 if let Ok(path)=std::env::var("CONCAT_REWRITTEN_MEDIA_INPUT"){let rewritten:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();let restored=import(serde_json::from_value(json!({"schemaVersion":1,"document":rewritten,"mediaBindings":bindings})).unwrap()).unwrap();assert_eq!(restored.status,"converted","{:?}",restored.diagnostics);let restored=restored.project.unwrap();assert_eq!(project.compositions[&project.root].content.tracks,restored.compositions[&restored.root].content.tracks);}
 // Compare actual DSP output, including the retained inactive source parameter keys.
 use video_kadr_backend::domain::{composition::CompositionTrack,filmcraft_dsp::Chain};
 let CompositionTrack::Audio{clips:original_audio,..}=&project.compositions[&project.root].content.tracks[2] else{panic!("audio track missing")};let CompositionTrack::Audio{clips:restored_audio,..}=&restored.compositions[&restored.root].content.tracks[2] else{panic!("restored audio missing")};
 let signal=(0..24000).map(|i|if i==0{0.8}else{(i as f32*0.057).sin()*0.2}).collect::<Vec<_>>();let mut reference=signal.clone();let mut actual=signal;Chain::new(48000,1,&original_audio[0].dsp_effects).unwrap().process(&mut[reference.as_mut_slice()],&mut||Ok(())).unwrap();Chain::new(48000,1,&restored_audio[0].dsp_effects).unwrap().process(&mut[actual.as_mut_slice()],&mut||Ok(())).unwrap();assert_eq!(reference,actual);
 assert_eq!(serde_json::to_value(&project).unwrap(),before);
 let mut renamed=before.clone();renamed["compositions"][&project.root]["content"]["tracks"][2]["id"]=json!("native-audio-lane");assert!(export_with_paths(&serde_json::from_value(renamed).unwrap(),&paths).is_ok());
 for mode in ["solo","derived"]{let mut changed=before.clone();let track=&mut changed["compositions"][&project.root]["content"]["tracks"][2];if mode=="solo"{track["solo"]=json!(true);}else{track["clips"][0]["dspEffects"][0]["concatSourceKeys"]["delay"][0]["derived"]=json!(true);}assert!(export_with_paths(&serde_json::from_value(changed).unwrap(),&paths).is_err(),"{mode}");}
 let mut changed=before.clone();changed["compositions"][&project.root]["content"]["tracks"][2]["clips"][0]["pan"]=json!({"mode":"constant","value":0.5});let changed=serde_json::from_value(changed).unwrap();assert!(export_with_paths(&changed,&paths).is_err());
}
