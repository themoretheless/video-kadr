//! Generator-backed source frames for the shared sequence renderer. Other source
//! kinds and unconnected colour/effect/mask/output stages fail explicitly.
use super::{filmcraft_source_frame::{SourceLayerProvider,LayerPixels,FrameRequest},filmcraft_video_jobs::SourceClipJob,filmcraft_project_index::ProjectIndex,filmcraft_generator_audio::Generator,filmcraft_generator_video,transition_kernel_pack::{context::RenderContext,image::Image,project::EffectInstance}};
use anyhow::{ensure,Context,Result};use serde_json::Value;
#[derive(Default)]pub struct GeneratorSourceProvider;
fn plain(request:&FrameRequest<'_>)->Result<()>{let color=&request.sequence["settings"]["color"];ensure!(color["working"]=="Rec709"&&color["wide_gamut"]==false,"generator provider requires plain Rec709 working space");Ok(())}
fn apply_effects(provider:&mut dyn SourceLayerProvider,image:Image,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{
 let rate=&request.sequence["settings"]["frame_rate"];
 let clock=super::filmcraft_source_effects::SourceClock{
  local_ticks:request.at.checked_sub(job.raw_clip["start"].as_i64().context("invalid source clip start")?).context("source local clock overflow")?,
  duration_ticks:job.raw_clip["duration"].as_i64().context("invalid source clip duration")?,
  frame_rate:rate["num"].as_f64().context("invalid sequence frame numerator")?/rate["den"].as_f64().context("invalid sequence frame denominator")?,
 };
 let width=job.source_size.context("source effect dimensions missing")?.0;
 let image=if request.options.effects{
  let mut tracks=std::collections::BTreeMap::new();
  let mut stabilization=std::collections::BTreeMap::new();
  let mut analysis_reservations=Vec::new();
  let mut subjects=std::collections::BTreeMap::new();
  let mut echoes=std::collections::BTreeMap::new();
  let mut temporal=std::collections::BTreeMap::new();
  let px_scale=image.w as f32/width as f32;
  for raw in job.raw_clip["effects"].as_array().context("missing source effects")?.iter().filter(|e|!request.temporal_prefix&&e["enabled"]!=false&&matches!(e["effect"].as_str(),Some("gradient_wipe_legacy"|"track_matte"|"compound_blur"))){
   let layer=match super::filmcraft_source_effects::compile(raw,job.parameter_ticks,[0.,0.])?{
    super::composition::VideoEffect::GradientWipe{parameters,..}=>{parameters.validate()?;if parameters.completion<=0.{continue;}parameters.layer},
    super::composition::VideoEffect::CompoundBlur{parameters,..}=>{parameters.validate()?;parameters.layer},
    super::composition::VideoEffect::TrackMatte{parameters}=>{parameters.validate()?;parameters.matte},
    _=>unreachable!(),
   };
   if layer==0||tracks.contains_key(&layer)||layer as usize>request.sequence["video_tracks"].as_array().context("missing source tracks")?.len(){continue;}
   let options=super::filmcraft_source_frame::FrameOptions{depth:request.options.depth.checked_add(1).context("gradient track depth overflow")?,captions:false,working_output:true,..request.options};
   tracks.insert(layer,super::filmcraft_source_frame::render_track(context,request.document,request.sequence_id,request.at,options,provider,Some(layer as usize-1))?);
  }
  if !request.temporal_prefix{
   for (index,raw) in job.raw_clip["effects"].as_array().context("missing temporal effects")?.iter().enumerate().filter(|(_,e)|e["enabled"]!=false&&matches!(e["effect"].as_str(),Some("posterize_time"|"echo"|"auto_reframe"|"warp_stabilizer"))){
    match super::filmcraft_source_effects::compile(raw,job.parameter_ticks,[0.,0.])?{
     super::composition::VideoEffect::PosterizeTime{parameters,..}=>{
      let seconds=clock.local_ticks as f64/254016000000f64;
      if let Some(dt)=parameters.frame_offset(seconds)?{if let Some(frame)=temporal_prefix_frame(provider,job,request,context,index,dt,(image.w,image.h),None)?{temporal.insert(index,frame);}}
     },
     super::composition::VideoEffect::Echo{parameters,..}=>if let Some(requests)=parameters.requests()?{
      let mut frames=Vec::with_capacity(requests.len());
      for (dt,weight) in requests{if let Some(frame)=temporal_prefix_frame(provider,job,request,context,index,dt,(image.w,image.h),None)?{frames.push((frame,weight));}}
      echoes.insert(index,frames);
     },
     super::composition::VideoEffect::WarpStabilizer{parameters,..}=>{
      ensure!(parameters.is_valid(),"invalid source stabilization parameters");
      let seconds=clock.local_ticks as f64/254016000000.;let duration=clock.duration_ticks as f64/254016000000.;let fps=clock.frame_rate.max(1.);
      let count=((duration*fps).round()as usize).clamp(1,20000);
      analysis_reservations.push(context.reserve_scratch(count.checked_mul(512).context("stabilizer path scratch overflow")?));
      let target=if parameters.detailed{960.}else{480.};let scale=(target/width.max(1)as f32).clamp(1./64.,1.);
      let (sw,sh)=job.source_size.context("stabilizer source size missing")?;
      let pixels=((sw as f32*scale).ceil()as usize).checked_mul((sh as f32*scale).ceil()as usize).context("stabilizer analysis dimensions overflow")?;
      let _scratch=context.reserve_scratch(pixels.checked_mul(64).and_then(|n|n.checked_add(128*1024)).context("stabilizer analysis scratch overflow")?);
      let analysis_cache=provider.analysis_cache();
      let analysis_reads=analysis_cache.clone();
      let mut source=|dt:f64,_:f32|->Result<Option<super::motion_tracker::Gray>>{
       let before=analysis_reads.as_ref().map(|cache|cache.lock().map(|c|c.read_scope()).map_err(|_|anyhow::anyhow!("analysis state lock poisoned"))).transpose()?;
       let result=(||->Result<Option<super::motion_tracker::Gray>>{
       let Some(frame)=temporal_prefix_frame(provider,job,request,context,0,dt,(image.w,image.h),Some(scale))?else{return Ok(None)};
       let mut gray=Vec::with_capacity(frame.w*frame.h);
       for row in frame.px.chunks(frame.w*4){ensure!(!context.is_cancelled(),"stabilizer analysis cancelled");for p in row.chunks_exact(4){gray.push(super::basic_grade::display((0.2126*p[0]+0.7152*p[1]+0.0722*p[2]).clamp(0.,1.)));}}
       Ok(Some(super::motion_tracker::Gray::new(frame.w,frame.h,gray)))
       })();
       if result.is_ok(){if let(Some(cache),Some(before))=(&analysis_reads,before){cache.lock().map_err(|_|anyhow::anyhow!("analysis state lock poisoned"))?.finish_read_scope(before);}}
       result
      };
      let mut checkpoint=||{ensure!(!context.is_cancelled(),"stabilizer analysis cancelled");Ok(())};
      let path=if let Some(cache)=analysis_cache{
       let key=(request.sequence_id,job.clip,job.item,request.at,index,request.options.depth);
       super::filmcraft_analysis_cache::resume(&cache,key,||super::stabilizer_analysis::StabilizerAnalysis::new(duration,fps,width,parameters.tracking_method(),parameters.detailed),seconds,&mut source,&mut checkpoint)?
      }else{super::warp_stabilizer::analyse_clip(seconds,duration,fps,width,parameters.tracking_method(),parameters.detailed,&mut source,&mut checkpoint)?.map(std::sync::Arc::new)};
      if let Some(path)=path{stabilization.insert(index,path);}
     },
     super::composition::VideoEffect::AutoReframe{parameters,..}=>{
      parameters.validate()?;
      let (half,step)=[(1.,0.25),(0.5,0.125),(0.15,0.05)][parameters.preset as usize];
      let seconds=clock.local_ticks as f64/254016000000.;let duration=clock.duration_ticks as f64/254016000000.;let fps=clock.frame_rate.max(1.);
      let mut acc=(0f64,0f64,0f64);let mut k=-half;
      while k<=half+1e-9{
       let t=seconds+k;
       if t>=-1e-9&&t<=duration+1e-9{
        let id=(t*fps).round()as i64;
        if let Some(frame)=temporal_prefix_frame(provider,job,request,context,0,id as f64/fps-seconds,(image.w,image.h),Some((160./width.max(1)as f32).clamp(1./64.,1.)))?{
         let _scratch=context.reserve_scratch(frame.w.checked_mul(frame.h).and_then(|n|n.checked_mul(24)).context("reframe analysis scratch overflow")?);
         let mut checkpoint=||{ensure!(!context.is_cancelled(),"reframe cancelled");Ok(())};
         let linear=super::linear_color_frame::LinearColorFrame::from_working(frame.w,frame.h,&frame.px,&mut checkpoint)?;
         let (u,v)=super::auto_reframe::saliency_centroid(&linear,&mut checkpoint)?;
         let weight=1.-(k/(half+step)).abs();acc=(acc.0+u as f64*weight,acc.1+v as f64*weight,acc.2+weight);
        }
       }
       k+=step;
      }
      subjects.insert(index,if acc.2>0.{(acc.0/acc.2,acc.1/acc.2)}else{(0.5,0.5)});
     },
     _=>unreachable!(),
    }

   }
  }
  let [a,b,c,d,e,f]=job.appearance.as_ref().context("source effect Motion missing")?.matrix;
  let motion=super::transition_kernel_pack::geom::Affine{a,b,c,d,e,f};
  let matrix=super::transition_kernel_pack::geom::Affine::scale(request.options.scale as f64,request.options.scale as f64).then_apply(&motion).then_apply(&super::transition_kernel_pack::geom::Affine::scale(1./px_scale as f64,1./px_scale as f64));
  let format=|at|->Result<String>{Ok(filmcraft_time::format_time(filmcraft_time::Tick(at),serde_json::from_value(request.sequence["settings"]["frame_rate"].clone())?,request.sequence["settings"]["drop_frame"].as_bool().context("missing drop frame setting")?,filmcraft_time::TimeDisplay::Timecode,request.sequence["settings"]["sample_rate"].as_i64().context("missing sample rate")?))};
  let index=ProjectIndex::new(request.document)?;
  let burnin=super::metadata_burnin::BurninValues{has_environment:!request.temporal_prefix,sequence_timecode:format(request.temporal_timecode_at.unwrap_or(request.at))?,media_timecode:format(job.frame_ticks)?,clip_name:job.raw_clip["name"].as_str().unwrap_or_default().into(),file_name:index.item(job.item).and_then(|i|i["name"].as_str()).unwrap_or_default().into(),sequence_name:index.item(request.sequence_id).and_then(|i|i["name"].as_str()).unwrap_or_default().into(),frame_rate:clock.frame_rate,seconds:clock.local_ticks as f64/254016000000.};
  let environment=super::filmcraft_source_effects::SourceEffectEnvironment{stabilization:&stabilization,subjects:&subjects,sequence_size:(request.sequence["settings"]["width"].as_u64().context("missing sequence width")?as u32,request.sequence["settings"]["height"].as_u64().context("missing sequence height")?as u32,width),burnin:&burnin,echoes:&echoes,temporal:&temporal,tracks:&tracks,matrix};
  super::filmcraft_source_effects::apply_with_environment(context,&job.raw_clip["effects"],job.parameter_ticks,clock,width,image,request.document.project(),Some(&environment))?
 }else{image};
 super::filmcraft_source_masks::opacity(context,&job.raw_clip["effects"],job.parameter_ticks,width,image)
}
/// Re-read the same clip without temporal recursion or intrinsic opacity.
fn temporal_prefix_frame(provider:&mut dyn SourceLayerProvider,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,context:&RenderContext,upto:usize,dt:f64,size:(usize,usize),base_scale:Option<f32>)->Result<Option<Image>>{
 let ticks=(dt*254016000000f64).round();ensure!(ticks.is_finite()&&ticks>=i64::MIN as f64&&ticks<=i64::MAX as f64,"temporal offset overflow");
 let at=request.at.checked_add(ticks as i64).context("temporal time overflow")?;
 let mut clip=job.raw_clip.clone();clip["effects"]=Value::Array(clip["effects"].as_array().context("missing temporal prefix")?[..upto].iter().filter(|e|!matches!(e["effect"].as_str(),Some("motion"|"opacity"|"posterize_time"|"echo"|"warp_stabilizer"|"auto_reframe"))).cloned().collect());
 let clocks=super::filmcraft_sequence_source::SourceTrackItem::read(&clip)?.clocks(at)?;
 let job=SourceClipJob{clip:job.clip,item:job.item,frame_ticks:clocks.frame,parameter_ticks:clocks.parameters,generator_ticks:clocks.generator,raw_clip:&clip,raw_source:job.raw_source,source_size:job.source_size,appearance:None};
 let request=FrameRequest{at,temporal_prefix:true,temporal_timecode_at:Some(request.temporal_timecode_at.unwrap_or(request.at)),..*request};
 let scale=base_scale.or_else(||job.source_size.map(|(w,_)|size.0 as f32/w as f32)).context("temporal source size missing")?;
 // Prefix effects still require current geometry for their own-coordinate kernels.
 let index=ProjectIndex::new(request.document)?;let sequence=super::filmcraft_sequence_source::SourceSequence::read(&index,request.sequence_id)?;let item=super::filmcraft_sequence_source::SourceTrackItem::read(&clip)?;
 let mut job=job;job.appearance=Some(super::filmcraft_video_appearance::sample_item(&index,&sequence,&item,at)?);
 match provider.layer(&job,&request,scale,context)?{
  LayerPixels::Missing=>Ok(None),
  LayerPixels::Source(image)=>if base_scale.is_some()||(image.w,image.h)==size{Ok(Some(image))}else{context.image_work(&[&image],||Ok(image.transformed(size.0,size.1,&super::transition_kernel_pack::geom::Affine::scale(size.0 as f64/image.w as f64,size.1 as f64/image.h as f64)))).map(Some).map_err(anyhow::Error::msg)},
  LayerPixels::Positioned(_)=>anyhow::bail!("temporal prefix requires source pixels"),
 }
}
/// Already decoded, library-authorized still images; no filesystem access in rendering.
pub struct ImageSourceProvider { pub images:std::sync::Arc<std::collections::BTreeMap<u64,super::group_surface::GroupSurface>> }
fn source_layer(provider:&mut dyn SourceLayerProvider,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{
  plain(request)?;
  if job.raw_source.pointer("/kind/Graphic").is_some(){return match super::filmcraft_source_graphic::render(job,request,scale,context)?{LayerPixels::Source(image)=>Ok(LayerPixels::Source(apply_effects(provider,image,job,request,context)?)),other=>Ok(other)};} 
  if let Some(nested)=job.raw_source.pointer("/kind/Sequence"){
   let multicam=job.raw_clip.pointer("/multicam/enabled").and_then(Value::as_bool)==Some(true);
   let only=if multicam{
    let angle=usize::try_from(job.raw_clip.pointer("/multicam/angle").and_then(Value::as_u64).context("invalid source multicam angle")?)?;
    let tracks=nested["video_tracks"].as_array().context("missing nested video tracks")?;
    let index=if let Some(cameras)=nested.get("multicam").filter(|v|!v.is_null()){
     cameras["cameras"].as_array().context("missing source cameras")?.get(angle).and_then(|camera|camera["video_track"].as_u64()).and_then(|id|tracks.iter().position(|track|track["id"].as_u64()==Some(id)))
    }else{(angle<tracks.len()).then_some(angle)};
    Some(index)
   }else{None};
   let options=super::filmcraft_source_frame::FrameOptions{scale,effects:request.options.effects,depth:request.options.depth.checked_add(1).context("source nesting depth overflow")?,captions:!multicam,working_output:true};
   let image=match only{
    Some(None)=>{let(w,h)=job.source_size.context("nested source dimensions missing")?;let(w,h)=(((w as f32*scale).round()as usize).max(1),((h as f32*scale).round()as usize).max(1));ensure!(context.canvas_allowed(w,h),"nested empty canvas budget exceeded");context.image_work(&[],||Ok(Image::new(w,h))).map_err(anyhow::Error::msg)?},
    _=>super::filmcraft_source_frame::render_track(context,request.document,job.item,job.frame_ticks,options,provider,only.flatten())?,
   };
   let image=apply_effects(provider,image,job,request,context)?;
   return Ok(LayerPixels::Source(image));
  }
  let index=ProjectIndex::new(request.document)?;
  let media=index.resolve_media(job.item).context("generator provider cannot render this source kind")?.media;
  if media["offline"]==true{return Ok(LayerPixels::Missing)}
  if media.pointer("/media/File").is_some(){return provider.file(job,request,scale,context)}
  let generator:Generator=serde_json::from_value(media.pointer("/media/Generator").context("file source provider is not connected")?.clone())?;
  if matches!(generator,Generator::Tone{..}){return Ok(LayerPixels::Missing)}
  ensure!(media.pointer("/interpret/ignore_alpha").is_none_or(|v|v==false)&&media.pointer("/interpret/invert_alpha").is_none_or(|v|v==false),"generator alpha interpretation is not connected");
  let frame=filmcraft_generator_video::source_frame(request.document,job.item,job.frame_ticks,scale,||context.is_cancelled())?;
  let full_width=job.source_size.context("generator source dimensions missing")?.0;
  let mut n=1;while n<16&&frame.surface.width as f32/(n as f32*2.)>=(full_width as f32*scale).max(1.){n*=2;}
  let image=interpreted_pixels(context,&frame.surface,n,media,request)?;
  let item=super::filmcraft_sequence_source::SourceTrackItem::read(job.raw_clip)?;
  let rate=media.pointer("/interpret/frame_rate").filter(|v|!v.is_null()).or_else(||media.pointer("/info/video/frame_rate")).map(|r|serde_json::from_value::<filmcraft_time::FrameRate>(r.clone())).transpose()?.unwrap_or_default();
  let image=if let Some(blend)=super::filmcraft_frame_blend::plan(&item,request.at,rate)?{
   let next=filmcraft_generator_video::source_frame(request.document,job.item,blend.next,scale,||context.is_cancelled())?;
   if (next.surface.width,next.surface.height)==(frame.surface.width,frame.surface.height){
    let b=interpreted_pixels(context,&next.surface,n,media,request)?;
    context.image_work(&[&b],||{let mut image=image;for(a,b)in image.px.chunks_mut(4096).zip(b.px.chunks(4096)){if context.is_cancelled(){return Err("frame blending cancelled".into())}for(a,b)in a.iter_mut().zip(b){*a+=(*b-*a)*blend.weight;}}Ok(image)}).map_err(anyhow::Error::msg)?
   }else{image}
  }else{image};
  let image=apply_effects(provider,image,job,request,context)?;
  Ok(LayerPixels::Source(image))
 }
impl SourceLayerProvider for GeneratorSourceProvider{
 fn layer(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{source_layer(self,job,request,scale,context)}
 fn transition(&mut self,raw:&Value,_:&FrameRequest<'_>)->Result<EffectInstance>{
  EffectInstance::from_source(raw).map_err(anyhow::Error::msg)
 }
 fn adjustment(&mut self,_:&SourceClipJob<'_>,_:Image,_:&FrameRequest<'_>,_:&RenderContext)->Result<Image>{anyhow::bail!("generator adjustment effects are not connected")}
 fn finish(&mut self,mut canvas:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{plain(request)?;if request.options.captions{super::filmcraft_source_captions::apply(context,request,&mut canvas)?;}Ok(canvas)}
}
impl SourceLayerProvider for ImageSourceProvider{
 fn file(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{
  let index=ProjectIndex::new(request.document)?;let source=index.resolve_media(job.item).context("file media is unresolved")?;
  let surface=self.images.get(&source.item_id).context("source image is not bound to the library")?;
  let image=file_pixels(surface,source.media,job,request,scale,context)?;
  Ok(LayerPixels::Source(apply_effects(self,image,job,request,context)?))
 }

 fn layer(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{source_layer(self,job,request,scale,context)}
 fn transition(&mut self,raw:&Value,request:&FrameRequest<'_>)->Result<EffectInstance>{GeneratorSourceProvider.transition(raw,request)}
 fn adjustment(&mut self,job:&SourceClipJob<'_>,image:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{GeneratorSourceProvider.adjustment(job,image,request,context)}
 fn finish(&mut self,image:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{GeneratorSourceProvider.finish(image,request,context)}
}
#[cfg(test)]mod tests{
 use super::*;use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument};use std::sync::{Arc,atomic::AtomicBool};
 #[test]fn original_generator_sequences_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-generator-sequence.json"),768,32)}
 #[test]fn original_generator_frame_blending_matches_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-generator-frame-blending.json"),1536,32)}
 #[test]fn original_nested_sequences_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-nested-sequence.json"),288,320*200)}
 #[test]fn original_multicam_sequences_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-multicam-sequence.json"),320,320*200)}
 #[test]fn original_source_mask_clocks_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-mask-clock.json"),1024,128)}
 #[test]fn original_source_masks_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-masks.json"),1008,128)}
 #[test]fn original_source_roughen_points_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-roughen-points.json"),576,128)}
 #[test]fn original_source_light_clocks_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-light-clock.json"),256,128)}
 #[test]fn original_source_vr_spatial_matches_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-vr-spatial.json"),336,128)}
 #[test]fn original_source_vr_layout_matches_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-vr-layout.json"),192,128)}
 #[test]fn original_source_magnify_points_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-magnify-points.json"),720,128)}
 #[test]fn original_source_shake_and_corners_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-shake-corners.json"),384,128)}
 #[test]fn original_source_duration_progress_matches_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-progress.json"),512,128)}
 #[test]fn original_source_pattern_points_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-pattern-points.json"),360,128)}
 #[test]fn original_source_wave_clock_matches_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-wave-clock.json"),128,128)}
 #[test]fn original_source_point_controls_match_every_bit(){compare(include_str!("../../tests/fixtures/filmcraft-source-points.json"),144,128)}
 #[test]fn original_connected_source_effects_match_every_bit(){
  let mut fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-source-effects.json")).unwrap();
  fixture["cases"].as_array_mut().unwrap().retain(|c|matches!(c["effect"].as_str(),Some("timecode"|"clip_name"|"metadata_burnin"|"simple_text"|"clone"|"auto_align"|"compound_blur"|"track_matte"|"gradient_wipe_legacy"|"lumetri"|"linear_wipe_legacy"|"lighting_effects"|"lightning"|"logo_cutout"|"write_on"|"brush_strokes"|"echo_glow"|"wonder_glow"|"volumetric_rays"|"lens_flare"|"ultra_key"|"alpha_glow"|"drop_shadow"|"bevel_alpha"|"long_shadow"|"stroke"|"block_dissolve"|"ellipse"|"paint_bucket"|"glint"|"edge_glow"|"bokeh_blur"|"focus_blur"|"roughen_edges"|"strobe"|"light_leaks"|"vr_glow"|"vr_color_gradients"|"vr_fractal_noise"|"vr_digital_glitch"|"vr_plane_to_sphere"|"vr_projection"|"vr_rotate_sphere"|"vr_blur"|"vr_sharpen"|"vr_denoise"|"vr_chromatic_aberrations"|"magnify"|"magnify_legacy"|"rotate_3d"|"wiggle"|"camera_shake"|"turbulent_displace"|"corner_pin"|"basic_3d"|"grow"|"shrink"|"spin"|"move"|"spacer"|"transform"|"checkerboard"|"cell_pattern"|"circle"|"ramp"|"four_color_gradient"|"grid"|"gradient"|"color_replace"|"video_limiter"|"noise"|"noise_legacy"|"vignette"|"rgb_split"|"reduce_interlace_flicker"|"twirl"|"twirl_legacy"|"wave_warp"|"spherize"|"lens_distortion"|"rounded_crop"|"offset"|"mirror"|"crop"|"edge_feather"|"camera_blur"|"directional_blur"|"directional_blur_legacy"|"gaussian_blur_legacy"|"channel_blur"|"unsharp_mask"|"mosaic"|"mosaic_legacy"|"replicate"|"color_key"|"find_edges"|"emboss"|"color_emboss"|"median"|"sharpen"|"gaussian_blur"|"leave_color"|"change_to_color"|"color_pass"|"asc_cdl"|"cineon_converter"|"alpha_adjust"|"brightness_contrast"|"channel_mix"|"color_balance"|"extract"|"gamma_correction"|"invert"|"levels"|"luma_key"|"posterize"|"proc_amp"|"tint"|"horizontal_flip"|"vertical_flip"|"black_white")));
  compare(&serde_json::to_string(&fixture).unwrap(),968,128);
 }
 fn compare(source:&str,count:usize,pixels:usize){
  let fixture:Value=serde_json::from_str(source).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),count);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,pixels,32*1024*1024).unwrap();let mut provider=GeneratorSourceProvider;
  for(index,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let output=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,effects:case.get("effects_enabled").and_then(Value::as_bool).unwrap_or(true),working_output:true,..FrameOptions::default()},&mut provider).unwrap_or_else(|error|panic!("source case {index}, effect {}, profile {}, layout {}: {error}",case["effect"],case["profile"],case["layout"]));assert_eq!(serde_json::json!([output.w,output.h]),serde_json::json!([case["width"],case["height"]]));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"generator frame {index}, mode {}, generator {}",case["mode"],case["generator"]);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);}
 }
}
#[cfg(test)]mod image_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};
 fn pattern(value:&Value)->Vec<u8>{let variant=value["pattern"].as_u64().unwrap();(0..value["width"].as_u64().unwrap()*value["height"].as_u64().unwrap()).flat_map(|i|[((i*37+11)%256)as u8,((i*73+51)%256)as u8,((i*19+83)%256)as u8,match variant{0=>255,1=>128,_=>[0,1,64,128,255][i as usize%5]}]).collect()}

 #[test]fn original_still_images_masks_and_nested_sequences_match_every_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-image-source.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1620);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,320*200,32*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=super::super::filmcraft_project_file::SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let mut images=std::collections::BTreeMap::new();for(key,value)in case["images"].as_object().unwrap(){images.insert(key.parse().unwrap(),super::super::group_surface::GroupSurface{width:value["width"].as_u64().unwrap()as usize,height:value["height"].as_u64().unwrap()as usize,rgba:pattern(value)});}
   let mut provider=ImageSourceProvider{images:Arc::new(images)};let output=super::super::filmcraft_source_frame::render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),super::super::filmcraft_source_frame::FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("image case {i}: {e}"));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"image case {i}, kind {}, scale {}, variant {}",case["kind"],case["scale"],case["variant"]);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

