//! End-to-end native composition rendering against original caption overlay pixels.
use super::{composition::{Composition,CanvasSpec,CompositionClipId,TrackId},caption_timeline::{CaptionCue,CaptionTrack},caption_graphic::CaptionStyle,caption_render::Overlay,flat_surface::render_generated,linear_color_frame::LinearColorFrame,group_surface::GroupSurface};
#[test]
fn original_caption_pixels_survive_composition_render_timing_and_visibility() {
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-render.json")).unwrap();
 for(index,case)in fixture["cases"].as_array().unwrap().iter().enumerate(){
  let width=case["width"].as_u64().unwrap() as usize;let height=case["height"].as_u64().unwrap() as usize;
  let canvas:CanvasSpec=serde_json::from_value(serde_json::json!({"width":width,"height":height,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
  let mut document=Composition::new(canvas);
  document.caption_tracks.push(CaptionTrack{format:None,language:None,sync_lock:None,vtt_blocks:vec![], id:TrackId::parse("captions").unwrap(),name:"Captions".into(),hidden:false,locked:false,style:serde_json::from_value::<CaptionStyle>(case["style"].clone()).unwrap(),clips:vec![CaptionCue{cue_id:None, id:CompositionClipId::parse("cue").unwrap(),timeline_start_tick:(500000_u64).into(),timeline_end_tick:(1500000_u64).into(),text:case["text"].as_str().unwrap().into(),speaker:Some("Narrator".into()),settings:case["settings"].as_str().unwrap().into()}]});
  let background=GroupSurface{width,height,rgba:[0,0,0,255].repeat(width*height)};
  let mut expected=LinearColorFrame::from_surface(&background,&mut||Ok(())).unwrap();
  if !case["overlay"].is_null(){let source=&case["overlay"];let bits:Vec<u32>=serde_json::from_value(source["bits"].clone()).unwrap();let overlay=Overlay{x:source["x"].as_i64().unwrap()as i32,y:source["y"].as_i64().unwrap()as i32,w:source["w"].as_u64().unwrap()as usize,h:source["h"].as_u64().unwrap()as usize,px:bits.into_iter().map(f32::from_bits).collect()};
   let mut pixels=expected.pixels().iter().flatten().copied().collect::<Vec<_>>();overlay.composite_onto(&mut pixels,width,height);
   let mut position=0;expected.map_premultiplied(&mut|_|{let pixel=[pixels[position],pixels[position+1],pixels[position+2],pixels[position+3]];position+=4;pixel},&mut||Ok(())).unwrap();
  }
  let expected=expected.to_surface(&mut||Ok(())).unwrap();
  for time in [500000,1000000,1499999]{assert_eq!(render_generated(&document,time,&mut||Ok(())).unwrap().unwrap(),expected,"active source frame{index}/{time}");}
  for time in [0,499999,1500000,2000000]{assert_eq!(render_generated(&document,time,&mut||Ok(())).unwrap().unwrap(),background,"inactive source frame{index}/{time}");}
  document.caption_tracks[0].hidden=true;assert_eq!(render_generated(&document,1000000,&mut||Ok(())).unwrap().unwrap(),background,"hidden{index}");
 }
}

pub(crate) fn fractional_caption_project(nested:bool,offset_us:u64)->super::nested_composition::NestedCompositionProject {
 use super::{nested_composition::{NestedCompositionProject,NestedCompositionDocument,CompositionInstance},composition_graph::CompositionReferenceTiming,composition::{TransformSpec,AnimatableValue,BlendMode}};
 let canvas:CanvasSpec=serde_json::from_value(serde_json::json!({"width":320,"height":180,"fpsMilli":23976,"fpsRate":{"numerator":24000,"denominator":1001},"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
 let mut content=Composition::new(canvas);
 let rate=filmcraft_time::FrameRate::FPS_23_976;
 let shift=offset_us as i128*254016-if nested{100000i128*254016}else{0};
 let cue:CaptionCue=serde_json::from_value(serde_json::json!({"id":"exact-cue","timelineStartTick":(rate.tick_of(13).0 as i128+shift).to_string(),"timelineEndTick":(rate.tick_of(14).0 as i128+shift).to_string(),"text":"A"})).unwrap();
 content.caption_tracks.push(CaptionTrack{format:None,language:None,sync_lock:None,vtt_blocks:vec![],id:TrackId::parse("exact-captions").unwrap(),name:"Exact".into(),hidden:false,locked:false,style:CaptionStyle::default(),clips:vec![cue]});
 let child=NestedCompositionDocument{content,duration_us:Some(2000000),instances:vec![]};
 let mut compositions=std::collections::BTreeMap::new();
 if nested {
  compositions.insert("child".into(),child);
  compositions.insert("root".into(),NestedCompositionDocument{content:Composition::new(canvas),duration_us:Some(2000000),instances:vec![CompositionInstance{id:"instance".into(),timing:CompositionReferenceTiming{composition:"child".into(),timeline_start_us:200000,source_in_us:100000,source_out_us:1900000},animation_offset_us:0,transform:TransformSpec::default(),opacity:AnimatableValue::constant(1.),blend_mode:BlendMode::Normal}]});
 }else{compositions.insert("root".into(),child);}
 NestedCompositionProject{fonts:vec![],timeline_order: vec![],schema_version:1,root:"root".into(),luts:vec![],compositions}
}
#[test]
fn caption_exact_export_sample_renders_start_end_nested_and_selected_range() {
 use super::{caption_frame_clock::CaptionFrameSample,group_media::{render_project_frame_sources,render_project_frame_sources_with_sample}};
 for nested in [false,true] {for offset in [0,1234567] {
  let project=fractional_caption_project(nested,offset);let empty=std::collections::BTreeMap::new();let sources=std::collections::BTreeMap::new();let cancel=std::sync::atomic::AtomicBool::new(false);
  for index in [12,13,14] {
   let sample=CaptionFrameSample::new(index,filmcraft_time::FrameRate::FPS_23_976,offset).unwrap();
   let surface=render_project_frame_sources_with_sample(&project,"root",std::path::Path::new("ffmpeg"),&empty,&sources,sample.host_us,std::time::Duration::from_secs(5),&cancel,Some(sample)).unwrap();
   let visible=surface.rgba.chunks_exact(4).any(|p|p[0]>0||p[1]>0||p[2]>0);
   assert_eq!(visible,index==13,"nested{nested} offset{offset} frame{index}");
   if index==13 {let legacy=render_project_frame_sources(&project,"root",std::path::Path::new("ffmpeg"),&empty,&sources,sample.host_us,std::time::Duration::from_secs(5),&cancel).unwrap();assert!(!legacy.rgba.chunks_exact(4).any(|p|p[0]>0||p[1]>0||p[2]>0),"arbitrary-us preview must keep its actual instant");}
  }
 }}
}
