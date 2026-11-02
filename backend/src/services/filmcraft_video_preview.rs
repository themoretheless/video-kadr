//! Bounded file preview adapter. Native PTS lookup chooses an integer decoder ordinal.
use crate::{domain::{filmcraft_video_clock::VideoFrameIndex,group_surface::GroupSurface},process_control::{capture_output,ProcessRuntime},tools::OFFLINE_PROTOCOLS};
use anyhow::{ensure,Context,Result};use std::{path::Path,time::Duration};use tokio::process::Command;
const FORMATS:&str="mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,avi,mpegvideo,mpegts,flv,ogg,asf,rm,mxf";
pub struct SourceVideoPreview{identity:FileIdentity,pub full_resolution:bool,pub surface:std::sync::Arc<GroupSurface>,pub color:filmcraft_color::ColorInfo,pub pts:i64,pub source_pts:i64,pub num:i64,pub den:i64,pub origin:i64,pub ordinal:usize}
#[derive(Clone,PartialEq,Eq)]
pub struct SourceVideoRevision(FileIdentity);
impl SourceVideoRevision{pub async fn read(path:&Path)->Result<Self>{Ok(Self(FileIdentity::read(path).await?))}
 pub(crate) fn fingerprint(&self)->Vec<u8>{let mut b=self.0.len.to_le_bytes().to_vec();let(sign,d)=match self.0.modified.duration_since(std::time::UNIX_EPOCH){Ok(d)=>(0u8,d),Err(e)=>(1,e.duration())};b.push(sign);b.extend(d.as_secs().to_le_bytes());b.extend(d.subsec_nanos().to_le_bytes());#[cfg(unix)]{b.extend(self.0.dev.to_le_bytes());b.extend(self.0.ino.to_le_bytes());b.extend(self.0.ctime.0.to_le_bytes());b.extend(self.0.ctime.1.to_le_bytes());}b}
}
impl SourceVideoPreview{pub fn revision(&self)->SourceVideoRevision{SourceVideoRevision(self.identity.clone())}pub fn same_source_revision(&self,other:&Self)->bool{self.identity==other.identity}}
#[derive(Clone,PartialEq,Eq)]
struct FileIdentity { len:u64, modified:std::time::SystemTime, #[cfg(unix)] dev:u64, #[cfg(unix)] ino:u64, #[cfg(unix)] ctime:(i64,i64) }
impl FileIdentity {
 async fn read(path:&Path)->Result<Self>{
  let m=tokio::fs::metadata(path).await?;ensure!(m.is_file(),"video source is not a regular file");
  #[cfg(unix)] use std::os::unix::fs::MetadataExt;
  Ok(Self{len:m.len(),modified:m.modified()?,#[cfg(unix)] dev:m.dev(),#[cfg(unix)] ino:m.ino(),#[cfg(unix)] ctime:(m.ctime(),m.ctime_nsec())})
 }
}
/// Per-application bounded index cache. Authorization remains at the library boundary.
#[derive(Default)]
pub struct VideoPreviewCache { disk:Option<std::sync::Arc<super::stabilizer_path_store::PathStore>>, paths:std::sync::Mutex<CompletedPaths>, decoded:std::sync::Mutex<DecodedFrames>, entries:std::sync::Mutex<std::collections::VecDeque<(std::path::PathBuf,FileIdentity,std::sync::Arc<VideoFrameIndex>)>> }
#[derive(Default)]struct CompletedPaths{entries:std::collections::VecDeque<([u8;32],crate::domain::filmcraft_analysis_cache::ReadyPaths,usize)>,bytes:usize,hits:u64}
impl VideoPreviewCache{
 pub fn with_storage(root:std::path::PathBuf)->Self{Self{disk:Some(std::sync::Arc::new(super::stabilizer_path_store::PathStore::new(root))),..Self::default()}}
 pub(crate) async fn restore_completed_paths(&self,key:[u8;32])->Result<Option<crate::domain::filmcraft_analysis_cache::ReadyPaths>>{
  if let Some(paths)=self.completed_paths(key)?{return Ok(Some(paths))}
  if let Some(store)=&self.disk{match store.load(key).await{Ok(Some(paths))=>{self.store_completed_paths(key,paths.clone())?;return Ok(Some(paths))},Ok(None)=>{},Err(e)=>tracing::warn!(error=%e,"ignoring unavailable completed trajectory cache")}}
  Ok(None)
 }
 pub(crate) async fn persist_completed_paths(&self,key:[u8;32],paths:crate::domain::filmcraft_analysis_cache::ReadyPaths)->Result<()>{
  self.store_completed_paths(key,paths)?;
  if let Some(store)=&self.disk{let paths=self.paths.lock().map_err(|_|anyhow::anyhow!("completed analysis cache unavailable"))?.entries.iter().find(|e|e.0==key).map(|e|e.1.clone());if let Some(paths)=paths{if let Err(e)=store.save(key,paths).await{tracing::warn!(error=%e,"completed trajectory persistence unavailable");}}}
  Ok(())
 }

 pub(crate) fn completed_paths(&self,key:[u8;32])->Result<Option<crate::domain::filmcraft_analysis_cache::ReadyPaths>>{let mut c=self.paths.lock().map_err(|_|anyhow::anyhow!("completed analysis cache unavailable"))?;let Some(i)=c.entries.iter().position(|e|e.0==key)else{return Ok(None)};let entry=c.entries.remove(i).unwrap();let paths=entry.1.clone();c.entries.push_back(entry);c.hits=c.hits.saturating_add(1);Ok(Some(paths))}
 pub(crate) fn store_completed_paths(&self,key:[u8;32],mut paths:crate::domain::filmcraft_analysis_cache::ReadyPaths)->Result<()>{
  paths.retain(|_,path|path.is_some());if paths.is_empty(){return Ok(())}
  ensure!(paths.len()<=128&&crate::domain::filmcraft_analysis_cache::ready_bytes(&paths)<=32*1024*1024,"completed analysis budget exceeded");
  let mut c=self.paths.lock().map_err(|_|anyhow::anyhow!("completed analysis cache unavailable"))?;
  if let Some(i)=c.entries.iter().position(|e|e.0==key){let old=c.entries.remove(i).unwrap();c.bytes-=old.2;for(k,v)in old.1{if paths.contains_key(&k){continue}paths.insert(k,v);if paths.len()>128||crate::domain::filmcraft_analysis_cache::ready_bytes(&paths)>32*1024*1024{paths.remove(&k);}}}
  let bytes=crate::domain::filmcraft_analysis_cache::ready_bytes(&paths);
  while c.entries.len()>=64||c.bytes+bytes>64*1024*1024{let old=c.entries.pop_front().ok_or_else(||anyhow::anyhow!("invalid completed analysis accounting"))?;c.bytes-=old.2;}c.bytes+=bytes;c.entries.push_back((key,paths,bytes));Ok(())
 }
 pub fn stabilization_path_cache_stats(&self)->Result<(usize,usize,u64)>{let c=self.paths.lock().map_err(|_|anyhow::anyhow!("completed analysis cache unavailable"))?;Ok((c.entries.len(),c.bytes,c.hits))}
}
const FRAME_BYTES:usize=16*1024*1024;
const FRAME_ENTRIES:usize=128;
#[derive(Default)]
struct DecodedFrames { entries:std::collections::VecDeque<(std::path::PathBuf,FileIdentity,std::sync::Arc<SourceVideoPreview>)>, bytes:usize }
impl DecodedFrames {
 fn get(&mut self,path:&Path,identity:&FileIdentity,ordinal:usize,full_resolution:bool)->Option<std::sync::Arc<SourceVideoPreview>>{
  self.retain(|p,id,_|p!=path||id==identity);
  let position=self.entries.iter().position(|(p,id,frame)|p==path&&id==identity&&frame.ordinal==ordinal&&frame.full_resolution==full_resolution)?;
  let entry=self.entries.remove(position).unwrap();let frame=entry.2.clone();self.entries.push_back(entry);Some(frame)
 }
 fn retain(&mut self,keep:impl Fn(&Path,&FileIdentity,&SourceVideoPreview)->bool){
  self.entries.retain(|(p,id,frame)|keep(p,id,frame));self.bytes=self.entries.iter().map(|(_,_,f)|f.surface.rgba.len()).sum();
 }
 fn insert(&mut self,path:&Path,identity:FileIdentity,frame:std::sync::Arc<SourceVideoPreview>){
  self.retain(|p,id,f|p!=path||(id==&identity&&(f.ordinal!=frame.ordinal||f.full_resolution!=frame.full_resolution)));
  let bytes=frame.surface.rgba.len();if bytes>FRAME_BYTES{return}
  while self.bytes+bytes>FRAME_BYTES||self.entries.len()>=FRAME_ENTRIES{let (_,_,old)=self.entries.pop_front().unwrap();self.bytes-=old.surface.rgba.len();}
  self.bytes+=bytes;self.entries.push_back((path.to_owned(),identity,frame));
 }
}
impl VideoPreviewCache {
 async fn index(&self,runtime:&ProcessRuntime,path:&Path)->Result<std::sync::Arc<VideoFrameIndex>>{
  let identity=FileIdentity::read(path).await?;
  {
   let mut entries=self.entries.lock().map_err(|_|anyhow::anyhow!("video index cache unavailable"))?;
   if let Some(pos)=entries.iter().position(|(p,id,_)|p==path&&id==&identity){let entry=entries.remove(pos).unwrap();let index=entry.2.clone();entries.push_back(entry);return Ok(index)}
   entries.retain(|(p,_,_)|p!=path);
  }
 let mut probe=Command::new("ffprobe");probe.args(["-v","error","-protocol_whitelist",OFFLINE_PROTOCOLS,"-format_whitelist",FORMATS,"-select_streams","v:0","-show_frames","-show_streams","-show_entries","stream=width,height,time_base,start_pts,duration_ts,color_transfer,color_primaries:frame=best_effort_timestamp,pkt_duration,duration","-of","json"]).arg(path);
 let output=capture_output(runtime,probe,runtime.probe_policy(path),Duration::from_secs(15)).await?;ensure!(output.status.success(),"video clock probe failed");let data:serde_json::Value=serde_json::from_slice(&output.stdout).context("invalid video clock probe")?;let index=std::sync::Arc::new(VideoFrameIndex::from_probe(&data)?);
  ensure!(FileIdentity::read(path).await?==identity,"video source changed during indexing");
  let mut entries=self.entries.lock().map_err(|_|anyhow::anyhow!("video index cache unavailable"))?;
  entries.retain(|(p,_,_)|p!=path);while entries.len()>=32{entries.pop_front();}
  entries.push_back((path.to_owned(),identity,index.clone()));Ok(index)
 }
}
pub async fn frame(runtime:&ProcessRuntime,path:&Path,ticks:i64)->Result<Option<std::sync::Arc<SourceVideoPreview>>>{
 frame_cached(&VideoPreviewCache::default(),runtime,path,ticks).await
}
pub async fn frame_cached(cache:&VideoPreviewCache,runtime:&ProcessRuntime,path:&Path,ticks:i64)->Result<Option<std::sync::Arc<SourceVideoPreview>>>{
 frame_mode(cache,runtime,path,ticks,false).await
}
/// Full source resolution, independently cached from trim thumbnails.
/// These are decoder RGBA8 bytes; this function does not normalize source transfer/gamut.
pub async fn frame_cached_full(cache:&VideoPreviewCache,runtime:&ProcessRuntime,path:&Path,ticks:i64)->Result<Option<std::sync::Arc<SourceVideoPreview>>>{
 frame_mode(cache,runtime,path,ticks,true).await
}
async fn frame_mode(cache:&VideoPreviewCache,runtime:&ProcessRuntime,path:&Path,ticks:i64,full_resolution:bool)->Result<Option<std::sync::Arc<SourceVideoPreview>>>{
 let identity=FileIdentity::read(path).await?;
 let index=cache.index(runtime,path).await?;
let Some(ordinal)=index.select(ticks)?else{return Ok(None)};
 let cached=cache.decoded.lock().map_err(|_|anyhow::anyhow!("video frame cache unavailable"))?.get(path,&identity,ordinal,full_resolution);
 if let Some(frame)=cached{ensure!(FileIdentity::read(path).await?==identity,"video source changed during cache lookup");return Ok(Some(frame))}

 ensure!(!full_resolution||(index.width<=3840&&index.height<=2160),"full video frame dimensions budget exceeded");
 let scale=if full_resolution{1.}else{(480./index.width as f64).min(270./index.height as f64).min(1.)};let width=((index.width as f64*scale).round() as usize).max(1);let height=((index.height as f64*scale).round() as usize).max(1);
 let mut command=Command::new("ffmpeg");command.args(["-nostdin","-v","error","-protocol_whitelist",OFFLINE_PROTOCOLS,"-format_whitelist",FORMATS,"-threads","1","-i"]).arg(path).args(["-map","0:v:0","-vf",&format!("select=eq(n\\,{ordinal}),scale={width}:{height}"),"-frames:v","1","-fps_mode","passthrough","-an","-pix_fmt","rgba","-f","rawvideo","pipe:1"]);
 let mut policy=runtime.probe_policy(path);if full_resolution{policy.output.capture_bytes=policy.output.capture_bytes.max(width*height*4);}
 let output=capture_output(runtime,command,policy,Duration::from_secs(15)).await?;ensure!(output.status.success(),"video frame decode failed");ensure!(output.stdout.len()==width*height*4,"incomplete video preview frame");
 ensure!(FileIdentity::read(path).await?==identity,"video source changed during decoding");
 let frame=std::sync::Arc::new(SourceVideoPreview{identity:identity.clone(),full_resolution,surface:std::sync::Arc::new(GroupSurface{width,height,rgba:output.stdout}),color:index.color,pts:index.tick_floor(ordinal)?,source_pts:index.pts(ordinal)?,num:index.num,den:index.den,origin:index.origin,ordinal});
 cache.decoded.lock().map_err(|_|anyhow::anyhow!("video frame cache unavailable"))?.insert(path,identity,frame.clone());
 Ok(Some(frame))
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn decoded_frames_evict_lru_and_respect_byte_and_entry_budgets(){
  let identity=FileIdentity{len:1,modified:std::time::UNIX_EPOCH,#[cfg(unix)] dev:1,#[cfg(unix)] ino:1,#[cfg(unix)] ctime:(1,0)};
  let make=|ordinal,bytes|std::sync::Arc::new(SourceVideoPreview{identity:identity.clone(),full_resolution:false,surface:std::sync::Arc::new(GroupSurface{width:1,height:1,rgba:vec![0;bytes]}),color:filmcraft_color::ColorInfo::SRGB_FULL,pts:0,source_pts:0,num:1,den:1,origin:0,ordinal});
  let mut cache=DecodedFrames::default();let path=Path::new("source");
  for ordinal in 0..FRAME_ENTRIES{cache.insert(path,identity.clone(),make(ordinal,4));}
  assert!(cache.get(path,&identity,0,false).is_some());cache.insert(path,identity.clone(),make(FRAME_ENTRIES,4));assert!(cache.get(path,&identity,1,false).is_none());assert!(cache.get(path,&identity,0,false).is_some());assert_eq!(cache.entries.len(),FRAME_ENTRIES);
  cache.insert(path,identity.clone(),make(200,FRAME_BYTES));assert_eq!(cache.entries.len(),1);assert_eq!(cache.bytes,FRAME_BYTES);
  cache.insert(path,identity.clone(),make(201,FRAME_BYTES+1));assert_eq!(cache.entries.len(),1);assert_eq!(cache.bytes,FRAME_BYTES);
  let mut changed=identity.clone();changed.len+=1;assert!(cache.get(path,&changed,200,false).is_none());assert_eq!(cache.bytes,0);assert!(cache.entries.is_empty());
 }
 #[tokio::test]async fn real_vfr_frames_hold_at_exact_boundaries_and_normalize_origin(){
  let dir=tempfile::tempdir().unwrap();let raw=dir.path().join("frames.rgba");let mut bytes=vec![];for color in [[255,0,0,255],[0,255,0,255],[0,0,255,255]]{bytes.extend(color.repeat(32));}std::fs::write(&raw,bytes).unwrap();
  for origin in [0,5]{let movie=dir.path().join(format!("vfr-{origin}.mkv"));let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","rawvideo","-pix_fmt","rgba","-s","8x4","-r","25","-i"]).arg(&raw).args(["-vf","setpts='if(eq(N,0),0,if(eq(N,1),2,5))'","-fps_mode","vfr","-c:v","ffv1","-pix_fmt","bgra","-threads","1","-output_ts_offset",&origin.to_string()]).arg(&movie).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
   let runtime=ProcessRuntime::local_default();let cache=VideoPreviewCache::default();
   let original_frame=frame_cached(&cache,&runtime,&movie,0).await.unwrap().unwrap();
   let held_frame=frame_cached(&cache,&runtime,&movie,20_321_279_999).await.unwrap().unwrap();assert!(std::sync::Arc::ptr_eq(&original_frame,&held_frame));
   let first=cache.index(&runtime,&movie).await.unwrap();let reused=cache.index(&runtime,&movie).await.unwrap();assert!(std::sync::Arc::ptr_eq(&first,&reused));
   // A replaced file at the same authorized library path must never retain its old clock.
   let replacement=dir.path().join("replacement.mkv");std::fs::copy(&movie,&replacement).unwrap();std::fs::rename(&replacement,&movie).unwrap();
   let replaced=cache.index(&runtime,&movie).await.unwrap();assert!(!std::sync::Arc::ptr_eq(&first,&replaced));
   let replaced_frame=frame_cached(&cache,&runtime,&movie,0).await.unwrap().unwrap();assert!(!std::sync::Arc::ptr_eq(&original_frame,&replaced_frame));assert!(!original_frame.same_source_revision(&replaced_frame));assert!(original_frame.same_source_revision(&held_frame));

   for (ticks,color) in [(0,[255,0,0,255]),(20_321_279_999,[255,0,0,255]),(20_321_280_000,[0,255,0,255]),(50_803_199_999,[0,255,0,255]),(50_803_200_000,[0,0,255,255])]{let frame=frame_cached(&cache,&runtime,&movie,ticks).await.unwrap().unwrap();assert_eq!(frame.surface.rgba,color.repeat(32),"origin {origin}, query {ticks}");assert_eq!((frame.surface.width,frame.surface.height),(8,4));assert_eq!(frame.origin,origin*1000);}
   assert!(frame(&runtime,&movie,-1).await.unwrap().is_none());assert!(frame(&runtime,&movie,60_963_840_000).await.unwrap().is_none());
  }
 }
 #[tokio::test]async fn full_resolution_frames_do_not_reuse_monitor_thumbnails(){
  let dir=tempfile::tempdir().unwrap();let movie=dir.path().join("full.mkv");let output=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=red:s=1920x1080:r=1","-frames:v","1","-c:v","ffv1","-pix_fmt","bgra","-threads","1"]).arg(&movie).output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
  let runtime=ProcessRuntime::local_default();let cache=VideoPreviewCache::default();
  let thumbnail=frame_cached(&cache,&runtime,&movie,0).await.unwrap().unwrap();assert_eq!((thumbnail.surface.width,thumbnail.surface.height),(480,270));assert!(!thumbnail.full_resolution);
  let full=frame_cached_full(&cache,&runtime,&movie,0).await.unwrap().unwrap();assert_eq!((full.surface.width,full.surface.height),(1920,1080));assert_eq!(full.surface.rgba.len(),1920*1080*4);assert!(full.full_resolution);
  assert!(std::sync::Arc::ptr_eq(&full,&frame_cached_full(&cache,&runtime,&movie,1).await.unwrap().unwrap()));assert!(std::sync::Arc::ptr_eq(&thumbnail,&frame_cached(&cache,&runtime,&movie,1).await.unwrap().unwrap()));
  assert!(frame_cached_full(&cache,&runtime,&movie,-1).await.unwrap().is_none());
 }

 #[tokio::test]async fn actual_decoder_preserves_pq_bt2020_metadata(){
  let dir=tempfile::tempdir().unwrap();let movie=dir.path().join("pq.mov");let out=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","color=gray:s=8x8:r=1","-frames:v","1","-vf","setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc","-c:v","qtrle","-pix_fmt","argb","-movflags","+write_colr","-color_trc","smpte2084","-color_primaries","bt2020","-colorspace","bt2020nc","-threads","1"]).arg(&movie).output().unwrap();assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));let frame=frame_cached_full(&VideoPreviewCache::default(),&ProcessRuntime::local_default(),&movie,0).await.unwrap().unwrap();assert_eq!(frame.color.transfer,filmcraft_color::Transfer::Pq);assert_eq!(frame.color.primaries,filmcraft_color::Primaries::Bt2020);assert_eq!(frame.color.range,filmcraft_color::Range::Full);
 }

}
#[cfg(test)]mod completed_path_tests{
 use super::*;use crate::domain::{filmcraft_analysis_cache::ReadyPaths,warp_stabilizer::{StabilizerPath,Affine}};
 #[test]fn completed_paths_are_bounded_and_promote_recent_entries(){let cache=VideoPreviewCache::default();for i in 0..65u8{let paths:ReadyPaths=[((1,2,3,4,0),Some(std::sync::Arc::new(StabilizerPath{w:32,h:24,fps:6.,steps:vec![Affine::IDENTITY;5]})))].into_iter().collect();cache.store_completed_paths([i;32],paths).unwrap();}assert!(cache.completed_paths([0;32]).unwrap().is_none());assert!(cache.completed_paths([64;32]).unwrap().is_some());let(entries,bytes,hits)=cache.stabilization_path_cache_stats().unwrap();assert_eq!(entries,64);assert!(bytes<=64*1024*1024);assert_eq!(hits,1);}
 #[tokio::test]async fn revision_fingerprint_changes_with_source(){let dir=tempfile::tempdir().unwrap();let path=dir.path().join("revision");std::fs::write(&path,b"one").unwrap();let a=SourceVideoRevision::read(&path).await.unwrap();let b=SourceVideoRevision::read(&path).await.unwrap();assert!(a==b);assert_eq!(a.fingerprint(),b.fingerprint());std::fs::write(&path,b"changed-length").unwrap();let c=SourceVideoRevision::read(&path).await.unwrap();assert!(a!=c);assert_ne!(a.fingerprint(),c.fingerprint());}
}
#[cfg(test)]mod completed_merge_tests{
 use super::*;use crate::domain::warp_stabilizer::{StabilizerPath,Affine};
 #[test]fn independent_clips_merge_but_absent_paths_do_not_persist(){let cache=VideoPreviewCache::default();cache.store_completed_paths([0;32],[((1,1,1,0,0),None)].into_iter().collect()).unwrap();assert!(cache.completed_paths([0;32]).unwrap().is_none());let path=std::sync::Arc::new(StabilizerPath{w:1,h:1,fps:1.,steps:vec![Affine::IDENTITY]});for clip in 0..2{cache.store_completed_paths([1;32],[((1,clip,1,0,0),Some(path.clone()))].into_iter().collect()).unwrap();}assert_eq!(cache.completed_paths([1;32]).unwrap().unwrap().len(),2);}
}
#[cfg(test)]mod completed_disk_tests{
 use super::*;use crate::domain::{filmcraft_analysis_cache::ReadyPaths,warp_stabilizer::{StabilizerPath,Affine}};
 fn paths()->ReadyPaths{[((1,2,3,4,0),Some(std::sync::Arc::new(StabilizerPath{w:96,h:64,fps:6.,steps:vec![Affine{e:0.12345678901234567,..Affine::IDENTITY};3]})))].into_iter().collect()}
 #[tokio::test]async fn cold_cache_restores_without_analyser_and_bad_storage_is_optional(){
  let dir=tempfile::tempdir().unwrap();let first=VideoPreviewCache::with_storage(dir.path().join("paths"));first.persist_completed_paths([1;32],paths()).await.unwrap();drop(first);
  let cold=VideoPreviewCache::with_storage(dir.path().join("paths"));assert_eq!(cold.stabilization_path_cache_stats().unwrap().0,0);let restored=cold.restore_completed_paths([1;32]).await.unwrap().expect("disk must restore without renderer");assert_eq!(restored.values().next().unwrap().as_ref().unwrap().steps[0].e.to_bits(),0.12345678901234567f64.to_bits());assert!(cold.restore_completed_paths([2;32]).await.unwrap().is_none());
  let file=std::fs::read_dir(dir.path().join("paths")).unwrap().next().unwrap().unwrap().path();std::fs::write(file,b"corrupt").unwrap();let cold=VideoPreviewCache::with_storage(dir.path().join("paths"));assert!(cold.restore_completed_paths([1;32]).await.unwrap().is_none());
  let blocked=dir.path().join("file");std::fs::write(&blocked,b"keep").unwrap();let cache=VideoPreviewCache::with_storage(blocked);cache.persist_completed_paths([1;32],paths()).await.unwrap();assert!(cache.restore_completed_paths([1;32]).await.unwrap().is_some());
 }
}

