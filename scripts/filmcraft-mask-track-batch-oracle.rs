use std::sync::{Arc,Mutex};use serde_json::{json,Value};use sha2::{Digest,Sha256};
use filmcraft_project::{Mask,MaskPath,Keyframe,ParamValue as V,Interpolation};use filmcraft_time::Tick;
mod inputs { include!(concat!(env!("KADR_SOURCE_ROOT"),"/scripts/reference/filmcraft_mask_tracking_frames.rs")); }
const FD:i64=10_584_000_000;
type Cancel=Arc<Mutex<Option<Box<dyn Fn()+Send+Sync>>>>;
struct Frames{info:filmcraft_media::MediaInfo,width:usize,height:usize,mode:String,cancel:Cancel}
impl filmcraft_media::MediaSource for Frames{
 fn info(&self)->&filmcraft_media::MediaInfo{&self.info}
 fn video_frame(&self,r:filmcraft_media::FrameRequest)->filmcraft_media::Result<Arc<filmcraft_frame::VideoFrame>>{
  let step=r.time.0.div_euclid(FD);
  if self.mode=="cancel"{loop{if self.cancel.lock().unwrap().is_some(){break;}std::thread::yield_now();}if step>=2{self.cancel.lock().unwrap().as_ref().unwrap()();}}
  if (self.mode=="decode"&&step>=2)||(self.mode=="firstDecode"){return Err(filmcraft_media::MediaError::Unsupported("oracle decode failure".into()));}
  Ok(Arc::new(filmcraft_frame::VideoFrame::rgba8(self.width as u32,self.height as u32,inputs::frame(self.width,self.height,step,&self.mode))))
 }
 fn audio(&self,_:i64,_:usize,_:u32)->filmcraft_media::Result<filmcraft_frame::AudioBuffer>{Err(filmcraft_media::MediaError::Unsupported("no audio".into()))}
}
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut seed=filmcraft_engine::Session::default();seed.execute("file.openDemoProject",json!({}))?;let seq=seed.state.active_sequence.unwrap();let clip=seed.project.sequence(seq).unwrap().video_tracks[0].items[0].id;let source=seed.project.sequence(seq).unwrap().video_tracks[0].items[0].item;
 let mut project=serde_json::to_value(seed.project.as_ref())?;let media=&mut project["items"][source.0.to_string()]["kind"]["Media"];media["media"]=json!({"File":{"path":"/untrusted/oracle.mov"}});media["offline"]=json!(false);media["info"]["kind"]=json!("Movie");media["info"]["video"]["width"]=json!(240);media["info"]["video"]["height"]=json!(180);media["info"]["video"]["frame_rate"]=json!({"num":24,"den":1});media["interpret"]["frame_rate"]=Value::Null;
 let raw=&mut project["items"][seq.0.to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0];raw["start"]=json!(0);raw["duration"]=json!(FD*6);raw["source_in"]=json!(0);raw["speed"]=json!(1.);raw["reverse"]=json!(false);raw["frame_hold"]=Value::Null;
 let base:filmcraft_project::Project=serde_json::from_value(project)?;let base=Arc::new(base);let project=serde_json::to_value(base.as_ref())?;let mut cases=Vec::new();
 for method in ["position","positionRotation","positionScaleRotation"]{for mode in ["translation","rotation","mixed","still","lost","decode","firstDecode","cancel"]{for backward in [false,true]{for (width,height) in [(240,180),(120,90)]{if width==120&&!matches!(mode,"translation"|"rotation"|"mixed"|"still"){continue;}if backward&&matches!(mode,"cancel"|"firstDecode"){continue;}
  let mut s=filmcraft_engine::Session::default();s.project=base.clone();s.state.active_sequence=Some(seq);let from=if backward{4*FD}else{0};s.state.playheads.insert(seq,Tick(from));
  let mut effect=filmcraft_project::find_effect("opacity").unwrap().instance();let mut path=MaskPath::rect(40.,30.,200.,150.);path.vertices[0].t_in.x=-3.25;path.vertices[0].t_out.y=7.75;let mut mask=Mask::new("Batch",path.clone());
  // Existing animation must keep endpoint keys and their interpolation/influence fields.
  if mode=="mixed"{let mut first=Keyframe::new(Tick(0),V::Path(path.clone()));first.interp=Interpolation::Hold;first.out_influence=0.7;mask.path.keyframes=vec![first,Keyframe::new(Tick(5*FD),V::Path(path))];}
  effect.masks.push(mask);let track=&mut Arc::make_mut(&mut s.project).sequence_mut(seq).unwrap().video_tracks[0];track.locked=mode=="still";track.items[0].effects=vec![effect];let initial=serde_json::to_value(&track.items[0])?;
  let info=s.project.item(source).unwrap().as_media().unwrap().info.clone();let cancel:Cancel=Arc::default();s.media.insert(source,Arc::new(Frames{info,width,height,mode:mode.into(),cancel:cancel.clone()}));
  let params=json!({"clip":clip.0,"effect":"opacity","mask":0,"method":method,"direction":if backward{"backward"}else{"forward"},"frames":4,"wait":mode!="cancel"});let result=s.execute("masks.track",params.clone());
  if mode=="cancel"&&result.is_ok(){let progress=s.jobs.last().unwrap().progress.clone();*cancel.lock().unwrap()=Some(Box::new(move||{progress.cancel.store(true,std::sync::atomic::Ordering::Relaxed);}));let deadline=std::time::Instant::now()+std::time::Duration::from_secs(60);while !s.jobs.last().unwrap().progress.finished.load(std::sync::atomic::Ordering::Relaxed){assert!(std::time::Instant::now()<deadline);s.poll_persistence();std::thread::yield_now();}s.poll_persistence();}
  let job=s.jobs.last().map(|j|j.to_json()).unwrap_or(Value::Null);cases.push(json!({"mode":mode,"width":width,"height":height,"clip":initial,"locked":mode=="still","params":params,"playhead":from,"ok":result.is_ok(),"steps":result.as_ref().ok().map(|v|v["frames"].clone()),"done":job["done"],"error":job["result"].get("error"),"projectSha256":hash(&serde_json::to_value(s.project.as_ref())?)}));
 }}}}
 let header=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"source":source.0,"project":project});let mut text=serde_json::to_string(&header)?;text.pop();text.push_str(",\"cases\":[\n");for(i,c)in cases.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(c)?);}text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
