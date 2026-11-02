// Host-owned regression: caller-thread checkpoint aborts active rendering and releases pixel reservations.
use crate::pack;

#[test]
fn caller_deadline_interrupts_active_kernel_without_sending_checkpoint(){
 use std::{rc::Rc,cell::Cell,sync::{Arc,atomic::{AtomicBool,Ordering}}};
 let cancel=Arc::new(AtomicBool::new(false));
 let context=pack::context::RenderContext::new_with_budget(cancel.clone(),2,512*512,32*1024*1024).unwrap();
 let e:pack::project::EffectInstance=serde_json::from_value(serde_json::json!({"effect":"spin_motion"})).unwrap();
 let a=pack::image::Image::filled(512,512,[0.2,0.1,0.0,0.5]);
 let b=pack::image::Image::filled(512,512,[0.0,0.1,0.3,0.4]);
 let caller=std::thread::current().id();let calls=Rc::new(Cell::new(0));
 let error=context.render_with_checkpoint(&e,&a,&b,0.5,1.0,&mut||{
  assert_eq!(std::thread::current().id(),caller);
  calls.set(calls.get()+1);
  if calls.get()>1{Err("caller deadline reached".into())}else{Ok(())}
 }).unwrap_err();
 assert_eq!(error,"caller deadline reached");assert!(calls.get()>1);
 assert!(cancel.load(Ordering::Relaxed));assert_eq!(context.pixel_bytes_in_use(),0);
}
