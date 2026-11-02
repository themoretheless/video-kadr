// SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat reference source preparation: crop/flip/fit before effect kernels.
use anyhow::{ensure,Result};
use super::group_surface::GroupSurface;
#[derive(Debug,Clone,PartialEq,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Preparation {
 /// Source document order: left, top, right, bottom.
 pub crop:[f64;4],
 pub canvas:[usize;2],
 #[serde(default)]pub flip_h:bool,
 #[serde(default)]pub flip_v:bool,
}
#[derive(Debug,Clone,PartialEq)]
pub struct PreparedPicture {pub surface:GroupSurface,pub center_offset:[f64;2]}
impl Preparation {
 pub fn validate(&self)->Result<()>{
  ensure!(self.canvas[0]>0&&self.canvas[0]<=3840&&self.canvas[1]>0&&self.canvas[1]<=2160,"invalid Concat fit canvas");
  ensure!(self.crop.iter().all(|edge|edge.is_finite()&&(0.0..=0.9).contains(edge))&&self.crop[0]+self.crop[2]<=0.9+f64::EPSILON&&self.crop[1]+self.crop[3]<=0.9+f64::EPSILON,"invalid Concat crop");Ok(())
 }
 pub fn prepare(&self,source:&GroupSurface,checkpoint:&mut dyn FnMut()->Result<()>)->Result<PreparedPicture>{
  self.validate()?;source.validate()?;checkpoint()?;
  let [left,top,right,bottom]=self.crop.map(|edge|edge.clamp(0.,1.) as f32);
  let (w,h)=(source.width as f32,source.height as f32);
  let x=(left*w).min(w-1.).max(0.);let y=(top*h).min(h-1.).max(0.);
  let cw=((1.-left-right).max(0.1)*w).max(1.).min(w-x);
  let ch=((1.-top-bottom).max(0.1)*h).max(1.).min(h-y);
  let fit=(self.canvas[0]as f32/cw).min(self.canvas[1]as f32/ch);
  let width=((cw*fit).round()as usize).max(1);let height=((ch*fit).round()as usize).max(1);
  let mut output=GroupSurface::transparent(width,height)?;
  for row in 0..height{checkpoint()?;for column in 0..width{
   let u=(column as f32+0.5)/width as f32;let v=(row as f32+0.5)/height as f32;
   let sx=x+(if self.flip_h{1.-u}else{u})*cw-0.5;
   let sy=y+(if self.flip_v{1.-v}else{v})*ch-0.5;
   let x0=(sx.floor().max(0.)as usize).min(source.width-1);let y0=(sy.floor().max(0.)as usize).min(source.height-1);
   let x1=(x0+1).min(source.width-1);let y1=(y0+1).min(source.height-1);
   let fx=(sx-x0 as f32).clamp(0.,1.);let fy=(sy-y0 as f32).clamp(0.,1.);
   let mut sample=[0f32;4];
   for (cx,cy,weight) in [(x0,y0,(1.-fx)*(1.-fy)),(x1,y0,fx*(1.-fy)),(x0,y1,(1.-fx)*fy),(x1,y1,fx*fy)]{
    for channel in 0..4{sample[channel]+=source.rgba[(cy*source.width+cx)*4+channel]as f32*weight;}
   }
   for channel in 0..4{output.rgba[(row*width+column)*4+channel]=sample[channel].round().clamp(0.,255.)as u8;}
  }}
  let base_x=(self.canvas[0]as i64-width as i64)/2;let base_y=(self.canvas[1]as i64-height as i64)/2;
  Ok(PreparedPicture{surface:output,center_offset:[base_x as f64+width as f64/2.-self.canvas[0]as f64/2.,base_y as f64+height as f64/2.-self.canvas[1]as f64/2.]})
 }
}

pub(crate) fn source_preparation(transform:&super::composition::TransformSpec)->Result<Option<Preparation>>{
 if let Some(p)=&transform.concat_preparation{return Ok(Some(p.clone()))}
 if let Some(animation)=&transform.concat_animation{
  ensure!(animation.canvas.iter().all(|v|v.is_finite()&&v.fract()==0.),"invalid source fit canvas");
  let p=Preparation{crop:[0.;4],canvas:animation.canvas.map(|v|v as usize),flip_h:false,flip_v:false};p.validate()?;return Ok(Some(p))
 }
 Ok(None)
}
pub(crate) fn prepare_transform_source(transform:&super::composition::TransformSpec,source:&mut GroupSurface,checkpoint:&mut dyn FnMut()->Result<()>)->Result<[f64;2]>{
 if let Some(preparation)=source_preparation(transform)?{
  let prepared=preparation.prepare(source,checkpoint)?;*source=prepared.surface;Ok(prepared.center_offset)
 }else{Ok([0.,0.])}
}

