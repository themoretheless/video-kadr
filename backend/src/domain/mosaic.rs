//! Filmcraft vfx Mosaic: block-center sampling and softened tile interpolation.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all="camelCase", deny_unknown_fields)]
pub struct Mosaic {pub horizontal:f32,pub vertical:f32,pub softness:f32,pub sharp:bool}
impl Default for Mosaic {fn default()->Self {Self{horizontal:10.0,vertical:10.0,softness:0.0,sharp:false}}}
pub(crate) fn sample(pixels:&[[f32;4]],w:usize,h:usize,x:f32,y:f32)->[f32;4] {
 let fx=(x-0.5).clamp(0.0,w as f32-1.0);let fy=(y-0.5).clamp(0.0,h as f32-1.0);
 let x0=fx.floor() as usize;let y0=fy.floor() as usize;
 let x1=(x0+1).min(w-1);let y1=(y0+1).min(h-1);
 let tx=fx-x0 as f32;let ty=fy-y0 as f32;
 let(a,b,c,d)=(pixels[y0*w+x0],pixels[y0*w+x1],pixels[y1*w+x0],pixels[y1*w+x1]);
 std::array::from_fn(|k|{let top=a[k]+(b[k]-a[k])*tx;let bottom=c[k]+(d[k]-c[k])*tx;top+(bottom-top)*ty})
}
impl Mosaic {
 pub fn validate(&self)->Result<()> {
  ensure!([self.horizontal,self.vertical].iter().all(|v|v.is_finite()&&(1.0..=4000.0).contains(v)),"Mosaic blocks out of range");
  ensure!(self.softness.is_finite()&&(0.0..=100.0).contains(&self.softness),"Mosaic softness out of range");Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;
  let(w,h)=frame.dimensions();let bx=self.horizontal as usize;let by=self.vertical as usize;
  let bw=(w as f32/bx as f32).max(1.0);let bh=(h as f32/by as f32).max(1.0);
  let nx=((w as f32/bw).ceil() as usize).max(1);let ny=((h as f32/bh).ceil() as usize).max(1);
  let mut small=Vec::with_capacity(nx*ny);
  for j in 0..ny {checkpoint()?;for i in 0..nx {small.push(sample(frame.pixels(),w,h,(i as f32+0.5)*bw,(j as f32+0.5)*bh));}}
  let soft=(self.softness/100.0).clamp(0.0,1.0);let mut index=0;
  // `sharp` is authored by the original definition but unused by vfx::mosaic.
  frame.map_premultiplied(&mut |_|{
   let x=index%w;let y=index/w;index+=1;
   let ti=(x as f32/bw) as usize;let tj=(y as f32/bh) as usize;
   let sharp=small[tj.min(ny-1)*nx+ti.min(nx-1)];
   if soft>0.0 {let sm=sample(&small,nx,ny,(x as f32+0.5)/bw,(y as f32+0.5)/bh);std::array::from_fn(|k|sharp[k]+(sm[k]-sharp[k])*soft)}else{sharp}
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
 use super::*;
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Mosaic, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-mosaic.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),96);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Mosaic frame {index}");
        }
    }

 use crate::domain::{linear_color_frame::LinearColorFrame,group_surface::GroupSurface};
 #[test]fn persisted_independent_block_and_softness_curves() {
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"mosaic","parameters":{"sharp":true},"animation":{"horizontal":curve(1.0,3.0),"vertical":curve(4.0,8.0),"softness":curve(0.0,100.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Mosaic{parameters,animation:Some(curves)}=effect else {panic!("Mosaic")};
  assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),Mosaic{horizontal:2.0,vertical:6.0,softness:50.0,sharp:true});
  let bad: MosaicAnimation=serde_json::from_value(serde_json::json!({"horizontal":curve(0.0,1.0)})).unwrap();assert!(validate_animation(&bad).is_err());
 }
 #[test]fn blocks_softness_thin_frames_and_cancellation() {
  let source=GroupSurface{width:4,height:1,rgba:vec![255,0,0,255,0,255,0,128,0,0,255,64,255,255,255,0]};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
  let mut unchanged=original.clone();
  Mosaic{horizontal:4000.0,vertical:4000.0,softness:100.0,sharp:true}.apply_linear(&mut unchanged,&mut||Ok(())).unwrap();
  assert_eq!(unchanged.pixels(),original.pixels());
  let mut blocks=original.clone();
  Mosaic{horizontal:2.0,vertical:1.0,..Default::default()}.apply_linear(&mut blocks,&mut||Ok(())).unwrap();
  assert_eq!(blocks.pixels()[0],blocks.pixels()[1]);assert_eq!(blocks.pixels()[2],blocks.pixels()[3]);
  let mut soft=original.clone();
  Mosaic{horizontal:2.0,vertical:1.0,softness:100.0,..Default::default()}.apply_linear(&mut soft,&mut||Ok(())).unwrap();
  assert_ne!(soft.pixels()[1],blocks.pixels()[1]);
  assert!(Mosaic::default().apply_linear(&mut soft,&mut||anyhow::bail!("cancelled")).is_err());
  assert!(Mosaic{horizontal:0.0,..Default::default()}.validate().is_err());
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum MosaicChannel {Horizontal,Vertical,Softness}
pub type MosaicAnimation=std::collections::BTreeMap<MosaicChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&MosaicAnimation)->Result<()> {
 for(channel,curve)in animation {
  curve.validate()?;let(min,max)=match channel {MosaicChannel::Softness=>(0.0,100.0),_=>(1.0,4000.0)};
  ensure!(super::composition::animatable_values_in(curve,min,max,true),"Mosaic animation out of range");
 }Ok(())
}
pub fn sample_animation(mut parameters:Mosaic,animation:&MosaicAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<Mosaic> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {MosaicChannel::Horizontal=>parameters.horizontal=value,MosaicChannel::Vertical=>parameters.vertical=value,MosaicChannel::Softness=>parameters.softness=value}}
 parameters.validate()?;Ok(parameters)
}
