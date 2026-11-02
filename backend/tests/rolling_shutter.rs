use video_kadr_backend::domain::{flat_surface,composition::VideoEffect,group_surface::GroupSurface,rolling_shutter::{RollingShutter,ScanDirection},spherical_video::{SphericalVideo,Input,Output,Orientation},orientation_track::CaptureTime};
const W:usize=192;const H:usize=96;
fn track()->Vec<Orientation>{(0..=200).map(|i|{let t=0.4+i as f64/1000.;let a=6.*(t-0.5)*0.5;Orientation{time_seconds:t,quaternion:[a.cos(),0.,a.sin(),0.]}}).collect()}
fn input(readout:f64,directions:[ScanDirection;2])->GroupSurface{input_with_pose(readout,directions,0.)}
fn input_with_pose(readout:f64,directions:[ScanDirection;2],base_angle:f64)->GroupSurface{
 let mut image=GroupSurface::transparent(W,H).unwrap();
 for y in 0..H{for x in 0..W{
  let lens=usize::from(x>=W/2);let center=if lens==0{0.25}else{0.75};let dx=((x as f64+0.5)/W as f64-center)/0.25;let dy=-( (y as f64+0.5)/H as f64-0.5)/0.5;let r=dx.hypot(dy);if r>1.{continue}
  let theta=r*190f64.to_radians()*0.5;let body=if r<1e-12{[0.,0.,1.]}else{[theta.sin()*dx/r,theta.sin()*dy/r,theta.cos()]};
  let body=if lens==0{body}else{[-body[0],body[1],-body[2]]};
  let sign=if directions[lens]==ScanDirection::TopToBottom{1.}else{-1.};let angle=base_angle+6.*sign*((y as f64+0.5)/H as f64-0.5)*readout;let(c,s)=(angle.cos(),angle.sin());let d=[c*body[0]+s*body[2],body[1],-s*body[0]+c*body[2]];
  let lon=d[0].atan2(d[2]);let lat=d[1].clamp(-1.,1.).asin();let at=(y*W+x)*4;
  image.rgba[at..at+4].copy_from_slice(&[(128.+100.*(lon*7.+lat*2.).sin())as u8,(128.+100.*(lat*9.-lon).sin())as u8,(128.+90.*(lon*3.-lat*4.).cos())as u8,255]);
 }}image
}
fn render(mut source:GroupSurface,rs:Option<RollingShutter>)->GroupSurface{
 let p=SphericalVideo{input:Input::DualFisheye,output:Output::Equirectangular,orientation:track(),rolling_shutter:rs,..Default::default()};
 flat_surface::apply_effect_stack(&mut source,&[VideoEffect::SphericalVideo{parameters:p,animation:None}],500000,&[],&mut||Ok(())).unwrap();source
}
fn mse(a:&GroupSurface,b:&GroupSurface)->f64{let mut sum=0.;let mut count=0;for(a,b)in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)){if a[3]>250&&b[3]>250{for k in 0..3{sum+=(a[k]as f64-b[k]as f64).powi(2);count+=1;}}}assert!(count>W*H*2);sum/count as f64}
#[test]fn source_row_fixed_point_correction_reduces_rotational_shutter_error_for_both_scans(){
 for dirs in [[ScanDirection::TopToBottom;2],[ScanDirection::BottomToTop,ScanDirection::TopToBottom]]{
  let ideal=render(input(0.,dirs),None);let raw=input(0.04,dirs);let distorted=render(raw.clone(),None);let corrected=render(raw,Some(RollingShutter{readout_seconds:0.04,scan_directions:dirs,..Default::default()}));let before=mse(&ideal,&distorted);let after=mse(&ideal,&corrected);println!("source-row rolling shutter MSE {before:.4} -> {after:.4}");assert!(after<before*0.35,"{before} -> {after}");assert!(corrected.rgba.chunks_exact(4).filter(|p|p[3]>250).count()>W*H*99/100);
 }
}
#[test]fn physical_capture_clock_is_inverted_instead_of_assuming_video_seconds(){
 let rs=RollingShutter{readout_seconds:0.02,capture_clock:vec![CaptureTime{time_seconds:0.,capture_seconds:2.},CaptureTime{time_seconds:1.,capture_seconds:4.}],..Default::default()};
 assert!((rs.row_time(0.5,1.,0).unwrap()-0.505).abs()<1e-12);assert!((rs.row_time(0.5,0.,0).unwrap()-0.495).abs()<1e-12);
 let reversed=RollingShutter{scan_directions:[ScanDirection::BottomToTop;2],..rs.clone()};assert!((reversed.row_time(0.5,1.,0).unwrap()-0.495).abs()<1e-12);
 assert!(rs.row_time(0.,0.,0).is_err());
}
#[test]fn missing_guard_bands_invalid_rows_and_already_stitched_inputs_are_explicit_errors(){
 let bad=SphericalVideo{rolling_shutter:Some(RollingShutter{readout_seconds:0.02,..Default::default()}),orientation:track(),..Default::default()};assert!(bad.validate().is_err());
 let mut source=input(0.02,[ScanDirection::TopToBottom;2]);let p=SphericalVideo{input:Input::DualFisheye,orientation:track(),rolling_shutter:Some(RollingShutter{readout_seconds:0.02,..Default::default()}),..Default::default()};assert!(flat_surface::apply_effect_stack(&mut source,&[VideoEffect::SphericalVideo{parameters:p,animation:None}],400000,&[],&mut||Ok(())).is_err());
 for n in [-1.,f64::NAN,0.101]{assert!(RollingShutter{readout_seconds:n,..Default::default()}.validate().is_err())}
}

