#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition, transition_spec::TransitionSpec, concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(strength:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.zoom-blur-transition","params":{"strength":{"Float":strength}}})).unwrap()}
#[test]
fn radial_blur_zoom_reverse_and_conversion(){
 let a:Vec<[f32;4]>=(0..45).map(|i|if i%9%3==0{[1.,0.,0.,1.]}else{[0.,0.,0.,1.]}).collect();let b=vec![[0.,0.,1.,1.];45];
 let low=spatial_transition(&spec(0.),9,5,&a,&b,0.5,false).unwrap();let high=spatial_transition(&spec(100.),9,5,&a,&b,0.5,false).unwrap();assert_ne!(low,high);assert!(high.iter().all(|p|p[3]==1.));
 assert_eq!(spatial_transition(&spec(70.),9,5,&a,&b,0.,false).unwrap(),a);
 assert_eq!(spatial_transition(&spec(70.),9,5,&a,&b,0.375,false).unwrap(),spatial_transition(&spec(70.),9,5,&b,&a,0.625,true).unwrap());
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.zoom-blur-transition","params":{"strength":37}})).unwrap();assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(37.));
 for strength in [-1.,101.]{assert!(spec(strength).validate().is_err());}
}

#[test]
#[ignore="requires CONCAT_ZOOM_BLUR_TRANSITION_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_ZOOM_BLUR_TRANSITION_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.zoom-blur-transition");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"strength");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Zoom Blur channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
