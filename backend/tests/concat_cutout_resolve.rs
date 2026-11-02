use std::sync::{Arc,atomic::AtomicBool};
use serde_json::json;
use video_kadr_backend::domain::{concat_cutout_import::plan,concat_cutout_clock::{ResolvedMask,MaskTimeline,Dispatch,apply_video},concat_cutout_resolve::resolve,composition::{ClipPlacement,PlaybackMode},concat_picture::Preparation,group_surface::GroupSurface};
fn blank()->ResolvedMask{ResolvedMask{width:8,height:8,probabilities:vec![0;64]}}
#[test]
fn automatic_blank_falls_back_but_custom_blank_remains_an_intentional_cut(){
 let cancel=AtomicBool::new(false);let auto=plan(&json!({"mode":"auto"})).unwrap();assert!(resolve(&auto,&blank(),&[],1.,&cancel).unwrap().is_none());
 let erased=plan(&json!({"mode":"custom","feather":0,"strokes":[{"tool":"eraser","size":0.5,"points":[[0.5,0.5]]}]})).unwrap();let output=resolve(&erased,&blank(),&[],1.,&cancel).unwrap().unwrap();assert!(output.probabilities.iter().all(|v|*v==0));
}
#[test]
fn corrected_mask_enters_source_clock_dispatch_and_alpha_application(){
 let p=plan(&json!({"mode":"custom","feather":0,"strokes":[{"tool":"brush","size":0.5,"points":[[0.5,0.5]]}]})).unwrap();
 let mask=resolve(&p,&blank(),&[],1.,&AtomicBool::new(false)).unwrap().unwrap();assert_eq!(mask.probabilities[36],255);assert_eq!(mask.probabilities[0],0);
 let placement:ClipPlacement=serde_json::from_value(json!({"timelineStartTick":1000000,"sourceInTick":200000,"sourceOutTick":1200000,"speed":2})).unwrap();
 let mut frame=GroupSurface{width:8,height:8,rgba:[99,88,77,255].repeat(64)};
 let status=apply_video(&mut frame,&placement,PlaybackMode::Forward,1100000,&MaskTimeline::new(vec![400]).unwrap(),&Preparation{crop:[0.;4],canvas:[8,8],flip_h:false,flip_v:false},false,&mut|_|Ok(Some(mask.clone())),&mut||Ok(())).unwrap();
 assert_eq!(status,Dispatch::Applied{source_us:400000,mask_millis:400});assert_eq!(&frame.rgba[..4],&[99,88,77,0]);assert_eq!(&frame.rgba[36*4..37*4],&[99,88,77,255]);
}
#[test]
fn smart_region_and_confidence_fallback_use_the_same_shared_engine(){
 let p=plan(&json!({"mode":"custom","feather":0,"strokes":[{"tool":"smartBrush","size":0.1,"points":[[0.25,0.25]],"at":0.25}]})).unwrap();let cancel=AtomicBool::new(false);
 let missing=resolve(&p,&blank(),&[],1.,&cancel).unwrap().unwrap();assert!(missing.probabilities.iter().all(|v|*v==0));
 let region=Arc::new(ResolvedMask{width:2,height:2,probabilities:vec![255,0,0,0]});let supplied=resolve(&p,&blank(),&[Some(region)],1.,&cancel).unwrap().unwrap();assert_eq!(supplied.probabilities[0],255);assert_eq!(supplied.probabilities[63],0);
 assert!(resolve(&p,&blank(),&[],1.,&AtomicBool::new(true)).is_err());assert!(resolve(&p,&ResolvedMask{width:2,height:2,probabilities:vec![0]},&[],1.,&cancel).is_err());
}
