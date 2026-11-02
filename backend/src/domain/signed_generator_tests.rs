//! Original negative/preroll generator regressions for signed effect methods.
use super::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
#[test]
fn all_original_signed_generator_frames_match_exactly() {
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-signed-generators.json")).unwrap();
 let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),560);
 for(index,case)in cases.iter().enumerate(){
  let source=GroupSurface{width:case["width"].as_u64().unwrap()as usize,height:case["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};
  let mut frame=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  let seconds=case["localTick"].as_i64().unwrap()as f64/1_000_000.;
  match case["effect"].as_str().unwrap(){
   "noise"=>serde_json::from_value::<super::noise::Noise>(case["parameters"].clone()).unwrap().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).unwrap(),
   "strobe"=>serde_json::from_value::<super::strobe::Strobe>(case["parameters"].clone()).unwrap().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).unwrap(),
   "wave_warp"=>serde_json::from_value::<super::wave_warp::WaveWarp>(case["parameters"].clone()).unwrap().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).unwrap(),
   _=>panic!("unknown fixture effect"),
  }
  let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
  assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"signed generator case {index} {case}");
 }
}

#[test]
fn signed_generators_reject_invalid_time_before_processing() {
 let surface=GroupSurface{width:1,height:1,rgba:vec![255;4]};
 for seconds in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY,f64::MAX] {
  let mut frame=LinearColorFrame::from_surface(&surface,&mut||Ok(())).unwrap();
  assert!(super::noise::Noise::default().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).is_err());
  assert!(super::strobe::Strobe::default().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).is_err());
  assert!(super::wave_warp::WaveWarp::default().apply_linear_at_seconds(&mut frame,seconds,&mut||Ok(())).is_err());
 }
}
