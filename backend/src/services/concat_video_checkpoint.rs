//! Independently verify a completed native frame against trusted decoded input.
//! Expected identities and source bytes come from the caller's owned decode lease.
use std::{path::{Path,PathBuf},time::Duration};
use anyhow::{ensure,Result};use serde::Deserialize;use sha2::{Digest,Sha256};use tokio_util::sync::CancellationToken;
use super::local_person_sequence_contract::{self as contract,Input,Metadata};
pub struct Expected<'a>{pub sequence_hash:&'a str,pub model_hash:&'a str,pub runtime_hash:&'a str,pub source_frame:&'a [u8],pub width:u32,pub height:u32,pub ordinal:u64,pub total:u64}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Context{source_hash:String,frame_hash:String,model_hash:String,runtime_hash:String,width:u32,height:u32,mask_width:u32,mask_height:u32,total:u64}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Receipt{schema_version:u32,ordinal:u64,context:Context,output_hash:String,mask_hash:String,blank:bool}
pub(crate) struct Verified{pub metadata:Metadata,pub mask:Vec<u8>,pub ordinal:u64}
struct Stage(PathBuf);impl Drop for Stage{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn digest(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
async fn private_directory(path:&Path)->Result<()>{let m=tokio::fs::symlink_metadata(path).await?;ensure!(m.is_dir(),"checkpoint path must be a real directory");#[cfg(unix)]{use std::os::unix::fs::PermissionsExt;ensure!(m.permissions().mode()&0o777==0o700,"checkpoint directory is not private");}Ok(())}
async fn snapshot(path:&Path,limit:u64)->Result<Vec<u8>>{let m=tokio::fs::symlink_metadata(path).await?;ensure!(m.is_file()&&m.len()<=limit,"invalid bounded checkpoint file");#[cfg(unix)]{use std::os::unix::fs::PermissionsExt;ensure!(m.permissions().mode()&0o777==0o600,"checkpoint file is not private");}use tokio::io::AsyncReadExt;let mut bytes=Vec::new();tokio::fs::File::open(path).await?.take(limit+1).read_to_end(&mut bytes).await?;ensure!(bytes.len()as u64==m.len()&&bytes.len()as u64<=limit,"checkpoint file changed while reading");Ok(bytes)}
async fn private_write(path:&Path,bytes:&[u8])->Result<()>{use tokio::io::AsyncWriteExt;let mut options=tokio::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{options.mode(0o600);}let mut file=options.open(path).await?;file.write_all(bytes).await?;file.flush().await?;Ok(())}
pub(crate) async fn verify(directory:&Path,expected:&Expected<'_>,root:&Path,cancel:&CancellationToken,limit:Duration)->Result<Verified>{
 ensure!(directory.is_absolute()&&root.is_absolute()&&root.is_dir()&&!limit.is_zero()&&limit<=Duration::from_secs(3600)&&!cancel.is_cancelled(),"invalid checkpoint verification paths/deadline");let deadline=tokio::time::Instant::now()+limit;
 let verify=async{
  let(mw,mh,_,_)=contract::layout_for(expected.model_hash,expected.width,expected.height,expected.total)?;let(_,_,bytes,mask_bytes)=contract::layout_for(expected.model_hash,expected.width,expected.height,1)?;ensure!(expected.ordinal<expected.total&&expected.source_frame.len()as u64==bytes,"invalid checkpoint expected frame");
  for hash in [expected.sequence_hash,expected.model_hash,expected.runtime_hash]{ensure!(hash.len()==64&&hash.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"invalid expected checkpoint identity");}
  private_directory(directory).await?;let frame=directory.join(expected.ordinal.to_string());private_directory(&frame).await?;
  let receipt:Receipt=serde_json::from_slice(&snapshot(&frame.join("receipt.json"),64*1024).await?)?;let context=&receipt.context;let frame_hash=digest(expected.source_frame);
  ensure!(receipt.schema_version==1&&receipt.ordinal==expected.ordinal&&context.total==expected.total&&context.source_hash==expected.sequence_hash&&context.frame_hash==frame_hash&&context.model_hash==expected.model_hash&&context.runtime_hash==expected.runtime_hash,"checkpoint source/model/runtime/ordinal mismatch");
  ensure!((context.width,context.height,context.mask_width,context.mask_height)==(expected.width,expected.height,mw,mh),"checkpoint layout mismatch");
  let rgba=snapshot(&frame.join("frame.rgba"),bytes).await?;let mask=snapshot(&frame.join("mask.raw"),mask_bytes).await?;ensure!(rgba.len()as u64==bytes&&mask.len()as u64==mask_bytes&&digest(&rgba)==receipt.output_hash&&digest(&mask)==receipt.mask_hash,"checkpoint content digest mismatch");
  let stage=Stage(root.join(format!("concat-checkpoint-{}",uuid::Uuid::new_v4())));tokio::fs::create_dir(&stage.0).await?;#[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&stage.0,std::fs::Permissions::from_mode(0o700)).await?;}
  let source=stage.0.join("source.rgba");let output=stage.0.join("frame.rgba");let masks=stage.0.join("mask.raw");private_write(&source,expected.source_frame).await?;private_write(&output,&rgba).await?;private_write(&masks,&mask).await?;
  let(output_hash,mask_hash,blank_frames)=contract::verify_frames(&Input{source:&source,source_hash:&frame_hash,mask_destination:None,width:expected.width,height:expected.height,frames:1},&output,&masks,mw,mh,cancel,deadline).await?;
  ensure!(output_hash==receipt.output_hash&&mask_hash==receipt.mask_hash&&blank_frames==u64::from(receipt.blank),"checkpoint pixel/blank verification mismatch");
  Ok(Verified{ordinal:expected.ordinal,mask,metadata:Metadata{schema_version:if expected.model_hash==super::local_person::MODEL_HASH{7}else{10},width:expected.width,height:expected.height,mask_width:mw,mask_height:mh,mask_bytes,blank_frames,mask_hash,frames:1,bytes,source_hash:frame_hash,model_hash:expected.model_hash.into(),runtime_hash:expected.runtime_hash.into(),output_hash}})
 };
 tokio::select!{biased;_=cancel.cancelled()=>anyhow::bail!("checkpoint verification cancelled"),result=tokio::time::timeout_at(deadline,verify)=>result?}
}
