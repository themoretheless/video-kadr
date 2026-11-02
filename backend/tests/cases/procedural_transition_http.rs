// Real router/jobs/FFmpeg delivery qualification. Registered by api.rs after unit gate.
use super::*;
#[tokio::test]
async fn procedural_transition_http_all_kernels_selected_range_delivery(){
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
 let catalog:Value=serde_json::from_str(include_str!("../fixtures/filmcraft-transition-definitions.json")).unwrap();
 let definitions=catalog["definitions"].as_array().unwrap();assert_eq!(definitions.len(),112);
 let mut cases:Vec<_>=definitions.iter().map(|definition|(false,false,false,definition)).collect();
 for id in ["block_motion","cross_dissolve","blur_dissolve","wipe","page_peel"]{
  let definition=definitions.iter().find(|d|d["id"]==id).unwrap();
  cases.push((true,false,false,definition));
  cases.push((true,true,true,definition));
 }
 for(index,(disabled,audio_only,dsp,definition))in cases.into_iter().enumerate(){
  let kind="procedural";let effect=definition["id"].as_str().unwrap();
  eprintln!("procedural delivery {index}: {effect} disabled{disabled} audio{audio_only} dsp{dsp}");
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
  let profile=if audio_only{json!({"container":"audio","codec":"wav"})}else{json!({"container":"mov","profile":"hq"})};
  let body=json!({"schemaVersion":1,"composition":c,"output":{"profile":profile,"range":{"startTicks":250000,"endTicks":1750000}}});
  let(status,accepted,_)=send(&app,post_json("/api/compositions/render",body.clone())).await;
  assert_eq!(status,StatusCode::ACCEPTED,"{kind}/{effect} disabled{disabled} audio{audio_only}: {accepted}");
  let done=poll_terminal(&app,accepted["jobId"].as_str().unwrap()).await;
  assert_eq!(done["status"],"done","{kind}/{effect} disabled{disabled} audio{audio_only}: {done}");
  let output=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
  if !audio_only{
   let decoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&output).args(["-map","0:v:0","-f","rawvideo","-pix_fmt","rgba","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),12*16*16*4,"{kind} selected frames");
  }
  let decoded=Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&output).args(["-map","0:a:0","-f","f32le","-ac","2","-ar","48000","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),72000*2*4);
  for(index,bytes)in decoded.stdout.chunks_exact(4).enumerate(){
   let sample=12000+index/2;let channel=index%2;
   let(id,source_sample)=if sample<48000{("a",48000+sample)}else{("b",sample)};
   let gain=if(24000..72000).contains(&sample){let p=(sample-24000)as f32/48000.;if id=="a"{(p*std::f32::consts::FRAC_PI_2).cos()}else{(p*std::f32::consts::FRAC_PI_2).sin()}}else if disabled&&id=="a"{0.}else{1.};
   // Original Amplify starts a fresh factory on seek: 20ms linear parameter ramp.
   // Compute its expected envelope independently, rather than assuming an instant gain jump.
   let target=10.0_f32.powf(-6.0/20.0);
   let dsp_gain=if index/2<960 {(1.0+(f64::from(target)-1.0)*(index/2+1)as f64/960.0)as f32}else{target};
   let expected=source_pcm[id][source_sample*2+channel]*gain + if dsp {source_pcm["a"][sample*2+channel]*dsp_gain} else {0.0};let actual=f32::from_le_bytes(bytes.try_into().unwrap());
   assert!((actual-expected).abs()<0.00014,"{kind}/{effect} disabled{disabled} audio{audio_only} sample{sample} channel{channel}: {actual} != {expected}");
  }
  let(_,cached,_)=send(&app,post_json("/api/compositions/render",body)).await;assert_eq!(cached["jobId"],accepted["jobId"]);
 }
}
