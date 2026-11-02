mod support;
use std::{path::{Path,PathBuf},time::Duration};
use tokio_util::sync::CancellationToken;
use video_kadr_backend::{domain::{concat_retime::AudioPiece,media_probe::StreamMetadata},tools::{concat_audio_frames::trace_source_piece,pcm_conform::probe_pcm_metadata}};
fn piece()->AudioPiece{AudioPiece{start_seconds:0.021,duration_seconds:0.09614285714285714,source_start_seconds:1.019,speed:1.0476190476190477,from:0.,to:1.,fade_in_seconds:0.,fade_out_seconds:0.}}
async fn fixture()->(video_kadr_backend::state::AppState,tempfile::TempDir,PathBuf,StreamMetadata){
    let(state,dir)=support::make_state(true,false).await;let input=state.sources_dir().join("frames.wav");
    let made=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","aevalsrc=0.1*sin(2*PI*440*t)|0.2*sin(2*PI*660*t):s=48000:d=30","-c:a","pcm_f32le"]).arg(&input).output().unwrap();assert!(made.status.success());
    let probe=probe_pcm_metadata(&state.process_runtime,Path::new("ffprobe"),&input,Duration::from_secs(20),&CancellationToken::new()).await.unwrap();
    let stream=probe.streams.into_iter().find(|s|s.sample_rate.is_some()).unwrap();(state,dir,input,stream)
}
#[tokio::test]
#[ignore="requires real FFmpeg/FFprobe"]
async fn real_probe_stops_at_first_complete_frame_and_handles_source_eof(){
    let(state,_dir,input,stream)=fixture().await;
    let p=piece();let span=trace_source_piece(&state.process_runtime,Path::new("ffprobe"),&input,&stream,&p,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();
    assert!(!span.source_exhausted);assert!(span.frame_count>0&&span.frame_count<20);assert!(span.sample_count>=span.feed.minimum_input_samples as u64);assert!(span.sample_count-(span.feed.minimum_input_samples as u64)<span.last_frame_samples);
    // Original seek casts source seconds to integer microseconds (not round).
    assert_eq!(span.seek_stream_microseconds,1018999);
    let mut tail=p;tail.source_start_seconds=29.95;tail.duration_seconds=2.;
    let tail=trace_source_piece(&state.process_runtime,Path::new("ffprobe"),&input,&stream,&tail,Duration::from_secs(10),&CancellationToken::new()).await.unwrap();assert!(tail.source_exhausted);assert!(tail.sample_count>0&&tail.sample_count<tail.feed.minimum_input_samples as u64);
}
#[cfg(unix)]
fn fake(path:&Path,text:&str){use std::os::unix::fs::PermissionsExt;std::fs::write(path,format!("#!/bin/sh\n{text}\n")).unwrap();std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o700)).unwrap();}
#[tokio::test]
#[cfg(unix)]
#[ignore="requires real FFmpeg metadata and Unix process groups"]
async fn completion_and_cancellation_reap_probe_descendants(){
    let(state,dir,input,stream)=fixture().await;let script=dir.path().join("fake-probe");let marker=dir.path().join("late");
    let child_ready=dir.path().join("child-ready");
    fake(&script,&format!("(trap '' TERM; touch '{}'; sleep 1; touch '{}') &\nwhile [ ! -e '{}' ]; do sleep 0.01; done\nprintf 'best_effort_timestamp=48912|nb_samples=8192\\n'\nwait",child_ready.display(),marker.display(),child_ready.display()));
    let span=trace_source_piece(&state.process_runtime,&script,&input,&stream,&piece(),Duration::from_secs(5),&CancellationToken::new()).await.unwrap();assert_eq!(span.frame_count,1);
    tokio::time::sleep(Duration::from_millis(1200)).await;assert!(!marker.exists(),"completed probe left a descendant running");
    let ready=dir.path().join("ready");fake(&script,&format!("(trap '' TERM; touch '{}'; sleep 1; touch '{}') &\nwait",ready.display(),marker.display()));
    let cancel=CancellationToken::new();let stop=cancel.clone();let runtime=state.process_runtime.clone();let metadata=stream.clone();let tool=script.clone();let source=input.clone();
    let task=tokio::spawn(async move{trace_source_piece(&runtime,&tool,&source,&metadata,&piece(),Duration::from_secs(5),&stop).await});
    tokio::time::timeout(Duration::from_secs(3),async{while !ready.exists(){tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();cancel.cancel();assert!(task.await.unwrap().is_err());tokio::time::sleep(Duration::from_millis(1200)).await;assert!(!marker.exists(),"cancelled probe left a descendant running");
    std::fs::remove_file(&ready).unwrap();assert!(trace_source_piece(&state.process_runtime,&script,&input,&stream,&piece(),Duration::from_secs(5),&cancel).await.is_err());assert!(!ready.exists(),"precancelled probe spawned");
}
#[tokio::test]
#[cfg(unix)]
#[ignore="requires real FFmpeg metadata and Unix process groups"]
async fn malformed_or_oversized_records_and_timeout_refuse(){
    let(state,dir,input,stream)=fixture().await;let script=dir.path().join("fake-probe");
    for body in ["printf 'best_effort_timestamp=1|nb_samples=invalid\\n'; sleep 2","awk 'BEGIN{for(i=0;i<100000;i++)printf \"x\";printf \"\\n\"}'; sleep 2","sleep 2"]{
        fake(&script,body);let start=std::time::Instant::now();assert!(trace_source_piece(&state.process_runtime,&script,&input,&stream,&piece(),Duration::from_millis(100),&CancellationToken::new()).await.is_err());assert!(start.elapsed()<Duration::from_secs(2));
    }
}

#[tokio::test]
#[ignore="requires real FFmpeg/FFprobe"]
async fn prepared_frame_capture_is_bounded_and_owns_its_cleanup(){
    use video_kadr_backend::tools::concat_audio_frames::prepare_source_piece;
    let(state,_dir,input,stream)=fixture().await;let cancel=CancellationToken::new();let staging=state.outputs_dir();
    let prepared=prepare_source_piece(&state.process_runtime,Path::new("ffmpeg"),Path::new("ffprobe"),&input,&stream,&piece(),&staging,1024*1024,Duration::from_secs(10),&cancel).await.unwrap();
    let path=prepared.path().to_path_buf();assert!(path.exists());assert!(std::fs::metadata(&path).unwrap().len()<1024*1024);assert_eq!(prepared.span.frame_samples.len()as u64,prepared.span.frame_count);assert_eq!(prepared.span.frame_samples.iter().map(|v|*v as u64).sum::<u64>(),prepared.span.sample_count);
    drop(prepared);assert!(!path.exists());
    assert!(prepare_source_piece(&state.process_runtime,Path::new("ffmpeg"),Path::new("ffprobe"),&input,&stream,&piece(),&staging,8,Duration::from_secs(10),&cancel).await.is_err());
    assert!(prepare_source_piece(&state.process_runtime,Path::new("/missing/ffmpeg"),Path::new("ffprobe"),&input,&stream,&piece(),&staging,1024*1024,Duration::from_secs(10),&cancel).await.is_err());
    assert_eq!(std::fs::read_dir(&staging).unwrap().count(),0,"failed capture retained an owned file");
}
