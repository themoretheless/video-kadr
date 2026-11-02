//! Verified bounded access to a native RVM run decoded on Concat's 10 Hz grid.
//! Caller owns original-video/decoded-sequence/start-time provenance and ACL.
use std::{fs::File,io::{Read,Seek,SeekFrom},path::{Path,PathBuf},sync::Arc};
use anyhow::{ensure,Result};use sha2::{Digest,Sha256};use tokio_util::sync::CancellationToken;
use crate::domain::concat_cutout_clock::{MaskTimeline,ResolvedMask};
use super::local_person_sequence::Metadata;
#[derive(Clone,Copy)]
pub struct ExpectedRun<'a>{pub decoded_source_hash:&'a str,pub model_hash:&'a str,pub runtime_hash:&'a str}
pub struct VerifiedMaskRun{path:PathBuf,file_bytes:u64,start_millis:u64,width:usize,height:usize,hashes:Vec<[u8;32]>}
// Open only while verifying/reading a plane. A checkpoint cache can contain
// hundreds of thousands of runs without retaining one OS descriptor per run.
pub(super) fn checked_file(path:&Path,bytes:u64)->Result<File>{
 let metadata=std::fs::symlink_metadata(path)?;ensure!(metadata.is_file()&&metadata.len()==bytes,"video mask must be a regular file of the verified size");
 let mut options=std::fs::OpenOptions::new();options.read(true);
 #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.custom_flags(libc::O_NOFOLLOW|libc::O_NONBLOCK);}
 let file=options.open(path)?;let metadata=file.metadata()?;ensure!(metadata.is_file()&&metadata.len()==bytes,"opened video mask identity/layout changed");Ok(file)
}
impl VerifiedMaskRun{
 pub fn timeline(&self)->Result<MaskTimeline>{MaskTimeline::new((0..self.hashes.len()).map(|i|self.start_millis+i as u64*100).collect())}
 pub fn read(&mut self,millis:u64,cancel:&CancellationToken)->Result<Arc<ResolvedMask>>{
  ensure!(!cancel.is_cancelled(),"video mask read cancelled");
  let offset=millis.checked_sub(self.start_millis).ok_or_else(||anyhow::anyhow!("mask precedes run"))?;ensure!(offset%100==0,"mask time is not on analysis grid");
  let index=usize::try_from(offset/100)?;let expected=self.hashes.get(index).ok_or_else(||anyhow::anyhow!("mask time outside run"))?;
  let size=self.width*self.height;let mut bytes=vec![0;size];let mut file=checked_file(&self.path,self.file_bytes)?;file.seek(SeekFrom::Start(index as u64*size as u64))?;file.read_exact(&mut bytes)?;ensure!(file.metadata()?.len()==self.file_bytes,"video mask size changed during read");
  ensure!(<[u8;32]>::from(Sha256::digest(&bytes))==*expected,"video mask changed after verification");ensure!(!cancel.is_cancelled(),"video mask read cancelled");
  Ok(Arc::new(ResolvedMask{width:self.width,height:self.height,probabilities:bytes}))
 }
 pub fn automatic(&mut self,millis:u64,cancel:&CancellationToken)->Result<Option<Arc<ResolvedMask>>>{let mask=self.read(millis,cancel)?;Ok((!mask.probabilities.iter().all(|v|*v<10)).then_some(mask))}
}
pub async fn open(path:&Path,start_millis:u64,metadata:&Metadata,expected:ExpectedRun<'_>,cancel:&CancellationToken)->Result<VerifiedMaskRun>{
 ensure!(path.is_absolute()&&start_millis%100==0&&!cancel.is_cancelled(),"invalid video mask path/grid/cancelled");
 ensure!(metadata.source_hash==expected.decoded_source_hash&&metadata.model_hash==expected.model_hash&&metadata.runtime_hash==expected.runtime_hash,"video mask source/model/runtime mismatch");
 let schema=if metadata.model_hash==super::local_person::MODEL_HASH{7}else if [super::local_selfie::MODEL_HASH,super::local_cutout::MODEL_HASH].contains(&metadata.model_hash.as_str()){10}else{anyhow::bail!("unsupported video mask family")};ensure!(metadata.schema_version==schema,"unsupported video mask run schema");
 for hash in [expected.decoded_source_hash,expected.model_hash,expected.runtime_hash,&metadata.mask_hash,&metadata.output_hash]{ensure!(hash.len()==64&&hash.bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v)),"invalid video mask content identity");}
 let(w,h,rgba_bytes,bytes)=super::local_person_sequence_contract::layout_for(&metadata.model_hash,metadata.width,metadata.height,metadata.frames)?;
 ensure!((w,h)==(metadata.mask_width,metadata.mask_height)&&bytes==metadata.mask_bytes&&rgba_bytes==metadata.bytes,"video mask run layout mismatch");
 start_millis.checked_add((metadata.frames-1)*100).ok_or_else(||anyhow::anyhow!("video mask clock overflow"))?;
 let path=path.to_owned();let metadata=metadata.clone();let token=cancel.clone();
 let run=tokio::task::spawn_blocking(move||->Result<_>{
  let mut file=checked_file(&path,metadata.mask_bytes)?;
  let mut total=Sha256::new();let mut hashes=Vec::with_capacity(metadata.frames as usize);let mut plane=vec![0;w as usize*h as usize];let mut blank=0;
  for _ in 0..metadata.frames{ensure!(!token.is_cancelled(),"video mask verification cancelled");file.read_exact(&mut plane)?;total.update(&plane);hashes.push(Sha256::digest(&plane).into());blank+=u64::from(plane.iter().all(|v|*v<10));}
  let mut extra=[0];ensure!(file.read(&mut extra)?==0&&format!("{:x}",total.finalize())==metadata.mask_hash&&blank==metadata.blank_frames,"video mask content identity mismatch");
  Ok(VerifiedMaskRun{path,file_bytes:metadata.mask_bytes,start_millis,width:w as usize,height:h as usize,hashes})
 }).await??;ensure!(!cancel.is_cancelled(),"video mask verification cancelled");Ok(run)
}
