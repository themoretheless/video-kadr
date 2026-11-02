//! Independent transition kernels in premultiplied linear working space.
//! Filmcraft render/transitions legacy::masked, MIT, see docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::Result;

pub fn cross_dissolve(a:&LinearColorFrame,b:&LinearColorFrame,progress:f32,reverse:bool,checkpoint:&mut dyn FnMut()->Result<()>)->Result<LinearColorFrame>{
 let(a,b,progress)=if reverse{(b,a,1.-progress)}else{(a,b,progress)};
 let p=if progress.is_finite(){progress.clamp(0.,1.)}else{0.};
 if a.dimensions()!=b.dimensions(){return Ok(if p<0.5{a.clone()}else{b.clone()})}
 if p<=0.{return Ok(a.clone())}if p>=1.{return Ok(b.clone())}
 let mut result=a.clone();let mut index=0;
 result.map_premultiplied(&mut|pixel|{
  let other=b.pixels()[index];index+=1;
  std::array::from_fn(|channel|pixel[channel]*(1.-p)+other[channel]*p)
 },checkpoint)?;
 Ok(result)
}

#[cfg(test)]mod tests{
 use super::*;
 use crate::domain::group_surface::GroupSurface;
 #[test]fn original_transition_pixels_cross_dissolve(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-pixels.json")).unwrap();
  assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),168);
  for case in cases{
   let frame=|key:&str|LinearColorFrame::from_surface(&GroupSurface{width:case["width"].as_u64().unwrap()as usize,height:case["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case[key].clone()).unwrap()},&mut||Ok(())).unwrap();
   let(a,b)=(frame("a"),frame("b"));
   let actual=cross_dissolve(&a,&b,case["progress"].as_f64().unwrap()as f32,case["reverse"].as_bool().unwrap(),&mut||Ok(())).unwrap();
   let bits:Vec<u32>=actual.pixels().iter().flatten().map(|v|v.to_bits()).collect();
   assert_eq!(bits,serde_json::from_value::<Vec<u32>>(case["bits"].clone()).unwrap(),"{case}");
  }
 }
 #[test]fn transition_pixels_propagate_cancellation_and_original_nonfinite_progress(){
  let frame=|rgba:Vec<u8>|LinearColorFrame::from_surface(&GroupSurface{width:1,height:1,rgba},&mut||Ok(())).unwrap();
  let(a,b)=(frame(vec![255,0,0,128]),frame(vec![0,255,0,64]));
  assert!(cross_dissolve(&a,&b,0.5,false,&mut||anyhow::bail!("cancelled")).is_err());
  assert_eq!(cross_dissolve(&a,&b,f32::NAN,false,&mut||Ok(())).unwrap().pixels(),a.pixels());
  assert_eq!(cross_dissolve(&a,&b,f32::NAN,true,&mut||Ok(())).unwrap().pixels(),b.pixels());
 }
}
