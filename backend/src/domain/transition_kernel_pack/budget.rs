//! Scoped accounting of kernel pixel storage. Budget aborts unwind scoped work without panic hooks.
use std::{cell::RefCell,ops::{Deref,DerefMut},sync::{Arc,atomic::{AtomicUsize,Ordering}}};
#[derive(Debug)]pub(super) enum BudgetAbort{Limit,Allocation}
pub(super) struct Ledger{used:AtomicUsize,limit:usize}
impl Ledger{
 pub fn new(limit:usize)->Arc<Self>{Arc::new(Self{used:AtomicUsize::new(0),limit})}
 pub fn used(&self)->usize{self.used.load(Ordering::Relaxed)}
 pub fn reserve(self:&Arc<Self>,bytes:usize)->Reservation{
  let result=self.used.fetch_update(Ordering::AcqRel,Ordering::Acquire,|used|used.checked_add(bytes).filter(|next|*next<=self.limit));
  if result.is_err(){std::panic::resume_unwind(Box::new(BudgetAbort::Limit));}
  Reservation{ledger:self.clone(),bytes}
 }
}
pub(crate) struct Reservation{ledger:Arc<Ledger>,bytes:usize}
impl Reservation{pub(crate) fn absorb(&mut self,mut other:Self){assert!(Arc::ptr_eq(&self.ledger,&other.ledger));self.bytes+=other.bytes;other.bytes=0;}}
impl Drop for Reservation{fn drop(&mut self){self.ledger.used.fetch_sub(self.bytes,Ordering::AcqRel);}}
thread_local!{static CURRENT:RefCell<Option<Arc<Ledger>>>=const{RefCell::new(None)};}
pub(super) fn install(ledger:Arc<Ledger>){CURRENT.with(|current|*current.borrow_mut()=Some(ledger));}
fn claim(bytes:usize)->Option<Reservation>{CURRENT.with(|current|current.borrow().as_ref().map(|ledger|ledger.reserve(bytes)))}
fn bytes(count:usize)->usize{count.checked_mul(std::mem::size_of::<f32>()).unwrap_or_else(||std::panic::resume_unwind(Box::new(BudgetAbort::Limit)))}
pub struct PixelBuffer{data:Vec<f32>,reservation:Option<Reservation>}
impl PixelBuffer{
 pub fn zeroed(count:usize)->Self{
  let reservation=claim(bytes(count));let mut data=Vec::new();
  if data.try_reserve_exact(count).is_err(){std::panic::resume_unwind(Box::new(BudgetAbort::Allocation));}
  data.resize(count,0.0);
  let mut result=Self{data,reservation};
  // Account the allocator's actual capacity, including any capacity beyond the request.
  if result.data.capacity()>count{let extra=claim(bytes(result.data.capacity()-count));
   if let Some(mut extra)=extra{if let Some(reservation)=result.reservation.as_mut(){reservation.bytes+=extra.bytes;extra.bytes=0;}}
  }
  result
 }
 pub(super) fn adopt(&mut self,ledger:&Arc<Ledger>){
  if self.reservation.as_ref().is_some_and(|r|Arc::ptr_eq(&r.ledger,ledger)){return}
  let reservation=ledger.reserve(bytes(self.data.capacity()));self.reservation=Some(reservation);
 }
 pub(super) fn borrowed_reservation(&self,ledger:&Arc<Ledger>)->Option<Reservation>{
  if self.reservation.as_ref().is_some_and(|r|Arc::ptr_eq(&r.ledger,ledger)){None}else{Some(ledger.reserve(bytes(self.data.capacity())))}
 }
}
impl From<Vec<f32>> for PixelBuffer{fn from(data:Vec<f32>)->Self{let reservation=claim(bytes(data.capacity()));Self{data,reservation}}}
impl Clone for PixelBuffer{fn clone(&self)->Self{let mut out=Self::zeroed(self.data.len());out.data.copy_from_slice(&self.data);out}}
impl std::fmt::Debug for PixelBuffer{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{self.data.fmt(f)}}
impl PartialEq for PixelBuffer{fn eq(&self,other:&Self)->bool{self.data==other.data}}
impl Deref for PixelBuffer{type Target=[f32];fn deref(&self)->&Self::Target{&self.data}}
impl DerefMut for PixelBuffer{fn deref_mut(&mut self)->&mut Self::Target{&mut self.data}}
impl FromIterator<f32> for PixelBuffer{fn from_iter<T:IntoIterator<Item=f32>>(iter:T)->Self{Vec::from_iter(iter).into()}}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn reservations_release_on_drop_and_budget_abort(){
  let ledger=Ledger::new(64);install(ledger.clone());
  {let buffer=PixelBuffer::zeroed(8);assert_eq!(ledger.used(),32);let copy=buffer.clone();assert_eq!(ledger.used(),64);drop(copy);assert_eq!(ledger.used(),32);
   let failure=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||PixelBuffer::zeroed(16)));assert!(failure.unwrap_err().is::<BudgetAbort>());assert_eq!(ledger.used(),32);}
  assert_eq!(ledger.used(),0);
  CURRENT.with(|current|*current.borrow_mut()=None);
 }
}
