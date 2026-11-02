//! Native working-frame adapter for validated procedural transitions.
use super::{linear_color_frame::LinearColorFrame,transition_spec::TransitionSpec,transition_kernel_pack::{context::RenderContext,image::Image}};
use anyhow::{Result,ensure};
use std::sync::{Arc,atomic::AtomicBool};
pub fn render(spec:&TransitionSpec,a:&LinearColorFrame,b:&LinearColorFrame,progress:f32,reverse:bool,checkpoint:&mut dyn FnMut()->Result<()>)->Result<LinearColorFrame>{
 checkpoint()?;
 if let Some(style)=spec.concat_style().map_err(anyhow::Error::msg)? {return super::concat_transitions::render(a,b,progress,reverse,style,checkpoint);}
 ensure!(a.dimensions()==b.dimensions(),"procedural transition canvas mismatch");
 let(w,h)=a.dimensions();
 let to_image=|frame:&LinearColorFrame,check:&mut dyn FnMut()->Result<()>|->Result<Image>{
  let mut px=Vec::with_capacity(w*h*4);
  for row in frame.pixels().chunks(w){check()?;for pixel in row{px.extend_from_slice(pixel);}}
  Ok(Image{w,h,px:px.into()})
 };
 let(ai,bi)=(to_image(a,checkpoint)?,to_image(b,checkpoint)?);
 let effect=spec.compile().map_err(anyhow::Error::msg)?;
 let context=RenderContext::new(Arc::new(AtomicBool::new(false)),2,3840*2160).map_err(anyhow::Error::msg)?;
 let(ai,bi,p)=if reverse{(&bi,&ai,1.-progress)}else{(&ai,&bi,progress)};
 let image=context.render_with_checkpoint(&effect,ai,bi,p,1.0,&mut||checkpoint().map_err(|e|e.to_string())).map_err(anyhow::Error::msg)?;
 let mut output=a.clone();let mut pixels=image.px.chunks_exact(4);
 output.map_premultiplied(&mut|_|{let p=pixels.next().expect("validated transition image shape");[p[0],p[1],p[2],p[3]]},checkpoint)?;
 Ok(output)
}

#[cfg(test)]mod tests{
 use super::*;
 #[test]fn native_adapter_matches_all_original_transition_pixels_and_controls(){
  use serde_json::Value;
  let fixtures=[include_str!("../../tests/fixtures/filmcraft-all-transition-pixels.json"),include_str!("../../tests/fixtures/filmcraft-transition-controls.json")];
  let mut checked=0;
  for fixture in fixtures{
   let oracle:Value=serde_json::from_str(fixture).unwrap();
   for c in oracle["cases"].as_array().unwrap(){
    let(w,h)=if c.get("width").is_some(){(c["width"].as_u64().unwrap()as usize,c["height"].as_u64().unwrap()as usize)}else{(oracle["width"].as_u64().unwrap()as usize,oracle["height"].as_u64().unwrap()as usize)};
    let frame=|value:&Value|LinearColorFrame::from_surface(&super::super::group_surface::GroupSurface{width:w,height:h,rgba:serde_json::from_value(value.clone()).unwrap()},&mut||Ok(())).unwrap();
    let(a,b)=(frame(c.get("a").unwrap_or(&oracle["a"])),frame(c.get("b").unwrap_or(&oracle["b"])));
    let params=if let Some(parameter)=c.get("parameter"){std::collections::BTreeMap::from([(parameter.as_str().unwrap().into(),c["value"].clone())])}else{Default::default()};
    let spec=TransitionSpec{effect:c["effect"].as_str().unwrap().into(),params};
    let output=render(&spec,&a,&b,c["progress"].as_f64().unwrap()as f32,c["reverse"].as_bool().unwrap(),&mut||Ok(())).unwrap();
    let expected:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();
    assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"native adapter case{checked} {c}");
    checked+=1;
   }
  }
  assert_eq!(checked,20244);
 }
}
