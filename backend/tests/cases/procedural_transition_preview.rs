// Prepared authorized native preview delivery gate; registration follows source-discovery integration.
use super::*;
#[tokio::test]
async fn procedural_transition_preview_all_kernels_authorized_source_handles(){
 use std::process::Command;
 use video_kadr_backend::domain::composition::{Composition,TransformSpec};
 let (mut state,_directory)=make_state(true,true).await;
 std::sync::Arc::make_mut(&mut state.tools).ffmpeg_encoders.push("pcm_s16le".into());
 std::sync::Arc::make_mut(&mut state.tools).ffmpeg_muxers.push("wav".into());
 let mut source_pcm=std::collections::BTreeMap::new();
 for(id,color,frequency)in[("a","red",440),("b","blue",660)]{
  let filename=format!("independent-{id}.mkv");let path=state.sources_dir().join(&filename);
  let encoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i",&format!("color=c={color}:s=16x16:r=8:d=4"),"-f","lavfi","-i",&format!("sine=frequency={frequency}:sample_rate=48000:duration=4"),"-c:v","ffv1","-pix_fmt","bgra","-c:a","pcm_f32le","-ac","2","-shortest"]).arg(&path).output().unwrap();
  assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
  assert!(state.library.add(MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video"}))).await);
  let decoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&path).args(["-vn","-f","f32le","-ac","2","-ar","48000","pipe:1"]).output().unwrap();assert!(decoded.status.success());
  source_pcm.insert(id,decoded.stdout.chunks_exact(4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).collect::<Vec<_>>());
 }
 let app=router(state.clone());
 let token=register_token(&app,"procedural-preview-owner").await;
 let catalog:Value=serde_json::from_str(include_str!("../fixtures/filmcraft-transition-definitions.json")).unwrap();
 let definitions=catalog["definitions"].as_array().unwrap();assert_eq!(definitions.len(),112);
 let mut cases:Vec<_>=definitions.iter().map(|definition|(false,false,false,definition)).collect();
 for id in ["block_motion","cross_dissolve","blur_dissolve","wipe","page_peel"]{
  let definition=definitions.iter().find(|d|d["id"]==id).unwrap();
  cases.push((true,false,false,definition));
  cases.push((true,true,true,definition));
 }
 for(index,(disabled,_audio_only,dsp,definition))in cases.into_iter().enumerate(){
  let kind="procedural";let effect=definition["id"].as_str().unwrap();
  eprintln!("procedural delivery {index}: {effect} disabled{disabled}  dsp{dsp}");
  let mut c=Composition::new(Default::default());c.canvas.width=16;c.canvas.height=16;c.canvas.fps_milli=8000;
  c.sources=serde_json::from_value(json!({"a":{"id":"a","kind":"video","durationTicks":4000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2},"b":{"id":"b","kind":"video","durationTicks":4000000,"width":16,"height":16,"hasAudio":true,"audioChannels":2}})).unwrap();
  c.tracks=serde_json::from_value(json!([{"kind":"video","id":"video","name":"Video","transitions":[{"id":"fade","fromClipId":"a","toClipId":"b","kind":kind,"timeline":{"startTick":"127008000000","durationTick":"254016000000"}}],"clips":[{"id":"a","sourceId":"a","placement":{"timelineStartTick":0,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1.0},"enabled":!disabled,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"},{"id":"b","sourceId":"b","placement":{"timelineStartTick":1000000,"sourceInTick":1000000,"sourceOutTick":2000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}])).unwrap();
  let video_kadr_backend::domain::composition::CompositionTrack::Video{transitions,..}=&mut c.tracks[0]else{unreachable!()};
  transitions[0].kernel=Some(video_kadr_backend::domain::transition_spec::TransitionSpec{
   effect:effect.into(),params:definition["params"].as_array().unwrap().iter().map(|p|(p["id"].as_str().unwrap().into(),p["default"].clone())).collect(),
  });
  if dsp {
   c.tracks.push(serde_json::from_value(json!({"kind":"audio","id":"music","name":"DSP Music","clips":[{"id":"music","sourceId":"a","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":2000000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":[{"id":"amplify","params":{"gain":-6.0}}]}]})).unwrap());
  }
  // Test the authorized native preview path at the edit, where outgoing media lies
  // outside its ordinary half-open clip interval and still needs a source handle.
  let time=1_000_000;
  let project=json!({"schemaVersion":1,"root":"editor","compositions":{"editor":{"content":c,"instances":[]}}});
  let response=app.clone().oneshot(bearer(post_json("/api/compositions/nested-preview",json!({"project":project,"timeUs":time,"renderedOutputs":{}})),&token)).await.unwrap();
  assert_eq!(response.status(),StatusCode::OK,"preview {index} {effect}");
  assert_eq!(response.headers()["x-frame-width"],"16");
  assert_eq!(response.headers()["x-frame-height"],"16");
  let bytes=response.into_body().collect().await.unwrap().to_bytes();
  assert_eq!(bytes.len(),16*16*4);
  let decode=|id:&str|{
   let source=state.sources_dir().join(format!("independent-{id}.mkv"));
   let decoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-ss","2","-i"]).arg(source).args(["-frames:v","1","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();
   assert!(decoded.status.success());
   video_kadr_backend::domain::group_surface::GroupSurface{width:16,height:16,rgba:decoded.stdout}
  };
  let red=decode("a");let blue=decode("b");
  let mut media=|id:&video_kadr_backend::domain::composition::SourceId,_:u64,_:bool|Ok(if id.as_str()=="a"{red.clone()}else{blue.clone()});
  let expected=video_kadr_backend::domain::flat_surface::render_media_layers_with_library(&c,time,&mut||Ok(()),None,Some(&mut media),&[]).unwrap().unwrap();
  assert_eq!(bytes.as_ref(),expected.rgba,"preview transport/decode {effect}");
 }
}
