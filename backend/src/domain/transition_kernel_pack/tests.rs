use super::{image,project,apply};
#[test]
fn all_original_transition_pixels_and_controls(){
 use serde_json::Value;
 fn image(w:usize,h:usize,v:&Value)->image::Image{
  let rgba:Vec<u8>=serde_json::from_value(v.clone()).unwrap();
  let linear=|v:f32|if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)};
  image::Image{w,h,px:rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[linear(p[0]as f32/255.)*a,linear(p[1]as f32/255.)*a,linear(p[2]as f32/255.)*a,a]}).collect()}
 }
 let fixtures=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
 let mut counts=Vec::new();
 let context=super::context::RenderContext::new(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),2,16*12).unwrap();
 for name in ["filmcraft-all-transition-pixels.json","filmcraft-transition-controls.json"]{
  let oracle:Value=serde_json::from_slice(&std::fs::read(fixtures.join(name)).unwrap()).unwrap();
  let cases=oracle["cases"].as_array().unwrap();
  for(case_index,c)in cases.iter().enumerate(){
   let (w,h)=if c.get("width").is_some(){(c["width"].as_u64().unwrap()as usize,c["height"].as_u64().unwrap()as usize)}else{(oracle["width"].as_u64().unwrap()as usize,oracle["height"].as_u64().unwrap()as usize)};
   let a=image(w,h,c.get("a").unwrap_or(&oracle["a"]));let b=image(w,h,c.get("b").unwrap_or(&oracle["b"]));
   let mut params=serde_json::Map::new();
   if let Some(parameter)=c.get("parameter"){params.insert(parameter.as_str().unwrap().into(),serde_json::json!({"value":c["value"]}));}
   let e:project::EffectInstance=serde_json::from_value(serde_json::json!({"effect":c["effect"],"params":params})).unwrap();
   let p=c["progress"].as_f64().unwrap()as f32;
   let output=if c["reverse"].as_bool().unwrap(){context.render(&e,&b,&a,1.-p,1.).unwrap()}else{context.render(&e,&a,&b,p,1.).unwrap()};
   let expected:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();
   assert_eq!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"{name} case{case_index} effect{} parameter{}",c["effect"],c["parameter"]);
  }
  counts.push(cases.len());
 }
 assert_eq!(counts,[18816,1428]);
}

#[test]
fn typed_boundary_rejects_fallback_ids_unknown_controls_and_invalid_values(){
 use serde_json::json;
 let effect=|id:&str,params:serde_json::Value|serde_json::from_value::<project::EffectInstance>(json!({"effect":id,"params":params})).unwrap();
 assert!(effect("not_a_transition",json!({})).validate().is_err());
 assert!(effect("block_motion",json!({"blocks":{"value":{"Float":33.0}}})).validate().is_err());
 assert!(effect("block_motion",json!({"blocks":{"value":{"Choice":3}}})).validate().is_err());
 assert!(effect("block_motion",json!({"direction":{"value":{"Choice":4}}})).validate().is_err());
 assert!(effect("block_motion",json!({"unrecognized":{"value":{"Float":1.0}}})).validate().is_err());
 assert!(effect("block_motion",json!({"blocks":{"value":{"Float":6.0}},"direction":{"value":{"Choice":3}}})).validate().is_ok());
 let catalog:serde_json::Value=serde_json::from_str(include_str!("definitions.json")).unwrap();
 for definition in catalog["definitions"].as_array().unwrap(){
  let params=definition["params"].as_array().unwrap().iter().map(|p|(p["id"].as_str().unwrap().to_string(),json!({"value":p["default"]}))).collect::<serde_json::Map<_,_>>();
  assert!(effect(definition["id"].as_str().unwrap(),params.into()).validate().is_ok(),"{}",definition["id"]);
 }
}

#[test]
fn bounded_context_validates_frames_and_keeps_cancellation_isolated(){
 use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
 let cancel=Arc::new(AtomicBool::new(false));
 let context=super::context::RenderContext::new(cancel.clone(),2,64).unwrap();
 let other=super::context::RenderContext::new(Arc::new(AtomicBool::new(false)),1,64).unwrap();
 let e:project::EffectInstance=serde_json::from_value(serde_json::json!({"effect":"cross_dissolve","params":{}})).unwrap();
 let a=image::Image::filled(4,4,[0.2,0.1,0.0,0.5]);let b=image::Image::filled(4,4,[0.0,0.1,0.3,0.4]);
 let expected=apply(&e,&a,&b,0.3,1.0);
 assert_eq!(context.render(&e,&a,&b,0.3,1.0).unwrap(),expected);
 cancel.store(true,Ordering::Relaxed);
 assert!(context.render(&e,&a,&b,0.3,1.0).unwrap_err().contains("cancelled"));
 assert_eq!(other.render(&e,&a,&b,0.3,1.0).unwrap(),expected);
 cancel.store(false,Ordering::Relaxed);
 assert_eq!(context.render(&e,&a,&b,0.3,1.0).unwrap(),expected);
 let bad=image::Image{w:4,h:4,px:vec![0.0;63].into()};
 assert!(context.render(&e,&bad,&b,0.3,1.0).unwrap_err().contains("shape"));
 let mut bad=a.clone();bad.px[0]=f32::NAN;
 assert!(context.render(&e,&bad,&b,0.3,1.0).unwrap_err().contains("nonfinite"));
 let large=image::Image::new(9,8);
 assert!(context.render(&e,&large,&large,0.3,1.0).unwrap_err().contains("budget"));
}

#[test]fn actual_kernel_allocations_respect_budget_and_release_on_failure(){
 use std::sync::{Arc,atomic::AtomicBool};
 let e:project::EffectInstance=serde_json::from_value(serde_json::json!({"effect":"cross_dissolve"})).unwrap();
 let a=image::Image::filled(4,4,[0.2,0.1,0.0,0.5]);let b=a.clone();
 let small=super::context::RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,64,640).unwrap();
 assert!(small.render(&e,&a,&b,0.3,1.0).unwrap_err().contains("storage budget"));assert_eq!(small.pixel_bytes_in_use(),0);
 let enough=super::context::RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,64,1024).unwrap();
 let output=enough.render(&e,&a,&b,0.3,1.0).unwrap();assert_eq!(enough.pixel_bytes_in_use(),256);
 drop(output);assert_eq!(enough.pixel_bytes_in_use(),0);
}
