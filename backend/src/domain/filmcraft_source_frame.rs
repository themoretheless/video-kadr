//! Pure source-sequence frame orchestration. Providers must implement complete
//! source/effect/mask, adjustment and output semantics; unsupported content fails.
use super::{filmcraft_project_file::SourceProjectDocument,filmcraft_video_jobs::{SourceFrameJobs,SourceClipJob},filmcraft_video_plan::{VideoLayer,PlannedClip},filmcraft_video_compositor::{PreparedClip,PreparedLayer,apply_layer},filmcraft_source_pixels,filmcraft_blend::Blend,transition_kernel_pack::{context::RenderContext,image::Image,project::EffectInstance}};
use anyhow::{ensure,Context,Result};
use serde_json::Value;
#[derive(Clone,Copy,Debug)]
pub struct FrameOptions{pub scale:f32,pub effects:bool,pub depth:u32,pub captions:bool,pub working_output:bool}
impl Default for FrameOptions{fn default()->Self{Self{scale:1.,effects:true,depth:0,captions:false,working_output:false}}}
pub struct FrameRequest<'a>{pub document:&'a SourceProjectDocument,pub sequence:&'a Value,pub sequence_id:u64,pub at:i64,pub canvas:(usize,usize),pub options:FrameOptions,pub temporal_prefix:bool,pub temporal_timecode_at:Option<i64>}
pub enum LayerPixels{
 /// Source-working-space pixels after effects/opacity masks, before Motion/global opacity.
 Source(Image),
 /// Full output canvas (e.g. directly rendered vectors), before global opacity.
 Positioned(Image),
 /// A source genuinely has no frame; unsupported content must return an error.
 Missing,
}
pub trait SourceLayerProvider:Send{
 fn analysis_cache(&self)->Option<super::filmcraft_analysis_cache::SharedAnalysis>{None}
 /// External media supplies decoded pixels; the domain never opens a source path.
 fn file(&mut self,_:&SourceClipJob<'_>,_:&FrameRequest<'_>,_:f32,_:&RenderContext)->Result<LayerPixels>{anyhow::bail!("file source provider is not connected")}
 fn layer(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,requested_scale:f32,context:&RenderContext)->Result<LayerPixels>;
 fn transition(&mut self,raw:&Value,request:&FrameRequest<'_>)->Result<EffectInstance>;
 /// Complete updated canvas, including adjustment masks, region, opacity and blend.
 fn adjustment(&mut self,job:&SourceClipJob<'_>,canvas:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>;
 /// Display conversion followed by caption overlays, respecting depth/output options.
 fn finish(&mut self,canvas:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>;
}
fn prepared(provider:&mut dyn SourceLayerProvider,jobs:&SourceFrameJobs<'_>,clip:&PlannedClip,request:&FrameRequest<'_>,context:&RenderContext)->Result<Option<PreparedClip>>{
 let job=jobs.clips.get(&clip.clip).context("missing source frame job")?;
 let appearance=job.appearance.as_ref();
 let wanted=appearance.map(|a|{let [a,b,c,d,_,_]=a.matrix;((a*a+b*b).sqrt().max((c*c+d*d).sqrt())*request.options.scale as f64).clamp(1./64.,1.)as f32}).unwrap_or(1.);
 let pixels=provider.layer(job,request,wanted,context)?;
 let image=match pixels{
  LayerPixels::Missing=>return Ok(None),
  LayerPixels::Source(image)=>{let appearance=appearance.context("source frame lacks Motion geometry")?;filmcraft_source_pixels::place(context,&image,request.canvas,job.source_size.context("source frame lacks dimensions")?.0,appearance.matrix,request.options.scale)?},
  LayerPixels::Positioned(image)=>{ensure!((image.w,image.h)==request.canvas,"positioned source canvas mismatch");context.image_work(&[],move||Ok(image)).map_err(anyhow::Error::msg)?},
 };
 let appearance=appearance.context("source frame lacks intrinsic appearance")?;
 Ok(Some(PreparedClip{image,opacity:appearance.opacity,blend:Blend::from_index(appearance.blend)}))
}
pub fn render(context:&RenderContext,document:&SourceProjectDocument,sequence:u64,at:i64,options:FrameOptions,provider:&mut dyn SourceLayerProvider)->Result<Image>{render_track(context,document,sequence,at,options,provider,None)}
pub(crate) fn render_track(context:&RenderContext,document:&SourceProjectDocument,sequence:u64,at:i64,options:FrameOptions,provider:&mut dyn SourceLayerProvider,only:Option<usize>)->Result<Image>{
 ensure!(options.scale.is_finite()&&options.scale>0.&&options.scale<=64.,"invalid source frame scale");
 let jobs=SourceFrameJobs::build_track(document,sequence,at,only)?;
 let dimension=|key:&str|->Result<usize>{let n=jobs.sequence["settings"][key].as_u64().context("missing sequence dimensions")?;ensure!(n>0&&n<=u32::MAX as u64,"invalid sequence dimensions");Ok(((n as f32*options.scale).round()as usize).max(1))};
 let canvas=(dimension("width")?,dimension("height")?);
 ensure!(canvas.0<=3840&&canvas.1<=2160&&context.canvas_allowed(canvas.0,canvas.1),"source frame canvas budget exceeded");
 let request=FrameRequest{document,sequence:jobs.sequence,sequence_id:sequence,at,canvas,options,temporal_prefix:false,temporal_timecode_at:None};
 context.image_work(&[],||{
  let mut run=||->Result<Image>{
   let mut output=Image::new(canvas.0,canvas.1);
   if options.depth>8{return Ok(output)}
   for layer in &jobs.layers{
    ensure!(!context.is_cancelled(),"source frame cancelled");
    match layer{
     VideoLayer::Clip{clip,..}=>{
      let job=jobs.clips.get(&clip.clip).context("missing source frame job")?;
      if job.raw_source.pointer("/kind/AdjustmentLayer").is_some(){
       if options.effects{output=provider.adjustment(job,output,&request,context)?;ensure!((output.w,output.h)==canvas,"adjustment canvas mismatch");output=context.image_work(&[],move||Ok(output)).map_err(anyhow::Error::msg)?;}
      }else if let Some(clip)=prepared(provider,&jobs,clip,&request,context)?{apply_layer(context,&mut output,&PreparedLayer::Clip(clip),options.scale)?;}
     },
     VideoLayer::Transition{from,to,progress,reverse,effect,..}=>{
      let effect=provider.transition(effect,&request)?;
      let from=from.as_ref().map(|clip|prepared(provider,&jobs,clip,&request,context)).transpose()?.flatten();
      let to=to.as_ref().map(|clip|prepared(provider,&jobs,clip,&request,context)).transpose()?.flatten();
      apply_layer(context,&mut output,&PreparedLayer::Transition{effect,from,to,progress:*progress as f32,reverse:*reverse},options.scale)?;
     },
    }
   }
   let output=provider.finish(output,&request,context)?;ensure!((output.w,output.h)==canvas,"finished source canvas mismatch");Ok(output)
  };
  run().map_err(|e|e.to_string())
 }).map_err(anyhow::Error::msg)
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::json;use std::sync::{Arc,atomic::AtomicBool};
 fn document(adjustment:bool)->SourceProjectDocument{
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-appearance.json")).unwrap();let mut doc=fixture["cases"][0]["document"].clone();
  doc["project"]["items"]["1"]["kind"]["Media"]["info"]["video"]["width"]=json!(8);
  doc["project"]["items"]["1"]["kind"]["Media"]["info"]["video"]["height"]=json!(4);
  let seq=&mut doc["project"]["items"]["6"]["kind"]["Sequence"];seq["settings"]["width"]=json!(8);seq["settings"]["height"]=json!(4);
  let mut track=seq["video_tracks"][0].clone();let mut clip=track["items"][0].clone();clip["effects"]=json!([]);track["items"]=json!([clip]);track["transitions"]=json!([]);track["enabled"]=json!(true);
  let mut tracks=vec![track.clone()];if adjustment{track["id"]=json!(900);track["items"][0]["id"]=json!(901);track["items"][0]["item"]=json!(99);tracks.push(track);}
  seq["video_tracks"]=json!(tracks);
  if adjustment{let mut item=doc["project"]["items"]["1"].clone();item["id"]=json!(99);item["kind"]=json!({"AdjustmentLayer":{"width":8,"height":4,"rate":{"num":24,"den":1},"duration":508032000000i64}});doc["project"]["items"]["99"]=item;}
  SourceProjectDocument::read(&serde_json::to_vec(&doc).unwrap()).unwrap()
 }
 #[derive(Default)]struct Provider{events:Vec<&'static str>,scales:Vec<f32>,refuse:bool}
 impl SourceLayerProvider for Provider{
  fn layer(&mut self,_:&SourceClipJob<'_>,_:&FrameRequest<'_>,scale:f32,_:&RenderContext)->Result<LayerPixels>{self.events.push("source");self.scales.push(scale);ensure!(!self.refuse,"unsupported source effect");let mut image=Image::new(8,4);for p in image.px.chunks_mut(4){p.copy_from_slice(&[1.,0.,0.,1.]);}Ok(LayerPixels::Source(image))}
  fn transition(&mut self,_:&Value,_:&FrameRequest<'_>)->Result<EffectInstance>{anyhow::bail!("unexpected transition")}
  fn adjustment(&mut self,_:&SourceClipJob<'_>,mut canvas:Image,_:&FrameRequest<'_>,_:&RenderContext)->Result<Image>{self.events.push("adjustment");assert!(canvas.px.chunks(4).all(|p|p==[1.,0.,0.,1.]));for p in canvas.px.chunks_mut(4){p[1]=0.25;}Ok(canvas)}
  fn finish(&mut self,canvas:Image,request:&FrameRequest<'_>,_:&RenderContext)->Result<Image>{self.events.push("finish");assert_eq!((canvas.w,canvas.h),request.canvas);Ok(canvas)}
 }
 fn context()->RenderContext{RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,32,4096).unwrap()}
 #[test]fn source_placement_adjustment_and_finish_follow_track_order(){
  let context=context();let mut provider=Provider::default();let output=render(&context,&document(true),6,0,FrameOptions::default(),&mut provider).unwrap();assert_eq!(provider.events,["source","adjustment","finish"]);assert_eq!(provider.scales,[1.]);assert!(output.px.chunks(4).all(|p|p==[1.,0.25,0.,1.]));drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
 }
 #[test]fn effects_toggle_depth_guard_and_scaled_source_request(){
  let context=context();let mut provider=Provider::default();let output=render(&context,&document(true),6,0,FrameOptions{effects:false,scale:0.5,..FrameOptions::default()},&mut provider).unwrap();assert_eq!(provider.events,["source","finish"]);assert_eq!(provider.scales,[0.5]);assert_eq!((output.w,output.h),(4,2));drop(output);
  provider.events.clear();let output=render(&context,&document(true),6,0,FrameOptions{depth:9,..FrameOptions::default()},&mut provider).unwrap();assert!(provider.events.is_empty());assert!(output.px.iter().all(|v|*v==0.));drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
 }
 #[test]fn provider_refusal_discards_frame_and_prevents_finish(){let context=context();let mut provider=Provider{refuse:true,..Provider::default()};assert!(render(&context,&document(false),6,0,FrameOptions::default(),&mut provider).unwrap_err().to_string().contains("unsupported source effect"));assert_eq!(provider.events,["source"]);assert_eq!(context.pixel_bytes_in_use(),0);}
}

#[cfg(test)]mod original_frame_tests{
 use super::*;use super::super::group_surface::GroupSurface;use std::sync::{Arc,atomic::AtomicBool};
 struct PatternProvider;
 impl SourceLayerProvider for PatternProvider{
  fn layer(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{
   ensure!(request.options.working_output&&!request.options.captions,"fixture provider only supports working output without captions");
   for effect in job.raw_clip["effects"].as_array().context("missing fixture effects")?{
    ensure!(matches!(effect["effect"].as_str(),Some("motion"|"opacity")),"unsupported fixture effect");
    ensure!(effect.get("masks").and_then(Value::as_array).is_none_or(|m|m.is_empty()),"unsupported fixture masks");
   }
   let q=job.frame_ticks.max(0)as u64/(254016000000u64/10);let side=job.item%2;
   let rgba=(0..16u64).flat_map(|i|{let(x,y)=(i%4,i/4);[
    ((11+x*37+y*19+q*13+side*71)%256)as u8,
    ((29+x*17+y*43+q*23+side*31)%256)as u8,
    ((47+x*53+y*7+q*41+side*89)%256)as u8,
    [64,128,192,255][((x+y+q+side)%4)as usize],
   ]}).collect();
   let mut n=1;while n<16&&4./(n as f32*2.)>=(4.*scale).max(1.){n*=2;}
   Ok(LayerPixels::Source(filmcraft_source_pixels::decode_srgb(context,&GroupSurface{width:4,height:4,rgba},n)?))
  }
  fn transition(&mut self,raw:&Value,_:&FrameRequest<'_>)->Result<EffectInstance>{Ok(serde_json::from_value(raw.clone())?)}
  fn adjustment(&mut self,_:&SourceClipJob<'_>,_:Image,_:&FrameRequest<'_>,_:&RenderContext)->Result<Image>{anyhow::bail!("fixture provider has no adjustment implementation")}
  fn finish(&mut self,canvas:Image,request:&FrameRequest<'_>,_:&RenderContext)->Result<Image>{ensure!(request.options.working_output&&!request.options.captions,"fixture output mode unsupported");Ok(canvas)}
 }
 #[test]fn original_complete_intrinsic_source_frames_match_every_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-source-frame.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),768);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,32,64*1024).unwrap();let mut provider=PatternProvider;
  for (index,case) in cases.iter().enumerate(){
   let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let options=FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..FrameOptions::default()};
   let output=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),options,&mut provider).unwrap();assert_eq!(serde_json::json!([output.w,output.h]),serde_json::json!([case["width"],case["height"]]));
   let bits:Vec<_>=output.px.iter().map(|v|v.to_bits()).collect();assert_eq!(serde_json::json!(bits),case["bits"],"frame {index}, mode {}, scale {}, at {}",case["mode"],case["scale"],case["at"]);drop(output);assert_eq!(context.pixel_bytes_in_use(),0,"frame {index} leaked storage");
  }
 }
}
