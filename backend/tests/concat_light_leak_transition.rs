#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition, transition_spec::TransitionSpec, concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(warmth:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.light-leak-transition","params":{"warmth":{"Float":warmth}}})).unwrap()}
#[test]
fn hot_core_warm_halo_preserve_alpha_hdr_and_reverse(){
 let a=vec![[0.,0.,0.,0.25];45];let b=vec![[0.,0.,0.,0.75];45];
 let zero=spatial_transition(&spec(0.),9,5,&a,&b,0.5,false).unwrap();assert!(zero.iter().all(|p|*p==[0.,0.,0.,0.5]));
 let flash=spatial_transition(&spec(50.),9,5,&a,&b,0.5,false).unwrap();assert_eq!(flash[4][0],1.);assert_eq!(flash[4][1],1.);assert!(flash[4][2]>0.5&&flash[4][2]<1.);assert!(flash.iter().all(|p|p[3]==0.5));
 let sweep=spatial_transition(&spec(80.),9,5,&a,&b,0.25,false).unwrap();assert!(sweep[0][0]>sweep[44][0]);
 assert_eq!(spatial_transition(&spec(80.),9,5,&a,&b,0.375,false).unwrap(),spatial_transition(&spec(80.),9,5,&b,&a,0.625,true).unwrap());
 let hdr=vec![[4.,3.,2.,0.];45];let result=spatial_transition(&spec(100.),9,5,&hdr,&hdr,0.5,false).unwrap();assert!(result.iter().all(|p|p[0]>3.99&&p[1]>2.99&&p[2]>1.99&&p[3]==0.));
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.light-leak-transition","params":{"warmth":37}})).unwrap();assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(37.));
 for warmth in [-1.,101.]{assert!(spec(warmth).validate().is_err());}
}

#[test]
#[ignore="requires CONCAT_LIGHT_LEAK_TRANSITION_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_LIGHT_LEAK_TRANSITION_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.light-leak-transition");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"warmth");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Light Leak channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
