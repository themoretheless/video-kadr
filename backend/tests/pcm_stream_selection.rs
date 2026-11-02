mod support;
use std::{path::Path,time::Duration};
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;
use video_kadr_backend::{process_control::ProcessRuntime,tools::pcm_conform::{probe_pcm_metadata_for_stream,conform_pcm,conform_pcm_stream,PcmConformSpec}};
use tokio_util::sync::CancellationToken;
#[tokio::test]
#[ignore="requires real FFmpeg/FFprobe"]
async fn absolute_stream_selection_keeps_distinct_audio_and_refuses_video_or_missing_index(){
 let dir=tempfile::tempdir().unwrap();let source=dir.path().join("two-streams.mov");
 let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i","color=c=black:s=16x16:r=4:d=1","-f","lavfi","-i","aevalsrc=0.125:s=48000:d=1","-f","lavfi","-i","aevalsrc=-0.25:s=48000:d=1","-map","0:v:0","-map","1:a:0","-map","2:a:0","-c:v","qtrle","-c:a","pcm_f32le","-metadata:s:a:1","language=rus","-metadata:s:a:1","handler_name=Commentary"]).arg(&source).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
 let runtime=ProcessRuntime::local_default();let cancel=CancellationToken::new();let timeout=Duration::from_secs(20);
 for index in [1,2]{let probe=probe_pcm_metadata_for_stream(&runtime,Path::new("ffprobe"),&source,timeout,&cancel,Some(index)).await.unwrap();assert_eq!(probe.streams.len(),1);assert_eq!(probe.streams[0].index,index);assert_eq!(probe.streams[0].channels,Some(1));}
 for index in [0,99]{assert!(probe_pcm_metadata_for_stream(&runtime,Path::new("ffprobe"),&source,timeout,&cancel,Some(index)).await.is_err());}
 let spec=PcmConformSpec{start_sample:480,end_sample:960,channels:1,staging_bytes:1024*1024};
 let mut first=conform_pcm(&runtime,Path::new("ffmpeg"),&source,dir.path(),spec,timeout,&cancel).await.unwrap();
 let mut selected=conform_pcm_stream(&runtime,Path::new("ffmpeg"),&source,dir.path(),spec,timeout,&cancel,2).await.unwrap();
 for time in [10000,15000,19979]{assert_eq!(first.read(time).unwrap(),(1,[0.125,0.]));assert_eq!(selected.read(time).unwrap(),(1,[-0.25,0.]));}
 assert!(conform_pcm_stream(&runtime,Path::new("ffmpeg"),&source,dir.path(),spec,timeout,&cancel,0).await.is_err());
 // The source-typed packet path must consume the same selected physical stream.
 let project:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":{"schemaVersion":1,"timeBase":1000000,"canvas":{"width":16,"height":16,"fpsMilli":4000,"background":{"red":0,"green":0,"blue":0,"alpha":1}},"sources":{"source":{"id":"source","kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":true,"audioChannels":1}},"tracks":[{"kind":"audio","id":"a","name":"Audio","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0,"dspEffects":[{"id":"concat.pitch","params":{"semitones":0}}]}]}]},"instances":[]}}})).unwrap();
 let mut individual=std::collections::BTreeMap::new();
 for index in [1,2]{
  let probe=probe_pcm_metadata_for_stream(&runtime,Path::new("ffprobe"),&source,timeout,&cancel,Some(index)).await.unwrap();
  let bound=video_kadr_backend::tools::native_audio_export::PcmSource::from_probe(source.clone(),probe).unwrap();
  let sources=std::collections::BTreeMap::from([(video_kadr_backend::domain::composition::SourceId::parse("source").unwrap(),bound)]);let(progress,_receiver)=tokio::sync::mpsc::unbounded_channel();
  let mixed=video_kadr_backend::tools::native_audio_export::mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&project,"root",100000..200000,4800,&sources,256*1024*1024,timeout,&cancel,&progress).await.unwrap();
  let bytes=std::fs::read(mixed.path()).unwrap();assert_eq!(bytes.len(),4800*2*4);
  let samples:Vec<f32>=bytes.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect();
  individual.insert(index,samples.clone());
  let stable=&samples[1024..];assert!(stable.iter().all(|sample|sample.is_finite()));
  if index==1{assert!(stable.iter().all(|sample|*sample>0.05),"packet decoder lost selected positive stream");}else{assert!(stable.iter().all(|sample|*sample< -0.05),"packet decoder fell back to first positive stream");}
 }

 let mut combined=project.clone();
 let track=&mut combined.compositions.get_mut("root").unwrap().content.tracks[0];
 let video_kadr_backend::domain::composition::CompositionTrack::Audio{clips,..}=track else{panic!()};
 clips[0].audio_stream=Some(1);clips[0].gain=video_kadr_backend::domain::composition::AnimatableValue::constant(0.5);
 let mut second=clips[0].clone();second.id=video_kadr_backend::domain::composition::CompositionClipId::parse("second").unwrap();second.audio_stream=Some(2);second.gain=video_kadr_backend::domain::composition::AnimatableValue::constant(1.);clips.push(second);
 let probe=probe_pcm_metadata_for_stream(&runtime,Path::new("ffprobe"),&source,timeout,&cancel,None).await.unwrap();
 let bound=video_kadr_backend::tools::native_audio_export::PcmSource::from_probe(source.clone(),probe).unwrap();let sources=std::collections::BTreeMap::from([(video_kadr_backend::domain::composition::SourceId::parse("source").unwrap(),bound)]);let(progress,_)=tokio::sync::mpsc::unbounded_channel();
 let mixed=video_kadr_backend::tools::native_audio_export::mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&combined,"root",100000..200000,4800,&sources,512*1024*1024,timeout,&cancel,&progress).await.unwrap();
 let bytes=std::fs::read(mixed.path()).unwrap();for(i,sample)in bytes.chunks_exact(4).enumerate(){let actual=f32::from_le_bytes(sample.try_into().unwrap());let expected=0.5*individual[&1][i]+individual[&2][i];assert!((actual-expected).abs()<=2e-7,"same media different stream sample{i}: {actual}/{expected}");}
 let packet_reference=bytes.clone();
 // Actual registered-source import and private HTTP preview, retaining one source ID.
 let(mut state,_state_dir)=support::make_state(true,true).await;
 let caps=std::sync::Arc::make_mut(&mut state.tools);caps.ffmpeg_encoders.extend(["pcm_s16le".into(),"pcm_f32le".into(),"ffv1".into()]);caps.ffmpeg_muxers.extend(["wav".into(),"matroska".into()]);
 let owner=state.db.register_auth_user("stream-owner","public synthetic password").await.unwrap().unwrap();
 let outsider=state.db.register_auth_user("stream-outsider","public synthetic password").await.unwrap().unwrap();
 let space=state.db.create_space(&owner.user.username,"Streams").await.unwrap();
 std::fs::copy(&source,state.sources_dir().join("streams.mov")).unwrap();
 state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":"source","filename":"streams.mov","url":"/files/sources/streams.mov","mediaType":"video","width":16,"height":16,"fps":4,"duration":1,"hasAudio":true,"audioChannels":1,"acodec":"pcm_f32le"}))).await;
 state.db.create_space_composition_project("Stream claim",&json!({"schemaVersion":1,"sources":{"source":{"id":"source"}}}),&["source".into()],&owner.user.username,&space.id).await.unwrap();
 let app=support::router(state.clone());
 for(token,expected)in [(&outsider.token,StatusCode::FORBIDDEN),(&owner.token,StatusCode::OK)]{
  let request=Request::builder().uri("/api/library/source/audio-streams").header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();
  let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),expected);
  let headers=response.headers().clone();let body=to_bytes(response.into_body(),1000000).await.unwrap();let catalog:Value=serde_json::from_slice(&body).unwrap();
  if expected==StatusCode::OK{assert_eq!(catalog["sourceId"],"source");assert_eq!(catalog["streams"].as_array().unwrap().len(),2);assert_eq!(catalog["streams"][0]["index"],1);assert_eq!(catalog["streams"][1]["index"],2);assert_eq!(catalog["streams"][1]["sampleRate"],48000);assert_eq!(catalog["streams"][1]["channels"],1);assert_eq!(catalog["streams"][1]["language"],"rus");assert_eq!(headers["cache-control"],"private, no-store");assert!(!catalog.to_string().contains("streams.mov"));}
 }
 let source_binding=serde_json::to_value(combined.compositions["root"].content.sources.values().next().unwrap()).unwrap();
 let import=json!({"schemaVersion":1,"mediaBindings":{"m":source_binding},"document":{"version":1,"media":[{"id":"m","kind":"video","path":"/never/open"}],"timelines":[{"id":"timeline","video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"a"},{"id":"b"}],"clips":[{"id":"first","trackId":"a","kind":"audio","mediaId":"m","duration":1,"volume":0.5,"audioStream":1,"filters":[{"id":"concat.pitch","keys":{"semitones":[{"at":0,"value":-12},{"at":1,"value":12}]}}]},{"id":"second","trackId":"b","kind":"audio","mediaId":"m","duration":1,"audioStream":2,"filters":[{"id":"concat.pitch","keys":{"semitones":[{"at":0,"value":-12},{"at":1,"value":12}]}}]}]}]}});
 let request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(import.to_string())).unwrap();let(status,report,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::OK);assert_eq!(report["status"],"converted","{report}");
 assert_eq!(report["sourceDocument"],import["document"]);assert_eq!(report["diagnostics"].as_array().unwrap().len(),1);
 let mut edited_project=report["project"].clone();let imported_root=edited_project["root"].as_str().unwrap().to_owned();
 if let Ok(path)=std::env::var("AUDIO_SOURCE_KEY_EDITOR_INPUT"){
  let effect:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(effect["id"],"concat.pitch");assert_eq!(effect["params"]["semitones"].as_f64(),Some(0.));assert_eq!(effect["concatSourceKeys"]["semitones"][0]["ease"][1].as_f64(),Some(-0.5));
  for track in edited_project["compositions"][&imported_root]["content"]["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){clip["dspEffects"]=json!([effect.clone()]);}}
 }
 let exported=stream_wav(&app,&state,edited_project["compositions"][&imported_root]["content"].clone(),&owner.token).await;
 for(actual,expected)in exported.iter().zip(packet_reference.chunks_exact(4)){let expected=f32::from_le_bytes(expected.try_into().unwrap()).clamp(-1.,1.-1./32768.);assert!((actual-expected).abs()<=1./32768.+2e-7,"inactive Pitch keys changed WAV: {actual}/{expected}");}
 let preview=json!({"project":edited_project,"startSample":4800,"frames":4800});
 for(token,expected)in [(&outsider.token,StatusCode::FORBIDDEN),(&owner.token,StatusCode::OK)]{
  let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(preview.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),expected);
  if expected==StatusCode::OK{let bytes=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(bytes.len(),4800*8);for(i,sample)in bytes.chunks_exact(4).enumerate(){let actual=f32::from_le_bytes(sample.try_into().unwrap());let expected=f32::from_le_bytes(packet_reference[i*4..i*4+4].try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"HTTP selected stream mix: {actual}/{expected}");}}
 }

 let mut dry=preview.clone();
 let root=dry["project"]["root"].as_str().unwrap().to_owned();
 for track in dry["project"]["compositions"][&root]["content"]["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){clip["dspEffects"]=json!([]);}}
 let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(dry.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(bytes.len(),4800*8);
 for sample in bytes.chunks_exact(4){let actual=f32::from_le_bytes(sample.try_into().unwrap());let expected=(-0.25+0.5*0.125)*std::f32::consts::FRAC_1_SQRT_2;assert!((actual-expected).abs()<=2e-7,"dry HTTP selected stream mix: {actual}/{expected}");}
 for index in [0,99]{
  let mut bad=dry.clone();bad["project"]["compositions"][&root]["content"]["tracks"][0]["clips"][0]["audioStream"]=json!(index);
  let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(bad.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();assert!(!response.status().is_success(),"invalid stream {index} fell back");
 }
 let content=dry["project"]["compositions"][&root]["content"].clone();
 let render=json!({"schemaVersion":1,"composition":content,"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":100000,"endTicks":200000}}});
 let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(render.to_string())).unwrap();let(status,accepted,_)=support::send(&app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
 let id=accepted["jobId"].as_str().unwrap();let mut done=Value::Null;
 for _ in 0..2000{let request=Request::builder().uri(format!("/api/jobs/{id}")).header("authorization",format!("Bearer {}",owner.token)).body(Body::empty()).unwrap();let(_,result,_)=support::send(&app,request).await;if result["status"]=="done"||result["status"]=="error"{done=result;break}tokio::time::sleep(Duration::from_millis(10)).await;}
 assert_eq!(done["status"],"done","{done}");let output=state.outputs_dir().join(done["result"]["filename"].as_str().unwrap());
 let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(output).args(["-f","f32le","-c:a","pcm_f32le","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),4800*8);
 for sample in decoded.stdout.chunks_exact(4){let actual=f32::from_le_bytes(sample.try_into().unwrap());let expected=(-0.25+0.5*0.125)*std::f32::consts::FRAC_1_SQRT_2;assert!((actual-expected).abs()<=1./32768.+2e-7,"selected streams WAV: {actual}/{expected}");}
 let mut embedded=import.clone();embedded["document"]["timelines"][0]["clips"]=json!([{"id":"picture","trackId":"a","kind":"video","mediaId":"m","duration":1,"audioStream":2}]);
 let request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(embedded.to_string())).unwrap();let(_,report,_)=support::send(&app,request).await;assert_eq!(report["status"],"converted","{report}");
 let body=json!({"project":report["project"],"startSample":4800,"frames":4800});let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(body.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let bytes=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(bytes.len(),4800*8);
 for sample in bytes.chunks_exact(4){let actual=f32::from_le_bytes(sample.try_into().unwrap());assert!((actual+0.25*std::f32::consts::FRAC_1_SQRT_2).abs()<=2e-7,"embedded stream selection fell back: {actual}");}

 let picture_track=report["project"]["compositions"][report["project"]["root"].as_str().unwrap()]["content"]["tracks"].as_array().unwrap().iter().position(|track|track["clips"].as_array().unwrap().iter().any(|clip|clip["id"]=="picture")).expect("imported picture track");
 let mut bypassed=report["project"].clone();let bypass_root=bypassed["root"].as_str().unwrap().to_owned();
 bypassed["compositions"][&bypass_root]["content"]["tracks"][picture_track]["clips"][0]["effects"]=json!([
  {"kind":"concat_bypass","link":{"id":"unknown.saved.package","enabled":false,"params":{"custom":123},"keys":{"custom":[{"at":0,"value":-7},{"at":1,"value":9}]}}},
  {"kind":"concat_bypass","link":{"id":"concat.exposure","enabled":false,"params":{"stops":4}}}
 ]);
 let bypass_pcm=stream_nested_pcm_with_rgb(&app,&state,&bypassed,&owner.token,100000,Some([0,0,0])).await;assert!(bypass_pcm.len()>=9600);
 for sample in &bypass_pcm[..9600]{assert!((*sample+0.25*std::f32::consts::FRAC_1_SQRT_2).abs()<=2e-7,"visual bypass changed embedded stream PCM");}
 assert!(bypass_pcm[9600..].iter().all(|sample|*sample==0.));
 // Exercise native bypass through actual lossless encoded delivery, including reactivation.
 let invert=json!({"kind":"color_invert","parameters":{"channel":0,"blend":0}});
 for disabled in [true,false,true]{
  let effect=if disabled{json!({"kind":"native_bypass","effect":invert.clone()})}else{invert.clone()};
  bypassed["compositions"][&bypass_root]["content"]["tracks"][picture_track]["clips"][0]["effects"]=json!([effect]);
  let pcm=stream_nested_pcm_with_rgb(&app,&state,&bypassed,&owner.token,100000,Some(if disabled{[0,0,0]}else{[255,255,255]})).await;
  assert!(pcm.len()>=9600);
  for sample in &pcm[..9600]{assert!((*sample+0.25*std::f32::consts::FRAC_1_SQRT_2).abs()<=2e-7,"native visual bypass changed selected-stream audio");}
  assert!(pcm[9600..].iter().all(|sample|*sample==0.));
 }
 // Import source opacity keys and deliver their linear-light result with selected audio.
 let mut pose_import=embedded.clone();
 // Black source is inverted before the clip-level opacity composite.
 pose_import["document"]["timelines"][0]["clips"][0]["videoEffects"]=json!([]);
 pose_import["document"]["timelines"][0]["clips"][0]["keys"]=json!([{"property":"opacity","at":0,"value":0.5},{"property":"opacity","at":1,"value":0.5}]);
 let request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(pose_import.to_string())).unwrap();
 let(_,pose_report,_)=support::send(&app,request).await;assert_eq!(pose_report["status"],"converted","{pose_report}");
 let mut pose_project=pose_report["project"].clone();let root=pose_project["root"].as_str().unwrap().to_owned();
 if let Ok(path)=std::env::var("CONCAT_POSE_EDITED_INPUT"){
  let captured:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
  assert_eq!(captured["concatAnimation"]["keys"]["opacity"][0]["value"],0.5);
  assert_eq!(captured["concatAnimation"]["keys"]["opacity"][1]["value"],0.5);
  for property in ["scale","offsetX","offsetY","rotation"]{assert!(captured["concatAnimation"]["keys"][property].as_array().unwrap().is_empty());}
  let _:video_kadr_backend::domain::composition::TransformSpec=serde_json::from_value(captured.clone()).unwrap();
  pose_project["compositions"][&root]["content"]["tracks"][picture_track]["clips"][0]["transform"]=captured;
 }
 pose_project["compositions"][&root]["content"]["tracks"][picture_track]["clips"][0]["effects"]=json!([{"kind":"color_invert","parameters":{"channel":0,"blend":0}}]);
 let pcm=stream_nested_pcm_with_rgb(&app,&state,&pose_project,&owner.token,100000,Some([191,191,191])).await;
 assert!(pcm.len()>=9600);for sample in &pcm[..9600]{assert!((*sample+0.25*std::f32::consts::FRAC_1_SQRT_2).abs()<=2e-7);}
 assert!(pcm[9600..].iter().all(|sample|*sample==0.));
 // The imported left-half crop leaves a fitted 8x16 white picture with black side bars.
 let mut cropped_import=embedded.clone();cropped_import["document"]["timelines"][0]["clips"][0]["crop"]=json!({"left":0.5,"top":0,"right":0,"bottom":0});
 let request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(cropped_import.to_string())).unwrap();let(_,cropped_report,_)=support::send(&app,request).await;assert_eq!(cropped_report["status"],"converted","{cropped_report}");
 let mut cropped_project=cropped_report["project"].clone();let cropped_root=cropped_project["root"].as_str().unwrap().to_owned();
 if let Ok(path)=std::env::var("CONCAT_CROP_EDITED_INPUT"){
  let captured:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
  let preparation:video_kadr_backend::domain::concat_picture::Preparation=serde_json::from_value(captured.clone()).unwrap();preparation.validate().unwrap();
  assert_eq!(preparation.crop,[0.5,0.,0.,0.]);assert_eq!(preparation.canvas,[16,16]);assert!(preparation.flip_h);assert!(!preparation.flip_v);
  cropped_project["compositions"][&cropped_root]["content"]["tracks"][picture_track]["clips"][0]["transform"]["concatPreparation"]=captured;
 }
 cropped_project["compositions"][&cropped_root]["content"]["tracks"][picture_track]["clips"][0]["effects"]=json!([{"kind":"color_invert","parameters":{"channel":0,"blend":0}}]);
 let expected:Vec<u8>=(0..16).flat_map(|_|(0..16).flat_map(|x|[if (4..12).contains(&x){255}else{0};3])).collect();
 let pcm=stream_nested_pcm_with_expected_pixels(&app,&state,&cropped_project,&owner.token,100000,Some(expected)).await;
 assert!(pcm.len()>=9600);for sample in &pcm[..9600]{assert!((*sample+0.25*std::f32::consts::FRAC_1_SQRT_2).abs()<=2e-7);}assert!(pcm[9600..].iter().all(|sample|*sample==0.));
 // Independent transition sides retain clip IDs while selecting distinct streams of one source.
 let mut transitioned=report["project"].clone();let transition_root=transitioned["root"].as_str().unwrap().to_owned();
 let track=&mut transitioned["compositions"][&transition_root]["content"]["tracks"][picture_track];
 let mut left=track["clips"][0].clone();left["id"]=json!("left");left["audioStream"]=json!(1);
 let mut right=left.clone();right["id"]=json!("right");right["audioStream"]=json!(2);right["placement"]["timelineStartTick"]=json!(1000000);
 track["clips"]=json!([left,right]);track["transitions"]=json!([{"id":"fade","fromClipId":"left","toClipId":"right","timeline":{"startTick":"127008000000","durationTick":"254016000000"},"kind":"dissolve"}]);
 for disabled in [false,true]{
 transitioned["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"][0]["enabled"]=json!(!disabled);
 for start in [28800u64,52800]{
  let bytes=stream_preview_at(&app,&transitioned,&owner.token,start).await;
  let expected:Vec<f32>=(0..4800).flat_map(|frame|{let progress=((start+frame-24000)as f32)/48000.;let value=if start<48000{0.125*(progress*std::f32::consts::FRAC_PI_2).cos()}else{-0.25*(progress*std::f32::consts::FRAC_PI_2).sin()}*std::f32::consts::FRAC_1_SQRT_2;[value,value]}).collect();
  for(actual,expected)in bytes.chunks_exact(4).zip(&expected){let actual=f32::from_le_bytes(actual.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"transition selected streams {start}: {actual}/{expected}");}
  let samples=stream_wav_at(&app,&state,transitioned["compositions"][&transition_root]["content"].clone(),&owner.token,start*1000000/48000).await;
  for(actual,expected)in samples.iter().zip(&expected){assert!((actual-expected).abs()<=1./32768.+2e-7,"transition selected WAV {start}: {actual}/{expected}");}
 }
 if disabled{
  let bytes=stream_preview_at(&app,&transitioned,&owner.token,4800).await;assert!(bytes.iter().all(|byte|*byte==0),"disabled transition side leaked outside its window");
 }
 }
 for incoming_start in [1000000u64,500000]{
 for mode in ["echo","pitch","mixed","loudness"]{
 for disabled in [false,true]{
  let mut processed=transitioned.clone();let clips=processed["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"].as_array_mut().unwrap();clips[0]["enabled"]=json!(!disabled);
  clips[1]["placement"]["timelineStartTick"]=json!(incoming_start);
  if incoming_start==500000{for(index,clip)in clips.iter_mut().enumerate(){clip["concatGainKeys"]=json!([{"at":0,"value":0.5},{"at":1,"value":1}]);clip["audioPan"]=json!({"mode":"constant","value":if index==0{-0.25}else{0.25}});}}
  for(index,clip)in clips.iter_mut().enumerate(){clip["dspEffects"]=match mode{
   "echo"=>json!([{"id":"concat.echo","params":{"delay":0.1}}]),
   "pitch"=>json!([{"id":"concat.pitch","params":{"semitones":0}}]),
   "mixed"=>if index==0{json!([{"id":"concat.pitch","params":{"semitones":2}}])}else{json!([{"id":"concat.echo","params":{"delay":0.1}}])},
   _=>json!([{"id":"concat.loudness","params":{}}]),
  };}
  for start in [28800u64,52800]{
   let mut sides=Vec::new();
   for side in 0..2{
    let mut reference=processed.clone();let track=&mut reference["compositions"][&transition_root]["content"]["tracks"][picture_track];for(index,clip)in track["clips"].as_array_mut().unwrap().iter_mut().enumerate(){clip["enabled"]=json!(true);clip["sourceAudioEnabled"]=json!(index==side);}track["transitions"]=json!([]);
    let bytes=stream_preview_at(&app,&reference,&owner.token,start).await;sides.push(bytes.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect::<Vec<_>>());
   }
   if incoming_start==500000&&start==28800{assert!(sides.iter().all(|side|side.iter().any(|sample|sample.abs()>1e-5)),"overlap reference must contain both audible sides: {mode}/{disabled}");}
   let expected:Vec<f32>=(0..9600).map(|index|{let p=((start+index as u64/2-24000)as f32)/48000.;sides[0][index]*(p*std::f32::consts::FRAC_PI_2).cos()+sides[1][index]*(p*std::f32::consts::FRAC_PI_2).sin()}).collect();
   assert!(expected.iter().any(|sample|sample.abs()>1e-5),"vacuous processed transition reference: {mode}/{start}/{disabled}");
   let bytes=stream_preview_at(&app,&processed,&owner.token,start).await;
   for(actual,expected)in bytes.chunks_exact(4).zip(&expected){let actual=f32::from_le_bytes(actual.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"processed transition {incoming_start}/{mode}/{start}/{disabled}: {actual}/{expected}");}
   let samples=stream_wav_at(&app,&state,processed["compositions"][&transition_root]["content"].clone(),&owner.token,start*1000000/48000).await;
   for(actual,expected)in samples.iter().zip(&expected){assert!((actual-expected).abs()<=1./32768.+2e-7,"processed transition WAV {incoming_start}/{mode}/{start}/{disabled}: {actual}/{expected}");}
   if incoming_start==500000&&!disabled&&start==28800&&(mode=="mixed"||mode=="loudness"){
    let earlier=stream_preview_at(&app,&processed,&owner.token,19200).await;let earlier:Vec<f32>=earlier.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect();
    let mut nested=processed.clone();let mut parent_content=nested["compositions"][&transition_root]["content"].clone();parent_content["tracks"]=json!([]);
    let transform=nested["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"][0]["transform"].clone();
    let instance=|id:&str,start:u64|json!({"id":id,"timing":{"composition":transition_root,"timelineStartUs":start,"sourceInUs":200000,"sourceOutUs":1400000},"transform":transform,"opacity":{"mode":"constant","value":1},"blendMode":"normal"});
    nested["root"]=json!("parent");nested["timelineOrder"]=json!([transition_root,"parent"]);nested["compositions"]["parent"]=json!({"content":parent_content,"instances":[instance("one",2000000),instance("two",2200000)]});
    let expected:Vec<f32>=expected.iter().zip(&earlier).map(|(a,b)|a+b).collect();
    let bytes=stream_preview_at(&app,&nested,&owner.token,115200).await;for(actual,expected)in bytes.chunks_exact(4).zip(&expected){let actual=f32::from_le_bytes(actual.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"nested processed instances {mode}: {actual}/{expected}");}
    let samples=stream_nested_pcm(&app,&state,&nested,&owner.token,2400000).await;assert!(samples.len()>=9600);
    for(actual,expected)in samples[..9600].iter().zip(&expected){assert!((actual-expected).abs()<=2e-7,"nested MKV processed instances {mode}: {actual}/{expected}");}
    assert!(samples[9600..].iter().all(|sample|*sample==0.),"nested export frame padding leaked audio");
   }
   if !disabled&&start==28800{
    let mut with_other=processed.clone();with_other["compositions"][&transition_root]["content"]["tracks"].as_array_mut().unwrap().push(json!({"kind":"audio","id":"other","name":"Other","clips":[{"id":"other-sound","sourceId":"source","audioStream":1,"placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"gain":{"mode":"constant","value":0.5},"pan":{"mode":"constant","value":0},"fadeInTicks":0,"fadeOutTicks":0}]}));
    let bytes=stream_preview_at(&app,&with_other,&owner.token,start).await;
    for(actual,expected)in bytes.chunks_exact(4).zip(&expected){let actual=f32::from_le_bytes(actual.try_into().unwrap());let expected=expected+0.0625*std::f32::consts::FRAC_1_SQRT_2;assert!((actual-expected).abs()<=2e-7,"processed transition replaced unrelated audio: {actual}/{expected}");}
    let samples=stream_wav_at(&app,&state,with_other["compositions"][&transition_root]["content"].clone(),&owner.token,600000).await;
    for(actual,expected)in samples.iter().zip(&expected){let expected=expected+0.0625*std::f32::consts::FRAC_1_SQRT_2;assert!((actual-expected).abs()<=1./32768.+2e-7,"processed transition WAV replaced unrelated audio: {actual}/{expected}");}
   }
  }
 }
 }
 }
 let mut unsupported=transitioned.clone();unsupported["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"][0]["dspEffects"]=json!([{"id":"concat.pitch","params":{"semitones":0}}]);
 unsupported["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"][0]["enabled"]=json!(true);unsupported["compositions"][&transition_root]["content"]["tracks"][picture_track]["clips"][0]["placement"]["speed"]=json!(2);
 let body=json!({"project":unsupported,"startSample":4800,"frames":4800});let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(body.to_string())).unwrap();let response=app.clone().oneshot(request).await.unwrap();assert!(!response.status().is_success(),"packet tempo must not silently bypass processing");
 if let Ok(path)=std::env::var("AUDIO_SOURCE_KEY_WINDOWS_INPUT"){
  let stages:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(stages.as_array().unwrap().len(),8);
  for stage in stages.as_array().unwrap(){
   eprintln!("source key window delivery: {}",stage["name"]);
   let project=&stage["project"];let root=project["root"].as_str().unwrap();let mut plain=project.clone();
   for track in plain["compositions"][root]["content"]["tracks"].as_array_mut().unwrap(){for clip in track["clips"].as_array_mut().unwrap(){for effect in clip["dspEffects"].as_array_mut().unwrap(){assert!(effect.as_object_mut().unwrap().remove("concatSourceKeys").is_some());}}}
   let actual=stream_preview_at(&app,project,&owner.token,14400).await;let reference=stream_preview_at(&app,&plain,&owner.token,14400).await;assert_eq!(actual,reference,"window metadata changed PCM: {}",stage["name"]);assert!(actual.chunks_exact(4).any(|sample|f32::from_le_bytes(sample.try_into().unwrap()).abs()>1e-5));
   if stage["name"].as_str().unwrap().contains("-native-"){
    let actual=stream_nested_pcm(&app,&state,project,&owner.token,300000).await;let reference=stream_nested_pcm(&app,&state,&plain,&owner.token,300000).await;assert_eq!(actual,reference,"window metadata changed native MKV: {}",stage["name"]);assert!(actual.len()>=9600);assert!(actual[9600..].iter().all(|sample|*sample==0.));
   }else{
    let actual=stream_wav_at(&app,&state,project["compositions"][root]["content"].clone(),&owner.token,300000).await;let reference=stream_wav_at(&app,&state,plain["compositions"][root]["content"].clone(),&owner.token,300000).await;assert_eq!(actual,reference,"window metadata changed WAV: {}",stage["name"]);
   }
  }
 }
 if let Ok(path)=std::env::var("CONCAT_STREAM_HISTORY_INPUT"){
  let stages:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();assert_eq!(stages.as_array().unwrap().len(),6);
  for stage in stages.as_array().unwrap(){
   let project=json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":stage["content"],"instances":[]}}});
   let bytes=stream_preview(&app,&project,&owner.token).await;
   let expected=if stage["stream"]==2{-0.25}else{0.125}*std::f32::consts::FRAC_1_SQRT_2;
   for sample in bytes.chunks_exact(4){let actual=f32::from_le_bytes(sample.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"history {} preview {actual}/{expected}",stage["name"]);}
   let samples=stream_wav(&app,&state,stage["content"].clone(),&owner.token).await;
   for actual in samples{assert!((actual-expected).abs()<=1./32768.+2e-7,"history {} WAV {actual}/{expected}",stage["name"]);}
  }
 }

}

async fn stream_preview(app:&axum::Router,project:&Value,token:&str)->Vec<u8>{stream_preview_at(app,project,token,4800).await}
async fn stream_preview_at(app:&axum::Router,project:&Value,token:&str,start:u64)->Vec<u8>{
 let body=json!({"project":project,"startSample":start,"frames":4800});
 let request=Request::builder().method("POST").uri("/api/compositions/audio-preview").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(body.to_string())).unwrap();
 let response=app.clone().oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1000000).await.unwrap();assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));assert_eq!(bytes.len(),4800*8);bytes.to_vec()
}
async fn stream_wav(app:&axum::Router,state:&video_kadr_backend::state::AppState,content:Value,token:&str)->Vec<f32>{stream_wav_at(app,state,content,token,100000).await}
async fn stream_wav_at(app:&axum::Router,state:&video_kadr_backend::state::AppState,content:Value,token:&str,start:u64)->Vec<f32>{
 let body=json!({"schemaVersion":1,"composition":content,"output":{"profile":{"container":"audio","codec":"wav"},"range":{"startTicks":start,"endTicks":start+100000}}});
 let request=Request::builder().method("POST").uri("/api/compositions/render").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(body.to_string())).unwrap();let(status,accepted,_)=support::send(app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
 let id=accepted["jobId"].as_str().unwrap();let deadline=std::time::Instant::now()+Duration::from_secs(240);let mut last=Value::Null;eprintln!("WAV export {id}, source start {start}");
 let done=loop{let request=Request::builder().uri(format!("/api/jobs/{id}")).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(_,result,_)=support::send(app,request).await;if result["status"]!=last{eprintln!("WAV {id} status {}",result["status"]);last=result["status"].clone();}if matches!(result["status"].as_str(),Some("done"|"error"|"cancelled"|"interrupted")){break result}assert!(std::time::Instant::now()<deadline,"WAV export observation timed out for {id}: {result}");tokio::time::sleep(Duration::from_millis(20)).await;};
 assert_eq!(done["status"],"done","{done}");
 let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(state.outputs_dir().join(done["result"]["filename"].as_str().unwrap())).args(["-f","f32le","-c:a","pcm_f32le","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),4800*8);decoded.stdout.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect()
}
async fn stream_nested_pcm(app:&axum::Router,state:&video_kadr_backend::state::AppState,project:&Value,token:&str,start:u64)->Vec<f32>{stream_nested_pcm_with_rgb(app,state,project,token,start,None).await}
async fn stream_nested_pcm_with_rgb(app:&axum::Router,state:&video_kadr_backend::state::AppState,project:&Value,token:&str,start:u64,expected_rgb:Option<[u8;3]>)->Vec<f32>{stream_nested_pcm_with_expected_pixels(app,state,project,token,start,expected_rgb.map(|rgb|rgb.repeat(16*16))).await}
async fn stream_nested_pcm_with_expected_pixels(app:&axum::Router,state:&video_kadr_backend::state::AppState,project:&Value,token:&str,start:u64,expected_rgb:Option<Vec<u8>>)->Vec<f32>{
 let body=json!({"schemaVersion":1,"mode":"video_audio","project":project,"compositionId":project["root"],"startUs":start,"endUs":start+100000});
 let request=Request::builder().method("POST").uri("/api/compositions/nested-export").header("content-type","application/json").header("authorization",format!("Bearer {token}")).body(Body::from(body.to_string())).unwrap();let(status,accepted,_)=support::send(app,request).await;assert_eq!(status,StatusCode::ACCEPTED,"{accepted}");
 let id=accepted["jobId"].as_str().unwrap();eprintln!("native PCM export {id}, source start {start}");let deadline=std::time::Instant::now()+Duration::from_secs(240);
 let done=loop{let request=Request::builder().uri(format!("/api/jobs/{id}")).header("authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(_,result,_)=support::send(app,request).await;if matches!(result["status"].as_str(),Some("done"|"error"|"cancelled"|"interrupted")){break result}assert!(std::time::Instant::now()<deadline,"native PCM export observation timed out for {id}: {result}");tokio::time::sleep(Duration::from_millis(20)).await;};
 assert_eq!(done["status"],"done","{done}");
 if let Some(expected)=expected_rgb{
  let pixels=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(state.outputs_dir().join(done["result"]["filename"].as_str().unwrap())).args(["-map","0:v:0","-frames:v","1","-f","rawvideo","-pix_fmt","rgb24","pipe:1"]).output().unwrap();assert!(pixels.status.success());assert_eq!(pixels.stdout.len(),16*16*3);assert_eq!(pixels.stdout,expected,"source geometry/color changed encoded frame");
 }
 let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(state.outputs_dir().join(done["result"]["filename"].as_str().unwrap())).args(["-map","0:a:0","-vn","-f","f32le","-c:a","pcm_f32le","pipe:1"]).output().unwrap();assert!(decoded.status.success(),"{}",String::from_utf8_lossy(&decoded.stderr));decoded.stdout.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect()
}
#[tokio::test]
#[ignore="requires actual mixed-layout FFmpeg media and private APIs"]
async fn selected_stereo_44100_survives_unrelated_mono_surround_and_unknown_first_stream(){
 let(mut state,_dir)=support::make_state(true,true).await;let caps=std::sync::Arc::make_mut(&mut state.tools);caps.ffmpeg_encoders.push("pcm_s16le".into());caps.ffmpeg_muxers.push("wav".into());
 let owner=state.db.register_auth_user("layout-owner","public synthetic password").await.unwrap().unwrap();let space=state.db.create_space(&owner.user.username,"Layouts").await.unwrap();let app=support::router(state.clone());
 for(label,channels,unknown)in [("mono",1,false),("surround",6,false),("unknown",6,true),("offset",1,false)]{
  let id=format!("layout-{label}");let filename=format!("{id}.mov");let source=state.sources_dir().join(&filename);
  let first=if channels==1{"aevalsrc=0.125:s=48000:d=1"}else{"aevalsrc=0.125|0.125|0.125|0.125|0.125|0.125:c=5.1:s=48000:d=1"};
  let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-f","lavfi","-i","color=c=black:s=16x16:r=4:d=1","-f","lavfi","-i",first,"-f","lavfi","-i","aevalsrc=-0.25|-0.125:s=44100:d=1","-map","0:v:0","-map","1:a:0","-map","2:a:0","-c:v","qtrle","-c:a:0","pcm_s16le","-c:a:1","pcm_f32le"]).arg(&source).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
  if label=="offset"{
   let shifted=state.staging_dir().join("offset-streams.mkv");
   let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-y","-i"]).arg(&source).args(["-filter_complex","[0:v]setpts=PTS+5/TB[v];[0:a:0]asetpts=PTS+5.25/TB[a];[0:a:1]asetpts=PTS+5.5/TB[b]","-map","[v]","-map","[a]","-map","[b]","-c:v","ffv1","-c:a:0","pcm_s16le","-c:a:1","pcm_f32le","-copyts"]).arg(&shifted).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));std::fs::rename(shifted,&source).unwrap();
  }
  if unknown{
   let tags=std::process::Command::new("ffprobe").args(["-v","error","-select_streams","1","-show_entries","stream=codec_tag_string","-of","json"]).arg(&source).output().unwrap();assert!(tags.status.success());let tags:Value=serde_json::from_slice(&tags.stdout).unwrap();let tag=tags["streams"][0]["codec_tag_string"].as_str().unwrap().as_bytes();assert_eq!(tag.len(),4);
   let mut bytes=std::fs::read(&source).unwrap();let offset=bytes.windows(4).position(|bytes|bytes==tag).expect("first audio sample entry");bytes[offset..offset+4].copy_from_slice(b"zzzz");std::fs::write(&source,bytes).unwrap();
   let probe=std::process::Command::new("ffprobe").args(["-v","error","-select_streams","1","-show_entries","stream=index,codec_name","-of","json"]).arg(&source).output().unwrap();assert!(probe.status.success());let probe:Value=serde_json::from_slice(&probe.stdout).unwrap();assert_eq!(probe["streams"][0]["index"],1);assert!(probe["streams"][0]["codec_name"].is_null()||probe["streams"][0]["codec_name"]=="unknown");
  }
  let probe=probe_pcm_metadata_for_stream(&ProcessRuntime::local_default(),Path::new("ffprobe"),&source,Duration::from_secs(20),&CancellationToken::new(),Some(2)).await.unwrap();let stream=&probe.streams[0];assert_eq!(stream.index,2);assert_eq!(stream.channels,Some(2));assert_eq!(stream.sample_rate,Some(44100));assert_eq!(stream.sample_format.as_deref(),Some("flt"));
  if label=="offset"{
   assert!(stream.start_time_seconds.unwrap()>=5.5);assert!(probe.container.start_time_seconds.unwrap()>=5.0);
   let spec=PcmConformSpec{start_sample:4800,end_sample:9600,channels:1,staging_bytes:1024*1024};let cancel=CancellationToken::new();
   let mut legacy=conform_pcm(&state.process_runtime,Path::new("ffmpeg"),&source,&state.staging_dir(),spec,Duration::from_secs(20),&cancel).await.unwrap();
   let mut selected=conform_pcm_stream(&state.process_runtime,Path::new("ffmpeg"),&source,&state.staging_dir(),spec,Duration::from_secs(20),&cancel,1).await.unwrap();
   assert_eq!(legacy.read(100000).unwrap(),(1,[0.,0.]),"default media clock must retain leading silence");
   assert_eq!(selected.read(100000).unwrap(),(1,[0.125,0.]),"explicit stream clock must begin at its first sample");
  }
  state.library.add(video_kadr_backend::library::MediaEntry::from_result("source",&json!({"id":id,"filename":filename,"url":format!("/files/sources/{filename}"),"mediaType":"video","width":16,"height":16,"fps":4,"duration":1,"hasAudio":true,"audioChannels":channels,"acodec":"pcm_s16le"}))).await;
  state.db.create_space_composition_project("Layout claim",&json!({"schemaVersion":1,"sources":{id.clone():{"id":id}}}),std::slice::from_ref(&id),&owner.user.username,&space.id).await.unwrap();
  let reference=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&source).args(["-map","0:2","-vn","-sn","-dn","-ar","48000","-ac","2","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(reference.status.success(),"reference {label}: {}",String::from_utf8_lossy(&reference.stderr));assert!(reference.stdout.len()>=9600*8);
  let reference:Vec<f32>=reference.stdout[4800*8..9600*8].chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect();
  assert!(reference.chunks_exact(2).all(|pair|pair[0]< -0.2&&pair[1]< -0.1&&(pair[0]*0.5-pair[1]).abs()<=2e-7));
  for kind in ["audio","video"]{
   let import=json!({"schemaVersion":1,"mediaBindings":{"m":{"id":id,"kind":"video","durationTicks":1000000,"width":16,"height":16,"hasAudio":true,"audioChannels":channels}},"document":{"version":1,"media":[{"id":"m","kind":"video","path":"/never/open"}],"timelines":[{"id":"tl","video":{"width":16,"height":16,"rateNum":4,"rateDen":1},"tracks":[{"id":"t"}],"clips":[{"id":"selected","kind":kind,"trackId":"t","mediaId":"m","duration":1,"audioStream":2}]}]}});
   let request=Request::builder().method("POST").uri("/api/compositions/concat-project/import").header("content-type","application/json").header("authorization",format!("Bearer {}",owner.token)).body(Body::from(import.to_string())).unwrap();let(_,report,_)=support::send(&app,request).await;assert_eq!(report["status"],"converted","{label}/{kind}: {report}");let project=report["project"].clone();
   let bytes=stream_preview(&app,&project,&owner.token).await;for(frame,samples)in bytes.chunks_exact(8).enumerate(){let left=f32::from_le_bytes(samples[..4].try_into().unwrap());let right=f32::from_le_bytes(samples[4..].try_into().unwrap());assert!((left-reference[frame*2]).abs()<=2e-7&&(right-reference[frame*2+1]).abs()<=2e-7,"{label}/{kind} stereo lost at{frame}: {left}/{right}, ref {:?}",&reference[frame*2..frame*2+2]);}
   let root=project["root"].as_str().unwrap();let samples=stream_wav(&app,&state,project["compositions"][root]["content"].clone(),&owner.token).await;for(frame,pair)in samples.chunks_exact(2).enumerate(){assert!((pair[0]-reference[frame*2]).abs()<=1./32768.+2e-7&&(pair[1]-reference[frame*2+1]).abs()<=1./32768.+2e-7,"WAV {label}/{kind} stereo lost at{frame}: {pair:?}");}
   if label=="offset"{
    let mut faster=project.clone();faster["compositions"][root]["content"]["tracks"][0]["clips"][0]["placement"]["speed"]=json!(2);
    let entry=state.library.get(&id).await.unwrap();let current_source=state.library.resolve_media_path(&entry).await.unwrap();
    let reference=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(current_source).args(["-map","0:2","-vn","-af","asetpts=PTS-STARTPTS,aresample=48000:async=1:first_pts=0,atrim=start=0:end=1,asetpts=PTS-STARTPTS,atempo=2,aresample=48000:async=1:first_pts=0,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,atrim=duration=0.5,asetpts=PTS-STARTPTS,atrim=start_sample=4800:end_sample=9600","-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();assert!(reference.status.success());let reference:Vec<f32>=reference.stdout.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect();assert_eq!(reference.len(),9600);
    let bytes=stream_preview(&app,&faster,&owner.token).await;let samples:Vec<f32>=bytes.chunks_exact(4).map(|sample|f32::from_le_bytes(sample.try_into().unwrap())).collect();
    for(actual,expected)in samples.iter().zip(&reference){assert!((actual-expected).abs()<=2e-7,"offset tempo preview {kind}: {actual}/{expected}");}
    let samples=stream_wav(&app,&state,faster["compositions"][root]["content"].clone(),&owner.token).await;
    for(actual,expected)in samples.iter().zip(&reference){assert!((actual-expected).abs()<=1./32768.+2e-7,"offset tempo WAV {kind}: {actual}/{expected}");}
   }
   let mut packet=project.clone();packet["compositions"][root]["content"]["tracks"][0]["clips"][0]["dspEffects"]=json!([{"id":"concat.pitch","params":{"semitones":0}}]);
   let packet_bytes=stream_preview(&app,&packet,&owner.token).await;
   let typed:video_kadr_backend::domain::nested_composition::NestedCompositionProject=serde_json::from_value(packet.clone()).unwrap();
   // Private export may relocate the claimed source into its space storage.
   let entry=state.library.get(&id).await.unwrap();
   let source=state.library.resolve_media_path(&entry).await.unwrap();
   let binding=video_kadr_backend::tools::native_audio_export::PcmSource::from_probe(source.clone(),probe.clone()).unwrap();let bindings=std::collections::BTreeMap::from([(video_kadr_backend::domain::composition::SourceId::parse(&id).unwrap(),binding)]);let(progress,_)=tokio::sync::mpsc::unbounded_channel();
   assert!(source.is_file(),"owned source disappeared before packet reference: {}",source.display());
   let mixed=video_kadr_backend::tools::native_audio_export::mix_project_audio_tracks(&state.process_runtime,Path::new("ffmpeg"),&state.staging_dir(),&typed,root,100000..200000,4800,&bindings,512*1024*1024,Duration::from_secs(20),&CancellationToken::new(),&progress).await.unwrap();
   let selected_reference=std::fs::read(mixed.path()).unwrap();assert_eq!(selected_reference.len(),packet_bytes.len());
   for(actual,expected)in packet_bytes.chunks_exact(4).zip(selected_reference.chunks_exact(4)){let actual=f32::from_le_bytes(actual.try_into().unwrap());let expected=f32::from_le_bytes(expected.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"packet shape {label}/{kind}: {actual}/{expected}");}
   let samples=stream_wav(&app,&state,packet["compositions"][root]["content"].clone(),&owner.token).await;
   for(actual,expected)in samples.iter().zip(selected_reference.chunks_exact(4)){let expected=f32::from_le_bytes(expected.try_into().unwrap()).clamp(-1.,1.-1./32768.);assert!((actual-expected).abs()<=1./32768.+2e-7,"packet WAV shape {label}/{kind}: {actual}/{expected}");}
   if channels==1&&kind=="audio"{
    let mut positive=packet.clone();let clip=&mut positive["compositions"][root]["content"]["tracks"][0]["clips"][0];clip["id"]=json!("positive");clip["audioStream"]=json!(1);clip["gain"]=json!({"mode":"constant","value":0.5});
    let positive_bytes=stream_preview(&app,&positive,&owner.token).await;
    let mut both=packet.clone();both["compositions"][root]["content"]["tracks"][0]["clips"].as_array_mut().unwrap().push(positive["compositions"][root]["content"]["tracks"][0]["clips"][0].clone());
    let both_bytes=stream_preview(&app,&both,&owner.token).await;
    for((actual,a),b)in both_bytes.chunks_exact(4).zip(packet_bytes.chunks_exact(4)).zip(positive_bytes.chunks_exact(4)){let actual=f32::from_le_bytes(actual.try_into().unwrap());let expected=f32::from_le_bytes(a.try_into().unwrap())+f32::from_le_bytes(b.try_into().unwrap());assert!((actual-expected).abs()<=2e-7,"mixed scalar/rate/channel cache leaked: {actual}/{expected}");}
   }

  }
 }
}
