#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition, transition_spec::TransitionSpec, concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(direction:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.zoom-transition","params":{"direction":{"Choice":if direction<0.5{0}else{1}}}})).unwrap()}
#[test]
fn directions_zoom_geometry_reverse_and_preserve_original_end_scale(){
 let a:Vec<[f32;4]>=(0..45).map(|i|[(i%9)as f32/8.,(i/9)as f32/4.,0.,1.]).collect();let b:Vec<[f32;4]>=(0..45).map(|i|[0.,(i/9)as f32/4.,(i%9)as f32/8.,1.]).collect();
 let render=|d,p|spatial_transition(&spec(d),9,5,&a,&b,p,false).unwrap();
 assert_ne!(render(0.,0.5),render(1.,0.5));
 for d in [0.,1.] {assert_eq!(render(d,0.),a);assert_eq!(render(d,0.375),spatial_transition(&spec(d),9,5,&b,&a,0.625,true).unwrap());}
 assert_eq!(render(1.,1.),b);
 // Original In ends at a clamped near-zero incoming scale; preserve this behavior.
 assert_ne!(render(0.,1.),b);
 for (direction,choice) in [(0.49999997,0),(0.49999999,1),(0.5,1),(1.,1)] {
  let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.zoom-transition","params":{"direction":direction}})).unwrap();
  assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap().params["direction"],json!({"Choice":choice}));
 }
 let bad:TransitionSpec=serde_json::from_value(json!({"effect":"concat.zoom-transition","params":{"direction":{"Choice":2}}})).unwrap();assert!(bad.validate().is_err());
}

#[test]
#[ignore="requires CONCAT_ZOOM_TRANSITION_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_ZOOM_TRANSITION_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.zoom-transition");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut selected=Vec::new();let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"direction");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());if selected.len()<16 && actual.iter().zip(&expected).any(|(a,b)|a.map(f32::to_bits)!=b.map(f32::to_bits)){selected.push(c.clone());}
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 if let Ok(path)=std::env::var("CONCAT_ZOOM_DIAGNOSTIC_SELECTION"){std::fs::write(path,serde_json::to_vec_pretty(&json!({"effect":"concat.zoom-transition","diagnosticSelection":true,"cases":selected})).unwrap()).unwrap();}
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Zoom channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