#[test]fn source_presentation_clock_uses_actual_held_frame_when_output_fps_differs(){
 use video_kadr_backend::domain::rolling_shutter::SourceFrameClock;
 let clock=SourceFrameClock{numerator:50,denominator:1,source_start_seconds:0.};
 assert!((clock.presentation_time(0.533333).unwrap()-0.52).abs()<1e-12);
 let trim=SourceFrameClock{source_start_seconds:0.101,..clock};assert!((trim.presentation_time(0.).unwrap()+0.001).abs()<1e-12);
 let ideal=render(input(0.,[ScanDirection::TopToBottom;2]),None);
 let held=input_with_pose(0.,[ScanDirection::TopToBottom;2],6.*(0.52-0.5));
 let run=|source_clock|{let mut s=held.clone();let p=SphericalVideo{input:Input::DualFisheye,output:Output::Equirectangular,orientation:track(),source_frame_clock:source_clock,..Default::default()};flat_surface::apply_effect_stack(&mut s,&[VideoEffect::SphericalVideo{parameters:p,animation:None}],533333,&[],&mut||Ok(())).unwrap();s};
 let wrong=run(None);let correct=run(Some(clock));assert!(mse(&ideal,&correct)<mse(&ideal,&wrong)*0.2);
}

#[test]fn semantic_camera_keys_use_the_same_gyro_virtual_frame_as_the_renderer(){
 use video_kadr_backend::domain::{spherical_tracking::Centre,spherical_video::StabilizationMode,rolling_shutter::SourceFrameClock};
 let q=[std::f64::consts::FRAC_1_SQRT_2,0.,std::f64::consts::FRAC_1_SQRT_2,0.];let p=SphericalVideo{orientation:vec![Orientation{time_seconds:0.,quaternion:q}],..Default::default()};let c=Centre{pan:0.,tilt:0.,fov:Some(70.)};let world=p.map_tracking_centre(c,0.).unwrap();assert!((world.pan-90.).abs()<1e-9&&world.fov==Some(70.));
 for mode in [StabilizationMode::Horizon,StabilizationMode::Smooth,StabilizationMode::Disabled]{let mapped=SphericalVideo{stabilization_mode:mode,..p.clone()}.map_tracking_centre(c,0.).unwrap();assert!(mapped.pan.abs()<1e-9&&mapped.tilt.abs()<1e-9)}
 let moving=SphericalVideo{orientation:vec![Orientation{time_seconds:0.,quaternion:[1.,0.,0.,0.]},Orientation{time_seconds:1.,quaternion:q}],source_frame_clock:Some(SourceFrameClock{numerator:2,denominator:1,source_start_seconds:0.}),..Default::default()};assert!(moving.map_tracking_centre(c,0.2).unwrap().pan.abs()<1e-9);
}
#[test]fn first_stream_semantic_seed_projects_to_sphere_without_overlap_attenuation(){
 use video_kadr_backend::domain::spherical_video::{SourceLayout,StabilizationMode};
 let source=input(0.,[ScanDirection::TopToBottom;2]);let body=SphericalVideo{input:Input::DualFisheye,source_layout:SourceLayout::DualVideoStreams,output:Output::Equirectangular,stabilization_mode:StabilizationMode::Disabled,..Default::default()};
 let mut mask=vec![0;256*256];for y in 120..136{for x in 245..256{mask[y*256+x]=255;}}
 let projected=body.project_tracking_mask(&mask,256,256,0.5,&source,&mut||Ok(())).unwrap();assert!(projected[128*256+192]>240);assert_eq!(projected[128*256+0],0);assert_eq!(projected[128*256+128],0);
 let mut packed=vec![0;256*256];for y in 118..138{for x in 59..69{packed[y*256+x]=255;}}
 let projected=SphericalVideo{source_layout:SourceLayout::Packed,..body}.project_tracking_mask(&packed,256,256,0.5,&source,&mut||Ok(())).unwrap();assert!(projected[128*256+128]>240);assert_eq!(projected[128*256+0],0);
}

