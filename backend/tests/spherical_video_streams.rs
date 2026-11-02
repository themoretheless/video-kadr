use std::{path::Path,sync::atomic::AtomicBool,time::Duration};
use video_kadr_backend::domain::{group_media::decode_source_frame_policy_layout,video_clock::FrameTime};
#[test]
fn paired_lens_streams_reach_the_shared_native_source_decoder() {
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("lenses.mkv");
 let made=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=red:s=16x16:r=2:d=1","-f","lavfi","-i","color=blue:s=16x16:r=2:d=1","-map","0:v","-map","1:v","-c:v","ffv1","-threads","1"]).arg(&path).output().unwrap();assert!(made.status.success(),"{}",String::from_utf8_lossy(&made.stderr));
 for hold in [false,true] {
  let frame=decode_source_frame_policy_layout(Path::new("ffmpeg"),&path,FrameTime::Microseconds(500000),32,16,Duration::from_secs(10),&AtomicBool::new(false),hold,true).unwrap();
  let left=&frame.rgba[(8*32+4)*4..(8*32+4)*4+4];let right=&frame.rgba[(8*32+28)*4..(8*32+28)*4+4];
  assert!(left[0]>240&&left[2]<10);assert!(right[2]>240&&right[0]<10);
 }
 let single=decode_source_frame_policy_layout(Path::new("ffmpeg"),&path,FrameTime::Microseconds(0),32,16,Duration::from_secs(10),&AtomicBool::new(false),false,false).unwrap();assert!(single.rgba.chunks_exact(4).all(|p|p[0]>240&&p[2]<10));
}