/// Tracking requests the original longest-width analysis scale, independently of monitor caches.
pub async fn frame_tracking(cache:&VideoPreviewCache,runtime:&ProcessRuntime,path:&Path,ticks:i64)->Result<Option<std::sync::Arc<SourceVideoPreview>>>{
 let identity=FileIdentity::read(path).await?;let index=cache.index(runtime,path).await?;let Some(ordinal)=index.select(ticks)?else{return Ok(None)};
 let scale=(960./index.width.max(1) as f64).min(1.);let width=(index.width as f64*scale).round().max(1.) as usize;let height=(index.height as f64*scale).round().max(1.) as usize;let bytes=width.checked_mul(height).and_then(|n|n.checked_mul(4)).context("tracking frame dimensions overflow")?;ensure!(bytes<=32*1024*1024,"tracking analysis frame budget exceeded");
 let mut command=Command::new("ffmpeg");command.args(["-nostdin","-v","error","-protocol_whitelist",OFFLINE_PROTOCOLS,"-format_whitelist",FORMATS,"-threads","1","-i"]).arg(path).args(["-map","0:v:0","-vf",&format!("select=eq(n\\,{ordinal}),scale={width}:{height}"),"-frames:v","1","-fps_mode","passthrough","-an","-pix_fmt","rgba","-f","rawvideo","pipe:1"]);
 let mut policy=runtime.probe_policy(path);policy.output.capture_bytes=policy.output.capture_bytes.max(bytes);let output=capture_output(runtime,command,policy,Duration::from_secs(15)).await?;ensure!(output.status.success()&&output.stdout.len()==bytes,"tracking frame decode failed");ensure!(FileIdentity::read(path).await?==identity,"tracking source changed during decoding");
 Ok(Some(std::sync::Arc::new(SourceVideoPreview{identity,full_resolution:false,surface:std::sync::Arc::new(GroupSurface{width,height,rgba:output.stdout}),color:index.color,pts:index.tick_floor(ordinal)?,source_pts:index.pts(ordinal)?,num:index.num,den:index.den,origin:index.origin,ordinal})))
}
