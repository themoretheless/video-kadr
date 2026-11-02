//! Source-PTS selection in Rust, ordinal RGBA decode through the host codec tool.
//! Caller owns source ACL and Analysis admission. No client paths are resolved.
use std::{path::{Path,PathBuf},io::{BufRead,Read}};
use anyhow::{ensure,Context,Result};use tokio::{process::Command,time::Instant,io::{AsyncReadExt,AsyncSeekExt,AsyncWriteExt}};use tokio_util::sync::CancellationToken;
use crate::{domain::{concat_cutout_clock::AnalysisRun,concat_video_pacer::{VideoPacer,DecodedFrame,FrameTimestamp},media_probe::Rational},process_control::{ProcessRuntime,capture_output_cancellable}};
use super::local_enhance_video::{Tools,fingerprint,remaining,execute};
const MAX_STAGE:u64=64*1024*1024*1024;
pub struct Input<'a>{pub source:&'a Path,pub source_hash:&'a str,pub run:AnalysisRun,pub staging_bytes:u64}
pub struct SampledRun{pub rgba:PathBuf,pub width:u32,pub height:u32,pub start_millis:u64,pub frames:u64,pub video_hash:String,pub rgba_hash:String,directory:PathBuf}
impl Drop for SampledRun{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.directory);}}
struct Stage(PathBuf);impl Drop for Stage{fn drop(&mut self){if !self.0.as_os_str().is_empty(){let _=std::fs::remove_dir_all(&self.0);}}}
struct Points{reader:std::io::BufReader<std::fs::File>,base:Rational,origin:i64,cancel:CancellationToken,next_ordinal:usize,discard_before:Option<u64>}
impl Iterator for Points{type Item=Result<DecodedFrame<usize>>;fn next(&mut self)->Option<Self::Item>{loop{
 if self.cancel.is_cancelled(){return Some(Err(anyhow::anyhow!("source PTS selection cancelled")))}
 let mut bytes=vec![];let count=match (&mut self.reader).take(65537).read_until(b'\n',&mut bytes){Ok(n)=>n,Err(e)=>return Some(Err(e.into()))};if count==0{return None}if count>65536{return Some(Err(anyhow::anyhow!("source PTS line exceeds limit")))}
 let line=match std::str::from_utf8(&bytes){Ok(s)=>s.trim(),Err(e)=>return Some(Err(e.into()))};if line!="frame"&&!line.starts_with("frame|"){continue}
 let ticks=line.split('|').find_map(|field|field.strip_prefix("best_effort_timestamp="));let timestamp=match ticks{None|Some("N/A")=>None,Some(value)=>match value.parse::<i64>(){Ok(ticks)=>Some(FrameTimestamp{ticks,time_base:self.base,start_ticks:self.origin}),Err(e)=>return Some(Err(e.into()))}};
 let ordinal=self.next_ordinal;self.next_ordinal=match ordinal.checked_add(1){Some(n)=>n,None=>return Some(Err(anyhow::anyhow!("source ordinal overflow")))};
 if let (Some(before),Some(time))=(self.discard_before,timestamp){match time.before_millis(before){Ok(true)=>continue,Ok(false)=>{},Err(e)=>return Some(Err(e))}}
 self.discard_before=None;return Some(Ok(DecodedFrame{timestamp,picture:ordinal}))
}}}
async fn probe_points(runtime:&ProcessRuntime,mut command:Command,source:&Path,path:&Path,budget:u64,deadline:Instant,cancel:&CancellationToken)->Result<()>{
 command.arg("-o").arg(path).arg(source);let child=cancel.child_token();
 let operation=capture_output_cancellable(runtime,command,runtime.render_policy(vec![source.into()],path),remaining(deadline,cancel)?,&child);tokio::pin!(operation);
 loop{tokio::select!{biased;result=&mut operation=>{ensure!(result?.status.success(),"source timestamp probe failed");break},_=tokio::time::sleep(std::time::Duration::from_millis(20))=>{if tokio::fs::metadata(path).await.is_ok_and(|m|m.len()>budget){child.cancel();let _=operation.await;anyhow::bail!("source timestamp staging budget exceeded")}}}}
 ensure!(tokio::fs::metadata(path).await?.len()<=budget,"source timestamp staging budget exceeded");Ok(())
}
pub async fn sample(runtime:&ProcessRuntime,tools:Tools<'_>,input:Input<'_>,root:&Path,cancel:&CancellationToken,limit:std::time::Duration)->Result<Option<SampledRun>>{
 ensure!(input.source.is_absolute()&&root.is_absolute()&&root.is_dir()&&!cancel.is_cancelled()&&!limit.is_zero()&&limit<=std::time::Duration::from_secs(3600),"invalid video sampling paths/deadline");ensure!(input.staging_bytes>0&&input.staging_bytes<=MAX_STAGE,"invalid video sampling budget");
 let _=VideoPacer::<(),_>::new(std::iter::empty::<Result<DecodedFrame<()>>>(),input.run)?;
 let deadline=Instant::now()+limit;let ffmpeg=tokio::fs::canonicalize(tools.ffmpeg).await?;let ffprobe=tokio::fs::canonicalize(tools.ffprobe).await?;
 let before=fingerprint(input.source).await?;ensure!(before.1==input.source_hash,"video sampling source identity changed");let engines=(fingerprint(&ffmpeg).await?,fingerprint(&ffprobe).await?);
 let directory=root.join(format!("concat-video-sample-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let mut stage=Stage(directory.clone());
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
 let mut command=Command::new(&ffprobe);command.args(["-v","error","-protocol_whitelist",crate::tools::OFFLINE_PROTOCOLS,"-select_streams","v:0","-show_streams","-of","json"]).arg(input.source);
 let info=capture_output_cancellable(runtime,command,runtime.probe_policy(input.source),remaining(deadline,cancel)?,cancel).await?;ensure!(info.status.success(),"video sampling stream probe failed");let value:serde_json::Value=serde_json::from_slice(&info.stdout)?;let parsed=crate::domain::media_probe::ProbeResult::from_ffprobe_json(&value)?;let video=parsed.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Video).context("video stream missing")?;
 super::local_person_video::require_byte_color(runtime,&ffprobe,video,input.source,deadline,cancel).await?;
 ensure!(video.rotation_degrees.rem_euclid(90)==0,"unsupported sampling orientation");let(width,height)=if video.rotation_degrees.rem_euclid(180)==0{(parsed.width,parsed.height)}else{(parsed.height,parsed.width)};ensure!(width>0&&width<=3840&&height>0&&height<=2160,"video sampling dimensions exceed render limit");
 let stream=&value["streams"][0];let base=Rational::parse(stream["time_base"].as_str().context("video time base missing")?).context("invalid video time base")?;let origin=stream["start_pts"].as_i64().unwrap_or(0).max(0);
 let points=directory.join("points.txt");let mut command=Command::new(&ffprobe);command.args(["-v","error","-protocol_whitelist",crate::tools::OFFLINE_PROTOCOLS,"-select_streams","v:0","-show_frames","-show_entries","frame=best_effort_timestamp:frame_side_data=","-of","compact=p=1:nk=0"]);probe_points(runtime,command,input.source,&points,input.staging_bytes,deadline,cancel).await?;
 let path=points.clone();let token=cancel.clone();let run=input.run;let selected=tokio::task::spawn_blocking(move||->Result<Vec<usize>>{let reader=Points{reader:std::io::BufReader::new(std::fs::File::open(path)?),base,origin,cancel:token,next_ordinal:0,discard_before:(run.start_millis>0).then_some(run.start_millis)};let mut pacer=VideoPacer::new(reader,run)?;let mut indices=vec![];while let Some(frame)=pacer.next_sample()?{indices.push(frame.picture)}Ok(indices)}).await??;
 if selected.is_empty(){ensure!(fingerprint(input.source).await?==before&&fingerprint(&ffmpeg).await?==engines.0&&fingerprint(&ffprobe).await?==engines.1,"video sampling source/codec changed");remaining(deadline,cancel)?;return Ok(None)}
 let mut unique=selected.clone();unique.sort_unstable();unique.dedup();let frame_bytes=u64::from(width)*u64::from(height)*4;let decoded_bytes=frame_bytes.checked_mul(unique.len()as u64).context("decoded video size overflow")?;let sampled_bytes=frame_bytes.checked_mul(selected.len()as u64).context("sampled video size overflow")?;
 let filter=directory.join("select.txt");let expression=crate::domain::concat_video_select::expression(&unique)?;tokio::fs::write(&filter,format!("select={expression}")).await?;
 ensure!(decoded_bytes.checked_add(sampled_bytes).and_then(|n|n.checked_add(std::fs::metadata(&points).ok()?.len())).and_then(|n|n.checked_add(expression.len()as u64+7)).is_some_and(|n|n<=input.staging_bytes),"video sampling staging budget exceeded");
 let decoded=directory.join("selected.rgba");let mut command=Command::new(&ffmpeg);command.args(["-nostdin","-v","error","-n","-protocol_whitelist",crate::tools::OFFLINE_PROTOCOLS,"-threads","2","-i"]).arg(input.source).args(["-map","0:v:0","-an","-sn","-dn","-/filter:v"]).arg(&filter).args(["-fps_mode","passthrough","-frames:v"]).arg(unique.len().to_string()).args(["-pix_fmt","rgba","-f","rawvideo","-fs"]).arg((decoded_bytes+1).to_string()).arg(&decoded);
 let(tx,_rx)=tokio::sync::mpsc::unbounded_channel();execute(runtime,command,vec![input.source.into(),filter.clone()],&decoded,deadline,cancel,&tx,|_|None).await?;ensure!(tokio::fs::metadata(&decoded).await?.len()==decoded_bytes,"decoded sampling frame count mismatch");
 let rgba=directory.join("sampled.rgba");let mut output=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&rgba).await?;
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;output.set_permissions(std::fs::Permissions::from_mode(0o600)).await?;}
 let mut reader=tokio::fs::File::open(&decoded).await?;let mut plane=vec![0;frame_bytes as usize];
 for index in &selected{remaining(deadline,cancel)?;let offset=unique.binary_search(index).unwrap()as u64*frame_bytes;reader.seek(std::io::SeekFrom::Start(offset)).await?;reader.read_exact(&mut plane).await?;output.write_all(&plane).await?;}output.sync_all().await?;drop(output);
 ensure!(fingerprint(input.source).await?==before&&fingerprint(&ffmpeg).await?==engines.0&&fingerprint(&ffprobe).await?==engines.1,"video sampling source/codec changed");let id=fingerprint(&rgba).await?;ensure!(id.0==sampled_bytes,"sampled RGBA length mismatch");remaining(deadline,cancel)?;
 stage.0=PathBuf::new();Ok(Some(SampledRun{rgba,width,height,start_millis:input.run.start_millis,frames:selected.len()as u64,video_hash:before.1,rgba_hash:id.1,directory}))
}
