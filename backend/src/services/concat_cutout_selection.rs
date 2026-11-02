//! Concat subject policy; model acquisition and source decoding stay with caller.
use anyhow::{ensure,Result};
use std::future::Future;
use tokio_util::sync::CancellationToken;
use crate::domain::{concat_cutout_import::Subject,concat_cutout_clock::ResolvedMask};
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Model {Person,Selfie,Object}
/// Cached subject/model mismatch invalidates the source mask store.
pub fn stale(subject:Subject,cached:Option<Model>)->bool{
 matches!((subject,cached),(Subject::Person,Some(Model::Object))|(Subject::Object,Some(Model::Person|Model::Selfie)))
}
pub(super) async fn person<H,L,F>(load:&mut L,cancel:&CancellationToken)->Result<(Model,H)>
where L:FnMut(Model)->F,F:Future<Output=Result<H>>{
 ensure!(!cancel.is_cancelled(),"cutout selection cancelled");
 match load(Model::Person).await{
  Ok(handle)=>Ok((Model::Person,handle)),
  Err(error)=>{ensure!(!cancel.is_cancelled(),"cutout selection cancelled: {error}");Ok((Model::Selfie,load(Model::Selfie).await?))}
 }
}
/// First undecided frame may be analysed twice. Inference errors never trigger
/// the download fallback. Caller must decode again at the chosen model's size.
/// Persist the returned model for all later frames/runs of this source+subject.
pub async fn analyse<H,L,LF,I,IF>(subject:Subject,cached:Option<Model>,mut load:L,mut infer:I,cancel:&CancellationToken)->Result<(Model,ResolvedMask)>
where L:FnMut(Model)->LF,LF:Future<Output=Result<H>>,I:FnMut(H)->IF,IF:Future<Output=Result<ResolvedMask>>{
 ensure!(!cancel.is_cancelled(),"cutout analysis cancelled");
 ensure!(!stale(subject,cached),"cached cutout model conflicts with subject");
 let undecided=subject==Subject::Auto&&cached.is_none();
 let(mut chosen,handle)=if subject==Subject::Object||(subject==Subject::Auto&&cached==Some(Model::Object)){
  (Model::Object,load(Model::Object).await?)
 }else{person(&mut load,cancel).await?};
 ensure!(!cancel.is_cancelled(),"cutout analysis cancelled");
 let mut mask=infer(handle).await?;
 validate(&mask)?;ensure!(!cancel.is_cancelled(),"cutout analysis cancelled");
 if undecided&&mask.probabilities.iter().all(|value|*value<10){
  chosen=Model::Object;let handle=load(chosen).await?;
  ensure!(!cancel.is_cancelled(),"cutout analysis cancelled");
  mask=infer(handle).await?;validate(&mask)?;
 }
 ensure!(!cancel.is_cancelled(),"cutout analysis cancelled");Ok((chosen,mask))
}
fn validate(mask:&ResolvedMask)->Result<()>{ensure!(mask.width>0&&mask.height>0&&mask.width<=4096&&mask.height<=4096&&mask.width.checked_mul(mask.height)==Some(mask.probabilities.len()),"invalid selected cutout mask");Ok(())}
