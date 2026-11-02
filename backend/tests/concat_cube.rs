#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition,transition_spec::TransitionSpec,concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(_:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.cube"})).unwrap()}
#[test]
fn cube_squeezes_shades_and_keeps_gap_opaque(){
 let a=vec![[1.,0.,0.,0.5];45];let b=vec![[0.,0.,1.,0.25];45];
 let render=|p|spatial_transition(&spec(0.),9,5,&a,&b,p,false).unwrap();assert_eq!(render(0.),a);assert_eq!(render(1.),b);
 let first=render(0.25);assert!(first[22][0]>0.&&first[22][0]<1.);assert_eq!(first[22][3],0.5);assert_eq!(first[0][3],1.);
 let second=render(0.75);assert!(second[22][2]>0.&&second[22][2]<1.);assert_eq!(second[22][3],0.25);
 assert_eq!(render(0.375),spatial_transition(&spec(0.),9,5,&b,&a,0.625,true).unwrap());
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.cube"})).unwrap();assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(0.));
 let bad:TransitionSpec=serde_json::from_value(json!({"effect":"concat.cube","params":{"unknown":{"Float":1}}})).unwrap();assert!(bad.validate().is_err());
}

#[test]
#[ignore="requires CONCAT_CUBE_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_CUBE_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.cube");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),96);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"none");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {33216}else{25408});eprintln!("Cube channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
