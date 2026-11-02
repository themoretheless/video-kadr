//! Filmcraft active Gradient Wipe (Legacy), including external-layer sampling.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
use super::linear_color_frame::LinearColorFrame;
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct GradientWipe {pub completion:f32,pub softness:f32,pub layer:u32,pub placement:u32,pub invert:bool}
impl Default for GradientWipe{fn default()->Self{Self{completion:0.,softness:0.,layer:0,placement:2,invert:false}}}
/// The compositor supplies the original layer-to-output coordinate transform.
pub struct GradientLayer<'a>{pub frame:&'a LinearColorFrame,pub layer_to_output:&'a dyn Fn(f64,f64)->(f64,f64)}
impl GradientWipe{
 pub fn validate(&self)->Result<()>{ensure!(self.completion.is_finite()&&(0.0..=100.).contains(&self.completion)&&self.softness.is_finite()&&(0.0..=100.).contains(&self.softness)&&self.placement<3&&self.layer<=8,"Gradient Wipe parameter out of range");Ok(())}
 pub fn apply_linear(&self,frame:&mut LinearColorFrame,layer:Option<GradientLayer<'_>>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{self.validate()?;checkpoint()?;let c=self.completion/100.;if c<=0.{return Ok(());}let soft=self.softness/100.;let(w,h)=frame.dimensions();let own=frame.pixels().to_vec();let mut index=0;
 frame.map_premultiplied(&mut|p|{let(x,y)=(index%w,index/w);let i=index;index+=1;let(px,py)=(x as f32+0.5,y as f32+0.5);let s=if self.layer>0{if let Some(layer)=&layer{let(tw,th)=layer.frame.dimensions();match self.placement{0=>layer.frame.pixels()[(y%th)*tw+x%tw],1=>transparent_sample(layer.frame,px-w as f32/2.+tw as f32/2.,py-h as f32/2.+th as f32/2.),_=>{let(qx,qy)=(layer.layer_to_output)(px as f64,py as f64);super::mosaic::sample(layer.frame.pixels(),tw,th,qx as f32,qy as f32)}}}else{own[i]}}else{own[i]};let l=(0.2126*s[0]+0.7152*s[1]+0.0722*s[2]).clamp(0.,1.);let mut g=super::basic_grade::display(l);if self.invert{g=1.-g;}let a=if soft<=1e-4{if c<=0.||g>=c{1.}else{0.}}else{let t=c*(1.+soft);((g-t+soft)/soft).clamp(0.,1.)};p.map(|v|v*a)},checkpoint)
 }
}
pub(crate) fn transparent_sample(frame:&LinearColorFrame,x:f32,y:f32)->[f32;4]{let(w,h)=frame.dimensions();let(fx,fy)=(x-0.5,y-0.5);let(x0,y0)=(fx.floor(),fy.floor());let(tx,ty)=(fx-x0,fy-y0);let(x0,y0)=(x0 as isize,y0 as isize);let get=|x:isize,y:isize|if x<0||y<0||x>=w as isize||y>=h as isize{[0.;4]}else{frame.pixels()[y as usize*w+x as usize]};let(a,b,c,d)=(get(x0,y0),get(x0+1,y0),get(x0,y0+1),get(x0+1,y0+1));std::array::from_fn(|k|{let top=a[k]+(b[k]-a[k])*tx;let bot=c[k]+(d[k]-c[k])*tx;top+(bot-top)*ty})}
#[cfg(test)]mod tests{use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn external_tile_center_transform_and_self_fallback(){let make=|w,h,rgba|LinearColorFrame::from_surface(&GroupSurface{width:w,height:h,rgba},&mut||Ok(())).unwrap();let own=make(4,2,vec![255;32]);let layer=make(2,1,vec![0,0,0,255,255,255,255,255]);let mapping=|x:f64,y:f64|(x,y);for placement in 0..3{let mut f=own.clone();GradientWipe{completion:50.,layer:1,placement,..Default::default()}.apply_linear(&mut f,Some(GradientLayer{frame:&layer,layer_to_output:&mapping}),&mut||Ok(())).unwrap();assert!(f.pixels().iter().any(|p|p[3]==0.));}let mut f=own.clone();GradientWipe{completion:50.,layer:1,..Default::default()}.apply_linear(&mut f,None,&mut||Ok(())).unwrap();assert_eq!(f.pixels(),own.pixels());}
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum GradientWipeChannel {Completion,Softness}
pub type GradientWipeAnimation=std::collections::BTreeMap<GradientWipeChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&GradientWipeAnimation)->Result<()>{for v in a.values(){v.validate()?;ensure!(super::composition::animatable_values_in(v,0.,100.,true),"Gradient Wipe animation out of range");}Ok(())}
pub fn sample_animation(mut p:GradientWipe,a:&GradientWipeAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<GradientWipe>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let value=super::nested_composition::sample_value(v,t)? as f32;match c{GradientWipeChannel::Completion=>p.completion=value,GradientWipeChannel::Softness=>p.softness=value}}p.validate()?;Ok(p)}
#[cfg(test)]mod animation_tests {use super::*;
#[test]fn curves_preserve_layer_placement_and_invert(){
 let a:GradientWipeAnimation=serde_json::from_value(serde_json::json!({"completion":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":500000,"value":100.}]}},"softness":{"mode":"constant","value":25.}})).unwrap();
 assert_eq!(a,serde_json::from_value(serde_json::to_value(&a).unwrap()).unwrap());
 let p=sample_animation(GradientWipe{layer:2,placement:1,invert:true,..Default::default()},&a,250000).unwrap();
 assert_eq!(p,GradientWipe{completion:50.,softness:25.,layer:2,placement:1,invert:true});
 let invalid:GradientWipeAnimation=serde_json::from_value(serde_json::json!({"softness":{"mode":"constant","value":100.1}})).unwrap();assert!(validate_animation(&invalid).is_err());
}
#[test]fn stretch_uses_layer_to_output_and_cancellation(){
 use crate::domain::group_surface::GroupSurface;
 let make=|w,h,rgba|LinearColorFrame::from_surface(&GroupSurface{width:w,height:h,rgba},&mut||Ok(())).unwrap();
 let source=make(2,1,vec![255;8]);let layer=make(2,1,vec![0,0,0,255,255,255,255,255]);
 let translated=|x:f64,y:f64|(x+1.,y);let mut frame=source.clone();
 let p=GradientWipe{completion:50.,layer:1,..Default::default()};
 p.apply_linear(&mut frame,Some(GradientLayer{frame:&layer,layer_to_output:&translated}),&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),source.pixels());
 let identity=|x:f64,y:f64|(x,y);p.apply_linear(&mut frame,Some(GradientLayer{frame:&layer,layer_to_output:&identity}),&mut||Ok(())).unwrap();assert_eq!(frame.pixels()[0],[0.;4]);assert_eq!(frame.pixels()[1],source.pixels()[1]);
 assert!(p.apply_linear(&mut frame,None,&mut||anyhow::bail!("cancelled")).is_err());
}
}
#[cfg(test)]mod stack_tests {use super::*;use crate::domain::{composition::VideoEffect,flat_surface::{EffectTrackContext,apply_effect_stack_with_context},group_surface::GroupSurface};
#[test]fn persisted_external_track_is_sampled_in_order(){
 struct Context {reads:Vec<usize>}
 impl EffectTrackContext for Context {
 fn track(&mut self,index:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<LinearColorFrame>>{self.reads.push(index);Ok(Some(LinearColorFrame::from_surface(&GroupSurface{width:2,height:1,rgba:vec![0,0,0,255,255,255,255,255]},checkpoint)?))}
 fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 let e:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"gradient_wipe","parameters":{"layer":2},"animation":{"completion":{"mode":"constant","value":50.}}})).unwrap();
 assert_eq!(e,serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap());
 let mut child=GroupSurface{width:2,height:1,rgba:vec![255;8]};let mut ctx=Context{reads:vec![]};
 apply_effect_stack_with_context(&mut child,&[e.clone()],0,&[],Some(&mut ctx),&mut||Ok(())).unwrap();assert_eq!(ctx.reads,vec![1]);assert_eq!(child.rgba[3],0);assert_eq!(&child.rgba[4..],&[255,255,255,255]);
 assert!(crate::domain::flat_surface::apply_effect_stack(&mut child,&[e],0,&[],&mut||Ok(())).is_err());
}
}
#[cfg(test)]mod composition_tests {use super::*;use crate::domain::{composition::{Composition,CanvasSpec,TransformSpec,SourceId},group_surface::GroupSurface,flat_surface::render_generated_images};
#[test]fn hidden_external_track_and_self_cycle_render_with_depth_guard(){
 let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
 let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
 let clip=serde_json::json!({"id":"clip","sourceId":"image","timelineStartTick":0,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"gradient_wipe","parameters":{"completion":50.,"layer":2}}]});
 let mut control=clip.clone();control["id"]=serde_json::json!("control");control["effects"]=serde_json::json!([]);
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"main","name":"Main","clips":[clip]},{"kind":"image","id":"control","name":"Control","hidden":true,"clips":[control]}])).unwrap();
 let mut reads=0;let mut decoder=|_:&SourceId|{reads+=1;Ok(GroupSurface{width:2,height:2,rgba:vec![0,0,0,255,255,255,255,255,0,0,0,255,255,255,255,255]})};
 let out=render_generated_images(&composition,0,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();assert_eq!(reads,2);assert_eq!(out.rgba[4..8],[255;4]);
 let mut json=serde_json::to_value(&composition).unwrap();json["tracks"][0]["clips"][0]["effects"][0]["parameters"]["layer"]=serde_json::json!(1);composition=serde_json::from_value(json).unwrap();
 let mut reads=0;let mut decoder=|_:&SourceId|{reads+=1;Ok(GroupSurface{width:2,height:2,rgba:vec![255;16]})};
 render_generated_images(&composition,0,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();assert_eq!(reads,9);
}
}
#[cfg(test)]mod original_tests {use super::*;use crate::domain::group_surface::GroupSurface;
#[test]fn original_external_self_and_missing_layer_rgba_bits(){
 #[derive(Deserialize)]#[serde(rename_all="camelCase")]struct Case {parameters:GradientWipe,width:usize,height:usize,rgba:Vec<u8>,track_rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gradient-wipe.json")).unwrap();assert_eq!(fixture.cases.len(),864);
 for(i,c)in fixture.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();let track=LinearColorFrame::from_surface(&GroupSurface{width:5,height:3,rgba:c.track_rgba},&mut||Ok(())).unwrap();let mapping=|x:f64,y:f64|(x+1.25,y-0.75);let layer=if c.parameters.layer==1{Some(GradientLayer{frame:&track,layer_to_output:&mapping})}else{None};c.parameters.apply_linear(&mut frame,layer,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original Gradient Wipe case {i}");}
}
}

#[cfg(test)]mod choice_tests {use super::*;
#[test]fn original_layer_choices_and_placement_bounds(){for layer in 0..=8{assert!(GradientWipe{layer,..Default::default()}.validate().is_ok());}assert!(GradientWipe{layer:9,..Default::default()}.validate().is_err());assert!(GradientWipe{placement:3,..Default::default()}.validate().is_err());}
}