/// Stills and generators supply sRGB RGBA8 regardless of authored stream metadata.
/// Interpret Footage overrides apply to those code values, following resolved media.
fn interpreted_pixels(context:&RenderContext,surface:&super::group_surface::GroupSurface,n:usize,media:&Value,request:&FrameRequest<'_>)->Result<Image>{
 decoded_pixels(context,surface,n,media,request,filmcraft_color::ColorInfo::SRGB_FULL)
}
fn decoded_pixels(context:&RenderContext,surface:&super::group_surface::GroupSurface,n:usize,media:&Value,request:&FrameRequest<'_>,color:filmcraft_color::ColorInfo)->Result<Image>{
 let pipeline:filmcraft_color::ColorPipeline=serde_json::from_value(request.sequence["settings"]["color"].clone()).context("invalid sequence color pipeline")?;
 let override_space=media.pointer("/interpret/color_space").filter(|v|!v.is_null()).map(|v|serde_json::from_value::<filmcraft_color::ColorSpace>(v.clone())).transpose().context("invalid source color-space override")?;
 let hdr=media.pointer("/info/video/hdr").filter(|v|!v.is_null()).map(|v|serde_json::from_value::<filmcraft_color::HdrMetadata>(v.clone())).transpose().context("invalid source HDR metadata")?;
 super::filmcraft_source_color::decode_rgba8(context,surface,n,color,override_space,pipeline,hdr.and_then(|h|h.peak_nits()))
}
fn file_pixels(surface:&super::group_surface::GroupSurface,media:&Value,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<Image>{
 file_pixels_color(surface,media,job,request,scale,context,filmcraft_color::ColorInfo::SRGB_FULL)
}
fn file_pixels_color(surface:&super::group_surface::GroupSurface,media:&Value,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext,color:filmcraft_color::ColorInfo)->Result<Image>{
 ensure!(media.pointer("/interpret/ignore_alpha").is_none_or(|v|v==false)&&media.pointer("/interpret/invert_alpha").is_none_or(|v|v==false),"file alpha interpretation is not connected");
 let width=job.source_size.context("file source dimensions missing")?.0;
 let mut n=1;while n<16&&surface.width as f32/(n as f32*2.)>=(width as f32*scale).max(1.){n*=2;}
 decoded_pixels(context,surface,n,media,request,color)
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub struct VideoFrameKey{pub item:u64,pub ticks:i64}
pub struct VideoSourceFrame{pub surface:std::sync::Arc<super::group_surface::GroupSurface>,pub color:filmcraft_color::ColorInfo}
pub type VideoFrames=std::collections::BTreeMap<VideoFrameKey,Option<std::sync::Arc<VideoSourceFrame>>>;
/// Request-scoped straight RGBA8 frames with actual decoder transfer/primaries.
/// YUV matrix/range expansion belongs to the decoder; working conversion is native.
/// Missing keys suspend rendering for the async
/// service; a present None means the decoder confirmed no source frame exists.
pub struct VideoSourceProvider{
 pub analysis:super::filmcraft_analysis_cache::SharedAnalysis,
 pub images:std::sync::Arc<std::collections::BTreeMap<u64,super::group_surface::GroupSurface>>,
 pub videos:std::collections::BTreeSet<u64>,pub frames:std::sync::Arc<VideoFrames>,pub pending:Option<VideoFrameKey>,
}
impl VideoSourceProvider{
 fn frame(&mut self,key:VideoFrameKey)->Result<Option<std::sync::Arc<VideoSourceFrame>>>{
  if let Some(frame)=self.frames.get(&key){self.analysis.lock().map_err(|_|anyhow::anyhow!("analysis state lock poisoned"))?.touch(key);return Ok(frame.clone())}
  self.pending=Some(key);anyhow::bail!("video source frame requires asynchronous decoding")
 }
}
impl SourceLayerProvider for VideoSourceProvider{
 fn analysis_cache(&self)->Option<super::filmcraft_analysis_cache::SharedAnalysis>{Some(self.analysis.clone())}
 fn file(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{
  let index=ProjectIndex::new(request.document)?;let source=index.resolve_media(job.item).context("file media is unresolved")?;
  if let Some(surface)=self.images.get(&source.item_id){let image=file_pixels(surface,source.media,job,request,scale,context)?;return Ok(LayerPixels::Source(apply_effects(self,image,job,request,context)?))}
  ensure!(self.videos.contains(&source.item_id),"file source is not bound to the library");
  let Some(surface)=self.frame(VideoFrameKey{item:source.item_id,ticks:job.frame_ticks})?else{return Ok(LayerPixels::Missing)};
  let item=super::filmcraft_sequence_source::SourceTrackItem::read(job.raw_clip)?;
  let rate=source.media.pointer("/interpret/frame_rate").filter(|v|!v.is_null()).or_else(||source.media.pointer("/info/video/frame_rate")).map(|r|serde_json::from_value::<filmcraft_time::FrameRate>(r.clone())).transpose()?.unwrap_or_default();
  let next=if let Some(blend)=super::filmcraft_frame_blend::plan(&item,request.at,rate)?{self.frame(VideoFrameKey{item:source.item_id,ticks:blend.next})?.map(|surface|(surface,blend.weight))}else{None};
  let mut image=file_pixels_color(&surface.surface,source.media,job,request,scale,context,surface.color)?;
  if let Some((next,weight))=next.filter(|(next,_)|(next.surface.width,next.surface.height)==(surface.surface.width,surface.surface.height)){
   let b=file_pixels_color(&next.surface,source.media,job,request,scale,context,next.color)?;
   image=context.image_work(&[&b],||{let mut image=image;for(a,b)in image.px.chunks_mut(4096).zip(b.px.chunks(4096)){if context.is_cancelled(){return Err("video frame blending cancelled".into())}for(a,b)in a.iter_mut().zip(b){*a+=(*b-*a)*weight;}}Ok(image)}).map_err(anyhow::Error::msg)?;
  }
  Ok(LayerPixels::Source(apply_effects(self,image,job,request,context)?))
 }
 fn layer(&mut self,job:&SourceClipJob<'_>,request:&FrameRequest<'_>,scale:f32,context:&RenderContext)->Result<LayerPixels>{source_layer(self,job,request,scale,context)}
 fn transition(&mut self,raw:&Value,request:&FrameRequest<'_>)->Result<EffectInstance>{GeneratorSourceProvider.transition(raw,request)}
 fn adjustment(&mut self,job:&SourceClipJob<'_>,image:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{GeneratorSourceProvider.adjustment(job,image,request,context)}
 fn finish(&mut self,image:Image,request:&FrameRequest<'_>,context:&RenderContext)->Result<Image>{GeneratorSourceProvider.finish(image,request,context)}
}
#[cfg(test)]mod video_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 fn clock_frame(document:&SourceProjectDocument,key:VideoFrameKey,variant:u64)->Option<Arc<VideoSourceFrame>>{
  const T:i64=254016000000;if key.ticks<0||key.ticks>=100*T{return None}
  let index=ProjectIndex::new(document).unwrap();let media=index.resolve_media(key.item).unwrap().media;let video=&media["info"]["video"];let rate:filmcraft_time::FrameRate=serde_json::from_value(video["frame_rate"].clone()).unwrap();let rate=rate.sane();let frame=(key.ticks as i128*rate.num as i128).div_euclid(T as i128*rate.den as i128)as u64;
  let width=video["width"].as_u64().unwrap()as usize;let height=video["height"].as_u64().unwrap()as usize;let rgba=(0..(width*height)as u64).flat_map(|i|[((i*37+frame*29+11)%256)as u8,((i*73+frame*17+51)%256)as u8,((i*19+frame*43+83)%256)as u8,if variant==0{255}else{[0,1,64,128,255][(i+frame)as usize%5]}]).collect();Some(Arc::new(VideoSourceFrame{surface:Arc::new(GroupSurface{width,height,rgba}),color:filmcraft_color::ColorInfo::SRGB_FULL}))
 }
 #[test]fn original_video_clocks_blending_and_pending_release_match_every_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-video-source.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),256);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,4096,4*1024*1024).unwrap();let mut pending_count=0;
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let index=ProjectIndex::new(&document).unwrap();let videos=case["document"]["project"]["items"].as_object().unwrap().keys().filter_map(|key|{let id=key.parse().unwrap();index.resolve_media(id).filter(|m|m.item_id==id).map(|_|id)}).collect();let mut provider=VideoSourceProvider{analysis:Default::default(),images:Arc::new(Default::default()),videos,frames:Arc::new(Default::default()),pending:None};
   let output=loop{provider.pending=None;let result=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider);if let Some(key)=provider.pending{assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);assert!(!provider.frames.contains_key(&key));assert!(provider.frames.len()<16);pending_count+=1;Arc::make_mut(&mut provider.frames).insert(key,clock_frame(&document,key,case["variant"].as_u64().unwrap()));continue}break result.unwrap_or_else(|e|panic!("video case {i}: {e}"))};
   assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"video case {i}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }assert!(pending_count>=256);
 } #[test]fn original_posterize_time_video_prefix_and_pending_release_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-posterize-time.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),768);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,4096,4*1024*1024).unwrap();let mut pending_count=0;
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let index=ProjectIndex::new(&document).unwrap();let videos=case["document"]["project"]["items"].as_object().unwrap().keys().filter_map(|key|{let id=key.parse().unwrap();index.resolve_media(id).filter(|m|m.item_id==id).map(|_|id)}).collect();let mut provider=VideoSourceProvider{analysis:Default::default(),images:Arc::new(Default::default()),videos,frames:Arc::new(Default::default()),pending:None};
   let output=loop{provider.pending=None;let result=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider);if let Some(key)=provider.pending{assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);assert!(!provider.frames.contains_key(&key));assert!(provider.frames.len()<64);pending_count+=1;Arc::make_mut(&mut provider.frames).insert(key,clock_frame(&document,key,case["variant"].as_u64().unwrap()));continue}break result.unwrap_or_else(|e|panic!("video case {i}: {e}"))};
   assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"video case {i}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }assert!(pending_count>=256);
 } fn stabilizer_clock_frame(document:&SourceProjectDocument,key:VideoFrameKey,variant:u64)->Option<Arc<VideoSourceFrame>>{
  const T:i64=254016000000;if key.ticks<0||key.ticks>=100*T{return None}
  let index=ProjectIndex::new(document).unwrap();let media=index.resolve_media(key.item).unwrap().media;let video=&media["info"]["video"];let rate:filmcraft_time::FrameRate=serde_json::from_value(video["frame_rate"].clone()).unwrap();let rate=rate.sane();let frame=(key.ticks as i128*rate.num as i128).div_euclid(T as i128*rate.den as i128)as u64;
  let width=video["width"].as_u64().unwrap()as usize;let height=video["height"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|{let x=(i%width)as i64-2*frame as i64;let y=(i/width)as i64-frame as i64;let u=x.rem_euclid(width as i64)as usize;let v=y.rem_euclid(height as i64)as usize;let a=((u*17+v*31+(u/7)*(v/5)*53)%256)as u8;[a,((u*3+v*11)%256)as u8,((u*29+v*7)%256)as u8,if variant==0{255}else{128}]}).collect();Some(Arc::new(VideoSourceFrame{surface:Arc::new(GroupSurface{width,height,rgba}),color:filmcraft_color::ColorInfo::SRGB_FULL}))
 }
 #[test]fn original_warp_stabilizer_video_analysis_and_pending_release_match_every_bit(){
  let fixture:Value=serde_json::from_reader(flate2::read::GzDecoder::new(std::fs::File::open(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-stabilizer.json.gz")).unwrap())).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1152);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,8192,4*1024*1024).unwrap();let mut pending_count=0;
  for(i,case)in cases.iter().enumerate(){if i%96==0{eprintln!("stabilizer source {i}/{}",cases.len());}let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let index=ProjectIndex::new(&document).unwrap();let videos=case["document"]["project"]["items"].as_object().unwrap().keys().filter_map(|key|{let id=key.parse().unwrap();index.resolve_media(id).filter(|m|m.item_id==id).map(|_|id)}).collect();let mut provider=VideoSourceProvider{analysis:Default::default(),images:Arc::new(Default::default()),videos,frames:Arc::new(Default::default()),pending:None};
   let output=loop{provider.pending=None;let result=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider);if let Some(key)=provider.pending{assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);assert!(!provider.frames.contains_key(&key));assert!(provider.frames.len()<64);pending_count+=1;Arc::make_mut(&mut provider.frames).insert(key,stabilizer_clock_frame(&document,key,case["variant"].as_u64().unwrap()));continue}break result.unwrap_or_else(|e|panic!("video case {i}: {e}"))};
   assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"video case {i}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }assert!(pending_count>=256);
 } #[test]fn original_auto_reframe_video_analysis_and_pending_release_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-reframe.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),3840);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,4096,4*1024*1024).unwrap();let mut pending_count=0;
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let index=ProjectIndex::new(&document).unwrap();let videos=case["document"]["project"]["items"].as_object().unwrap().keys().filter_map(|key|{let id=key.parse().unwrap();index.resolve_media(id).filter(|m|m.item_id==id).map(|_|id)}).collect();let mut provider=VideoSourceProvider{analysis:Default::default(),images:Arc::new(Default::default()),videos,frames:Arc::new(Default::default()),pending:None};
   let output=loop{provider.pending=None;let result=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider);if let Some(key)=provider.pending{assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);assert!(!provider.frames.contains_key(&key));assert!(provider.frames.len()<64);pending_count+=1;Arc::make_mut(&mut provider.frames).insert(key,clock_frame(&document,key,case["variant"].as_u64().unwrap()));continue}break result.unwrap_or_else(|e|panic!("video case {i}: {e}"))};
   assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"video case {i}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }assert!(pending_count>=256);
 } #[test]fn original_echo_video_prefix_and_pending_release_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-echo.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1792);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),1,4096,4*1024*1024).unwrap();let mut pending_count=0;
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let index=ProjectIndex::new(&document).unwrap();let videos=case["document"]["project"]["items"].as_object().unwrap().keys().filter_map(|key|{let id=key.parse().unwrap();index.resolve_media(id).filter(|m|m.item_id==id).map(|_|id)}).collect();let mut provider=VideoSourceProvider{analysis:Default::default(),images:Arc::new(Default::default()),videos,frames:Arc::new(Default::default()),pending:None};
   let output=loop{provider.pending=None;let result=render(&context,&document,case["sequence"].as_u64().unwrap(),case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider);if let Some(key)=provider.pending{assert!(result.is_err());assert_eq!(context.pixel_bytes_in_use(),0);assert!(!provider.frames.contains_key(&key));assert!(provider.frames.len()<64);pending_count+=1;Arc::make_mut(&mut provider.frames).insert(key,clock_frame(&document,key,case["variant"].as_u64().unwrap()));continue}break result.unwrap_or_else(|e|panic!("video case {i}: {e}"))};
   assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"video case {i}");drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }assert!(pending_count>=256);
 }
}
#[cfg(test)]mod interpreted_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_interpreted_stills_and_generators_compose_through_subclips(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-interpreted-source.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),384);
  let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,4096,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let mut provider:Box<dyn SourceLayerProvider>=if case["kind"]=="image"{let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[((i*37+11)%256)as u8,((i*73+51)%256)as u8,((i*19+83)%256)as u8,[0,1,64,128,255][i%5]]).collect();Box::new(ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())})}else{Box::new(GeneratorSourceProvider)};
   let output=render(&context,&document,6,0,FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},provider.as_mut()).unwrap_or_else(|e|panic!("interpreted source case {i}: {e}"));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"interpreted frame {i}");let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"interpreted display bytes {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}
