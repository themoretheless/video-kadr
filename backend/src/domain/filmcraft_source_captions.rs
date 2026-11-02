//! Source caption track/style bridge to the qualified caption rasterizer.
use super::{filmcraft_project_index::ProjectIndex,filmcraft_sequence_source::SourceSequence,filmcraft_source_frame::FrameRequest,caption_graphic::CaptionStyle,caption_timeline::CaptionCue,caption_time::CaptionTime,composition::CompositionClipId,caption_render,transition_kernel_pack::{context::RenderContext,image::Image}};
use anyhow::{ensure,Context,Result};use serde_json::Value;
fn style(raw:Option<&Value>)->Result<CaptionStyle>{
 let Some(raw)=raw else{return Ok(CaptionStyle::default())};let mut raw=raw.clone();let object=raw.as_object_mut().context("invalid source caption style")?;
 for(from,to)in [("background_color","backgroundColor"),("outline_color","outlineColor"),("line_spacing","lineSpacing")]{if let Some(value)=object.remove(from){ensure!(!object.contains_key(to),"ambiguous source caption style");object.insert(to.into(),value);}}
 for key in ["align","anchor"]{if let Some(value)=object.get_mut(key){*value=Value::String(value.as_str().context("invalid source caption style enum")?.to_lowercase());}}
 let style:CaptionStyle=serde_json::from_value(raw)?;style.validate()?;Ok(style)
}
/// Caller owns unpublished canvas inside RenderContext::image_work.
pub(crate) fn apply(context:&RenderContext,request:&FrameRequest<'_>,canvas:&mut Image)->Result<()>{
 let index=ProjectIndex::new(request.document)?;let sequence=SourceSequence::read(&index,request.sequence_id)?;ensure!(sequence.caption_tracks.len()<=256,"caption track budget exceeded");
 for track in sequence.caption_tracks.iter().rev(){
  ensure!(!context.is_cancelled(),"caption rendering cancelled");if !track.raw["enabled"].as_bool().context("missing caption enabled state")?{continue}
  let Some(caption)=track.caption_at(request.at)else{continue};ensure!(caption.text.len()<=1_000_000,"caption text budget exceeded");
  let style=style(track.raw.get("style"))?;
  let cue=CaptionCue{id:CompositionClipId::parse(format!("source-caption-{}",caption.id))?,cue_id:None,timeline_start_tick:CaptionTime::SourceTicks(caption.start),timeline_end_tick:CaptionTime::SourceTicks(caption.end()?),text:caption.text.to_owned(),speaker:caption.raw["speaker"].as_str().map(str::to_owned),settings:caption.raw.get("settings").and_then(Value::as_str).unwrap_or("").to_owned()};
  let mut checkpoint=||{ensure!(!context.is_cancelled(),"caption rendering cancelled");Ok(())};
  if let Some(overlay)=caption_render::render_caption_with_admission(&cue,&style,canvas.w,canvas.h,|bytes|Ok(context.reserve_scratch(bytes)),&mut checkpoint)?{
   let bytes=overlay.px.capacity().checked_mul(4).context("caption output storage overflow")?;let storage=context.reserve_scratch(bytes);
   for y in 0..overlay.h{checkpoint()?;let cy=overlay.y as i64+y as i64;if cy<0||cy>=canvas.h as i64{continue}for x in 0..overlay.w{let cx=overlay.x as i64+x as i64;if cx<0||cx>=canvas.w as i64{continue}let p=&overlay.px[(y*overlay.w+x)*4..][..4];if p[3]<=0.{continue}let i=(cy as usize*canvas.w+cx as usize)*4;let k=1.-p[3];for c in 0..4{canvas.px[i+c]=p[c]+canvas.px[i+c]*k;}}}
   drop(overlay);drop(storage);
  }
 }
 Ok(())
}
#[cfg(test)]mod tests{
 use super::*;use super::super::{filmcraft_project_file::SourceProjectDocument,filmcraft_source_frame::{render,FrameOptions},filmcraft_generator_source_provider::GeneratorSourceProvider};use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
 #[test]fn original_caption_sequence_frames_match_every_bit(){let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-sequence.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),96);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,320*200,32*1024*1024).unwrap();let mut provider=GeneratorSourceProvider;for(index,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,captions:true,working_output:true,..FrameOptions::default()},&mut provider).unwrap();assert_eq!(serde_json::json!([output.w,output.h]),serde_json::json!([case["width"],case["height"]]));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"caption frame {index}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);}}
 #[test]fn raster_admission_and_mid_raster_cancellation_release_storage(){
  let cue=CaptionCue{id:CompositionClipId::parse("budget-cue").unwrap(),cue_id:None,timeline_start_tick:CaptionTime::ZERO,timeline_end_tick:CaptionTime::SourceTicks(254016000000),text:"Caption scratch".into(),speaker:None,settings:String::new()};let style=CaptionStyle::default();
  for cancel_at_admission in [false,true]{let flag=Arc::new(AtomicBool::new(false));let context=RenderContext::new_with_budget(flag.clone(),1,80*64,if cancel_at_admission{1024*1024}else{80*64*16+1}).unwrap();let outcome=context.image_work(&[],||{let image=Image::new(80,64);let result=caption_render::render_caption_with_admission(&cue,&style,80,64,|bytes|{let guard=context.reserve_scratch(bytes);if cancel_at_admission{flag.store(true,Ordering::Relaxed);}Ok(guard)},&mut||{ensure!(!context.is_cancelled(),"caption cancelled");Ok(())});result.map_err(|e|e.to_string())?;Ok(image)});assert!(outcome.is_err());assert_eq!(context.pixel_bytes_in_use(),0);}
 }
}
