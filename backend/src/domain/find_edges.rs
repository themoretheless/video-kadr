//! Filmcraft Find Edges, premultiplied Rec709 Sobel reference.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,Default,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct FindEdges {pub invert:bool,pub blend:f32}
impl FindEdges {
 pub fn validate(&self)->Result<()> {ensure!(self.blend.is_finite()&&(0.0..=100.0).contains(&self.blend),"Find Edges blend out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();let blend=self.blend/100.0;let mut index=0;
  frame.map_premultiplied(&mut |_|{
   let x=index%w;let y=index/w;let original=src[index];index+=1;
   let l=|dx:isize,dy:isize|{let sx=(x as isize+dx).clamp(0,w as isize-1)as usize;let sy=(y as isize+dy).clamp(0,h as isize-1)as usize;let p=src[sy*w+sx];0.2126*p[0]+0.7152*p[1]+0.0722*p[2]};
   let gx=l(1,-1)+2.0*l(1,0)+l(1,1)-l(-1,-1)-2.0*l(-1,0)-l(-1,1);
   let gy=l(-1,1)+2.0*l(0,1)+l(1,1)-l(-1,-1)-2.0*l(0,-1)-l(1,-1);
   let mut m=(gx*gx+gy*gy).sqrt().min(1.0);if !self.invert {m=1.0-m;}
   let mut out=original;for k in 0..3 {out[k]=m*original[3]*(1.0-blend)+original[k]*blend;}out
  },checkpoint)
 }
}
pub fn validate_animation(animation:&super::composition::AnimatableValue)->Result<()> {animation.validate()?;ensure!(super::composition::animatable_values_in(animation,0.0,100.0,true),"Find Edges animation out of range");Ok(())}
pub fn sample_animation(mut parameters:FindEdges,animation:&super::composition::AnimatableValue,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<FindEdges> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();
parameters.validate()?;validate_animation(animation)?;parameters.blend=super::nested_composition::sample_value(animation,time)?as f32;parameters.validate()?;Ok(parameters)}
#[cfg(test)]mod tests {
 use super::*;
    #[test]
    fn persisted_blend_curve_uses_local_clock_and_checks_range() {
        let effect: crate::domain::composition::VideoEffect = serde_json::from_value(serde_json::json!({
            "kind":"find_edges", "parameters":{"blend":1,"invert":true}, "animation":{
                "mode":"keyframes", "track":{"timeBase":1000000,"interpolation":"linear",
                "keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}
        })).unwrap();
        let restored = serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();
        assert_eq!(effect, restored);
        let crate::domain::composition::VideoEffect::FindEdges {parameters, animation:Some(curve)} = effect else {panic!("FindEdges")};
        assert_eq!(sample_animation(parameters, &curve, 250000).unwrap().blend, 50.0);
        assert!(FindEdges {blend:101.0,..Default::default()}.validate().is_err());
    }

    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: FindEdges, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-find-edges.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),32);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original FindEdges frame {index}");
        }
    }

 use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn uniform_alpha_inversion_blend_identity_and_cancellation(){
  let source=GroupSurface{width:1,height:3,rgba:[64,128,192,128].repeat(3)};
  let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  FindEdges::default().apply_linear(&mut frame,&mut||Ok(())).unwrap();let a=128.0/255.0;assert!(frame.pixels().iter().all(|p|*p==[a,a,a,a]));
  FindEdges{invert:true,blend:0.0}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert!(frame.pixels().iter().all(|p|p[..3].iter().all(|v|v.abs()<1e-6)&&p[3]==a));
  let mut frame=original.clone();FindEdges{invert:true,blend:100.0}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  assert!(FindEdges::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(FindEdges{blend:101.0,..Default::default()}.validate().is_err());
 }
}
