//! Per-renderer worker ownership and cooperative cancellation, isolated across exports.
use std::{cell::RefCell,sync::{Arc,atomic::{AtomicBool,Ordering}}};
use super::{image::Image,project::EffectInstance};
thread_local! {static CANCEL:RefCell<Option<Arc<AtomicBool>>>=const {RefCell::new(None)};}
pub(super) fn cancelled()->bool {CANCEL.with(|flag|flag.borrow().as_ref().is_some_and(|flag|flag.load(Ordering::Relaxed)))}
pub struct RenderContext {pool:rayon::ThreadPool,cancel:Arc<AtomicBool>,max_pixels:usize,ledger:Arc<super::budget::Ledger>}
impl RenderContext {
 pub fn new(cancel:Arc<AtomicBool>,threads:usize,max_pixels:usize)->Result<Self,String>{
  Self::new_with_budget(cancel,threads,max_pixels,8*1024*1024*1024)
 }
 pub fn new_with_budget(cancel:Arc<AtomicBool>,threads:usize,max_pixels:usize,max_bytes:usize)->Result<Self,String>{
  if max_bytes==0||max_bytes>8*1024*1024*1024{return Err("invalid transition pixel storage budget".into());}
  let ledger=super::budget::Ledger::new(max_bytes);let worker_ledger=ledger.clone();
  if !(1..=4).contains(&threads)||max_pixels==0||max_pixels>3840*2160{return Err("invalid transition worker/image budget".into());}
  let worker_cancel=cancel.clone();
  let pool=rayon::ThreadPoolBuilder::new().num_threads(threads).start_handler(move |_|{super::budget::install(worker_ledger.clone());CANCEL.with(|flag|*flag.borrow_mut()=Some(worker_cancel.clone()));}).build().map_err(|e|e.to_string())?;
  Ok(Self{pool,cancel,max_pixels,ledger})
 }
 pub fn pixel_bytes_in_use(&self)->usize{self.ledger.used()}
 pub fn render(&self,e:&EffectInstance,a:&Image,b:&Image,p:f32,scale:f32)->Result<Image,String>{
  if self.cancel.load(Ordering::Relaxed){return Err("transition cancelled".into());}
  e.validate()?;
  for image in [a,b]{
   let count=image.w.checked_mul(image.h).ok_or("transition image overflow")?;
   if image.w==0||image.h==0||count>self.max_pixels||count.checked_mul(4)!=Some(image.px.len()){return Err("invalid transition image shape/budget".into());}
   for chunk in image.px.chunks(4096){
    if self.cancel.load(Ordering::Relaxed){return Err("transition cancelled".into());}
    if !chunk.iter().all(|v|v.is_finite()){return Err("nonfinite transition input".into());}
   }
  }
  if (a.w,a.h)!=(b.w,b.h){return Err("transition canvas mismatch".into());}
  let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
   let _a=a.px.borrowed_reservation(&self.ledger);let _b=b.px.borrowed_reservation(&self.ledger);
   self.pool.install(||super::apply(e,a,b,p,scale))
  }));
  let output=match outcome{Ok(output)=>output,Err(error)=>{
   if error.is::<super::budget::BudgetAbort>(){return Err("transition pixel storage budget/allocation exceeded".into());}
   std::panic::resume_unwind(error)
  }};
  if self.cancel.load(Ordering::Relaxed){return Err("transition cancelled".into());}
  if !output.px.iter().all(|v|v.is_finite()){return Err("nonfinite transition output".into());}
  Ok(output)
 }
}

impl RenderContext {
 /// Keep the caller's non-Send deadline/cancellation checkpoint on its own thread.
 /// Borrowed image work remains scoped; errors wait for workers and discard partial output.
 pub fn render_with_checkpoint(&self,e:&EffectInstance,a:&Image,b:&Image,p:f32,scale:f32,checkpoint:&mut dyn FnMut()->Result<(),String>)->Result<Image,String>{
  checkpoint()?;
  self.pool.in_place_scope(|scope|{
   let(tx,rx)=std::sync::mpsc::sync_channel(1);
   scope.spawn(move |_|{let _=tx.send(self.render(e,a,b,p,scale));});
   let mut failure=None;
   loop {
    match rx.recv_timeout(std::time::Duration::from_millis(10)) {
     Ok(result)=>return failure.map_or(result,Err),
     Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>return Err("transition worker disconnected".into()),
     Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{
      if failure.is_none(){if let Err(error)=checkpoint(){self.cancel.store(true,Ordering::Relaxed);failure=Some(error);}}
     }
    }
   }
  })
 }
}

