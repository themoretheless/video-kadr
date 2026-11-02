//! Filmcraft Ultra Key CPU matte/foreground processing; spatial parity qualification is pending.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};

// Ultra Key uses transparent boundaries, unlike HSL Secondary's repeat-edge blur.
// Preserve the original sliding-sum order and multiplication to match f32 results.
fn blur_matte(plane: &[f32], w: usize, h: usize, sigma: f32,
 checkpoint: &mut dyn FnMut()->Result<()>) -> Result<Vec<f32>> {
 ensure!(w > 0 && h > 0 && w.checked_mul(h) == Some(plane.len()), "invalid Ultra Key matte dimensions");
 let mut out=plane.to_vec();
 if sigma<=0.3 {return Ok(out);}
 fn radii(s: f32, dimension: usize)->Vec<usize> {
  let s=if s.is_finite(){s.min(dimension.max(8) as f32*2.0)}else{0.0};
  if s<=0.3 {return vec![];}
  let mut wl=(12.0*s*s/3.0+1.0).sqrt().floor() as i32;
  if wl%2==0 {wl-=1;}
  let wu=wl+2;
  let m=((12.0*s*s-(3*wl*wl) as f32-4.0*3.0*wl as f32-3.0*3.0)/(-4.0*wl as f32-4.0)).round() as i32;
  (0..3).map(|i|(((if i<m {wl}else{wu})-1)/2).max(0) as usize).collect()
 }
 fn rows(p: &mut [f32], w: usize, r: usize, checkpoint: &mut dyn FnMut()->Result<()>)->Result<()> {
  if r==0 {return Ok(());}
  for row in p.chunks_mut(w) {
   checkpoint()?;
   let src=row.to_vec();let inv=1.0/(2*r+1) as f32;let mut acc=0.0;
   for i in -(r as isize)..=r as isize {if i>=0&&(i as usize)<w {acc+=src[i as usize];}}
   for (x,value) in row.iter_mut().enumerate() {
    *value=acc*inv;
    let removed=x as isize-r as isize;
    let added=x+r+1;
    if removed>=0 {acc-=src[removed as usize];}
    if added<w {acc+=src[added];}
   }
  }
  Ok(())
 }
 for r in radii(sigma,w) {rows(&mut out,w,r,checkpoint)?;}
 let ry=radii(sigma,h);
 if !ry.is_empty() {
  let mut transposed=vec![0.0;out.len()];
  for y in 0..h {checkpoint()?;for x in 0..w {transposed[x*h+y]=out[y*w+x];}}
  for r in ry {rows(&mut transposed,h,r,checkpoint)?;}
  for y in 0..h {checkpoint()?;for x in 0..w {out[y*w+x]=transposed[x*h+y];}}
 }
 Ok(out)
}
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct UltraKey {
 pub key_color:[f32;4],pub output:u8,pub setting:u8,
 pub transparency:f32,
 pub highlight:f32,
 pub shadow:f32,
 pub tolerance:f32,
 pub pedestal:f32,
 pub choke:f32,
 pub soften:f32,
 pub spill:f32,
 pub contrast:f32,pub mid_point:f32,pub desaturate:f32,pub range:f32,pub spill_luma:f32,
 pub cc_saturation:f32,pub cc_hue:f32,pub cc_luminance:f32,
}
impl Default for UltraKey {fn default()->Self {Self{key_color:[0.0,0.8,0.2,1.0],output:0,setting:0,transparency:45.0,highlight:10.0,shadow:50.0,tolerance:50.0,pedestal:10.0,choke:0.0,soften:0.0,spill:50.0,contrast:0.0,mid_point:50.0,desaturate:25.0,range:50.0,spill_luma:50.0,cc_saturation:100.0,cc_hue:0.0,cc_luminance:100.0}}}
impl UltraKey {
 pub fn validate(&self)->Result<()> {
  ensure!(self.key_color.iter().all(|v|v.is_finite()),"nonfinite Ultra Key color");
  ensure!(self.output<=2&&self.setting<=3,"Ultra Key choice out of range");
  ensure!([self.transparency,self.highlight,self.shadow,self.tolerance,self.pedestal,self.choke,self.soften,self.spill,self.contrast,self.mid_point,self.desaturate,self.range,self.spill_luma].iter().all(|v|v.is_finite()&&(0.0..=100.0).contains(v)),"Ultra Key parameter out of range");
  ensure!([self.cc_saturation,self.cc_luminance].iter().all(|v|v.is_finite()&&(0.0..=200.0).contains(v))&&self.cc_hue.is_finite(),"Ultra Key color correction out of range");Ok(())
 }
 fn encoded(p:[f32;4])->[f32;3] {
  if p[3]<=1e-6 {[0.0;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]].map(|v|super::basic_grade::display(v.max(0.0)))}
 }
 fn matte(&self,p:[f32;4])->f32 {
  if p[3]<=1e-6 {return 0.0;}
  let k=super::color_key::ycbcr([self.key_color[0],self.key_color[1],self.key_color[2]]);
  let kmag2=(k[1]*k[1]+k[2]*k[2]).max(1e-6);
  let (g_mul,ped_mul)=match self.setting {1=>(0.85,0.7),2=>(1.2,1.5),_=>(1.0,1.0)};
  let tol=0.08+self.tolerance/100.0*0.5;
  let pedestal=(self.pedestal/100.0*0.3*ped_mul).min(0.9);
  let gain=(1.0+2.0*(self.transparency/100.0))*g_mul;
  let y=super::color_key::ycbcr(Self::encoded(p));
  let proj=(y[1]*k[1]+y[2]*k[2])/kmag2;
  let perp=((y[1]-proj*k[1]).powi(2)+(y[2]-proj*k[2]).powi(2)).sqrt()/kmag2.sqrt();
  let t=(perp/tol.max(1e-6)).clamp(0.0,1.0);
  let keyness=proj.clamp(0.0,1.5)*(1.0-t*t*(3.0-2.0*t));
  let mut m=(1.0-keyness*gain).clamp(0.0,1.0);
  m+=(y[0]-k[0]).max(0.0)*(self.highlight/100.0)*2.0*keyness.min(1.0);
  m+=(k[0]-y[0]).max(0.0)*(self.shadow/100.0)*2.0*keyness.min(1.0);
  ((m.clamp(0.0,1.0)-pedestal)/(1.0-pedestal)).clamp(0.0,1.0)
 }
 fn foreground(&self,p:[f32;4],a:f32)->[f32;4] {
  let kc=self.key_color;
  let dom=if kc[1]>=kc[0]&&kc[1]>=kc[2] {1}else if kc[2]>=kc[0] {2}else{0};
  let mut v=Self::encoded(p);
  let (o1,o2)=(v[(dom+1)%3],v[(dom+2)%3]);
  let range=self.range/100.0;
  let limit=o1.max(o2)*(1.0-range)+(o1+o2)*0.5*range;
  let excess=(v[dom]-limit).max(0.0);
  let luma=|c:[f32;3]|0.2126*c[0]+0.7152*c[1]+0.0722*c[2];
  if excess>0.0&&self.spill>0.0 {
   let l0=luma(v);v[dom]-=excess*(self.spill/100.0);
   let l1=luma(v);let lm=luma(v);
   v=v.map(|q|lm+(q-lm)*(1.0-(self.desaturate/100.0)*(excess*4.0).min(1.0)));
   let comp=(l0-l1)*(self.spill_luma/100.0);v=v.map(|q|q+comp);
  }
  let (cc_s,cc_h,cc_l)=(self.cc_saturation/100.0,self.cc_hue/360.0,self.cc_luminance/100.0);
  if (cc_s-1.0).abs()>1e-4||cc_h.abs()>1e-6||(cc_l-1.0).abs()>1e-4 {
   let mut h=super::tone_curves::rgb_to_hsl(v[0].clamp(0.0,1.0),v[1].clamp(0.0,1.0),v[2].clamp(0.0,1.0));
   h[0]=(h[0]+cc_h).rem_euclid(1.0);h[1]=(h[1]*cc_s).clamp(0.0,1.0);h[2]=(h[2]*cc_l).clamp(0.0,1.0);
   v=super::tone_curves::hsl_to_rgb(h[0],h[1],h[2]);
  }
  let l=v.map(|q|super::basic_grade::linear(q.clamp(0.0,1.0)));
  match self.output {
   1=>{let g=super::basic_grade::linear(a);[g*p[3],g*p[3],g*p[3],p[3]]},
   2=>[l[0]*p[3],l[1]*p[3],l[2]*p[3],p[3]],
   _=>{let na=a*p[3];[l[0]*na,l[1]*na,l[2]*na,na]}
  }
 }
 fn cleanup(&self,alpha:&mut Vec<f32>,w:usize,h:usize,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  let choke=self.choke/100.0;
  if choke>0.0 {
   let b=blur_matte(alpha,w,h,(choke*4.0*px_scale).max(0.5),checkpoint)?;
   let c=choke*0.5;
   for (a,bb) in alpha.iter_mut().zip(b) {*a=a.min(((bb-c)/(1.0-c)).clamp(0.0,1.0));}
  }
  if self.soften>0.0 {*alpha=blur_matte(alpha,w,h,(self.soften/100.0)*10.0*px_scale,checkpoint)?;}
  if self.contrast>0.0 {
   let g=1.0+(self.contrast/100.0)*4.0;let mid=self.mid_point/100.0;
   for a in alpha {*a=((*a-mid)*g+mid).clamp(0.0,1.0);}
  }
  Ok(())
 }
 pub fn pixel(&self,p:[f32;4])->[f32;4] {
  let mut alpha=vec![self.matte(p)];
  self.cleanup(&mut alpha,1,1,1.0,&mut||Ok(())).expect("valid singleton matte");
  self.foreground(p,alpha[0])
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.apply_linear_scaled(frame,1.0,checkpoint)
 }
 pub fn apply_linear_scaled(&self,frame:&mut super::linear_color_frame::LinearColorFrame,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;
  ensure!(px_scale.is_finite()&&px_scale>0.0,"invalid Ultra Key pixel scale");
  let (w,h)=frame.dimensions();let mut alpha=Vec::with_capacity(w*h);
  for row in frame.pixels().chunks(w) {checkpoint()?;alpha.extend(row.iter().map(|p|self.matte(*p)));}
  self.cleanup(&mut alpha,w,h,px_scale,checkpoint)?;
  let mut i=0;
  frame.map_premultiplied(&mut|p|{let out=self.foreground(p,alpha[i]);i+=1;out},checkpoint)
 }

}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum UltraChannel {Transparency,Highlight,Shadow,Tolerance,Pedestal,Choke,Soften,Spill,Contrast,MidPoint,Desaturate,Range,SpillLuma,CcSaturation,CcHue,CcLuminance}
pub type UltraAnimation=std::collections::BTreeMap<UltraChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&UltraAnimation)->Result<()> {
 for (channel,value) in animation {value.validate()?;let (min,max)=match channel {UltraChannel::CcHue=>(-(f32::MAX as f64),f32::MAX as f64),UltraChannel::CcSaturation|UltraChannel::CcLuminance=>(0.0,200.0),_=>(0.0,100.0)};ensure!(super::composition::animatable_values_in(value,min,max,true),"Ultra Key animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:UltraKey,animation:&UltraAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<UltraKey> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {UltraChannel::Transparency=>parameters.transparency=value,UltraChannel::Highlight=>parameters.highlight=value,UltraChannel::Shadow=>parameters.shadow=value,UltraChannel::Tolerance=>parameters.tolerance=value,UltraChannel::Pedestal=>parameters.pedestal=value,UltraChannel::Choke=>parameters.choke=value,UltraChannel::Soften=>parameters.soften=value,UltraChannel::Spill=>parameters.spill=value,UltraChannel::Contrast=>parameters.contrast=value,UltraChannel::MidPoint=>parameters.mid_point=value,UltraChannel::Desaturate=>parameters.desaturate=value,UltraChannel::Range=>parameters.range=value,UltraChannel::SpillLuma=>parameters.spill_luma=value,UltraChannel::CcSaturation=>parameters.cc_saturation=value,UltraChannel::CcHue=>parameters.cc_hue=value,UltraChannel::CcLuminance=>parameters.cc_luminance=value}}
 parameters.validate()?;Ok(parameters)
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn matte_blur_transparent_edges_and_cancellation() {
  let mut checkpoint=||Ok(());
  let one=blur_matte(&[1.0],1,1,1.0,&mut checkpoint).unwrap();
  assert_eq!(one[0].to_bits(),((1.0f32/3.0)*(1.0f32/3.0)).to_bits());
  let grid=blur_matte(&[1.0;9],3,3,1.0,&mut checkpoint).unwrap();
  assert_eq!(grid[4],1.0);
  assert_eq!(grid[0].to_bits(),((2.0f32/3.0)*(2.0f32/3.0)).to_bits());
  assert_eq!(blur_matte(&[0.25],1,1,0.3,&mut checkpoint).unwrap(),[0.25]);
  assert!(blur_matte(&[1.0;9],3,3,1.0,&mut||anyhow::bail!("cancelled")).is_err());
  assert!(blur_matte(&[1.0],2,1,1.0,&mut checkpoint).is_err());
 }
 #[test]fn original_cpu_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:UltraKey,input:[f32;4],bits:[u32;4]}
  #[derive(Deserialize)]struct Oracle {commit:String,cases:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ultra-key.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),315);
  for(index,case)in oracle.cases.iter().enumerate(){assert_eq!(case.parameters.pixel(case.input).map(f32::to_bits),case.bits,"original Ultra Key case {index}");}
 }
 #[test]fn original_spatial_cpu_rgba_bits() {
  #[derive(Deserialize)]struct Case {parameters:UltraKey,width:usize,height:usize,rgba:Vec<u8>,input:Vec<[f32;4]>,bits:Vec<u32>}
  #[derive(Deserialize)]struct Oracle {commit:String,spatial:Vec<Case>}
  let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ultra-key.json")).unwrap();
  assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.spatial.len(),384);
  for (index,case) in oracle.spatial.iter().enumerate() {
   case.parameters.validate().unwrap();
   assert_eq!(case.input.len(),case.width*case.height);
   let mut matte=case.input.iter().map(|p|case.parameters.matte(*p)).collect();
   case.parameters.cleanup(&mut matte,case.width,case.height,1.0,&mut||Ok(())).unwrap();
   let bits:Vec<u32>=case.input.iter().zip(matte).flat_map(|(p,a)|case.parameters.foreground(*p,a).map(f32::to_bits)).collect();
   assert_eq!(bits,case.bits,"original spatial Ultra Key case {index}");
   let surface=super::super::group_surface::GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()};
   let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&surface,&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels(),case.input,"source decode case {index}");
   case.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();
   let native_bits:Vec<u32>=frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
   assert_eq!(native_bits,case.bits,"native frame Ultra Key case {index}");
  }
 }
 #[test]fn persisted_scalar_and_key_components_preserve_output_and_controls() {
  let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"ultra_key","parameters":{"output":1,"setting":3,"highlight":75,"shadow":25,"choke":100,"ccSaturation":150,"ccHue":720,"ccLuminance":125,"contrast":40,"midPoint":35,"desaturate":80,"range":20,"spillLuma":75},"animation":{"spill":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}},"tolerance":{"mode":"constant","value":25}},"colorAnimation":{"g":{"mode":"constant","value":0.5},"a":{"mode":"constant","value":0}}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let super::super::composition::VideoEffect::UltraKey{parameters,animation:Some(animation),color_animation:Some(colors)}=effect else {panic!("Ultra Key")};
  let sampled=sample_color(sample_animation(parameters,&animation,500000).unwrap(),&colors,500000).unwrap();
  assert_eq!((sampled.cc_saturation,sampled.cc_hue,sampled.cc_luminance),(150.0,720.0,125.0));assert_eq!((sampled.contrast,sampled.mid_point,sampled.desaturate,sampled.range,sampled.spill_luma),(40.0,35.0,80.0,20.0,75.0));
  assert_eq!(sampled.spill,50.0);assert_eq!(sampled.tolerance,25.0);assert_eq!(sampled.key_color,[0.0,0.5,0.2,0.0]);assert_eq!((sampled.output,sampled.setting),(1,3));assert_eq!((sampled.highlight,sampled.shadow,sampled.choke),(75.0,25.0,100.0));
 }
 #[test]fn output_channels_and_hidden_alpha() {
  let effect=UltraKey::default();
  assert_eq!(effect.pixel([1.0,2.0,3.0,0.0]),[0.0;4]);
  let p=[0.0,0.5,0.0,0.5];
  assert_eq!(UltraKey{output:2,..effect}.pixel(p)[3],0.5);
  assert_eq!(UltraKey{output:1,..effect}.pixel(p)[3],0.5);
  assert!(effect.pixel(p)[3]<0.5);
  assert!(UltraKey{output:3,..effect}.validate().is_err());
 }
 #[test]fn all_scalar_channels_sample_independently_and_validate_source_ranges() {
  let mut animation=serde_json::Map::new();
  for channel in ["transparency","highlight","shadow","tolerance","pedestal","choke","soften","spill","contrast","midPoint","desaturate","range","spillLuma","ccSaturation","ccHue","ccLuminance"] {
   animation.insert(channel.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":1000000,"value":100}]}}));
  }
  let curves:UltraAnimation=serde_json::from_value(serde_json::Value::Object(animation)).unwrap();
  let parameters=sample_animation(UltraKey::default(),&curves,250000).unwrap();
  assert_eq!([parameters.transparency,parameters.highlight,parameters.shadow,parameters.tolerance,parameters.pedestal,parameters.choke,parameters.soften,parameters.spill,parameters.contrast,parameters.mid_point,parameters.desaturate,parameters.range,parameters.spill_luma,parameters.cc_saturation,parameters.cc_hue,parameters.cc_luminance],[25.0;16]);
  for (channel,value,accepted) in [("ccHue",-720.0,true),("ccSaturation",200.0,true),("ccLuminance",200.1,false),("contrast",100.1,false),("spillLuma",-1.0,false)] {
   let curve:UltraAnimation=serde_json::from_value(serde_json::json!({channel:{"mode":"constant","value":value}})).unwrap();
   assert_eq!(validate_animation(&curve).is_ok(),accepted,"{channel} {value}");
  }
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum UltraColorChannel {R,G,B,A}
pub type UltraColorAnimation=std::collections::BTreeMap<UltraColorChannel,super::composition::AnimatableValue>;
pub fn validate_color_animation(animation:&UltraColorAnimation)->Result<()> {
 for value in animation.values(){value.validate()?;ensure!(super::composition::animatable_values_in(value,-(f32::MAX as f64),f32::MAX as f64,true),"Ultra Key animation exceeds finite f32 range");}Ok(())
}
pub fn sample_color(mut parameters:UltraKey,animation:&UltraColorAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<UltraKey> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_color_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;let index=match channel {UltraColorChannel::R=>0,UltraColorChannel::G=>1,UltraColorChannel::B=>2,UltraColorChannel::A=>3};parameters.key_color[index]=value;}
 parameters.validate()?;Ok(parameters)
}
