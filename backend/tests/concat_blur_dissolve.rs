#![cfg(feature = "concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition, transition_spec::TransitionSpec, concat_effect_upgrade::{native_transition, AppliedFilter}};
fn spec(strength:f32)->TransitionSpec { serde_json::from_value(json!({"effect":"concat.blur-dissolve","params":{"strength":{"Float":strength}}})).unwrap() }
#[test]
fn blur_peak_endpoints_reverse_and_source_conversion() {
 let mut a=vec![[0.,0.,0.,1.];81]; a[40]=[1.,0.,0.,1.];
 let b=vec![[0.,0.,1.,1.];81];
 let render=|s,p,reverse| spatial_transition(&spec(s),9,9,&a,&b,p,reverse).unwrap();
 assert_eq!(render(60.,0.,false),a); assert_eq!(render(60.,1.,false),b);
 let sharp=render(0.,0.5,false);let blurred=render(10.,0.5,false);
 assert!(blurred[40][0]<sharp[40][0]);assert!(blurred[37][0]>sharp[37][0]);
 assert!(blurred.iter().all(|p|p[3]==1.));
 assert_eq!(render(10.,0.375,false),spatial_transition(&spec(10.),9,9,&b,&a,0.625,true).unwrap());
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.blur-dissolve","params":{"strength":10}})).unwrap();
 assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(10.));
 for value in [-1.,101.] {assert!(spec(value).validate().is_err());}
 let bad:TransitionSpec=serde_json::from_value(json!({"effect":"concat.blur-dissolve","params":{"unknown":{"Float":1}}})).unwrap();assert!(bad.validate().is_err());
}
#[test]
#[ignore="requires CONCAT_BLUR_DISSOLVE_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_BLUR_DISSOLVE_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.blur-dissolve");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"strength");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Blur Dissolve channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
