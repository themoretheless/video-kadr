//! Filmcraft Twirl CPU reference at source-resolution pixel scale.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};
use serde::{Deserialize,Serialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct Twirl {pub angle:f32,pub radius:f32,pub center:Option<[Option<f64>;2]>}
impl Default for Twirl {fn default()->Self {Self{angle:50.0,radius:30.0,center:None}}}
pub(crate) fn sample(pixels:&[[f32;4]],w:usize,h:usize,x:f32,y:f32)->[f32;4] {
 let fx=x-0.5;let fy=y-0.5;
 // Outside this interval every transparent-border tap is zero.
 if fx< -1.0||fy< -1.0||fx>=w as f32||fy>=h as f32{return [0.0;4];}
 let x0=fx.floor();let y0=fy.floor();let tx=fx-x0;let ty=fy-y0;let(x0,y0)=(x0 as isize,y0 as isize);
 let get=|x:isize,y:isize|if x<0||y<0||x>=w as isize||y>=h as isize{[0.0;4]}else{pixels[y as usize*w+x as usize]};
 let(a,b,c,d)=(get(x0,y0),get(x0+1,y0),get(x0,y0+1),get(x0+1,y0+1));
 std::array::from_fn(|k|{let top=a[k]+(b[k]-a[k])*tx;let bottom=c[k]+(d[k]-c[k])*tx;top+(bottom-top)*ty})
}
impl Twirl {
 pub fn validate(&self)->Result<()> {ensure!(self.angle.is_finite(),"Twirl angle out of range");ensure!(self.radius.is_finite()&&(0.0..=100.0).contains(&self.radius),"Twirl radius out of range");ensure!(self.center.map_or(true,|p|p.iter().flatten().all(|v|v.is_finite())),"Twirl center out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
  self.validate()?;checkpoint()?;let(w,h)=frame.dimensions();let src=frame.pixels().to_vec();let authored=self.center.unwrap_or([None,None]);let center=[authored[0].unwrap_or(w as f64/2.0),authored[1].unwrap_or(h as f64/2.0)];let angle=(self.angle as f64).to_radians();let radius=self.radius as f64/100.0*w.min(h)as f64;let mut index=0;
  frame.map_premultiplied(&mut|_|{
   let x=(index%w)as f64+0.5;let y=(index/w)as f64+0.5;index+=1;
   let(dx,dy)=(x-center[0],y-center[1]);let distance=(dx*dx+dy*dy).sqrt();
   let(u,v)=if distance>=radius{(x,y)}else{let k=1.0-distance/radius;let a=angle*k*k;let(s,c)=a.sin_cos();(center[0]+dx*c-dy*s,center[1]+dx*s+dy*c)};
   sample(&src,w,h,u as f32,v as f32)
  },checkpoint)
 }
}
#[cfg(test)]mod tests {
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: Twirl, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-twirl.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),216);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original Twirl frame {index}");
        }
    }

 use super::*;
 #[test]fn persisted_independent_curves_and_automatic_other_center_axis(){
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"twirl","parameters":{},"animation":{"angle":curve(-45.0,45.0),"radius":curve(0.0,100.0),"centerX":curve(2.0,3.0)}})).unwrap();
  let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
  let crate::domain::composition::VideoEffect::Twirl{parameters,animation:Some(curves)}=effect else {panic!("Twirl")};
  assert_eq!(sample_animation(parameters,&curves,250000,(10,8)).unwrap(),Twirl{angle:0.0,radius:50.0,center:Some([Some(2.5),None])});
  let precise=1.23456789012345;
  let single:TwirlAnimation=serde_json::from_value(serde_json::json!({"centerX":{"mode":"constant","value":precise}})).unwrap();
  assert_eq!(sample_animation(parameters,&single,0,(10,8)).unwrap().center.unwrap()[0],Some(precise));
  let bad:TwirlAnimation=serde_json::from_value(serde_json::json!({"radius":curve(0.0,100.1)})).unwrap();assert!(validate_animation(&bad).is_err());
 }
use crate::domain::{linear_color_frame::LinearColorFrame,group_surface::GroupSurface};
 #[test]fn zero_radius_identity_transparent_sampling_and_cancellation(){
  let source=GroupSurface{width:3,height:2,rgba:(0..6).flat_map(|i|[i*40,255-i*30,i*20,128]).collect()};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();
  Twirl{radius:0.0,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
  assert_eq!(sample(&[[1.0;4]],1,1,0.0,0.5),[0.5;4]);assert_eq!(sample(&[[1.0;4]],1,1,-1.0,0.5),[0.0;4]);
  assert!(Twirl::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(Twirl{radius:101.0,..Default::default()}.validate().is_err());
 }
}
#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum TwirlChannel {Angle,Radius,CenterX,CenterY}
pub type TwirlAnimation=std::collections::BTreeMap<TwirlChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&TwirlAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {TwirlChannel::Angle=>(-(f32::MAX as f64),f32::MAX as f64),TwirlChannel::Radius=>(0.0,100.0),_=>(-f64::MAX,f64::MAX)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Twirl animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:Twirl,animation:&TwirlAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>,_dimensions:(usize,usize))->Result<Twirl> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)?;match channel {TwirlChannel::Angle=>parameters.angle=value as f32,TwirlChannel::Radius=>parameters.radius=value as f32,TwirlChannel::CenterX|TwirlChannel::CenterY=>{let center=parameters.center.get_or_insert([None,None]);center[if *channel==TwirlChannel::CenterX {0}else{1}]=Some(value);}}}
 parameters.validate()?;Ok(parameters)
}