#[cfg(test)]mod alpha_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_alpha_effects_with_animated_color_points_and_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-source-alpha-effects.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1728);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[((i*37+11)%256)as u8,((i*73+51)%256)as u8,((i*19+83)%256)as u8,if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

#[cfg(test)]mod light_key_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_light_key_effects_with_animated_controls_and_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-light-key.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1296);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

#[cfg(test)]mod strokes_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_strokes_effects_with_history_and_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-strokes.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),648);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

#[cfg(test)]mod lighting_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_lighting_effect_with_five_animated_lights_and_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-grade-light.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases:Vec<_>=fixture["cases"].as_array().unwrap().iter().filter(|c|c["effect"]=="lighting_effects").collect();assert_eq!(cases.len(),216);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.into_iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

#[cfg(test)]mod linear_wipe_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_linear_wipe_source_animated_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-linear-wipe.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),216);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod lumetri_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_lumetri_source_all_sections_luts_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-grade-light.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases:Vec<_>=fixture["cases"].as_array().unwrap().iter().filter(|c|c["effect"]=="lumetri").collect();assert_eq!(cases.len(),648);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,32*1024*1024).unwrap();
  for(i,case)in cases.into_iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod lumetri_input_hue_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_lumetri_input_luts_and_all_hue_curves_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-lumetri-input-hue.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases:Vec<_>=fixture["cases"].as_array().unwrap().iter().filter(|c|c["effect"]=="lumetri").collect();assert_eq!(cases.len(),648);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,32*1024*1024).unwrap();
  for(i,case)in cases.into_iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}

