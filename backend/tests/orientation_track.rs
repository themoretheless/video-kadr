use video_kadr_backend::domain::{spherical_video::Orientation,orientation_track::{capture_to_clip_track,compress,slerp,angular_error_degrees}};
fn q(angle:f64)->[f64;4]{[(angle*0.5).cos(),0.,(angle*0.5).sin(),0.]}
fn error(track:&[Orientation],at:Orientation)->f64{let i=track.partition_point(|s|s.time_seconds<at.time_seconds).clamp(1,track.len()-1);let(a,b)=(track[i-1],track[i]);angular_error_degrees(slerp(a.quaternion,b.quaternion,(at.time_seconds-a.time_seconds)/(b.time_seconds-a.time_seconds)),at.quaternion)}
#[test]fn high_frequency_motion_survives_adaptive_compression_while_slow_uniform_motion_is_small(){
 let native:Vec<_>=(0..=10000).map(|i|{let t=i as f64/1000.;Orientation{time_seconds:t,quaternion:q(t*0.2+if (4. ..=4.2).contains(&t){0.01*(t*400.).sin()}else{0.})}}).collect();
 let compressed=compress(&native,0.005,20000).unwrap();assert!(compressed.len()>50&&compressed.len()<1000);for s in &native{assert!(error(&compressed,*s)<=0.0050001)}
 let constant:Vec<_>=(0..=10000).map(|i|Orientation{time_seconds:i as f64/1000.,quaternion:q(i as f64/10000.)}).collect();assert_eq!(compress(&constant,0.005,20000).unwrap().len(),2);
}
#[test]fn nonlinear_capture_clock_knots_antipodes_and_revolutions_are_preserved(){
 let native:Vec<_>=(0..=4000).map(|i|{let a=i as f64/1000.;let mut quaternion=q(a*4.);if i%2==0{quaternion=quaternion.map(|v|-v)}Orientation{time_seconds:a,quaternion}}).collect();
 let retimed=capture_to_clip_track(&native,&[1.,1.5,3.],1.,2.).unwrap();assert_eq!(retimed.first().unwrap().time_seconds,0.);assert_eq!(retimed.last().unwrap().time_seconds,2.);let compressed=compress(&retimed,0.0001,20000).unwrap();assert!(compressed.len()>3);for s in retimed{assert!(error(&compressed,s)<=0.000101)}
}
#[test]fn impossible_storage_or_invalid_clocks_fail_instead_of_silent_motion_loss(){
 let native:Vec<_>=(0..100).map(|i|Orientation{time_seconds:i as f64/100.,quaternion:q(if i%2==0{0.}else{1.})}).collect();assert!(compress(&native,0.001,2).is_err());assert!(capture_to_clip_track(&native,&[0.,0.],1.,1.).is_err());assert!(capture_to_clip_track(&native,&[0.,0.5],1.,1e308).is_err());
}

#[test]fn precise_exposure_breakpoints_and_guard_clock_survive_retimation_and_clock_compression(){
 use video_kadr_backend::domain::orientation_track::{CaptureTime,capture_to_clip_from_clock,compress_clock,pad_clock_knots};
 let clock=[CaptureTime{time_seconds:0.,capture_seconds:0.4},CaptureTime{time_seconds:0.005,capture_seconds:0.404},CaptureTime{time_seconds:0.01,capture_seconds:0.41}];
 let padded=pad_clock_knots(&clock,0.01).unwrap();let compressed=compress_clock(&padded,1e-7).unwrap();assert!(compressed.iter().any(|k|(k.time_seconds-0.015).abs()<1e-12));
 let native:Vec<_>=(380..=430).map(|i|Orientation{time_seconds:i as f64/1000.,quaternion:q(i as f64/100.)}).collect();let retimed=capture_to_clip_from_clock(&native,&compressed).unwrap();let at=retimed.iter().find(|k|(k.time_seconds-0.015).abs()<1e-12).unwrap();assert!(angular_error_degrees(at.quaternion,q(4.04))<1e-5);
 let zigzag:Vec<_>=(0..10000).map(|i|CaptureTime{time_seconds:i as f64/10000.,capture_seconds:i as f64/10000.+if i%2==0{0.00001}else{0.}}).collect();assert!(compress_clock(&zigzag,1e-8).unwrap().len()>9000);
}