impl Preparation {
 /// Original reference compositor for a source picture without effect kernels.
 /// Sampling and blending are one operation: no intermediate RGBA8 fit image.
 pub fn composite_reference(&self,source:&GroupSurface,destination:&mut GroupSurface,pose:super::concat_clip_animation::Pose,blend:super::composition::BlendMode,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
  use super::composition::BlendMode;
  self.validate()?;source.validate()?;destination.validate()?;
  ensure!(self.canvas==[destination.width,destination.height],"Concat compositor canvas mismatch");
  ensure!(matches!(blend,BlendMode::Normal|BlendMode::Multiply|BlendMode::Screen|BlendMode::Addition|BlendMode::Lighten|BlendMode::Darken),"unsupported original Concat blend");
  ensure!([pose.x,pose.y,pose.scale_x,pose.scale_y,pose.rotation_degrees,pose.opacity].iter().all(|v|v.is_finite())&&(0.0..=1.0).contains(&pose.opacity),"invalid Concat reference pose");
  let [left,top,right,bottom]=self.crop.map(|edge|edge as f32);
  let(w,h)=(source.width as f32,source.height as f32);
  let ox=(left*w).min(w-1.).max(0.);let oy=(top*h).min(h-1.).max(0.);
  let cw=((1.-left-right).max(0.1)*w).max(1.).min(w-ox);let ch=((1.-top-bottom).max(0.1)*h).max(1.).min(h-oy);
  let fit=(destination.width as f32/cw).min(destination.height as f32/ch);
  let fw=((cw*fit).round()as u32).max(1)as f32;let fh=((ch*fit).round()as u32).max(1)as f32;
  let cx=((destination.width as i64-fw as i64)/2)as f32+fw/2.+pose.x as f32;
  let cy=((destination.height as i64-fh as i64)/2)as f32+fh/2.+pose.y as f32;
  let sx=(pose.scale_x as f32).max(1e-6);let sy=(pose.scale_y as f32).max(1e-6);
  let angle=pose.rotation_degrees.to_radians()as f32;let(sin,cos)=angle.sin_cos();
  ensure!([cx,cy,sx,sy,angle].iter().all(|v|v.is_finite()),"Concat reference geometry overflow");
  let reach_x=(fw*sx/2.*cos.abs())+(fh*sy/2.*sin.abs());let reach_y=(fw*sx/2.*sin.abs())+(fh*sy/2.*cos.abs());
  let x0=(cx-reach_x).floor().max(0.)as usize;let x1=(cx+reach_x).ceil().min(destination.width as f32).max(0.)as usize;
  let y0=(cy-reach_y).floor().max(0.)as usize;let y1=(cy+reach_y).ceil().min(destination.height as f32).max(0.)as usize;
  for y in y0..y1{checkpoint()?;for x in x0..x1{
   let dx=(x as f32+0.5)-cx;let dy=(y as f32+0.5)-cy;
   let px=(dx*cos+dy*sin)/sx+fw/2.;let py=(-dx*sin+dy*cos)/sy+fh/2.;
   if px<0.||py<0.||px>fw||py>fh{continue}
   let u=px/fw;let v=py/fh;
   let tx=ox+(if self.flip_h{1.-u}else{u})*cw-0.5;let ty=oy+(if self.flip_v{1.-v}else{v})*ch-0.5;
   let a=(tx.floor().max(0.)as usize).min(source.width-1);let b=(ty.floor().max(0.)as usize).min(source.height-1);
   let fx=(tx-a as f32).clamp(0.,1.);let fy=(ty-b as f32).clamp(0.,1.);let mut sample=[0f32;4];
   for(xx,yy,weight)in[(a,b,(1.-fx)*(1.-fy)),((a+1).min(source.width-1),b,fx*(1.-fy)),(a,(b+1).min(source.height-1),(1.-fx)*fy),((a+1).min(source.width-1),(b+1).min(source.height-1),fx*fy)]{for k in 0..4{sample[k]+=source.rgba[(yy*source.width+xx)*4+k]as f32*weight;}}
   let alpha=(sample[3]/255.)*pose.opacity as f32;if alpha<=0.{continue}
   let at=(y*destination.width+x)*4;
   for k in 0..3{
    let colour=(sample[k]/255.).max(0.).powf(2.4);let under=(destination.rgba[at+k]as f32/255.).max(0.).powf(2.4);let over=colour*alpha;
    let mixed=match blend{BlendMode::Normal=>over+under*(1.-alpha),BlendMode::Multiply=>over*under+under*(1.-alpha),BlendMode::Screen=>over*(1.-under)+under,BlendMode::Addition=>over+under,BlendMode::Lighten=>colour.max(under)*alpha+under*(1.-alpha),BlendMode::Darken=>colour.min(under)*alpha+under*(1.-alpha),_=>unreachable!()};
    destination.rgba[at+k]=(mixed.clamp(0.,1.).powf(1./2.4)*255.).round().clamp(0.,255.)as u8;
   }
   destination.rgba[at+3]=255;
  }}Ok(())
 }
}
