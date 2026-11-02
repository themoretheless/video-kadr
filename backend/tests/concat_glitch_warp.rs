#![cfg(feature="concat-qualification")]
use serde_json::json;
use video_kadr_backend::domain::{concat_qualification::spatial_transition,transition_spec::TransitionSpec,concat_effect_upgrade::{native_transition,AppliedFilter}};
fn spec(amount:f32)->TransitionSpec {serde_json::from_value(json!({"effect":"concat.glitch-warp","params":{"amount":{"Float":amount}}})).unwrap()}
#[test]
fn chromatic_displacement_preserves_unshifted_alpha(){
 let a:Vec<[f32;4]>=(0..64*32).map(|i|if i%64%16<8{[1.,1.,1.,1.]}else{[0.,0.,0.,0.25]}).collect();let b=vec![[0.,0.,1.,0.5];64*32];
 let render=|amount,p|spatial_transition(&spec(amount),64,32,&a,&b,p,false).unwrap();
 assert_eq!(render(100.,0.),a);
 let low=render(0.,0.5);let high=render(100.,0.5);assert_ne!(low,high);assert_eq!(high,render(100.,0.5));assert!(high.iter().any(|p|p[0]!=p[1]));assert!(high.iter().zip(&low).all(|(a,b)|a[3]==b[3]));assert_ne!(&high[..64],&high[128..192]);
 assert_eq!(render(37.,0.375),spatial_transition(&spec(37.),64,32,&b,&a,0.625,true).unwrap());
 let link:AppliedFilter=serde_json::from_value(json!({"id":"concat.glitch-warp","params":{"amount":37}})).unwrap();assert_eq!(native_transition(&link).unwrap().unwrap().kernel.unwrap(),spec(37.));for amount in [-1.,101.]{assert!(spec(amount).validate().is_err());}
}

#[test]
#[ignore="requires CONCAT_GLITCH_WARP_ORACLE generated from pinned original GPU"]
fn original_metal_spatial_images() {
 let oracle:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_GLITCH_WARP_ORACLE").unwrap()).unwrap()).unwrap();
 assert_eq!(oracle["effect"],"concat.glitch-warp");assert_eq!(oracle["reference"]["commit"],"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170");
 assert_eq!(oracle["rendererBodyUnchanged"],true);assert_eq!(oracle["fullImage"],true);assert_eq!(oracle["upstreamProbes"],12);assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
 let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
 let mut mismatches=[0usize;4];let mut channels=0;let mut max_abs=[0f32;4];
 for c in cases {
  assert_eq!(c["parameter"],"amount");
  let a:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let b:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let expected:Vec<[f32;4]>=serde_json::from_value(c["expected"].clone()).unwrap();
  let actual=spatial_transition(&spec(c["value"].as_f64().unwrap() as f32),c["width"].as_u64().unwrap() as usize,c["height"].as_u64().unwrap() as usize,&a,&b,c["progress"].as_f64().unwrap() as f32,false).unwrap();assert_eq!(actual.len(),expected.len());
  for (a,b) in actual.iter().zip(expected.iter()){for k in 0..4 {channels+=1;mismatches[k]+=usize::from(a[k].to_bits()!=b[k].to_bits());max_abs[k]=max_abs[k].max((a[k]-b[k]).abs());}}
 }
 assert_eq!(channels,if oracle["independentValidation"]==true {132864}else{101632});eprintln!("Glitch Warp channels={channels} mismatches={mismatches:?} max_abs={max_abs:?}");assert_eq!(mismatches,[0;4]);
}