#[test]
#[ignore="requires explicitly supplied local Insta360 footage inventory"]
fn actual_x6_sensor_row_correction_uses_metadata_and_padded_native_gyro(){
 use std::{fs::File,io::Read,path::Path,sync::atomic::AtomicBool,time::Duration};
 use video_kadr_backend::domain::{insta360_metadata,insta360_gyro::{self,GyroRequest},insta360_camera_profile,group_media,video_clock::FrameTime,spherical_video::{SourceLayout,StabilizationMode}};
 let inventory=std::env::var("VIDEO_KADR_INSTA360_INVENTORY").unwrap();let mut text=String::new();File::open(inventory).unwrap().read_to_string(&mut text).unwrap();let json:serde_json::Value=serde_json::from_str(&text).unwrap();let mut rows:Vec<_>=json["files"].as_array().unwrap().iter().filter(|r|r["bytes"].as_u64().unwrap_or(0)>1_000_000).collect();rows.sort_by_key(|r|r["bytes"].as_u64().unwrap());
 let path=rows[0]["path"].as_str().unwrap();let metadata=insta360_metadata::inspect(&mut File::open(path).unwrap()).unwrap().unwrap();let profile=insta360_camera_profile::inspect(&metadata).profile.unwrap();let readout=metadata.rolling_shutter_time.unwrap()/1000.;assert!(readout>0.&&readout<0.04);
 let report=insta360_gyro::analyze(&mut File::open(path).unwrap(),&GyroRequest{duration_seconds:0.5,orientation_tolerance_degrees:Some(0.005),motion_padding_seconds:0.1,..Default::default()}).unwrap().unwrap();assert_eq!(report.synchronization,"exposure_midpoint_linear");
 let packed=group_media::decode_source_frame_policy_layout(Path::new("ffmpeg"),Path::new(path),FrameTime::Microseconds(200000),1024,512,Duration::from_secs(30),&AtomicBool::new(false),true,true).unwrap();
 let base=SphericalVideo{input:Input::DualFisheye,source_layout:SourceLayout::DualVideoStreams,output:Output::Equirectangular,camera_profile:Some(profile),orientation:report.orientation,orientation_offset_seconds:report.orientation_offset_seconds,source_frame_clock:report.source_frame_clock,stabilization_mode:StabilizationMode::Horizon,..Default::default()};
 let run=|params:SphericalVideo|{let mut frame=packed.clone();flat_surface::apply_effect_stack(&mut frame,&[VideoEffect::SphericalVideo{parameters:params,animation:None}],200000,&[],&mut||Ok(())).unwrap();frame};let baseline=run(base.clone());let corrected=run(SphericalVideo{rolling_shutter:Some(RollingShutter{readout_seconds:readout,capture_clock:report.capture_clock,..Default::default()}),..base});
 let opaque=corrected.rgba.chunks_exact(4).filter(|p|p[3]>250).count();let changed=baseline.rgba.chunks_exact(4).zip(corrected.rgba.chunks_exact(4)).filter(|(a,b)|a[..3]!=b[..3]).count();println!("real X6 sensor-row readout {readout:.6}s: opaque {opaque}/{}, changed {changed} pixels",1024*512);assert!(opaque>1024*512*999/1000);assert!(changed>1000);
 if let Ok(folder)=std::env::var("VIDEO_KADR_INSTA360_ARTIFACTS"){std::fs::create_dir_all(&folder).unwrap();for(name,frame)in[("rs-real-baseline.png",baseline),("rs-real-corrected.png",corrected)]{image::save_buffer(Path::new(&folder).join(name),&frame.rgba,1024,512,image::ColorType::Rgba8).unwrap();}}
}
