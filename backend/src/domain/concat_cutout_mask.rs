// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original concat-vision apply::cut: source-owned alpha mapping before effect passes.
use anyhow::{ensure,Result};
use super::{concat_picture::Preparation,group_surface::GroupSurface};
/// Resolved probability plane. Ownership, model identity and source time are
/// established by the caller; this kernel neither guesses nor generates masks.
pub struct MaskPlane<'a>{pub width:usize,pub height:usize,pub probabilities:&'a[u8]}
impl MaskPlane<'_>{
 fn validate(&self)->Result<()>{
  ensure!(self.width>0&&self.width<=4096&&self.height>0&&self.height<=4096&&self.width.checked_mul(self.height)==Some(self.probabilities.len()),"invalid source cutout mask plane");Ok(())
 }
 fn at(&self,x:i64,y:i64)->f32{
  self.probabilities[y.clamp(0,self.height as i64-1)as usize*self.width+x.clamp(0,self.width as i64-1)as usize]as f32
 }
 fn sample(&self,u:f32,v:f32)->f32{
  let x=u.clamp(0.,1.)*self.width as f32-0.5;let y=v.clamp(0.,1.)*self.height as f32-0.5;
  let x0=x.floor();let y0=y.floor();let fx=x-x0;let fy=y-y0;let(x0,y0)=(x0 as i64,y0 as i64);
  let top=self.at(x0,y0)*(1.-fx)+self.at(x0+1,y0)*fx;
  let bottom=self.at(x0,y0+1)*(1.-fx)+self.at(x0+1,y0+1)*fx;
  (top*(1.-fy)+bottom*fy)/255.
 }
}
/// Changes straight alpha only. On cancellation after a row, callers discard
/// the working frame, as for the other native render kernels.
pub fn apply(frame:&mut GroupSurface,mask:MaskPlane<'_>,mapping:&Preparation,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 frame.validate()?;mask.validate()?;mapping.validate()?;checkpoint()?;
 let [left,top,right,bottom]=mapping.crop.map(|edge|edge as f32);
 for y in 0..frame.height{checkpoint()?;let fy=(y as f32+0.5)/frame.height as f32;
  for x in 0..frame.width{
   let fx=(x as f32+0.5)/frame.width as f32;
   let u=if mapping.flip_h{1.-fx}else{fx};let v=if mapping.flip_v{1.-fy}else{fy};
   let u=left+u*(1.-left-right).max(0.);let v=top+v*(1.-top-bottom).max(0.);
   let keep=mask.sample(u,v);let at=(y*frame.width+x)*4+3;
   frame.rgba[at]=(frame.rgba[at]as f32*keep+0.5)as u8;
  }
 }Ok(())
}

/// CutoutJob::of uses source mapping only when decode already consumed crop/
/// flips; a raw frame uses identity mapping before later picture preparation.
pub fn apply_for_decode(frame:&mut GroupSurface,mask:MaskPlane<'_>,geometry:&Preparation,treated:bool,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 if treated{return apply(frame,mask,geometry,checkpoint)}
 let identity=Preparation{crop:[0.;4],flip_h:false,flip_v:false,..geometry.clone()};
 apply(frame,mask,&identity,checkpoint)
}