impl RenderContext {
 pub(crate) fn reserve_scratch(&self,bytes:usize)->super::budget::Reservation{self.ledger.reserve(bytes)}
 pub(crate) fn canvas_allowed(&self,width:usize,height:usize)->bool{width>0&&height>0&&width.checked_mul(height).is_some_and(|n|n<=self.max_pixels)}
 pub(crate) fn is_cancelled(&self)->bool{self.cancel.load(Ordering::Relaxed)}
 /// Execute image assembly in this renderer's worker/memory scope. Inputs are
 /// borrowed into the ledger; returned storage remains charged until dropped.
 pub(crate) fn image_work(&self,inputs:&[&Image],work:impl FnOnce()->Result<Image,String>+Send)->Result<Image,String>{
  if self.is_cancelled(){return Err("frame cancelled".into())}
  for image in inputs{
   if image.w==0||image.h==0||image.w.checked_mul(image.h).is_none_or(|n|n>self.max_pixels||n.checked_mul(4)!=Some(image.px.len())){return Err("invalid frame input shape/budget".into())}
   for chunk in image.px.chunks(4096){if self.is_cancelled(){return Err("frame cancelled".into())}if !chunk.iter().all(|v|v.is_finite()){return Err("nonfinite frame input".into())}}
  }
  let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
   let _reservations:Vec<_>=inputs.iter().map(|image|image.px.borrowed_reservation(&self.ledger)).collect();
   self.pool.install(||{let mut output=work()?;output.px.adopt(&self.ledger);Ok::<Image,String>(output)})
  }));
  let output=match outcome{Ok(result)=>result?,Err(error)=>{if error.is::<super::budget::BudgetAbort>(){return Err("frame pixel storage budget/allocation exceeded".into())}std::panic::resume_unwind(error)}};
  if self.is_cancelled(){return Err("frame cancelled".into())}
  if output.w==0||output.h==0||output.w.checked_mul(output.h).is_none_or(|n|n>self.max_pixels||n.checked_mul(4)!=Some(output.px.len()))||!output.px.iter().all(|v|v.is_finite()){return Err("invalid frame output".into())}
  Ok(output)
 }
}

#[cfg(test)]
mod assembly_memory_tests {
 use super::*;
 fn context(bytes:usize)->RenderContext {
  RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,32,bytes).unwrap()
 }
 #[test]
 fn returned_external_and_foreign_storage_is_owned_until_drop() {
  let destination=context(1024);
  let external=Image::new(8,4);
  assert_eq!(destination.pixel_bytes_in_use(),0);
  let output=destination.image_work(&[],move ||Ok(external)).unwrap();
  assert_eq!(destination.pixel_bytes_in_use(),512);
  drop(output);assert_eq!(destination.pixel_bytes_in_use(),0);
  let source=context(1024);
  let foreign=source.image_work(&[],||Ok(Image::new(8,4))).unwrap();
  assert_eq!(source.pixel_bytes_in_use(),512);
  let output=destination.image_work(&[],move ||Ok(foreign)).unwrap();
  assert_eq!(source.pixel_bytes_in_use(),0);
  assert_eq!(destination.pixel_bytes_in_use(),512);
  drop(output);assert_eq!(destination.pixel_bytes_in_use(),0);
 }
 #[test]
 fn adoption_refusal_releases_foreign_storage_and_invalid_outputs() {
  let source=context(1024);let destination=context(64);
  let foreign=source.image_work(&[],||Ok(Image::new(8,4))).unwrap();
  assert!(destination.image_work(&[],move ||Ok(foreign)).is_err());
  assert_eq!(source.pixel_bytes_in_use(),0);
  assert_eq!(destination.pixel_bytes_in_use(),0);
  let destination=context(1024);
  let mut invalid=Image::new(8,4);invalid.px[0]=f32::NAN;
  assert!(destination.image_work(&[],move ||Ok(invalid)).is_err());
  assert_eq!(destination.pixel_bytes_in_use(),0);
 }
}

impl RenderContext {
 /// Export straight RGBA bytes while keeping both borrowed input and returned
 /// storage charged to this renderer. The reservation must live with the bytes.
 pub(crate) fn rgba8_work(&self,image:&Image,pixel:impl Fn([f32;4])->[u8;4]+Send+Sync)->Result<(Vec<u8>,super::budget::Reservation),String>{
  if self.is_cancelled(){return Err("frame encoding cancelled".into())}
  if !self.canvas_allowed(image.w,image.h)||image.w.checked_mul(image.h).and_then(|n|n.checked_mul(4))!=Some(image.px.len()){return Err("invalid encoding input shape/budget".into())}
  let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
   let _input=image.px.borrowed_reservation(&self.ledger);
   let reservation=self.reserve_scratch(image.px.len());
   self.pool.install(|| ->Result<_,String>{
    let mut bytes=Vec::new();bytes.try_reserve_exact(image.px.len()).map_err(|_|"frame encoding allocation failed")?;
    let extra=self.reserve_scratch(bytes.capacity()-image.px.len());
    bytes.resize(image.px.len(),0);
    for (src,dst) in image.px.chunks(image.w*4).zip(bytes.chunks_mut(image.w*4)){
     if self.is_cancelled(){return Err("frame encoding cancelled".into())}
     for (s,d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)){
      if !s.iter().all(|v|v.is_finite()){return Err("nonfinite frame encoding input".into())}
      d.copy_from_slice(&pixel([s[0],s[1],s[2],s[3]]));
     }
    }
    if self.is_cancelled(){return Err("frame encoding cancelled".into())}
    Ok((bytes,(reservation,extra)))
   })
  }));
  match outcome{
   Ok(result)=>result.map(|(bytes,(mut reservation,extra))|{reservation.absorb(extra);(bytes,reservation)}),
   Err(error)=>{if error.is::<super::budget::BudgetAbort>(){Err("frame encoding storage budget exceeded".into())}else{std::panic::resume_unwind(error)}}
  }
 }
}
