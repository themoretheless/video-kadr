//! Source controls sampled at the source parameter clock, then native working-pixel effects.
use super::{composition::VideoEffect,filmcraft_effect_scalar as scalar,linear_color_frame::LinearColorFrame,transition_kernel_pack::{context::RenderContext,image::Image}};
use anyhow::{ensure,Context,Result};
use serde_json::{Value,json};
#[derive(Clone,Copy)]
pub(crate) struct SourceClock{pub local_ticks:i64,pub duration_ticks:i64,pub frame_rate:f64}
impl SourceClock{
 fn values(self)->Result<(f64,f64,f64)>{
  ensure!(self.duration_ticks>=0&&self.frame_rate.is_finite()&&self.frame_rate>0.,"invalid source effect duration/rate");
  let seconds=self.local_ticks as f64/254016000000.;let duration=self.duration_ticks as f64/254016000000.;
  Ok((seconds,if duration<=1e-9{0.}else{(seconds/duration).clamp(0.,1.)},1./(self.frame_rate*duration).max(1.)))
 }
}
pub(super) fn compile(raw:&Value,tick:i64,fallback:[f64;2])->Result<VideoEffect>{
 let id=raw["effect"].as_str().context("missing source effect id")?;
 let kind=match id{
  "warp_stabilizer"=>"warp_stabilizer",
  "auto_reframe"=>"auto_reframe",
  "timecode"=>"timecode",
  "clip_name"=>"clip_name",
  "metadata_burnin"=>"metadata_burnin",
  "simple_text"=>"simple_text",
  "clone"=>"clone",
  "auto_align"=>"auto_align",
  "echo"=>"echo",
  "posterize_time"=>"posterize_time",
  "compound_blur"=>"compound_blur",
  "track_matte"=>"track_matte",
  "gradient_wipe_legacy"=>"gradient_wipe",
  "linear_wipe_legacy"=>"linear_wipe",
  "lighting_effects"=>"lighting_effects",
  "lightning"=>"lightning",
  "logo_cutout"=>"logo_cutout",
  "write_on"=>"write_on",
  "brush_strokes"=>"brush_strokes",
  "echo_glow"=>"echo_glow",
  "wonder_glow"=>"wonder_glow",
  "volumetric_rays"=>"volumetric_rays",
  "lens_flare"=>"lens_flare",
  "ultra_key"=>"ultra_key",

  "alpha_glow"=>"alpha_glow",
  "drop_shadow"=>"drop_shadow",
  "bevel_alpha"=>"bevel_alpha",
  "long_shadow"=>"long_shadow",
  "stroke"=>"stroke",
  "block_dissolve"=>"block_dissolve",
  "ellipse"=>"ellipse",
  "paint_bucket"=>"paint_bucket",
  "glint"=>"glint",
  "edge_glow"=>"edge_glow",
  "bokeh_blur"=>"bokeh_blur",
  "focus_blur"=>"focus_blur",
  "roughen_edges"=>"roughen_edges",
  "strobe"=>"strobe",
  "light_leaks"=>"light_leaks",
  "vr_glow"=>"vr_glow",
  "vr_color_gradients"=>"vr_color_gradients",
  "vr_fractal_noise"=>"vr_fractal_noise",
  "vr_digital_glitch"=>"vr_digital_glitch",
  "vr_plane_to_sphere"=>"vr_plane_to_sphere",
  "vr_projection"=>"vr_projection",
  "vr_rotate_sphere"=>"vr_rotate_sphere",

  "vr_blur"=>"vr_blur",
  "vr_sharpen"=>"vr_sharpen",
  "vr_denoise"=>"vr_denoise",
  "vr_chromatic_aberrations"=>"vr_chromatic_aberrations",

  "magnify"|"magnify_legacy"=>"magnify",
  "rotate_3d"=>"rotate_3d",
  "basic_3d"=>"basic_3d",
  "corner_pin"=>"corner_pin",
  "turbulent_displace"=>"turbulent_displace",
  "camera_shake"=>"camera_shake",
  "wiggle"=>"wiggle",
  "transform"=>"transform",
  "spacer"=>"spacer",
  "move"=>"move",
  "spin"=>"spin",
  "shrink"=>"shrink",
  "grow"=>"grow",
  "gradient"=>"gradient",
  "grid"=>"grid",
  "four_color_gradient"=>"four_color_gradient",
  "ramp"=>"ramp",
  "circle"=>"circle",
  "cell_pattern"=>"cell_pattern",
  "checkerboard"=>"checkerboard",
  "reduce_interlace_flicker"=>"reduce_interlace_flicker",
  "rgb_split"=>"rgb_split",
  "vignette"=>"vignette",
  "noise_legacy"=>"noise",
  "noise"=>"noise",
  "video_limiter"=>"video_limiter",
  "color_replace"=>"color_replace",
  "rounded_crop"=>"rounded_crop",
  "lens_distortion"=>"lens_distortion",
  "spherize"=>"spherize",
  "wave_warp"=>"wave_warp",
  "twirl_legacy"=>"twirl",
  "twirl"=>"twirl",
  "edge_feather"=>"edge_feather",
  "crop"=>"crop",
  "mirror"=>"mirror",
  "offset"=>"offset",
  "replicate"=>"replicate",
  "mosaic_legacy"=>"mosaic",
  "mosaic"=>"mosaic",
  "unsharp_mask"=>"unsharp_mask",
  "channel_blur"=>"channel_blur",
  "gaussian_blur_legacy"=>"gaussian_blur",
  "directional_blur_legacy"=>"directional_blur",
  "directional_blur"=>"directional_blur",
  "camera_blur"=>"camera_blur",
  "gaussian_blur"=>"gaussian_blur",
  "sharpen"=>"sharpen",
  "median"=>"median",
  "color_emboss"=>"color_emboss",
  "emboss"=>"emboss",
  "find_edges"=>"find_edges",
  "color_key"=>"color_key",
  "cineon_converter"=>"cineon_converter",
  "asc_cdl"=>"asc_cdl",
  "color_pass"=>"color_pass",
  "change_to_color"=>"change_to_color",
  "leave_color"=>"leave_color",
  "alpha_adjust"=>"alpha_adjust",
  "brightness_contrast"=>"brightness_contrast",
  "channel_mix"=>"channel_mix",
  "color_balance"=>"color_balance",
  "extract"=>"color_extract",
  "gamma_correction"=>"gamma_correction",
  "invert"=>"color_invert",
  "levels"=>"color_levels",
  "luma_key"=>"luma_key",
  "posterize"=>"posterize",
  "proc_amp"=>"proc_amp",
  "tint"=>"color_tint",
  "horizontal_flip"|"vertical_flip"|"black_white"=>id,
  _=>anyhow::bail!("source effect is not connected: {id}"),
 };
 let unit=matches!(kind,"horizontal_flip"|"vertical_flip"|"black_white");
 let baseline:VideoEffect=serde_json::from_value(if unit{json!({"kind":kind})}else{json!({"kind":kind,"parameters":{}})})?;
 let mut value=serde_json::to_value(baseline)?;
 for (name,param) in raw["params"].as_object().context("missing source effect params")?{
  let mut parts=name.split('_');let mut key=parts.next().unwrap_or_default().to_owned();for part in parts{let mut chars=part.chars();if let Some(first)=chars.next(){key.extend(first.to_uppercase());key.extend(chars);}}
  let target=if kind=="lighting_effects"&&name.starts_with('l')&&name.as_bytes().get(2)==Some(&b'_'){
   let index=name.as_bytes()[1].checked_sub(b'1').filter(|i|*i<5).context("invalid source light index")? as usize;
   let field=&name[3..];let field=if field=="type"{"kind"}else{field};
   value["parameters"]["lights"][index].get_mut(field)
  }else{value["parameters"].get_mut(&key)}.with_context(||format!("unsupported source control {id}/{name}"))?;
  let point_fallback=match (id,name.as_str()){
   ("lightning","start")=>fallback.map(|v|v*0.2),("lightning","end")=>fallback.map(|v|v*1.8),
   ("volumetric_rays","center")=>[fallback[0],fallback[1]*0.5],
   ("corner_pin","upper_left")=>[0.,0.],("corner_pin","upper_right")=>[fallback[0]*2.,0.],
   ("corner_pin","lower_left")=>[0.,fallback[1]*2.],("corner_pin","lower_right")=>fallback.map(|v|v*2.),
   ("gradient","start")=>[fallback[0],0.],("gradient","end")=>[fallback[0],fallback[1]*2.],
   ("auto_reframe","offset")|("roughen_edges","offset")|("move","from")|("move","to")|("checkerboard","anchor")|("cell_pattern","offset")=>[0.,0.],_=>fallback,
  };
  *target=if kind=="ramp"&&name=="shape"{json!(if param["value"]["Choice"].as_u64().context("invalid ramp shape")?==1{"radial"}else{"linear"})}
  else if let Some(point)=param["value"].get("Vec2"){
   if param.get("keyframes").and_then(Value::as_array).is_none_or(|keys|keys.is_empty()){
    ensure!(point.as_object().is_some_and(|p|p.len()==2&&p.contains_key("x")&&p.contains_key("y")),"invalid source point");
    for axis in ["x","y"]{ensure!(point[axis].is_null()||point[axis].as_f64().is_some_and(f64::is_finite),"invalid source point coordinate");}
    if matches!(kind,"auto_reframe"|"move"|"roughen_edges"){json!([point["x"].as_f64().unwrap_or(0.),point["y"].as_f64().unwrap_or(0.)])}else{json!([point["x"],point["y"]])}
   }else{let curves=super::filmcraft_component_curve::point_with_runtime_fallback(param,point_fallback)?;json!(curves.map(|curve|scalar::sample(&curve,tick)))}
  }else if target.is_string(){let text=param["value"]["Text"].as_str().context("invalid source text")?;ensure!(text.len()<=16384,"source text exceeds layout budget");json!(text)}
  else if target.is_boolean(){json!(param["value"]["Bool"].as_bool().context("invalid source flag")?)}
   else if target.is_array(){let curves=super::filmcraft_component_curve::color(param)?;json!(curves.map(|curve|scalar::sample(&curve,tick)as f32))}
   else{let curve=scalar::convert(param,scalar::Evaluation::EffectParameter)?;let number=scalar::sample(&curve,tick);if target.is_u64(){json!(param["value"]["Choice"].as_u64().context("invalid source choice")?)}else{json!(number)}};
 }
 Ok(serde_json::from_value(value)?)
}
pub(crate) fn apply(context:&RenderContext,raw:&Value,tick:i64,clock:SourceClock,source_width:u32,image:Image)->Result<Image>{
 apply_with_project(context,raw,tick,clock,source_width,image,&Value::Null)
}
pub(super) struct SourceEffectEnvironment<'a>{pub stabilization:&'a std::collections::BTreeMap<usize,std::sync::Arc<super::warp_stabilizer::StabilizerPath>>,pub subjects:&'a std::collections::BTreeMap<usize,(f64,f64)>,pub sequence_size:(u32,u32,u32),pub burnin:&'a super::metadata_burnin::BurninValues,pub echoes:&'a std::collections::BTreeMap<usize,Vec<(Image,f32)>>,pub temporal:&'a std::collections::BTreeMap<usize,Image>,pub tracks:&'a std::collections::BTreeMap<u32,Image>,pub matrix:super::transition_kernel_pack::geom::Affine}
pub(crate) fn apply_with_project(context:&RenderContext,raw:&Value,tick:i64,clock:SourceClock,source_width:u32,image:Image,project:&Value)->Result<Image>{
 apply_with_environment(context,raw,tick,clock,source_width,image,project,None)
}
pub(super) fn apply_with_environment(context:&RenderContext,raw:&Value,tick:i64,clock:SourceClock,source_width:u32,image:Image,project:&Value,environment:Option<&SourceEffectEnvironment<'_>>)->Result<Image>{
 ensure!(source_width>0,"invalid source effect width");
 let px_scale=image.w as f32/source_width as f32;
 context.image_work(&[],||{
  let run=||->Result<Image>{
   let mut extra=0usize;
   for effect in raw.as_array().context("missing source effects")?.iter().filter(|e|e["enabled"]!=false&&e["effect"]=="lumetri"){
    extra=extra.checked_add(super::filmcraft_source_lumetri::scratch(effect,project)?).context("Lumetri scratch overflow")?;
   }
   for raw in raw.as_array().context("missing source effects")?.iter().filter(|e|e["enabled"]!=false&&e["effect"]=="simple_text"){
    let VideoEffect::SimpleText{parameters,..}=compile(raw,tick,[image.w as f64/2./px_scale as f64,image.h as f64/2./px_scale as f64])? else{unreachable!()};
    parameters.validate()?;
    let size=(parameters.size*px_scale).max(2.) as f64;
    let bound=(size*2.+4.).powi(2)*(parameters.text.chars().count()+4)as f64*16.;
    ensure!(bound.is_finite()&&bound<=usize::MAX as f64,"text scratch overflow");
    extra=extra.checked_add(bound.ceil()as usize).context("text scratch overflow")?;
   }
   for raw in raw.as_array().context("missing source effects")?.iter().filter(|e|e["enabled"]!=false&&matches!(e["effect"].as_str(),Some("timecode"|"clip_name"|"metadata_burnin"))){
    let values=environment.context("source label context missing")?.burnin;
    let (size,text)=match compile(raw,tick,[image.w as f64/2./px_scale as f64,image.h as f64/2./px_scale as f64])?{
     VideoEffect::Timecode{parameters,..}=>((parameters.size/100.*image.h as f32*0.5).max(6.),values.sequence_timecode.clone()),
     VideoEffect::ClipName{parameters,..}=>((parameters.size/100.*image.h as f32*0.5).max(6.),values.clip_name.clone()),
     VideoEffect::MetadataBurnin{parameters,..}=>((parameters.size/100.*image.h as f32).max(6.),parameters.text(values.context())?),
     _=>unreachable!(),
    };
    ensure!(text.len()<=16384,"source label exceeds layout budget");
    let bound=(size as f64*2.+4.).powi(2)*(text.chars().count()+4)as f64*16.;
    ensure!(bound.is_finite()&&bound<=usize::MAX as f64,"label scratch overflow");extra=extra.checked_add(bound.ceil()as usize).context("label scratch overflow")?;
   }
   let _compile_scratch=context.reserve_scratch(extra);
 let effects=raw.as_array().context("missing source effects")?.iter().filter(|e|e["enabled"]!=false&&!matches!(e["effect"].as_str(),Some("motion"|"opacity"|"graphic_shape"|"graphic_text"))).map(|e|Ok((if e["effect"]=="lumetri"{VideoEffect::Lumetri{parameters:super::filmcraft_source_lumetri::compile(e,tick,project)?}}else{compile(e,tick,[image.w as f64/2./px_scale as f64,image.h as f64/2./px_scale as f64])?},e))).collect::<Result<Vec<_>>>()?;
 if effects.is_empty(){return Ok(image)}
 let(seconds,progress,span)=clock.values()?;
 let scratch_per_pixel=if effects.iter().any(|(e,_)|matches!(e,VideoEffect::WarpStabilizer{..}|VideoEffect::CompoundBlur{..}|VideoEffect::Lumetri{..}|VideoEffect::LightingEffects{..}|VideoEffect::Lightning{..}|VideoEffect::WriteOn{..}|VideoEffect::BrushStrokes{..}|VideoEffect::EchoGlow{..}|VideoEffect::WonderGlow{..}|VideoEffect::VolumetricRays{..}|VideoEffect::UltraKey{..}|VideoEffect::AlphaGlow{..}|VideoEffect::DropShadow{..}|VideoEffect::BevelAlpha{..}|VideoEffect::LongShadow{..}|VideoEffect::Stroke{..}|VideoEffect::Glint{..}|VideoEffect::EdgeGlow{..}|VideoEffect::BokehBlur{..}|VideoEffect::FocusBlur{..})){256}else if effects.iter().any(|(e,_)|matches!(e,VideoEffect::Move{..}|VideoEffect::CameraShake{..})){180}else{80};
   let _scratch=context.reserve_scratch(image.w.checked_mul(image.h).and_then(|n|n.checked_mul(scratch_per_pixel).and_then(|n|n.checked_add(1024))).context("effect scratch overflow")?);
   let mut checkpoint=||{ensure!(!context.is_cancelled(),"source effect cancelled");Ok(())};
   let mut frame=LinearColorFrame::from_working(image.w,image.h,&image.px,&mut checkpoint)?;
   let scaled_point=|p:[Option<f64>;2]|p.map(|v|v.map(|n|n*px_scale as f64));
   for(effect,raw_effect)in effects{
    let coverage=super::filmcraft_source_masks::coverage(context,&raw_effect["masks"],tick,px_scale,image.w,image.h,&mut checkpoint)?;
    let _original_reservation=coverage.as_ref().map(|_|context.reserve_scratch(image.w*image.h*16));
    let original=coverage.as_ref().map(|_|frame.pixels().to_vec());
    match effect{
    VideoEffect::WarpStabilizer{parameters,..}=>{
     let index=raw.as_array().unwrap().iter().position(|e|std::ptr::eq(e,raw_effect)).context("stabilizer index missing")?;
     if let Some(path)=environment.and_then(|e|e.stabilization.get(&index)){super::warp_stabilizer_render::apply_linear(&parameters,path,seconds,&mut frame,&mut checkpoint)?;}
    },
    VideoEffect::AutoReframe{parameters,..}=>{
     let index=raw.as_array().unwrap().iter().position(|e|std::ptr::eq(e,raw_effect)).context("reframe index missing")?;
     parameters.apply_linear(&mut frame,px_scale,environment.and_then(|e|e.subjects.get(&index).copied()),environment.filter(|e|e.subjects.contains_key(&index)).map(|e|e.sequence_size),&mut checkpoint)?;
    },
    VideoEffect::Timecode{parameters,..}=>parameters.apply_linear(&mut frame,environment.context("timecode context missing")?.burnin.context(),px_scale,&mut checkpoint)?,
    VideoEffect::ClipName{parameters,..}=>parameters.apply_linear(&mut frame,environment.context("clip name context missing")?.burnin.context(),px_scale,&mut checkpoint)?,
    VideoEffect::MetadataBurnin{parameters,..}=>parameters.apply_linear(&mut frame,environment.context("metadata context missing")?.burnin.context(),px_scale,&mut checkpoint)?,
    VideoEffect::SimpleText{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Clone{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::AutoAlign{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Echo{parameters,..}=>{
     let index=raw.as_array().unwrap().iter().position(|e|std::ptr::eq(e,raw_effect)).context("echo effect index missing")?;
     if let Some(images)=environment.and_then(|env|env.echoes.get(&index)){
      let count=images.len().checked_add(1).context("echo layer count overflow")?;
      let _echo=context.reserve_scratch(image.w.checked_mul(image.h).and_then(|n|n.checked_mul(20)).and_then(|n|n.checked_mul(count)).context("echo scratch overflow")?);
      let mut layers=Vec::with_capacity(count);layers.push((frame.clone(),parameters.start));
      for (image,weight) in images{layers.push((LinearColorFrame::from_working(image.w,image.h,&image.px,&mut checkpoint)?,*weight));}
      frame=parameters.combine(&layers,&mut checkpoint)?;
     }
    },
    VideoEffect::PosterizeTime{..}=>{
     let index=raw.as_array().unwrap().iter().position(|e|std::ptr::eq(e,raw_effect)).context("temporal effect index missing")?;
     if let Some(image)=environment.and_then(|env|env.temporal.get(&index)){
      frame=LinearColorFrame::from_working(image.w,image.h,&image.px,&mut checkpoint)?;
     }
    },
    VideoEffect::Lumetri{parameters}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::TrackMatte{parameters}=>{
     let external=environment.and_then(|env|env.tracks.get(&parameters.matte).map(|image|(env,image)));
     if let Some((env,external))=external{
      let _external=context.reserve_scratch(external.w.checked_mul(external.h).and_then(|n|n.checked_mul(20)).context("matte track scratch overflow")?);
      let track=LinearColorFrame::from_working(external.w,external.h,&external.px,&mut checkpoint)?;
      let mapping=|x:f64,y:f64|{let p=env.matrix.apply(super::transition_kernel_pack::geom::Vec2::new(x,y));(p.x,p.y)};
      parameters.apply_linear(&mut frame,Some(super::track_matte::MatteLayer{frame:&track,layer_to_output:&mapping}),&mut checkpoint)?;
     }else{parameters.apply_linear(&mut frame,None,&mut checkpoint)?;}
    },
    VideoEffect::CompoundBlur{parameters,..}=>{
     let external=environment.and_then(|env|env.tracks.get(&parameters.layer).map(|image|(env,image)));
     if let Some((env,external))=external {
      let _external=context.reserve_scratch(external.w.checked_mul(external.h).and_then(|n|n.checked_mul(20)).context("gradient track scratch overflow")?);
      let track=LinearColorFrame::from_working(external.w,external.h,&external.px,&mut checkpoint)?;
      let mapping=|x:f64,y:f64|{let p=env.matrix.apply(super::transition_kernel_pack::geom::Vec2::new(x,y));(p.x,p.y)};
      parameters.apply_linear(&mut frame,Some(super::compound_blur::CompoundLayer{frame:&track,layer_to_output:&mapping}),px_scale,&mut checkpoint)?;
     }else{parameters.apply_linear(&mut frame,None,px_scale,&mut checkpoint)?;}
    },
    VideoEffect::GradientWipe{parameters,..}=>{
     let external=environment.and_then(|env|env.tracks.get(&parameters.layer).map(|image|(env,image)));
     if let Some((env,external))=external {
      let _external=context.reserve_scratch(external.w.checked_mul(external.h).and_then(|n|n.checked_mul(20)).context("gradient track scratch overflow")?);
      let track=LinearColorFrame::from_working(external.w,external.h,&external.px,&mut checkpoint)?;
      let mapping=|x:f64,y:f64|{let p=env.matrix.apply(super::transition_kernel_pack::geom::Vec2::new(x,y));(p.x,p.y)};
      parameters.apply_linear(&mut frame,Some(super::gradient_wipe::GradientLayer{frame:&track,layer_to_output:&mapping}),&mut checkpoint)?;
     }else{parameters.apply_linear(&mut frame,None,&mut checkpoint)?;}
    },
    VideoEffect::LinearWipe{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::LightingEffects{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Lightning{parameters,..}=>parameters.apply_linear(&mut frame,seconds,px_scale,&mut checkpoint)?,
    VideoEffect::LogoCutout{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::WriteOn{parameters,..}=>{
     let curves=super::filmcraft_component_curve::point_with_runtime_fallback(&raw_effect["params"]["brush"],[image.w as f64/2./px_scale as f64,image.h as f64/2./px_scale as f64])?;
     let mut history=|at:filmcraft_time::Tick|Ok(curves.each_ref().map(|curve|Some(scalar::sample(curve,at.0))));
     parameters.apply_linear(&mut frame,filmcraft_time::Tick(tick),seconds,px_scale,&mut history,&mut checkpoint)?;
    },
    VideoEffect::BrushStrokes{mut parameters,..}=>{parameters.size*=px_scale;parameters.length*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::EchoGlow{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::WonderGlow{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::VolumetricRays{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::LensFlare{mut parameters,..}=>{parameters.center=scaled_point(parameters.center);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::UltraKey{parameters,..}=>parameters.apply_linear_scaled(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::AlphaGlow{mut parameters,..}=>{parameters.glow*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::DropShadow{parameters,..}=>parameters.apply_linear_scaled(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::BevelAlpha{mut parameters,..}=>{parameters.thickness*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::LongShadow{mut parameters,..}=>{parameters.length*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Stroke{mut parameters,..}=>{parameters.width*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::BlockDissolve{mut parameters,..}=>{parameters.block_w*=px_scale;parameters.block_h*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Ellipse{mut parameters,..}=>{parameters.center=scaled_point(parameters.center);parameters.width*=px_scale;parameters.height*=px_scale;parameters.thickness*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::PaintBucket{mut parameters,..}=>{parameters.point=scaled_point(parameters.point);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Glint{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::EdgeGlow{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::BokehBlur{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::FocusBlur{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::RoughenEdges{parameters,..}=>parameters.apply_linear_scaled(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Strobe{parameters,..}=>parameters.apply_linear_at_seconds(&mut frame,seconds,&mut checkpoint)?,
    VideoEffect::LightLeaks{parameters,..}=>parameters.apply_linear_at_seconds(&mut frame,seconds,&mut checkpoint)?,
    VideoEffect::VrGlow{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::VrColorGradients{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::VrFractalNoise{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::VrDigitalGlitch{parameters,..}=>parameters.apply_linear_at_seconds(&mut frame,seconds,&mut checkpoint)?,
    VideoEffect::VrPlaneToSphere{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::VrProjection{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::VrRotateSphere{parameters,..}=>parameters.projection().apply_linear(&mut frame,&mut checkpoint)?,

    VideoEffect::VrBlur{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::VrSharpen{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::VrDenoise{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::VrChromaticAberrations{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,

    VideoEffect::Magnify{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Rotate3D{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Wiggle{parameters,..}=>parameters.apply_linear(&mut frame,seconds,px_scale,&mut checkpoint)?,
    VideoEffect::CameraShake{parameters,..}=>parameters.apply_linear(&mut frame,seconds,clock.frame_rate,px_scale,&mut checkpoint)?,
    VideoEffect::TurbulentDisplace{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::CornerPin{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Basic3D{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::Grow{parameters,..}=>parameters.apply_linear(&mut frame,progress,px_scale,&mut checkpoint)?,
    VideoEffect::Shrink{parameters,..}=>super::grow_shrink::GrowShrink::from(parameters).apply_linear(&mut frame,progress,px_scale,&mut checkpoint)?,
    VideoEffect::Spin{parameters,..}=>parameters.apply_linear(&mut frame,progress,px_scale,&mut checkpoint)?,
    VideoEffect::Move{parameters,..}=>parameters.apply_linear(&mut frame,progress,span,px_scale,&mut checkpoint)?,
    VideoEffect::Spacer{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Transform{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Checkerboard{mut parameters,..}=>{parameters.anchor=scaled_point(parameters.anchor);parameters.width*=px_scale;parameters.height*=px_scale;parameters.feather*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::CellPattern{mut parameters,..}=>{parameters.offset=scaled_point(parameters.offset);parameters.size*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Circle{mut parameters,..}=>{parameters.center=scaled_point(parameters.center);parameters.radius*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Ramp{mut parameters,..}=>{parameters.start=scaled_point(parameters.start);parameters.end=scaled_point(parameters.end);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::FourColorGradient{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::Grid{mut parameters,..}=>{parameters.size=(parameters.size*px_scale).max(1.);parameters.border*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Gradient{mut parameters,..}=>{parameters.start=scaled_point(parameters.start);parameters.end=scaled_point(parameters.end);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::ColorReplace{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::VideoLimiter{parameters,..}=>frame.video_limiter(&parameters,&mut checkpoint)?,
    VideoEffect::Noise{mut parameters,..}=>{parameters.grain_size=(parameters.grain_size.max(1.)*px_scale).max(1.);parameters.apply_linear_at_seconds(&mut frame,seconds,&mut checkpoint)?;},
    VideoEffect::VignetteEffect{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::RgbSplit{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::ReduceInterlaceFlicker{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Twirl{mut parameters,..}=>{parameters.center=parameters.center.map(|p|p.map(|v|v.map(|n|n*px_scale as f64)));parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::WaveWarp{parameters,..}=>parameters.apply_linear_at_seconds_scaled(&mut frame,seconds,px_scale,&mut checkpoint)?,
    VideoEffect::Spherize{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::LensDistortion{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::RoundedCrop{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Offset{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Mirror{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Crop{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::EdgeFeather{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::CameraBlur{mut parameters,..}=>{parameters.percent*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::DirectionalBlur{mut parameters,..}=>{parameters.length*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::ChannelBlur{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::UnsharpMask{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::Mosaic{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::Replicate{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorKey{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::FindEdges{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::Emboss{mut parameters,..}=>{parameters.relief*=px_scale.max(0.25);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::ColorEmboss{mut parameters,..}=>{parameters.relief*=px_scale.max(0.25);parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Median{mut parameters,..}=>{parameters.radius*=px_scale;parameters.apply_linear(&mut frame,&mut checkpoint)?;},
    VideoEffect::Sharpen{parameters,..}=>parameters.apply_linear(&mut frame,px_scale,&mut checkpoint)?,
    VideoEffect::GaussianBlur{parameters,..}=>{parameters.validate()?;parameters.apply_sigma(&mut frame,parameters.blurriness*0.5*px_scale,&mut checkpoint)?;},

    VideoEffect::CineonConverter{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::AscCdl{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorPass{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ChangeToColor{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::LeaveColor{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::AlphaAdjust{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::BrightnessContrast{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ChannelMix{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorBalance{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorExtract{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::GammaCorrection{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorInvert{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorLevels{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::LumaKey{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::Posterize{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ProcAmp{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::ColorTint{parameters,..}=>parameters.apply_linear(&mut frame,&mut checkpoint)?,
    VideoEffect::NativeBypass{..}|VideoEffect::ConcatBypass{..}=>{},
    VideoEffect::HorizontalFlip=>super::flip::apply_linear(&mut frame,true,&mut checkpoint)?,
    VideoEffect::VerticalFlip=>super::flip::apply_linear(&mut frame,false,&mut checkpoint)?,
    VideoEffect::BlackWhite=>super::black_white::apply_linear(&mut frame,&mut checkpoint)?,
    _=>anyhow::bail!("unconnected source effect dispatch"),
   }
    if let(Some(cov),Some(original))=(coverage,original){let mut i=0;frame.map_premultiplied(&mut|p|{let o=original[i];let c=cov.values[i];i+=1;std::array::from_fn(|k|o[k]+(p[k]-o[k])*c)},&mut checkpoint)?;}
   }
   let mut image=image;for (row,pixels) in image.px.chunks_mut(image.w*4).zip(frame.pixels().chunks(image.w)){checkpoint()?;for(out,pixel)in row.chunks_exact_mut(4).zip(pixels){out.copy_from_slice(pixel);}}
   Ok(image)
  };run().map_err(|e|e.to_string())
 }).map_err(anyhow::Error::msg)
}

#[cfg(test)]mod tests{
 use super::*;use std::sync::{Arc,atomic::AtomicBool};
 #[test]fn source_effect_scratch_refusal_and_cancel_release(){
  for id in ["horizontal_flip","alpha_glow","lumetri"]{
  let raw=json!([{"effect":id,"enabled":true,"params":{}}]);
  for cancel in [false,true]{
   let flag=Arc::new(AtomicBool::new(false));let context=RenderContext::new_with_budget(flag.clone(),1,128,if cancel{8192}else{128*16+1}).unwrap();
   let image=context.image_work(&[],||Ok(Image::new(16,8))).unwrap();
   if cancel{flag.store(true,std::sync::atomic::Ordering::Relaxed);}
   assert!(apply(&context,&raw,0,SourceClock{local_ticks:0,duration_ticks:254016000000,frame_rate:24.},16,image).is_err());assert_eq!(context.pixel_bytes_in_use(),0);
  }}
 }
 #[test]fn source_effect_unknown_controls_are_refused(){
  for raw in [json!({"effect":"unknown","params":{}}),json!({"effect":"brightness_contrast","params":{"unknown":{"value":{"Float":0.}}}})]{assert!(compile(&raw,0,[8.,4.]).is_err());}
 }
}