#[cfg(test)]mod gradient_wipe_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_gradient_wipe_external_tracks_motion_cycles_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-gradient-wipe.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1296);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,32*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba}),(9,GroupSurface{width:5,height:7,rgba:(0..35).flat_map(|i|{let v=((i*31+17)%256)as u8;[v,v,v,[64,128,255][i%3]]}).collect()})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod track_matte_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_track_matte_external_tracks_motion_cycles_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-track-matte.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1296);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,32*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba}),(9,GroupSurface{width:5,height:7,rgba:(0..35).flat_map(|i|{let v=((i*31+17)%256)as u8;[v,v,v,[64,128,255][i%3]]}).collect()})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod compound_blur_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_compound_blur_external_tracks_motion_cycles_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-compound-blur.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1296);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,32*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba}),(9,GroupSurface{width:5,height:7,rgba:(0..35).flat_map(|i|{let v=((i*31+17)%256)as u8;[v,v,v,[64,128,255][i%3]]}).collect()})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod clone_align_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_clone_align_source_animated_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-clone-align.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),432);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod simple_text_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_simple_text_source_animated_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-simple-text.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),216);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


#[cfg(test)]mod labels_effect_tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};use super::super::{filmcraft_source_frame::{render,FrameOptions},filmcraft_project_file::SourceProjectDocument,group_surface::GroupSurface};
 #[test]fn original_labels_source_animated_masks_match_every_bit(){
  let fixture:Value=serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/fixtures/filmcraft-source-labels.json")).unwrap()).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),1728);let context=RenderContext::new_with_budget(Arc::new(AtomicBool::new(false)),2,128,4*1024*1024).unwrap();
  for(i,case)in cases.iter().enumerate(){let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();let width=case["sourceWidth"].as_u64().unwrap()as usize;let height=case["sourceHeight"].as_u64().unwrap()as usize;let rgba=(0..width*height).flat_map(|i|[if case["pattern"]==2&&i%3!=0{0}else{((i*37+11)%256)as u8},if case["pattern"]==2&&i%3!=0{204}else{((i*73+51)%256)as u8},if case["pattern"]==2&&i%3!=0{51}else{((i*19+83)%256)as u8},if case["pattern"]==0{255}else{[0,1,64,128,255][i%5]}]).collect();let mut provider=ImageSourceProvider{images:Arc::new([(1,GroupSurface{width,height,rgba})].into_iter().collect())};let output=render(&context,&document,6,case["at"].as_i64().unwrap(),FrameOptions{scale:case["scale"].as_f64().unwrap()as f32,working_output:true,..Default::default()},&mut provider).unwrap_or_else(|e|panic!("alpha source case {i}, effect {}, scale {}: {e}",case["effect"],case["scale"]));assert_eq!((output.w,output.h),(case["width"].as_u64().unwrap()as usize,case["height"].as_u64().unwrap()as usize));assert_eq!(serde_json::json!(output.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()),case["bits"],"alpha source {i}, effect {}, scale {}, mask {}",case["effect"],case["scale"],case["masked"]);let bytes=super::super::filmcraft_source_pixels::encode_srgb(&context,&output).unwrap();assert_eq!(serde_json::json!(bytes.surface().rgba),case["rgba"],"alpha display {i}");drop(bytes);drop(output);assert_eq!(context.pixel_bytes_in_use(),0);
  }
 }
}


