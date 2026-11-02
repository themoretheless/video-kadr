use video_kadr_backend::domain::{concat_video_pacer::{VideoPacer,DecodedFrame,FrameTimestamp},concat_cutout_clock::AnalysisRun,media_probe::Rational};
fn frames(ticks:&[i64],denominator:i64,start_ticks:i64)->Vec<anyhow::Result<DecodedFrame<usize>>>{ticks.iter().enumerate().map(|(i,t)|Ok(DecodedFrame{timestamp:Some(FrameTimestamp{ticks:*t,time_base:Rational{numerator:1,denominator},start_ticks}),picture:i})).collect()}
fn ordinals(input:Vec<anyhow::Result<DecodedFrame<usize>>>,start_millis:u64,count:usize)->Vec<usize>{let mut pacer=VideoPacer::new(input.into_iter(),AnalysisRun{start_millis,frames:count}).unwrap();let mut out=vec![];while let Some(frame)=pacer.next_sample().unwrap(){out.push(frame.source_ordinal)}out}
#[test]
fn vfr_holds_the_current_picture_and_stops_at_original_eof_horizon(){assert_eq!(ordinals(frames(&[0,80,240],1000,0),0,5),vec![0,1,1,2]);assert_eq!(ordinals(frames(&[5000,5080,5240],1000,5000),0,5),vec![0,1,1,2]);assert_eq!(ordinals(frames(&[50,180],1000,0),0,4),vec![0,0,1]);}
#[test]
fn sub_microsecond_future_picture_is_not_rounded_onto_grid(){assert_eq!(ordinals(frames(&[0,100000001,200000000],1000000000,0),0,4),vec![0,0,2]);}
#[test]
fn decoder_errors_propagate_and_unknown_pts_follow_source_order(){let mut pacer=VideoPacer::new(vec![Err::<DecodedFrame<usize>,_>(anyhow::anyhow!("decode failed"))].into_iter(),AnalysisRun{start_millis:0,frames:1}).unwrap();assert!(pacer.next_sample().is_err());let input=(0..2).map(|i|Ok(DecodedFrame{timestamp:None,picture:i})).collect();assert_eq!(ordinals(input,0,3),vec![1,1,1]);}
#[test]
#[ignore="requires real FFmpeg/FFprobe"]
fn encoded_vfr_video_selects_exact_decoded_colors_on_ten_hz_grid(){
 let dir=tempfile::tempdir().unwrap();let raw=dir.path().join("source.rgba");let pixels:Vec<u8>=[[255,0,0,255],[0,255,0,255],[0,0,255,255]].into_iter().flat_map(|rgba|rgba.repeat(4)).collect();std::fs::write(&raw,pixels).unwrap();let video=dir.path().join("vfr.mkv");
 let encoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","rawvideo","-pix_fmt","rgba","-s","2x2","-r","25","-i"]).arg(&raw).args(["-vf","setpts=if(eq(N\\,0)\\,0\\,if(eq(N\\,1)\\,2\\,6))","-fps_mode","passthrough","-c:v","ffv1","-pix_fmt","bgra"]).arg(&video).output().unwrap();assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
 let probe=std::process::Command::new("ffprobe").args(["-v","error","-select_streams","v:0","-show_frames","-show_streams","-show_entries","frame=best_effort_timestamp:stream=time_base,start_pts","-of","json"]).arg(&video).output().unwrap();assert!(probe.status.success());let info:serde_json::Value=serde_json::from_slice(&probe.stdout).unwrap();let base=Rational::parse(info["streams"][0]["time_base"].as_str().unwrap()).unwrap();let origin=info["streams"][0]["start_pts"].as_i64().unwrap_or(0);
 let decoded=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&video).args(["-map","0:v:0","-fps_mode","passthrough","-pix_fmt","rgba","-f","rawvideo","pipe:1"]).output().unwrap();assert!(decoded.status.success());assert_eq!(decoded.stdout.len(),3*16);
 let frames=info["frames"].as_array().unwrap().iter().zip(decoded.stdout.chunks_exact(16)).map(|(frame,pixels)|Ok(DecodedFrame{timestamp:Some(FrameTimestamp{ticks:frame["best_effort_timestamp"].as_i64().unwrap(),time_base:base,start_ticks:origin}),picture:pixels.to_vec()}));let mut pacer=VideoPacer::new(frames,AnalysisRun{start_millis:0,frames:5}).unwrap();let mut colors=vec![];while let Some(frame)=pacer.next_sample().unwrap(){colors.push(frame.picture[..4].to_vec())}assert_eq!(colors,vec![vec![255,0,0,255],vec![0,255,0,255],vec![0,255,0,255],vec![0,0,255,255]]);
}

#[test]
fn decoded_frame_count_is_not_confused_with_analysis_frame_budget(){
 let input=(0..=600000).map(|ticks|Ok(DecodedFrame{timestamp:Some(FrameTimestamp{ticks,time_base:Rational{numerator:1,denominator:30},start_ticks:0}),picture:ticks}));
 let mut pacer=VideoPacer::new(input,AnalysisRun{start_millis:20000000,frames:1}).unwrap();assert_eq!(pacer.next_sample().unwrap().unwrap().source_ordinal,600000);
}
