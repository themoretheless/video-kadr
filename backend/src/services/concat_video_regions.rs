//! Model-free random access to verified portable temporal SAM planes.
//! Caller owns project/source ACL and restores encoded masks with the existing
//! lossless mask-video decoder before opening this file.
use std::{io::{Read,Seek,SeekFrom},path::{Path,PathBuf},sync::Arc};
use anyhow::{ensure,Result};
use sha2::{Digest,Sha256};
use tokio_util::sync::CancellationToken;
use crate::domain::{video_region::Attachment,concat_cutout_clock::ResolvedMask};
const PLANE:usize=256*256;
pub struct VerifiedRegions {path:PathBuf,snapshot:Attachment,hashes:Vec<[u8;32]>}
impl VerifiedRegions {
 pub fn snapshot(&self)->&Attachment{&self.snapshot}
 /// Full-source rational presentation clock; never clamps an out-of-range time.
 pub fn ordinal(&self,source_us:u64)->Result<usize>{
  Ok(usize::try_from(crate::domain::video_region::frame_ordinal(source_us,self.snapshot.frames,self.snapshot.frame_rate,self.snapshot.frame_times_us.as_deref())?)?)
 }
 pub fn read(&self,source_hash:&str,source_us:u64,cancel:&CancellationToken)->Result<Arc<ResolvedMask>>{
  ensure!(source_hash==self.snapshot.source_hash,"temporal region belongs to another source");
  ensure!(!cancel.is_cancelled(),"temporal region read cancelled");let ordinal=self.ordinal(source_us)?;
  let bytes=self.snapshot.frames*PLANE as u64;let mut file=super::concat_video_masks::checked_file(&self.path,bytes)?;
  file.seek(SeekFrom::Start(ordinal as u64*PLANE as u64))?;let mut plane=vec![0;PLANE];file.read_exact(&mut plane)?;
  ensure!(<[u8;32]>::from(Sha256::digest(&plane))==self.hashes[ordinal],"temporal region changed after verification");
  ensure!(file.metadata()?.len()==bytes&&!cancel.is_cancelled(),"temporal region changed/cancelled during read");
  Ok(Arc::new(ResolvedMask{width:256,height:256,probabilities:plane}))
 }
}
/// Streams the complete raw content once, retaining only per-plane SHA256 hashes.
/// Subsequent reads use one short-lived descriptor and reverify the chosen plane.
pub async fn open(path:&Path,snapshot:&Attachment,cancel:&CancellationToken)->Result<VerifiedRegions>{
 snapshot.validate()?;ensure!(path.is_absolute()&&!cancel.is_cancelled(),"invalid temporal region path/cancelled");
 let path=path.to_owned();let snapshot=snapshot.clone();let token=cancel.clone();
 let verified=tokio::task::spawn_blocking(move||->Result<_>{
  let mut file=super::concat_video_masks::checked_file(&path,snapshot.frames*PLANE as u64)?;
  let mut total=Sha256::new();let mut hashes=Vec::with_capacity(usize::try_from(snapshot.frames)?);let mut plane=vec![0;PLANE];
  for index in 0..snapshot.frames{
   ensure!(!token.is_cancelled(),"temporal region verification cancelled");file.read_exact(&mut plane)?;
   let lost=snapshot.forward_lost_at.is_some_and(|at|index>=at)||snapshot.backward_lost_at.is_some_and(|at|index<=at);
   ensure!(!lost||plane.iter().all(|v|*v==0),"lost temporal region contains a subject");
   total.update(&plane);hashes.push(Sha256::digest(&plane).into());
  }
  let mut extra=[0];ensure!(file.read(&mut extra)?==0&&format!("{:x}",total.finalize())==snapshot.mask_hash,"temporal region content identity mismatch");
  Ok(VerifiedRegions{path,snapshot,hashes})
 }).await??;
 ensure!(!cancel.is_cancelled(),"temporal region verification cancelled");Ok(verified)
}
