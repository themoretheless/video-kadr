//! One oriented image through the existing verified one-frame RVM sequence.
//! Caller owns source ACL, model inventory and Analysis admission.
use std::{path::{Path,PathBuf},time::Duration};
use anyhow::{ensure,Result};
use sha2::{Digest,Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use crate::process_control::ProcessRuntime;
use super::{local_person,local_person_sequence,concat_cutout_asset::load_verified_image};
pub struct AnalysedPersonStill {
 pub encoded_source_hash:String,
 pub sequence:local_person_sequence::Metadata,
 pub copy:PathBuf,pub mask:PathBuf,
 pub copy_file_hash:String,pub mask_file_hash:String,
 directory:PathBuf,
}
impl Drop for AnalysedPersonStill{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.directory);}}
struct Stage(PathBuf);
impl Drop for Stage{fn drop(&mut self){if !self.0.as_os_str().is_empty(){let _=std::fs::remove_dir_all(&self.0);}}}
pub async fn analyse(runtime:&ProcessRuntime,config:&local_person::Config,source:&Path,source_hash:&str,root:&Path,cancel:&CancellationToken,limit:Duration,progress:&tokio::sync::mpsc::UnboundedSender<f64>)->Result<AnalysedPersonStill>{
 ensure!(root.is_absolute()&&root.is_dir()&&!cancel.is_cancelled()&&!limit.is_zero()&&limit<=Duration::from_secs(3600),"invalid Person still admission");
 let deadline=tokio::time::Instant::now()+limit;
 let frame=load_verified_image(source,Some(source_hash),cancel).await?;
 let directory=root.join(format!("person-still-{}",uuid::Uuid::new_v4()));tokio::fs::create_dir(&directory).await?;let mut stage=Stage(directory.clone());
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;tokio::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).await?;}
 let raw=directory.join("input.rgba");let output=directory.join("output.rgba");let masks=directory.join("mask.raw");
 let mut file=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&raw).await?;
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;file.set_permissions(std::fs::Permissions::from_mode(0o600)).await?;}
 file.write_all(&frame.rgba).await?;file.sync_all().await?;drop(file);
 let raw_hash=format!("{:x}",Sha256::digest(&frame.rgba));
 let remaining=deadline.saturating_duration_since(tokio::time::Instant::now());ensure!(!remaining.is_zero(),"Person still deadline");
 let metadata=local_person_sequence::segment(runtime,config,local_person_sequence::Input{source:&raw,source_hash:&raw_hash,mask_destination:Some(&masks),width:frame.width as u32,height:frame.height as u32,frames:1},&output,cancel,remaining,progress).await?;
 ensure!(!cancel.is_cancelled()&&tokio::time::Instant::now()<deadline,"Person still cancelled/deadline");
 let rgba=tokio::fs::read(&output).await?;let probabilities=tokio::fs::read(&masks).await?;
 ensure!(rgba.len()==frame.rgba.len()&&probabilities.len()==metadata.mask_bytes as usize,"Person still result layout changed");
 ensure!(format!("{:x}",Sha256::digest(&rgba))==metadata.output_hash&&format!("{:x}",Sha256::digest(&probabilities))==metadata.mask_hash,"Person still result changed");
 let copy=directory.join("copy.png");let mask=directory.join("mask.png");let(c,m)=(copy.clone(),mask.clone());let(w,h,mw,mh)=(metadata.width,metadata.height,metadata.mask_width,metadata.mask_height);
 tokio::task::spawn_blocking(move||->Result<()>{
  image::RgbaImage::from_raw(w,h,rgba).ok_or_else(||anyhow::anyhow!("invalid Person RGBA"))?.save(c)?;
  image::GrayImage::from_raw(mw,mh,probabilities).ok_or_else(||anyhow::anyhow!("invalid Person mask"))?.save(m)?;Ok(())
 }).await??;
 #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;for path in [&copy,&mask]{tokio::fs::set_permissions(path,std::fs::Permissions::from_mode(0o600)).await?;}}
 let copy_file_hash=super::local_speech::fingerprint_model(&copy).await?.1;let mask_file_hash=super::local_speech::fingerprint_model(&mask).await?.1;
 // Source can change while native work or PNG encoding runs.
 let original=load_verified_image(source,Some(source_hash),cancel).await?;
 ensure!(original==frame&&tokio::time::Instant::now()<deadline,"Person still source changed/deadline");
 stage.0=PathBuf::new();Ok(AnalysedPersonStill{encoded_source_hash:source_hash.into(),sequence:metadata,copy,mask,copy_file_hash,mask_file_hash,directory})
}
