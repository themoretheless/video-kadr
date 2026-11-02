use sha2::{Digest,Sha256};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{domain::{video_region::Attachment,composition::FrameRateRatio},services::concat_video_regions};
fn hash(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn snapshot(raw:&[u8])->Attachment{let points=vec![[0.25,0.5]];Attachment{source_id:"raw".into(),mask_source_id:"region".into(),source_hash:"a".repeat(64),mask_hash:hash(raw),mask_file_hash:"b".repeat(64),encoder_hash:"c".repeat(64),decoder_hash:"d".repeat(64),runtime_hash:"e".repeat(64),prompt_hash:hash(&serde_json::to_vec(&points).unwrap()),points,width:32,height:16,frames:3,frame_rate:FrameRateRatio{numerator:30000,denominator:1001},frame_times_us:None,seed_frame:1,tracked_frames:2,forward_lost_at:Some(2),backward_lost_at:None}}
#[tokio::test]
async fn temporal_planes_use_rational_source_time_and_verify_every_read(){
 let root=tempfile::tempdir().unwrap();let path=root.path().join("regions.raw");let mut raw=vec![17;256*256];raw.extend(vec![255;256*256]);raw.extend(vec![0;256*256]);std::fs::write(&path,&raw).unwrap();let snapshot=snapshot(&raw);let cancel=CancellationToken::new();let reader=concat_video_regions::open(&path,&snapshot,&cancel).await.unwrap();
 for(time,value)in [(0,17),(33366,17),(33367,255),(66733,255),(66734,0),(100099,0)]{let mask=reader.read(&snapshot.source_hash,time,&cancel).unwrap();assert!(mask.probabilities.iter().all(|v|*v==value));}
 assert!(reader.read(&snapshot.source_hash,100100,&cancel).is_err());assert!(reader.read(&"f".repeat(64),0,&cancel).is_err());
 let mut changed=raw.clone();changed[256*256]=0;std::fs::write(&path,&changed).unwrap();assert!(reader.read(&snapshot.source_hash,33367,&cancel).is_err());assert!(reader.read(&snapshot.source_hash,0,&cancel).is_ok());
 std::fs::write(&path,&raw[..raw.len()-1]).unwrap();assert!(reader.read(&snapshot.source_hash,0,&cancel).is_err());
 std::fs::write(&path,&raw).unwrap();cancel.cancel();assert!(reader.read(&snapshot.source_hash,0,&cancel).is_err());
}
#[tokio::test]
async fn temporal_restore_rejects_changed_content_nonzero_lost_planes_and_links(){
 let root=tempfile::tempdir().unwrap();let path=root.path().join("regions.raw");let raw=vec![255;3*256*256];std::fs::write(&path,&raw).unwrap();let snapshot=snapshot(&raw);let cancel=CancellationToken::new();assert!(concat_video_regions::open(&path,&snapshot,&cancel).await.is_err());
 let mut raw=raw;raw[2*256*256..].fill(0);std::fs::write(&path,&raw).unwrap();assert!(concat_video_regions::open(&path,&snapshot,&cancel).await.is_err());let accepted=crate::snapshot(&raw);let reader=concat_video_regions::open(&path,&accepted,&cancel).await.unwrap();
 #[cfg(unix)]{std::fs::remove_file(&path).unwrap();let target=root.path().join("elsewhere.raw");std::fs::write(&target,&raw).unwrap();std::os::unix::fs::symlink(&target,&path).unwrap();assert!(reader.read(&accepted.source_hash,0,&cancel).is_err());assert!(concat_video_regions::open(&path,&accepted,&cancel).await.is_err());}
}

#[tokio::test]
#[ignore="requires actual FFmpeg/ffprobe; procedural planes, no SAM inference"]
async fn portable_lossless_region_movie_restores_into_verified_temporal_reader(){
 use video_kadr_backend::{process_control::ProcessRuntime,services::local_person_mask_video::{self,Input,Tools}};
 let root=tempfile::tempdir().unwrap();let source=root.path().join("source.raw");let movie=root.path().join("regions.mkv");let restored=root.path().join("restored.raw");let mut raw=vec![0;3*256*256];raw[..256*256].fill(17);raw[256*256..2*256*256].fill(255);std::fs::write(&source,&raw).unwrap();let mut snapshot=snapshot(&raw);
 let ffmpeg=std::path::PathBuf::from("/opt/homebrew/bin/ffmpeg");let ffprobe=std::path::PathBuf::from("/opt/homebrew/bin/ffprobe");let runtime=ProcessRuntime::local_default();let cancel=CancellationToken::new();let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();let limit=std::time::Duration::from_secs(30);
 let metadata=local_person_mask_video::encode(&runtime,Tools{ffmpeg:&ffmpeg,ffprobe:&ffprobe},Input{masks:&source,mask_hash:&snapshot.mask_hash,width:256,height:256,frames:3,frame_rate:snapshot.frame_rate,staging_bytes:16*1024*1024},&movie,&cancel,limit,&tx).await.unwrap();snapshot.mask_file_hash=metadata.file_hash.clone();
 local_person_mask_video::decode(&runtime,Tools{ffmpeg:&ffmpeg,ffprobe:&ffprobe},&movie,&metadata,&restored,16*1024*1024,&cancel,limit,&tx).await.unwrap();let saved=serde_json::to_vec(&snapshot).unwrap();drop(snapshot);std::fs::remove_file(&source).unwrap();let reopened:Attachment=serde_json::from_slice(&saved).unwrap();
 let reader=concat_video_regions::open(&restored,&reopened,&cancel).await.unwrap();for(time,value)in [(0,17),(33367,255),(66734,0)]{assert!(reader.read(&reopened.source_hash,time,&cancel).unwrap().probabilities.iter().all(|v|*v==value));}assert_eq!(std::fs::read(restored).unwrap(),raw);
}

#[test]
fn portable_timed_binding_uses_exact_microsecond_seed_clock(){
 use video_kadr_backend::domain::{concat_video_binding::{Binding,RegionTrack},cutout::{Correction,Stroke,Tool}};
 let mut raw=vec![0;3*256*256];raw[256*256..2*256*256].fill(255);let snapshot=snapshot(&raw);
 let mut binding=Binding{source_hash:snapshot.source_hash.clone(),model_hash:"88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828".into(),runtime_hash:"f".repeat(64),subject:"person".into(),feather:None,correction:Some(Correction{custom:true,feather:0.,strokes:vec![Stroke{tool:Tool::SmartBrush,size:0.1,points:snapshot.points.clone(),at:Some(0.033367),region:None}]}),region_tracks:vec![RegionTrack{stroke:0,snapshot}]};assert!(binding.validate().is_ok());binding.correction.as_mut().unwrap().strokes[0].at=Some(0.033366);assert!(binding.validate().is_err());binding.correction.as_mut().unwrap().strokes[0].at=Some(f64::MAX);assert!(binding.validate().is_err());
}

#[tokio::test]
async fn variable_presentation_clock_holds_exact_planes_and_validates_seed(){
 let root=tempfile::tempdir().unwrap();let path=root.path().join("vfr.raw");let mut raw=vec![17;256*256];raw.extend(vec![255;256*256]);raw.extend(vec![0;256*256]);std::fs::write(&path,&raw).unwrap();let mut snapshot=snapshot(&raw);snapshot.frame_times_us=Some(vec![0,100000,400000,500000]);
 let cancel=CancellationToken::new();let reader=concat_video_regions::open(&path,&snapshot,&cancel).await.unwrap();
 for(time,value)in [(0,17),(99999,17),(100000,255),(250000,255),(399999,255),(400000,0),(499999,0)]{assert!(reader.read(&snapshot.source_hash,time,&cancel).unwrap().probabilities.iter().all(|v|*v==value));}
 assert!(reader.read(&snapshot.source_hash,500000,&cancel).is_err());
 for clock in [vec![1,100000,400000,500000],vec![0,100000,100000,500000],vec![0,100000,500000],vec![0,100000,400000,9_007_199_254_740_992]]{let mut wrong=snapshot.clone();wrong.frame_times_us=Some(clock);assert!(wrong.validate().is_err());}
 let restored:Attachment=serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();assert_eq!(restored,snapshot);
 use video_kadr_backend::domain::{concat_video_binding::{Binding,RegionTrack},cutout::{Correction,Stroke,Tool}};
 let mut binding=Binding{source_hash:snapshot.source_hash.clone(),model_hash:"88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828".into(),runtime_hash:"f".repeat(64),subject:"person".into(),feather:None,correction:Some(Correction{custom:true,feather:0.,strokes:vec![Stroke{tool:Tool::SmartBrush,size:0.1,points:snapshot.points.clone(),at:Some(0.25),region:None}]}),region_tracks:vec![RegionTrack{stroke:0,snapshot}]};assert!(binding.validate().is_ok());binding.correction.as_mut().unwrap().strokes[0].at=Some(0.4);assert!(binding.validate().is_err());
}

#[tokio::test]
#[ignore="requires actual FFmpeg/ffprobe"]
async fn fractional_vfr_probe_uses_exact_rational_microsecond_boundaries(){
 let root=tempfile::tempdir().unwrap();let source=root.path().join("fractional.nut");
 let encoded=std::process::Command::new("/opt/homebrew/bin/ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=c=red:s=16x16:r=30000/1001","-vf","setpts=N*N+60","-fps_mode","passthrough","-frames:v","3","-c:v","ffv1"]).arg(&source).output().unwrap();assert!(encoded.status.success(),"{}",String::from_utf8_lossy(&encoded.stderr));
 let runtime=video_kadr_backend::process_control::ProcessRuntime::local_default();let cancel=CancellationToken::new();
 for(time,ordinal)in [(33366,0),(33367,1),(133466,1),(133467,2),(166833,2)]{
  let resolved=video_kadr_backend::services::concat_video_sam_seed::resolve(&runtime,&source,time,&cancel).await.unwrap();
  assert_eq!(resolved.frame_times_us,Some(vec![0,33367,133467,166834]));assert_eq!(resolved.seed_frame,ordinal);
 }
 assert!(video_kadr_backend::services::concat_video_sam_seed::resolve(&runtime,&source,166834,&cancel).await.is_err());
}
