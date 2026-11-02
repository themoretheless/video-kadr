#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition, transition_spec::TransitionSpec, concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(speed:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.whip-pan","params":{"speed":{"Float":speed}}})).unwrap()}
#[test]
fn directional_seam_blur_reverse_and_conversion(){
 let a=vec![[1.,0.,0.,1.];64*4];let b=vec![[0.,0.,1.,1.];64*4];
 let middle=spatial_transition(&spec(80.),64,4,&a,&b,0.5,false).unwrap();
 assert_eq!(middle[16],[1.,0.,0.,1.]);assert_eq!(middle[48],[0.,0.,1.,1.]);
 assert_eq!(spatial_transition(&spec(80.),64,4,&a,&b,0.,false).unwrap(),a);
 assert_eq!(spatial_transition(&spec(80.),64,4,&a,&b,1.,false).unwrap(),b);
 let striped:Vec<[f32;4]>=(0..256).map(|i|if i%64%8<4{[1.;4]}else{[0.,0.,0.,1.]}).collect();
 let low=spatial_transition(&spec(20.),64,4,&striped,&striped,0.5,false).unwrap();
 let high=spatial_transition(&spec(100.),64,4,&striped,&striped,0.5,false).unwrap();assert_ne!(low,high);
 assert_eq!(spatial_transition(&spec(80.),64,4,&a,&b,0.375,false).unwrap(),spatial_transition(&spec(80.),64,4,&b,&a,0.625,true).unwrap());
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.whip-pan","params":{"speed":37}})).unwrap();assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(37.));
 for speed in [19.,101.]{assert!(spec(speed).validate().is_err());}
}

#[test]
#[ignore="requires CONCAT_WHIP_PAN_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_WHIP_PAN_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.whip-pan");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"speed");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Whip Pan channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
