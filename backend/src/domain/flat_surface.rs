//! Native generated-color layers for nested previews without intermediate movies.
use super::{
    animated_color::{ColorSampleRequest, PreparedColorTrack},
    composition::{BlendMode, CanvasBackgroundMode, Composition, CompositionTrack},
    composition_graph::CompositionReferenceTiming,
    group_surface::GroupSurface,
    group_transform::GroupAffine,
    nested_composition::CompositionInstance,
};
use anyhow::{ensure, Result};

fn composite_source_picture(transform:&super::composition::TransformSpec,child:&GroupSurface,surface:&mut GroupSurface,pose:&super::nested_composition::NestedGroupPose,blend:BlendMode,prepared:bool,checkpoint:&mut dyn FnMut()->Result<()>)->Result<bool>{
 let Some(mut preparation)=super::concat_picture::source_preparation(transform)?else{return Ok(false)};
 if prepared{preparation.crop=[0.;4];preparation.flip_h=false;preparation.flip_v=false;}
 preparation.composite_reference(child,surface,super::concat_clip_animation::Pose{scale_x:pose.scale_x,scale_y:pose.scale_y,x:pose.x,y:pose.y,rotation_degrees:pose.rotation_degrees,opacity:pose.opacity},blend,checkpoint)?;Ok(true)
}

fn clocked_pose(transform:&super::composition::TransformSpec,opacity:&super::composition::AnimatableValue)->bool {
    let clocked=|value:&super::composition::AnimatableValue|matches!(value,super::composition::AnimatableValue::Keyframes{track} if track.sample_clock.is_some());
    if transform.position.as_ref().is_some_and(|position|match position {
        super::motion_path::PositionAnimation::Xy{x,y}=>clocked(x)||clocked(y),
        super::motion_path::PositionAnimation::Path{progress,..}=>clocked(progress),
    }){return true}
    [&transform.x,&transform.y,&transform.scale_x,&transform.scale_y,&transform.rotation_degrees,opacity].into_iter().any(|value|matches!(value,super::composition::AnimatableValue::Keyframes{track} if track.sample_clock.is_some()))
}
pub fn requires_native_video(composition: &Composition) -> bool {
    if composition.tracks.iter().any(|track| matches!(track,CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.placement.concat_source_eof))) {return true}
    if composition.tracks.iter().any(|track|match track {
        CompositionTrack::Video{clips,..}=>clips.iter().any(|clip|clocked_pose(&clip.transform,&clip.opacity)),
        CompositionTrack::Image{clips,..}=>clips.iter().any(|clip|clocked_pose(&clip.transform,&clip.opacity)),
        CompositionTrack::Text{clips,..}=>clips.iter().any(|clip|clocked_pose(&clip.transform,&clip.opacity)),
        CompositionTrack::Color{clips,..}=>clips.iter().any(|clip|clocked_pose(&clip.transform,&clip.opacity)),
        CompositionTrack::Graphic{clips,..}=>clips.iter().any(|clip|clocked_pose(&clip.transform,&clip.opacity)),
        _=>false,
    }){return true}
    if composition.metadata.as_ref().is_some_and(|metadata|!metadata.concat_lane_groups.is_empty()){return true}
    if composition.tracks.iter().any(|track|matches!(track,CompositionTrack::Video{clips,..} if clips.iter().any(|clip|clip.enabled&&(clip.concat_cutout.is_some()||clip.placement.concat_retime.is_some()||clip.placement.concat_source_eof)))){return true}
    if composition.tracks.iter().any(|track|match track {
        CompositionTrack::Video{clips,..}=>clips.iter().any(|clip|(clip.transform.concat_animation.is_some()||clip.transform.concat_preparation.is_some())),
        CompositionTrack::Image{clips,..}=>clips.iter().any(|clip|(clip.transform.concat_animation.is_some()||clip.transform.concat_preparation.is_some())),
        _=>false,
    }){return true}

    if super::transition_export::has_independent_windows(composition) {return true}
    if super::transition_export::sparse_primary_video_track(composition).is_some() {return true}
    if composition.caption_tracks.iter().any(|track|!track.hidden&&!track.clips.is_empty()) {return true}
    composition.tracks.iter().any(|track| match track {
        CompositionTrack::Graphic {hidden:false,clips,..} => clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Text {hidden:false,clips,..} => clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Video {
            hidden: false,
            clips,
            ..
        } => clips.iter().any(|clip| {
            clip.enabled
                && ((clip.effect_timing.is_some() && !clip.effects.is_empty()) || clip.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        super::composition::VideoEffect::NativeBypass { .. } | super::composition::VideoEffect::ConcatBypass { .. } | super::composition::VideoEffect::ChromaMatte { .. } | super::composition::VideoEffect::ProcAmp { .. } | super::composition::VideoEffect::GammaCorrection { .. } | super::composition::VideoEffect::Posterize { .. } | super::composition::VideoEffect::ColorInvert { .. } | super::composition::VideoEffect::ColorReplace { .. } | super::composition::VideoEffect::UltraKey { .. } | super::composition::VideoEffect::ColorKey { .. } | super::composition::VideoEffect::ColorPass { .. } | super::composition::VideoEffect::ChangeToColor { .. } | super::composition::VideoEffect::LeaveColor { .. } | super::composition::VideoEffect::AlphaAdjust { .. } | super::composition::VideoEffect::Emboss { .. } | super::composition::VideoEffect::Noise { .. } | super::composition::VideoEffect::NoiseLegacy { .. } | super::composition::VideoEffect::Strobe { .. } | super::composition::VideoEffect::Twirl { .. } | super::composition::VideoEffect::TwirlLegacy { .. } | super::composition::VideoEffect::WaveWarp { .. } | super::composition::VideoEffect::LensDistortion { .. } | super::composition::VideoEffect::Grid { .. } | super::composition::VideoEffect::LensFlare { .. } | super::composition::VideoEffect::Ellipse { .. } | super::composition::VideoEffect::PaintBucket { .. } | super::composition::VideoEffect::Gradient { .. } | super::composition::VideoEffect::BrushStrokes { .. } | super::composition::VideoEffect::AlphaGlow { .. } | super::composition::VideoEffect::BlockDissolve { .. } | super::composition::VideoEffect::GradientWipe { .. } | super::composition::VideoEffect::LinearWipe { .. } | super::composition::VideoEffect::Magnify { .. } | super::composition::VideoEffect::Mirror { .. } | super::composition::VideoEffect::Offset { .. } | super::composition::VideoEffect::Transform { .. } | super::composition::VideoEffect::AscCdl { .. } | super::composition::VideoEffect::VignetteEffect { .. } | super::composition::VideoEffect::LightingEffects{..} | super::composition::VideoEffect::LogoCutout{..} | super::composition::VideoEffect::EdgeGlow{..} | super::composition::VideoEffect::WonderGlow{..} | super::composition::VideoEffect::Glint{..} | super::composition::VideoEffect::PosterizeTime{..}|super::composition::VideoEffect::Echo{..}|super::composition::VideoEffect::AutoReframe{..}|super::composition::VideoEffect::VrDigitalGlitch{..}|super::composition::VideoEffect::VrFractalNoise{..}|super::composition::VideoEffect::VrGlow{..}|super::composition::VideoEffect::Lightning{..}|super::composition::VideoEffect::WriteOn{..}|super::composition::VideoEffect::Timecode{..}|super::composition::VideoEffect::ClipName{..}|super::composition::VideoEffect::MetadataBurnin{..}|super::composition::VideoEffect::SimpleText{..}|super::composition::VideoEffect::VrPlaneToSphere{..}|super::composition::VideoEffect::VrColorGradients{..}|super::composition::VideoEffect::VrChromaticAberrations{..}|super::composition::VideoEffect::VrDenoise{..}|super::composition::VideoEffect::VrBlur{..}|super::composition::VideoEffect::VrSharpen{..}|super::composition::VideoEffect::SphericalVideo{..}|super::composition::VideoEffect::VrProjection{..}|super::composition::VideoEffect::VrRotateSphere{..}|super::composition::VideoEffect::LightLeaks{..}|super::composition::VideoEffect::WarpStabilizer{..} | super::composition::VideoEffect::VolumetricRays{..} | super::composition::VideoEffect::EchoGlow{..} | super::composition::VideoEffect::RgbSplit{..} | super::composition::VideoEffect::Spherize { .. } | super::composition::VideoEffect::CornerPin { .. } | super::composition::VideoEffect::Rotate3D { .. } | super::composition::VideoEffect::Grow { .. } | super::composition::VideoEffect::Spin { .. } | super::composition::VideoEffect::Move { .. } | super::composition::VideoEffect::Wiggle { .. } | super::composition::VideoEffect::CameraShake { .. } | super::composition::VideoEffect::Spacer { .. } | super::composition::VideoEffect::RoundedCrop { .. } | super::composition::VideoEffect::Clone { .. } | super::composition::VideoEffect::AutoAlign { .. } | super::composition::VideoEffect::GraphicText { .. } | super::composition::VideoEffect::GraphicShape { .. } | super::composition::VideoEffect::TrackMatte { .. } | super::composition::VideoEffect::CompoundBlur { .. } |super::composition::VideoEffect::Crop { .. } | super::composition::VideoEffect::BrightnessContrast { .. } | super::composition::VideoEffect::UnsharpMask { .. } | super::composition::VideoEffect::BokehBlur { .. } |super::composition::VideoEffect::FocusBlur { .. } | super::composition::VideoEffect::EdgeFeather { .. } | super::composition::VideoEffect::Sharpen { .. } | super::composition::VideoEffect::ReduceInterlaceFlicker { .. } | super::composition::VideoEffect::ChannelBlur { .. } | super::composition::VideoEffect::TurbulentDisplace { .. } | super::composition::VideoEffect::Shrink { .. } | super::composition::VideoEffect::ColorEmboss { .. } | super::composition::VideoEffect::RoughenEdges { .. } | super::composition::VideoEffect::LongShadow { .. } | super::composition::VideoEffect::Stroke { .. } | super::composition::VideoEffect::CellPattern { .. } | super::composition::VideoEffect::Checkerboard { .. } | super::composition::VideoEffect::Circle { .. } | super::composition::VideoEffect::Ramp { .. } | super::composition::VideoEffect::FourColorGradient { .. } | super::composition::VideoEffect::BevelAlpha { .. } | super::composition::VideoEffect::DropShadow { .. } | super::composition::VideoEffect::Basic3D { .. } | super::composition::VideoEffect::Mosaic { .. } | super::composition::VideoEffect::MosaicLegacy { .. } | super::composition::VideoEffect::DirectionalBlur { .. } | super::composition::VideoEffect::DirectionalBlurLegacy { .. } | super::composition::VideoEffect::FindEdges { .. } | super::composition::VideoEffect::Replicate { .. } | super::composition::VideoEffect::Median { .. } | super::composition::VideoEffect::CameraBlur { .. } | super::composition::VideoEffect::GaussianBlur { .. } | super::composition::VideoEffect::GaussianBlurLegacy { .. } | super::composition::VideoEffect::LumaKey { .. } | super::composition::VideoEffect::ChannelMix { .. } | super::composition::VideoEffect::ColorBalance { .. } | super::composition::VideoEffect::ColorTint { .. } | super::composition::VideoEffect::ColorLevels { .. } | super::composition::VideoEffect::ColorExtract { .. } | super::composition::VideoEffect::HorizontalFlip | super::composition::VideoEffect::VerticalFlip | super::composition::VideoEffect::BlackWhite | super::composition::VideoEffect::CineonConverter { .. }
                            | super::composition::VideoEffect::VideoLimiter { .. }
                            | super::composition::VideoEffect::BasicGrade { .. }
                            | super::composition::VideoEffect::CreativeAdjust { .. }
                            | super::composition::VideoEffect::ConcatGaussianBlur { .. } | super::composition::VideoEffect::ConcatBloomPulse { .. } | super::composition::VideoEffect::ConcatMotionBlur { .. } | super::composition::VideoEffect::ConcatZoomBlur { .. } | super::composition::VideoEffect::ConcatFilmGrain { .. } | super::composition::VideoEffect::ConcatGlow { .. } | super::composition::VideoEffect::ConcatHalftone { .. } | super::composition::VideoEffect::ConcatFisheye { .. } | super::composition::VideoEffect::ConcatChromaticAberration { .. } | super::composition::VideoEffect::ConcatPosterize { .. } | super::composition::VideoEffect::ConcatLumaKey { .. } | super::composition::VideoEffect::ConcatPixelate { .. } | super::composition::VideoEffect::ConcatChromaKey { .. } | super::composition::VideoEffect::ConcatSaturation { .. } | super::composition::VideoEffect::ConcatVibrance { .. } | super::composition::VideoEffect::ConcatSharpen { .. } | super::composition::VideoEffect::ConcatVhs { .. } | super::composition::VideoEffect::ConcatFallingDust { .. } | super::composition::VideoEffect::ConcatFilm { .. } | super::composition::VideoEffect::ConcatDigicam { .. } | super::composition::VideoEffect::ConcatMono { .. } | super::composition::VideoEffect::ConcatMidnight { .. } | super::composition::VideoEffect::ConcatMarquee { .. } | super::composition::VideoEffect::ConcatRetroCartoon { .. } | super::composition::VideoEffect::ConcatSuper8 { .. } | super::composition::VideoEffect::ConcatBleachBypass { .. } | super::composition::VideoEffect::ConcatAutumn { .. } | super::composition::VideoEffect::ConcatNeon { .. } | super::composition::VideoEffect::ConcatPinkFuzz { .. } | super::composition::VideoEffect::ConcatNight { .. } | super::composition::VideoEffect::ConcatSpooky { .. } | super::composition::VideoEffect::ConcatCool { .. } | super::composition::VideoEffect::ConcatWarm { .. } | super::composition::VideoEffect::ConcatTealOrange { .. } | super::composition::VideoEffect::ConcatNoir { .. } | super::composition::VideoEffect::ConcatDreamy { .. } | super::composition::VideoEffect::ConcatHighContrastMono { .. } | super::composition::VideoEffect::ConcatLandscapePop { .. } | super::composition::VideoEffect::ConcatNatural { .. } | super::composition::VideoEffect::ConcatRetro70s { .. } | super::composition::VideoEffect::ConcatSilver { .. } | super::composition::VideoEffect::ConcatTropical { .. } | super::composition::VideoEffect::ConcatVaporwave { .. } | super::composition::VideoEffect::ConcatWinterCool { .. } | super::composition::VideoEffect::ConcatClassicChrome { .. } | super::composition::VideoEffect::ConcatCyberpunk { .. } | super::composition::VideoEffect::ConcatDelicious { .. } | super::composition::VideoEffect::ConcatRosy { .. } | super::composition::VideoEffect::ConcatSciFi { .. } | super::composition::VideoEffect::ConcatMatcha { .. } | super::composition::VideoEffect::ConcatSage { .. } | super::composition::VideoEffect::ConcatIvory { .. } | super::composition::VideoEffect::ConcatMilk { .. } | super::composition::VideoEffect::ConcatBronze { .. } | super::composition::VideoEffect::ConcatCoolBlue { .. } | super::composition::VideoEffect::ConcatFresh { .. } | super::composition::VideoEffect::ConcatVivid { .. } | super::composition::VideoEffect::ConcatSoftLight { .. } | super::composition::VideoEffect::ConcatMoody { .. } | super::composition::VideoEffect::ConcatRain { .. } | super::composition::VideoEffect::ConcatUmber { .. } | super::composition::VideoEffect::ConcatFair { .. } | super::composition::VideoEffect::ConcatPeach { .. } | super::composition::VideoEffect::ConcatPolaroid { .. } | super::composition::VideoEffect::ConcatVintage { .. } | super::composition::VideoEffect::ConcatCloudlessSky { .. } | super::composition::VideoEffect::ConcatLagoon { .. } | super::composition::VideoEffect::ConcatGoldenHour { .. } | super::composition::VideoEffect::ConcatDaydream { .. } | super::composition::VideoEffect::ConcatClean { .. } | super::composition::VideoEffect::ConcatContrastPop { .. } | super::composition::VideoEffect::ConcatVignette { .. } | super::composition::VideoEffect::ConcatAdjust { .. } | super::composition::VideoEffect::ConcatCurves { .. } | super::composition::VideoEffect::ConcatColorWheels { .. } | super::composition::VideoEffect::ConcatWhiteBalance { .. } | super::composition::VideoEffect::ConcatExposure { .. } | super::composition::VideoEffect::ConcatContrast { .. } | super::composition::VideoEffect::ColorSharpen { .. }
                            | super::composition::VideoEffect::ColorVignette { .. }
                            | super::composition::VideoEffect::ColorLut { .. }
                            | super::composition::VideoEffect::CreativeGrade { .. }
                            | super::composition::VideoEffect::ToneCurves { .. }
                            | super::composition::VideoEffect::HslSecondary { .. }
                            | super::composition::VideoEffect::Lumetri { .. }
                    )
                }))
        }),
        CompositionTrack::Image {hidden:false,clips,..} => clips.iter().any(|clip|clip.enabled && !clip.effects.is_empty()),
        _ => false,
    })
}

/// Context reads a separately rendered track at output size, preserving working RGBA.
pub trait EffectTrackContext {
    /// Absolute composition time, distinct from the clip-local generator clock.
    fn timeline_seconds(&self)->Option<f64>{None}
    fn burnin_values(&self,_local_us:u64)->Result<Option<super::metadata_burnin::BurninValues>>{Ok(None)}
    fn stabilizer_path(&mut self,_parameters:&super::warp_stabilizer::WarpStabilizer,_checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::warp_stabilizer::StabilizerPath>>{Ok(None)}
    fn auto_reframe_environment(&mut self,_preset:u32,_checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::auto_reframe::AutoReframeEnvironment>>{Ok(None)}
    fn temporal_frame(&mut self,_dt:f64,_prefix:&[super::composition::VideoEffect],_checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>{Ok(None)}
    /// Duration of the active authored clip in timeline microseconds.
    fn clip_duration_us(&self)->Option<u64>{None}
    fn frame_rate(&self)->Option<f64>{None}
    fn motion_blur_span(&self)->Option<f64>{let fps=self.frame_rate()?;let duration=self.clip_duration_us()? as f64/1000000.;if !fps.is_finite()||fps<=0.{return None;}Some(1./(fps*duration).max(1.))}
    fn clip_progress(&self,local_us:u64)->Option<f64>{self.clip_duration_us().map(|duration|if duration==0{0.}else{(local_us as f64/duration as f64).clamp(0.,1.)})}
    /// Filmcraft clip progress uses generator seconds, including signed preroll.
    fn clip_progress_seconds(&self, seconds:f64)->Option<f64>{
        if !seconds.is_finite(){return None;}
        self.clip_duration_us().map(|duration|{
            let duration=duration as f64/1_000_000.0;
            if duration<=1e-9{0.0}else{(seconds/duration).clamp(0.0,1.0)}
        })
    }
    fn track(&mut self,index:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>;
    fn layer_to_output(&self,x:f64,y:f64)->(f64,f64);
}
pub fn apply_effect_stack(child:&mut GroupSurface, effects:&[super::composition::VideoEffect], local:u64, library:&[super::project_luts::ProjectLut], checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
    apply_effect_stack_with_context(child,effects,local,library,None,checkpoint)
}
pub fn apply_effect_stack_with_context(child:&mut GroupSurface,effects:&[super::composition::VideoEffect],local:u64,library:&[super::project_luts::ProjectLut],context:Option<&mut dyn EffectTrackContext>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
    apply_effect_stack_retained(child,effects,local,library,context,checkpoint).map(|_|())
}

pub(crate) fn apply_effect_stack_working(frame:&mut super::linear_color_frame::LinearColorFrame,effects:&[super::composition::VideoEffect],local:u64,library:&[super::project_luts::ProjectLut],context:Option<&mut dyn EffectTrackContext>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 let mut auxiliary=frame.to_surface(checkpoint)?;
 let rendered=apply_effect_stack_seeded(&mut auxiliary,effects,local,library,context,checkpoint,Some(frame.clone()))?;
 *frame=rendered;Ok(())
}

fn apply_effect_stack_retained(child:&mut GroupSurface,effects:&[super::composition::VideoEffect],local:u64,library:&[super::project_luts::ProjectLut],context:Option<&mut dyn EffectTrackContext>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<super::linear_color_frame::LinearColorFrame> {
apply_effect_stack_seeded(child,effects,local,library,context,checkpoint,None)
}
fn apply_effect_stack_seeded(child:&mut GroupSurface,effects:&[super::composition::VideoEffect],local:u64,library:&[super::project_luts::ProjectLut],context:Option<&mut dyn EffectTrackContext>,checkpoint:&mut dyn FnMut()->Result<()>,seed:Option<super::linear_color_frame::LinearColorFrame>)->Result<super::linear_color_frame::LinearColorFrame> {
    apply_effect_stack_seeded_at_time(child,effects,local,local.into(),local as f64 / 1_000_000.0,library,context,checkpoint,seed)
}
fn apply_effect_stack_seeded_at_time(child:&mut GroupSurface,effects:&[super::composition::VideoEffect],local:u64,parameter_time:super::animation_sample_time::AnimationSampleTime,generator_seconds:f64,library:&[super::project_luts::ProjectLut],mut context:Option<&mut dyn EffectTrackContext>,checkpoint:&mut dyn FnMut()->Result<()>,seed:Option<super::linear_color_frame::LinearColorFrame>)->Result<super::linear_color_frame::LinearColorFrame> {
                let mut linear_frame:Option<super::linear_color_frame::LinearColorFrame>=seed;
                for (effect_index,effect) in effects.iter().enumerate() {
                    if matches!(effect,super::composition::VideoEffect::NativeBypass{..}){checkpoint()?;effect.validate().map_err(anyhow::Error::msg)?;continue;}
                    if let super::composition::VideoEffect::ConcatBypass{link}=effect{checkpoint()?;super::concat_effect_upgrade::validate_bypassed(link)?;continue;}
                    if matches!(effect,super::composition::VideoEffect::ProcAmp{..}|super::composition::VideoEffect::GammaCorrection{..}|super::composition::VideoEffect::Posterize{..}|super::composition::VideoEffect::ColorInvert{..}|super::composition::VideoEffect::ColorReplace{..}|super::composition::VideoEffect::UltraKey{..}|super::composition::VideoEffect::ColorKey{..}|super::composition::VideoEffect::ColorPass{..}|super::composition::VideoEffect::ChangeToColor{..}|super::composition::VideoEffect::LeaveColor{..}|super::composition::VideoEffect::AlphaAdjust{..}|super::composition::VideoEffect::Emboss{..}|super::composition::VideoEffect::Noise{..} | super::composition::VideoEffect::NoiseLegacy{..}|super::composition::VideoEffect::Strobe{..}|super::composition::VideoEffect::Twirl{..} | super::composition::VideoEffect::TwirlLegacy{..}|super::composition::VideoEffect::WaveWarp{..}|super::composition::VideoEffect::LensDistortion{..}|super::composition::VideoEffect::Grid{..}|super::composition::VideoEffect::LensFlare{..}|super::composition::VideoEffect::Ellipse{..}|super::composition::VideoEffect::PaintBucket{..}|super::composition::VideoEffect::Gradient{..}|super::composition::VideoEffect::BrushStrokes{..}|super::composition::VideoEffect::AlphaGlow{..}|super::composition::VideoEffect::BlockDissolve{..}|super::composition::VideoEffect::GradientWipe{..}|super::composition::VideoEffect::LinearWipe{..}|super::composition::VideoEffect::Magnify{..}|super::composition::VideoEffect::Mirror{..}|super::composition::VideoEffect::Offset{..} | super::composition::VideoEffect::Transform { .. } | super::composition::VideoEffect::AscCdl { .. } | super::composition::VideoEffect::VignetteEffect { .. } | super::composition::VideoEffect::LightingEffects{..} | super::composition::VideoEffect::LogoCutout{..} | super::composition::VideoEffect::EdgeGlow{..} | super::composition::VideoEffect::WonderGlow{..} | super::composition::VideoEffect::Glint{..} | super::composition::VideoEffect::PosterizeTime{..}|super::composition::VideoEffect::Echo{..}|super::composition::VideoEffect::AutoReframe{..}|super::composition::VideoEffect::VrDigitalGlitch{..}|super::composition::VideoEffect::VrFractalNoise{..}|super::composition::VideoEffect::VrGlow{..}|super::composition::VideoEffect::Lightning{..}|super::composition::VideoEffect::WriteOn{..}|super::composition::VideoEffect::Timecode{..}|super::composition::VideoEffect::ClipName{..}|super::composition::VideoEffect::MetadataBurnin{..}|super::composition::VideoEffect::SimpleText{..}|super::composition::VideoEffect::VrPlaneToSphere{..}|super::composition::VideoEffect::VrColorGradients{..}|super::composition::VideoEffect::VrChromaticAberrations{..}|super::composition::VideoEffect::VrDenoise{..}|super::composition::VideoEffect::VrBlur{..}|super::composition::VideoEffect::VrSharpen{..}|super::composition::VideoEffect::SphericalVideo{..}|super::composition::VideoEffect::VrProjection{..}|super::composition::VideoEffect::VrRotateSphere{..}|super::composition::VideoEffect::LightLeaks{..}|super::composition::VideoEffect::WarpStabilizer{..} | super::composition::VideoEffect::VolumetricRays{..} | super::composition::VideoEffect::EchoGlow{..} | super::composition::VideoEffect::RgbSplit{..} | super::composition::VideoEffect::Spherize{..}|super::composition::VideoEffect::CornerPin{..}|super::composition::VideoEffect::Rotate3D{..}|super::composition::VideoEffect::Grow{..}|super::composition::VideoEffect::Spin{..}|super::composition::VideoEffect::Move{..}|super::composition::VideoEffect::Wiggle{..}|super::composition::VideoEffect::CameraShake{..}|super::composition::VideoEffect::Spacer{..}|super::composition::VideoEffect::RoundedCrop{..}|super::composition::VideoEffect::Clone{..}|super::composition::VideoEffect::AutoAlign{..}|super::composition::VideoEffect::GraphicText { .. } | super::composition::VideoEffect::GraphicShape { .. } | super::composition::VideoEffect::TrackMatte { .. } | super::composition::VideoEffect::CompoundBlur{..}|super::composition::VideoEffect::Crop{..} | super::composition::VideoEffect::BrightnessContrast{..} | super::composition::VideoEffect::UnsharpMask{..} | super::composition::VideoEffect::BokehBlur{..}|super::composition::VideoEffect::FocusBlur{..}|super::composition::VideoEffect::EdgeFeather{..} | super::composition::VideoEffect::Sharpen{..}|super::composition::VideoEffect::ReduceInterlaceFlicker{..}|super::composition::VideoEffect::ChannelBlur{..}|super::composition::VideoEffect::TurbulentDisplace{..}|super::composition::VideoEffect::Shrink{..}|super::composition::VideoEffect::ColorEmboss{..}|super::composition::VideoEffect::RoughenEdges{..}|super::composition::VideoEffect::LongShadow{..}|super::composition::VideoEffect::Stroke{..}|super::composition::VideoEffect::CellPattern{..}|super::composition::VideoEffect::Checkerboard{..}|super::composition::VideoEffect::Circle{..}|super::composition::VideoEffect::Ramp{..}|super::composition::VideoEffect::FourColorGradient{..}|super::composition::VideoEffect::BevelAlpha{..}|super::composition::VideoEffect::DropShadow{..}|super::composition::VideoEffect::Basic3D{..}|super::composition::VideoEffect::Mosaic{..} | super::composition::VideoEffect::MosaicLegacy{..}|super::composition::VideoEffect::DirectionalBlur{..} | super::composition::VideoEffect::DirectionalBlurLegacy{..}|super::composition::VideoEffect::FindEdges{..}|super::composition::VideoEffect::Replicate{..}|super::composition::VideoEffect::Median{..}|super::composition::VideoEffect::CameraBlur{..}|super::composition::VideoEffect::GaussianBlur{..} | super::composition::VideoEffect::GaussianBlurLegacy{..}|super::composition::VideoEffect::LumaKey{..}|super::composition::VideoEffect::ChannelMix{..}|super::composition::VideoEffect::ColorBalance{..}|super::composition::VideoEffect::ColorTint{..}|super::composition::VideoEffect::ColorLevels{..}|super::composition::VideoEffect::ColorExtract{..}|super::composition::VideoEffect::HorizontalFlip|super::composition::VideoEffect::VerticalFlip|super::composition::VideoEffect::BlackWhite|super::composition::VideoEffect::CineonConverter{..}|super::composition::VideoEffect::VideoLimiter{..}|super::composition::VideoEffect::BasicGrade{..}|super::composition::VideoEffect::CreativeAdjust{..}|super::composition::VideoEffect::ConcatGaussianBlur { .. } | super::composition::VideoEffect::ConcatBloomPulse { .. } | super::composition::VideoEffect::ConcatMotionBlur { .. } | super::composition::VideoEffect::ConcatZoomBlur { .. } | super::composition::VideoEffect::ConcatFilmGrain { .. } | super::composition::VideoEffect::ConcatGlow { .. } | super::composition::VideoEffect::ConcatHalftone { .. } | super::composition::VideoEffect::ConcatFisheye { .. } | super::composition::VideoEffect::ConcatChromaticAberration { .. } | super::composition::VideoEffect::ConcatPosterize { .. } | super::composition::VideoEffect::ConcatLumaKey { .. } | super::composition::VideoEffect::ConcatPixelate { .. } | super::composition::VideoEffect::ConcatChromaKey { .. } | super::composition::VideoEffect::ConcatSaturation { .. }|super::composition::VideoEffect::ConcatVibrance{..}|super::composition::VideoEffect::ConcatSharpen{..}|super::composition::VideoEffect::ConcatVhs{..}|super::composition::VideoEffect::ConcatFallingDust{..}|super::composition::VideoEffect::ConcatFilm{..}|super::composition::VideoEffect::ConcatDigicam{..}|super::composition::VideoEffect::ConcatMono{..}|super::composition::VideoEffect::ConcatMidnight{..}|super::composition::VideoEffect::ConcatMarquee{..}|super::composition::VideoEffect::ConcatRetroCartoon{..}|super::composition::VideoEffect::ConcatSuper8{..}|super::composition::VideoEffect::ConcatBleachBypass{..}|super::composition::VideoEffect::ConcatAutumn{..}|super::composition::VideoEffect::ConcatNeon{..}|super::composition::VideoEffect::ConcatPinkFuzz{..}|super::composition::VideoEffect::ConcatNight{..}|super::composition::VideoEffect::ConcatSpooky{..}|super::composition::VideoEffect::ConcatCool{..}|super::composition::VideoEffect::ConcatWarm{..}|super::composition::VideoEffect::ConcatTealOrange{..}|super::composition::VideoEffect::ConcatNoir{..}|super::composition::VideoEffect::ConcatDreamy{..}|super::composition::VideoEffect::ConcatHighContrastMono{..}|super::composition::VideoEffect::ConcatLandscapePop{..}|super::composition::VideoEffect::ConcatNatural{..}|super::composition::VideoEffect::ConcatRetro70s{..}|super::composition::VideoEffect::ConcatSilver{..}|super::composition::VideoEffect::ConcatTropical{..}|super::composition::VideoEffect::ConcatVaporwave{..}|super::composition::VideoEffect::ConcatWinterCool{..}|super::composition::VideoEffect::ConcatClassicChrome{..}|super::composition::VideoEffect::ConcatCyberpunk{..}|super::composition::VideoEffect::ConcatDelicious{..}|super::composition::VideoEffect::ConcatRosy{..}|super::composition::VideoEffect::ConcatSciFi{..}|super::composition::VideoEffect::ConcatMatcha{..}|super::composition::VideoEffect::ConcatSage{..}|super::composition::VideoEffect::ConcatIvory{..}|super::composition::VideoEffect::ConcatMilk{..}|super::composition::VideoEffect::ConcatBronze{..}|super::composition::VideoEffect::ConcatCoolBlue{..}|super::composition::VideoEffect::ConcatFresh{..}|super::composition::VideoEffect::ConcatVivid{..}|super::composition::VideoEffect::ConcatSoftLight{..}|super::composition::VideoEffect::ConcatMoody{..}|super::composition::VideoEffect::ConcatRain{..}|super::composition::VideoEffect::ConcatUmber{..}|super::composition::VideoEffect::ConcatFair{..}|super::composition::VideoEffect::ConcatPeach{..}|super::composition::VideoEffect::ConcatPolaroid{..}|super::composition::VideoEffect::ConcatVintage{..}|super::composition::VideoEffect::ConcatCloudlessSky{..}|super::composition::VideoEffect::ConcatLagoon{..}|super::composition::VideoEffect::ConcatGoldenHour{..}|super::composition::VideoEffect::ConcatDaydream{..}|super::composition::VideoEffect::ConcatClean{..}|super::composition::VideoEffect::ConcatContrastPop{..}|super::composition::VideoEffect::ConcatVignette{..}|super::composition::VideoEffect::ConcatAdjust{..}|super::composition::VideoEffect::ConcatCurves{..}|super::composition::VideoEffect::ConcatColorWheels{..}|super::composition::VideoEffect::ConcatWhiteBalance{..}|super::composition::VideoEffect::ConcatExposure{..}|super::composition::VideoEffect::ConcatContrast{..}|super::composition::VideoEffect::ColorSharpen{..}|super::composition::VideoEffect::ColorVignette{..}|super::composition::VideoEffect::ColorLut{..}|super::composition::VideoEffect::CreativeGrade{..}|super::composition::VideoEffect::ToneCurves{..}|super::composition::VideoEffect::HslSecondary{..}|super::composition::VideoEffect::Lumetri{..}) {
                        if linear_frame.is_none(){linear_frame=Some(super::linear_color_frame::LinearColorFrame::from_surface(child,checkpoint)?);}
                        let frame=linear_frame.as_mut().ok_or_else(||anyhow::anyhow!("missing linear working frame"))?;
                        match effect {
                            super::composition::VideoEffect::ProcAmp{parameters,animation}=>{let p=if let Some(a)=animation {super::proc_amp::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::GammaCorrection{parameters,animation}=>{let p=if let Some(a)=animation {super::gamma_correction::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Posterize{parameters,animation}=>{let p=if let Some(a)=animation {super::posterize::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorInvert{parameters,animation}=>{let p=if let Some(a)=animation {super::color_invert::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorReplace{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::color_replace::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::color_replace::sample_colors(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::UltraKey{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::ultra_key::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::ultra_key::sample_color(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorKey{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::color_key::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::color_key::sample_color(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorPass{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::color_pass::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::color_pass::sample_color(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ChangeToColor{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::change_to_color::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::change_to_color::sample_colors(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::LeaveColor{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::leave_color::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::leave_color::sample_color(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::AlphaAdjust{parameters,animation}=>{let p=if let Some(a)=animation {super::alpha_adjust::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::DirectionalBlur{parameters,animation} | super::composition::VideoEffect::DirectionalBlurLegacy{parameters,animation}=>{let p=if let Some(a)=animation {super::directional_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Mosaic{parameters,animation} | super::composition::VideoEffect::MosaicLegacy{parameters,animation}=>{let p=if let Some(a)=animation {super::mosaic::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Twirl{parameters,animation} | super::composition::VideoEffect::TwirlLegacy{parameters,animation}=>{let p=if let Some(a)=animation {super::twirl::sample_animation(*parameters,a,parameter_time,frame.dimensions())?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::WaveWarp{parameters,animation}=>{let p=if let Some(a)=animation {super::wave_warp::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?;},
                            super::composition::VideoEffect::LensDistortion{parameters,animation}=>{let p=if let Some(a)=animation {super::lens_distortion::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Grid{parameters,animation}=>{let p=if let Some(a)=animation {super::grid::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::LensFlare{parameters,animation}=>{let p=if let Some(a)=animation {super::lens_flare::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Ellipse{parameters,animation}=>{let p=if let Some(a)=animation {super::ellipse::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::PaintBucket{parameters,animation}=>{let p=if let Some(a)=animation {super::paint_bucket::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Gradient{parameters,animation}=>{let p=if let Some(a)=animation {super::gradient::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorEmboss{parameters,animation}=>{let p=if let Some(a)=animation {super::color_emboss::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::GraphicText{parameters,animation,source_text_animation}=>{let(w,h)=frame.dimensions();let mut p=if let Some(a)=animation{super::graphic_text_animation::sample(parameters.clone(),a,parameter_time,(w as u32,h as u32))?}else{parameters.clone()};if let Some(a)=source_text_animation{p.text=a.sample(parameter_time)?.into();}p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::GraphicShape{parameters,animation}=>{let(w,h)=frame.dimensions();let p=if let Some(a)=animation{super::graphic_shape_animation::sample(parameters.clone(),a,parameter_time,(w as u32,h as u32))?}else{parameters.clone()};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::TrackMatte{parameters}=>{
                                if parameters.matte>0 {
                                    if let Some(ctx)=context.as_deref_mut() {
                                        let track=ctx.track((parameters.matte-1) as usize,checkpoint)?;
                                        let mapping=|x,y|ctx.layer_to_output(x,y);
                                        let layer=track.as_ref().map(|frame|super::track_matte::MatteLayer{frame,layer_to_output:&mapping});
                                        parameters.apply_linear(frame,layer,checkpoint)?;
                                    }else{parameters.apply_linear(frame,None,checkpoint)?;}
                                }else{parameters.apply_linear(frame,None,checkpoint)?;}
                            },
                            super::composition::VideoEffect::CompoundBlur{parameters,animation}=>{
                                let p=if let Some(a)=animation {super::compound_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};
                                if p.layer>0 && p.max*0.5>0.3 {
                                    let ctx=context.as_deref_mut().ok_or_else(||anyhow::anyhow!("Compound Blur external layer requires track render context"))?;
                                    let track=ctx.track((p.layer-1) as usize,checkpoint)?;
                                    let mapping=|x,y|ctx.layer_to_output(x,y);
                                    let layer=track.as_ref().map(|frame|super::compound_blur::CompoundLayer{frame,layer_to_output:&mapping});
                                    p.apply_linear(frame,layer,1.,checkpoint)?;
                                }else{p.apply_linear(frame,None,1.,checkpoint)?;}
                            },
                            super::composition::VideoEffect::GradientWipe{parameters,animation}=>{
                                let p=if let Some(a)=animation {super::gradient_wipe::sample_animation(*parameters,a,parameter_time)?}else{*parameters};
                                if p.layer>0 && p.completion>0. {
                                    let ctx=context.as_deref_mut().ok_or_else(||anyhow::anyhow!("Gradient Wipe external layer requires track render context"))?;
                                    let track=ctx.track((p.layer-1) as usize,checkpoint)?;
                                    let mapping=|x,y|ctx.layer_to_output(x,y);
                                    let layer=track.as_ref().map(|frame|super::gradient_wipe::GradientLayer{frame,layer_to_output:&mapping});
                                    p.apply_linear(frame,layer,checkpoint)?;
                                }else{p.apply_linear(frame,None,checkpoint)?;}
                            },
                            super::composition::VideoEffect::LinearWipe{parameters,animation}=>{let p=if let Some(a)=animation{super::linear_wipe::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Magnify{parameters,animation}=>{let p=if let Some(a)=animation{super::magnify::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Mirror{parameters,animation}=>{let p=if let Some(a)=animation{super::mirror::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Offset{parameters,animation}=>{let p=if let Some(a)=animation{super::offset::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Transform{parameters,animation}=>{let p=if let Some(a)=animation{super::transform_effect::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::AscCdl{parameters,animation}=>{let p=if let Some(a)=animation{super::asc_cdl::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::LogoCutout{parameters,animation}=>{let p=if let Some(a)=animation{super::logo_cutout::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::EdgeGlow{parameters,animation}=>{let p=if let Some(a)=animation{super::edge_glow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::WonderGlow{parameters,animation}=>{let p=if let Some(a)=animation{super::wonder_glow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Glint{parameters,animation}=>{let p=if let Some(a)=animation{super::glint::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::WarpStabilizer{parameters,animation}=>{let p=if let Some(a)=animation{super::warp_stabilizer_animation::sample(parameters.clone(),a,parameter_time)?}else{parameters.clone()};if let Some(ctx)=context.as_deref_mut(){if let Some(path)=ctx.stabilizer_path(&p,checkpoint)?{super::warp_stabilizer_render::apply_linear(&p,&path,generator_seconds,frame,checkpoint)?;}}},
 super::composition::VideoEffect::VrChromaticAberrations{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_chromatic::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::VrColorGradients{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_color_gradients::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::Lightning{parameters,animation}=>{let p=if let Some(a)=animation{super::lightning_animation::sample(parameters.clone(),a,parameter_time)?}else{parameters.clone()};p.apply_linear(frame,generator_seconds,1.,checkpoint)?;},
 super::composition::VideoEffect::WriteOn{parameters,animation}=>{super::write_on_animation::apply_at_time(parameters,animation.as_ref(),frame,parameter_time,generator_seconds,1.,checkpoint)?;},
 super::composition::VideoEffect::Timecode{parameters,animation}=>{let p=if let Some(a)=animation{super::clock_label::sample_timecode(parameters.clone(),a,parameter_time)?}else{parameters.clone()};let values=context.as_deref().ok_or_else(||anyhow::anyhow!("label requires composition context"))?.burnin_values(local)?.ok_or_else(||anyhow::anyhow!("label clip context unavailable"))?;p.apply_linear(frame,values.context(),1.,checkpoint)?;},
 super::composition::VideoEffect::ClipName{parameters,animation}=>{let p=if let Some(a)=animation{super::clock_label::sample_clip_name(parameters.clone(),a,parameter_time)?}else{parameters.clone()};let values=context.as_deref().ok_or_else(||anyhow::anyhow!("label requires composition context"))?.burnin_values(local)?.ok_or_else(||anyhow::anyhow!("label clip context unavailable"))?;p.apply_linear(frame,values.context(),1.,checkpoint)?;},
 super::composition::VideoEffect::MetadataBurnin{parameters,animation}=>{let p=if let Some(a)=animation{super::metadata_burnin::sample(parameters.clone(),a,parameter_time)?}else{parameters.clone()};let values=context.as_deref().ok_or_else(||anyhow::anyhow!("Metadata Burn-in requires composition context"))?.burnin_values(local)?.ok_or_else(||anyhow::anyhow!("Metadata Burn-in clip context unavailable"))?;p.apply_linear(frame,values.context(),1.,checkpoint)?;},
 super::composition::VideoEffect::SimpleText{parameters,animation}=>{let p=if let Some(a)=animation{super::simple_text::sample(parameters.clone(),a,parameter_time)?}else{parameters.clone()};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::VrPlaneToSphere{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_plane_to_sphere::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::VrGlow{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_glow::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::VrFractalNoise{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_fractal_noise::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::VrDigitalGlitch{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_digital_glitch::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?;},
 super::composition::VideoEffect::VrDenoise{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_denoise::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::VrBlur{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_filter::sample_blur(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::VrSharpen{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_filter::sample_sharpen(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
 super::composition::VideoEffect::SphericalVideo{parameters,animation}=>{parameters.sample(animation.as_ref(),parameter_time)?.apply_linear(frame,generator_seconds,checkpoint)?;},
 super::composition::VideoEffect::VrProjection{parameters,animation}=>{let p=if let Some(a)=animation{super::vr_sphere::sample_animation(*parameters,a,parameter_time,false)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::VrRotateSphere{parameters,animation}=>{let base=parameters.projection();let p=if let Some(a)=animation{super::vr_sphere::sample_animation(base,a,parameter_time,true)?}else{base};p.apply_linear(frame,checkpoint)?;},
 super::composition::VideoEffect::LightLeaks{parameters,animation}=>{let p=if let Some(a)=animation{super::light_leaks::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?;},
 super::composition::VideoEffect::AutoReframe{parameters,animation}=>{let p=if let Some(a)=animation{super::auto_reframe::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let environment=if let Some(ctx)=context.as_deref_mut(){ctx.auto_reframe_environment(p.preset,checkpoint)?}else{None};p.apply_linear(frame,1.,environment.as_ref().map(|e|e.subject),environment.as_ref().map(|e|e.sequence),checkpoint)?;},
 super::composition::VideoEffect::Echo{parameters,animation}=>{
 let p=if let Some(a)=animation{super::echo::sample_animation(*parameters,a,parameter_time)?}else{*parameters};
 if let Some(ctx)=context.as_deref_mut(){if let Some(requests)=p.requests()?{
 let mut layers=vec![(frame.clone(),p.start)];for(dt,weight)in requests{checkpoint()?;if let Some(replacement)=ctx.temporal_frame(dt,&effects[..effect_index],checkpoint)?{layers.push((replacement,weight));}}
 *frame=p.combine(&layers,checkpoint)?;
 }}},
 super::composition::VideoEffect::PosterizeTime{parameters,animation}=>{let p=if let Some(a)=animation{super::posterize_time::sample_animation(*parameters,a,parameter_time)?}else{*parameters};if let Some(dt)=p.frame_offset(generator_seconds)?{if let Some(ctx)=context.as_deref_mut(){if let Some(replacement)=ctx.temporal_frame(dt,&effects[..effect_index],checkpoint)?{*frame=replacement;}}}},
                            super::composition::VideoEffect::VolumetricRays{parameters,animation}=>{let p=if let Some(a)=animation{super::volumetric_rays::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::EchoGlow{parameters,animation}=>{let p=if let Some(a)=animation{super::echo_glow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::RgbSplit{parameters,animation}=>{let p=if let Some(a)=animation{super::rgb_split::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::LightingEffects{parameters,animation}=>{let p=if let Some(a)=animation{super::lighting_effects::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.0,checkpoint)?;},
                            super::composition::VideoEffect::VignetteEffect{parameters,animation}=>{let p=if let Some(a)=animation{super::vignette_effect::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Spherize{parameters,animation}=>{let p=if let Some(a)=animation{super::spherize::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::CornerPin{parameters,animation}=>{let p=if let Some(a)=animation{super::corner_pin::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Rotate3D{parameters,animation}=>{let p=if let Some(a)=animation{super::rotate_3d::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;},
                            super::composition::VideoEffect::Shrink{parameters,animation}=>{let base:super::grow_shrink::GrowShrink=(*parameters).into();let p=if let Some(a)=animation{super::grow_shrink::sample_animation(base,a,parameter_time)?}else{base};let progress=context.as_ref().and_then(|c|if matches!(parameter_time,super::animation_sample_time::AnimationSampleTime::SourceTicks(_)|super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(_)){c.clip_progress_seconds(generator_seconds)}else{c.clip_progress(local)}).ok_or_else(||anyhow::anyhow!("Grow/Shrink requires active clip duration context"))?;p.apply_linear(frame,progress,1.,checkpoint)?;}
                            super::composition::VideoEffect::Grow{parameters,animation}=>{let base:super::grow_shrink::GrowShrink=(*parameters).into();let p=if let Some(a)=animation{super::grow_shrink::sample_animation(base,a,parameter_time)?}else{base};let progress=context.as_ref().and_then(|c|if matches!(parameter_time,super::animation_sample_time::AnimationSampleTime::SourceTicks(_)|super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(_)){c.clip_progress_seconds(generator_seconds)}else{c.clip_progress(local)}).ok_or_else(||anyhow::anyhow!("Grow/Shrink requires active clip duration context"))?;p.apply_linear(frame,progress,1.,checkpoint)?;}
                            super::composition::VideoEffect::Spin{parameters,animation}=>{let base:super::spin::Spin=(*parameters).into();let p=if let Some(a)=animation{super::spin::sample_animation(base,a,parameter_time)?}else{base};let progress=context.as_ref().and_then(|c|if matches!(parameter_time,super::animation_sample_time::AnimationSampleTime::SourceTicks(_)|super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(_)){c.clip_progress_seconds(generator_seconds)}else{c.clip_progress(local)}).ok_or_else(||anyhow::anyhow!("Spin requires active clip duration context"))?;p.apply_linear(frame,progress,1.,checkpoint)?;}
                            super::composition::VideoEffect::Clone{parameters,animation}=>{let p=if let Some(a)=animation{super::clone_effect::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::AutoAlign{parameters,animation}=>{let p=if let Some(a)=animation{super::auto_align::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::Crop{parameters,animation}=>{let p=if let Some(a)=animation{super::crop::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::BrightnessContrast{parameters,animation}=>{let p=if let Some(a)=animation{super::brightness_contrast::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}
                            super::composition::VideoEffect::UnsharpMask{parameters,animation}=>{let p=if let Some(a)=animation{super::unsharp_mask::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::BokehBlur{parameters,animation}=>{let p=if let Some(a)=animation{super::bokeh_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::FocusBlur{parameters,animation}=>{let p=if let Some(a)=animation{super::focus_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::EdgeFeather{parameters,animation}=>{let p=if let Some(a)=animation{super::edge_feather::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}
                            super::composition::VideoEffect::Sharpen{parameters,animation}=>{let p=if let Some(a)=animation{super::sharpen::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::ReduceInterlaceFlicker{parameters,animation}=>{let p=if let Some(a)=animation{super::reduce_interlace_flicker::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::ChannelBlur{parameters,animation}=>{let p=if let Some(a)=animation{super::channel_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::TurbulentDisplace{parameters,animation}=>{let p=if let Some(a)=animation{super::turbulent_displace::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::RoundedCrop{parameters,animation}=>{let p=if let Some(a)=animation{super::rounded_crop::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::Spacer{parameters,animation}=>{let p=if let Some(a)=animation{super::spacer::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,1.,checkpoint)?;}
                            super::composition::VideoEffect::CameraShake{parameters,animation}=>{let p=if let Some(a)=animation{super::camera_shake::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let fps=if p.motion_blur>0.{context.as_ref().and_then(|c|c.frame_rate()).ok_or_else(||anyhow::anyhow!("CameraShake motion blur requires frame rate context"))?}else{30.};p.apply_linear(frame,generator_seconds,fps,1.,checkpoint)?;}
                            super::composition::VideoEffect::Wiggle{parameters,animation}=>{let p=if let Some(a)=animation{super::wiggle::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,generator_seconds,1.,checkpoint)?;}
                            super::composition::VideoEffect::Move{parameters,animation}=>{let p=if let Some(a)=animation{super::move_effect::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let progress=context.as_ref().and_then(|c|if matches!(parameter_time,super::animation_sample_time::AnimationSampleTime::SourceTicks(_)|super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(_)){c.clip_progress_seconds(generator_seconds)}else{c.clip_progress(local)}).ok_or_else(||anyhow::anyhow!("Move requires active clip duration context"))?;let span=if p.motion_blur>0.{context.as_ref().and_then(|c|c.motion_blur_span()).ok_or_else(||anyhow::anyhow!("Move motion blur requires frame rate context"))?}else{0.};p.apply_linear(frame,progress,span,1.,checkpoint)?;}
                            super::composition::VideoEffect::BlockDissolve{parameters,animation}=>{let p=if let Some(a)=animation {super::block_dissolve::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::AlphaGlow{parameters,animation}=>{let p=if let Some(a)=animation {super::alpha_glow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::BrushStrokes{parameters,animation}=>{let p=if let Some(a)=animation {super::brush_strokes::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Stroke{parameters,animation}=>{let p=if let Some(a)=animation {super::stroke::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::LongShadow{parameters,animation}=>{let p=if let Some(a)=animation {super::long_shadow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::RoughenEdges{parameters,animation}=>{let p=if let Some(a)=animation {super::roughen_edges::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::CellPattern{parameters,animation}=>{let p=if let Some(a)=animation {super::cell_pattern::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Checkerboard{parameters,animation}=>{let p=if let Some(a)=animation {super::checkerboard::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Circle{parameters,animation}=>{let p=if let Some(a)=animation {super::circle::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Ramp{parameters,animation}=>{let p=if let Some(a)=animation {super::ramp::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::FourColorGradient{parameters,animation}=>{let p=if let Some(a)=animation {super::four_color_gradient::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::BevelAlpha{parameters,animation}=>{let p=if let Some(a)=animation {super::bevel_alpha::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::DropShadow{parameters,animation}=>{let p=if let Some(a)=animation {super::drop_shadow::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Basic3D{parameters,animation}=>{let p=if let Some(a)=animation {super::basic_3d::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Strobe{parameters,animation}=>{let p=if let Some(a)=animation {super::strobe::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?;},
                            super::composition::VideoEffect::Noise{parameters,animation} | super::composition::VideoEffect::NoiseLegacy{parameters,animation}=>{let p=if let Some(a)=animation {super::noise::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?;},
                            super::composition::VideoEffect::Emboss{parameters,animation}=>{let p=if let Some(a)=animation {super::emboss::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::CameraBlur{parameters,animation}=>{let p=if let Some(a)=animation {super::camera_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Median{parameters,animation}=>{let p=if let Some(a)=animation {super::median::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::Replicate{parameters,animation}=>{let p=if let Some(a)=animation {super::replicate::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::FindEdges{parameters,animation}=>{let p=if let Some(a)=animation {super::find_edges::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::GaussianBlur{parameters,animation} | super::composition::VideoEffect::GaussianBlurLegacy{parameters,animation}=>{let p=if let Some(a)=animation {super::gaussian_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::LumaKey{parameters,animation}=>{let p=if let Some(a)=animation {super::luma_key::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ChannelMix{parameters,animation}=>{let p=if let Some(a)=animation {super::channel_mix::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorBalance{parameters,animation}=>{let p=if let Some(a)=animation {super::color_balance::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorTint{parameters,animation,color_animation}=>{let p=if let Some(a)=animation {super::color_tint::sample_animation(*parameters,a,parameter_time)?}else{*parameters};let p=if let Some(a)=color_animation {super::color_tint::sample_colors(p,a,parameter_time)?}else{p};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorLevels{parameters,animation}=>{let p=if let Some(a)=animation {super::color_levels::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorExtract{parameters,animation}=>{let p=if let Some(a)=animation {super::color_extract::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::HorizontalFlip=>super::flip::apply_linear(frame,true,checkpoint)?,
                            super::composition::VideoEffect::VerticalFlip=>super::flip::apply_linear(frame,false,checkpoint)?,
                            super::composition::VideoEffect::BlackWhite=>super::black_white::apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::CineonConverter{parameters,animation}=>{let p=if let Some(a)=animation {super::cineon_converter::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::VideoLimiter{parameters,animation}=>{let p=if let Some(a)=animation {super::video_limiter::sample_animation(*parameters,a,parameter_time)?}else{*parameters};frame.video_limiter(&p,checkpoint)?;},
                            super::composition::VideoEffect::BasicGrade{parameters,animation}=>{let p=if let Some(a)=animation{super::basic_grade::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::CreativeAdjust{parameters,animation}=>{let p=if let Some(a)=animation{super::creative_adjust::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ConcatGaussianBlur{parameters,animation,concat_keys}=>{let p=if let Some(keys)=concat_keys.as_deref().filter(|keys|!keys.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Concat keys require active clip duration"))?;super::concat_gaussian_blur::sample_concat_keys(*parameters,keys,progress)?}else if let Some(a)=animation{super::concat_gaussian_blur::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ConcatMotionBlur{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let p=super::concat_motion_blur::sample_keys(*parameters,keys,progress)?;p.apply_sampled_linear(frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatBloomPulse{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let p=super::concat_bloom_pulse::sample_keys(*parameters,keys,progress)?;p.apply_sampled_linear(p.amount,p.speed,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds) as f32,frame,checkpoint)?;}else{parameters.apply_linear(context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds) as f32,frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatZoomBlur{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let p=super::concat_zoom_blur::sample_keys(*parameters,keys,progress)?;p.apply_sampled_linear(frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatFilmGrain{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|k|!k.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let shift=super::concat_film_grain::sample_concat_keys(*parameters,keys,progress)?;parameters.apply_sampled_linear(shift,frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?;}else{parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?;}},
                            super::composition::VideoEffect::ConcatGlow{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|k|!k.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let shift=super::concat_glow::sample_concat_keys(*parameters,keys,progress)?;parameters.apply_sampled_linear(shift,frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatHalftone{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|k|!k.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let shift=super::concat_halftone::sample_concat_keys(*parameters,keys,progress)?;parameters.apply_sampled_linear(shift,frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatFisheye{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|k|!k.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let shift=super::concat_fisheye::sample_concat_keys(*parameters,keys,progress)?;parameters.apply_sampled_linear(shift,frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatChromaticAberration{parameters,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|k|!k.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Concat source keys require clip duration"))?;let shift=super::concat_chromatic_aberration::sample_concat_keys(*parameters,keys,progress)?;parameters.apply_sampled_linear(shift,frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatPosterize{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatLumaKey{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatPixelate{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatChromaKey{parameters,concat_keys}=>{let p=if let Some(keys)=concat_keys.as_ref().filter(|keys|keys.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Chroma Key keys require active clip duration"))?;super::concat_chroma_key::sample_keys(*parameters,keys,progress)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ConcatSaturation{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|keys|!keys.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Concat saturation keys require active clip duration"))?;let amount=super::concat_chroma::sample_concat_keys(*parameters,keys,progress)?;super::concat_chroma::Saturation::apply_sampled_linear(amount,frame,checkpoint)?;}else{let p=if let Some(a)=animation{super::concat_chroma::sample_saturation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatVibrance{parameters,animation}=>{let p=if let Some(a)=animation{super::concat_chroma::sample_vibrance(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ConcatSharpen{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatVhs{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatFallingDust{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatFilm{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatDigicam{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatMono{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatMidnight{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatMarquee{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatRetroCartoon{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSuper8{parameters}=>parameters.apply_linear(frame,context.as_ref().and_then(|c|c.timeline_seconds()).unwrap_or(generator_seconds),checkpoint)?,
                            super::composition::VideoEffect::ConcatBleachBypass{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatAutumn{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatNeon{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatPinkFuzz{parameters}=>parameters.apply_linear(frame,checkpoint)?,

                            super::composition::VideoEffect::ConcatNight{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSpooky{parameters}=>parameters.apply_linear(frame,checkpoint)?,

                            super::composition::VideoEffect::ConcatCool{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatWarm{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatTealOrange{parameters}=>parameters.apply_linear(frame,checkpoint)?,

                            super::composition::VideoEffect::ConcatNoir{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatDreamy{parameters}=>parameters.apply_linear(frame,checkpoint)?,

                            super::composition::VideoEffect::ConcatHighContrastMono{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatLandscapePop{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatNatural{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatRetro70s{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSilver{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatTropical{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatVaporwave{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatWinterCool{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatClassicChrome{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatCyberpunk{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatDelicious{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatRosy{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSciFi{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatMatcha{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSage{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatIvory{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatMilk{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatBronze{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatCoolBlue{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatFresh{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatVivid{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatSoftLight{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatMoody{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatRain{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatUmber{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatFair{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatPeach{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatPolaroid{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatVintage{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatCloudlessSky{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatLagoon{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatGoldenHour{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatDaydream{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatClean{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatContrastPop{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatVignette{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatAdjust{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Adjust keys require active clip duration"))?;super::concat_adjust::sample_keys(parameters,keys,progress)?.apply_sampled_linear(frame,checkpoint)?;}else{if let Some(a)=animation{super::concat_adjust::sample_animation(parameters,a,parameter_time)?.apply_linear(frame,checkpoint)?;}else{parameters.apply_linear(frame,checkpoint)?;}}},
                            super::composition::VideoEffect::ConcatCurves{parameters}=>parameters.apply_linear(frame,checkpoint)?,
                            super::composition::VideoEffect::ConcatColorWheels{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original wheel keys require active clip duration"))?;super::concat_color_wheels::sample_keys(*parameters,keys,progress)?.apply_sampled_linear(frame,checkpoint)?;}else{let p=if let Some(a)=animation{super::concat_color_wheels::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatWhiteBalance{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_ref().filter(|k|k.values().any(|run|!run.is_empty())){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original White Balance keys require active clip duration"))?;super::concat_white_balance::sample_keys(*parameters,keys,progress)?.apply_sampled_linear(frame,checkpoint)?;}else{let p=if let Some(a)=animation{super::concat_white_balance::sample(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatExposure{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|keys|!keys.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Concat exposure keys require active clip duration"))?;let stops=super::concat_exposure::sample_concat_keys(*parameters,keys,progress)?;super::concat_exposure::ConcatExposure::apply_sampled_linear(stops,frame,checkpoint)?;}else{let p=if let Some(a)=animation{super::concat_exposure::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ConcatContrast{parameters,animation,concat_keys}=>{if let Some(keys)=concat_keys.as_deref().filter(|keys|!keys.is_empty()){let progress=context.as_ref().and_then(|c|c.clip_progress(local)).ok_or_else(||anyhow::anyhow!("Original Concat contrast keys require active clip duration"))?;let amount=super::concat_contrast::sample_concat_keys(*parameters,keys,progress)?;super::concat_contrast::ConcatContrast::apply_sampled_linear(amount,frame,checkpoint)?;}else{let p=if let Some(a)=animation{super::concat_contrast::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;}},
                            super::composition::VideoEffect::ColorSharpen{parameters,animation}=>{let p=if let Some(a)=animation{super::color_sharpen::sample_animation(a,parameter_time)?}else{*parameters};p.validate()?;frame.sharpen(p.amount/100.0,1.0,checkpoint)?;},
                            super::composition::VideoEffect::ColorVignette{parameters,animation}=>{let p=if let Some(a)=animation{super::color_vignette::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ColorLut{lut}=>{lut.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::CreativeGrade{parameters,animation}=>{let p=if let Some(a)=animation{a.sample(*parameters,parameter_time)?}else{*parameters};p.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::ToneCurves{curves}=>{curves.apply_linear(frame,checkpoint)?;},
                            super::composition::VideoEffect::HslSecondary{parameters,animation}=>{let p=if let Some(a)=animation{super::hsl_secondary::sample_animation(*parameters,a,parameter_time)?}else{*parameters};p.apply_linear_scaled(frame,1.0,checkpoint)?;},
                            super::composition::VideoEffect::Lumetri{parameters}=>{parameters.sample(parameter_time)?.apply_linear_with_library(frame,1.0,library,checkpoint)?;},
                            _=>anyhow::bail!("unexpected linear effect"),
                        }
                        continue;
                    }
                    if let Some(frame)=linear_frame.take(){*child=frame.to_surface(checkpoint)?;}

                    if let super::composition::VideoEffect::ChromaMatte {
                        parameters,
                        animation,
                    } = effect
                    {
                        let parameters = if let Some(animation) = animation {
                            animation
                                .sample(*parameters, parameter_time)?
                        } else {
                            *parameters
                        };
                        *child = super::chroma_surface::render(child, parameters, checkpoint)?;
                    } else {
                        super::surface_masks::apply(
                            child,
                            std::slice::from_ref(effect),
                            parameter_time,
                            checkpoint,
                        )?;
                    }
                }
                let frame=if let Some(frame)=linear_frame.take(){*child=frame.to_surface(checkpoint)?;frame}else{super::linear_color_frame::LinearColorFrame::from_surface(child,checkpoint)?};
    Ok(frame)
}

pub fn supported_effect_stack(effects:&[super::composition::VideoEffect])->bool {
                    effects.iter().all(|effect| {
                        super::surface_masks::supported(effect)
                            || matches!(
                                effect,
                                super::composition::VideoEffect::NativeBypass { .. } | super::composition::VideoEffect::ConcatBypass { .. } | super::composition::VideoEffect::ChromaMatte { .. }
                                    | super::composition::VideoEffect::ToneCurves { .. }
                                    | super::composition::VideoEffect::CreativeGrade { .. }
                                    | super::composition::VideoEffect::ColorLut { .. }
                                    | super::composition::VideoEffect::ConcatGaussianBlur { .. } | super::composition::VideoEffect::ConcatBloomPulse { .. } | super::composition::VideoEffect::ConcatMotionBlur { .. } | super::composition::VideoEffect::ConcatZoomBlur { .. } | super::composition::VideoEffect::ConcatFilmGrain { .. } | super::composition::VideoEffect::ConcatGlow { .. } | super::composition::VideoEffect::ConcatHalftone { .. } | super::composition::VideoEffect::ConcatFisheye { .. } | super::composition::VideoEffect::ConcatChromaticAberration { .. } | super::composition::VideoEffect::ConcatPosterize { .. } | super::composition::VideoEffect::ConcatLumaKey { .. } | super::composition::VideoEffect::ConcatPixelate { .. } | super::composition::VideoEffect::ConcatChromaKey { .. } | super::composition::VideoEffect::ConcatSaturation { .. } | super::composition::VideoEffect::ConcatVibrance { .. } | super::composition::VideoEffect::ConcatSharpen { .. } | super::composition::VideoEffect::ConcatVhs { .. } | super::composition::VideoEffect::ConcatFallingDust { .. } | super::composition::VideoEffect::ConcatFilm { .. } | super::composition::VideoEffect::ConcatDigicam { .. } | super::composition::VideoEffect::ConcatMono { .. } | super::composition::VideoEffect::ConcatMidnight { .. } | super::composition::VideoEffect::ConcatMarquee { .. } | super::composition::VideoEffect::ConcatRetroCartoon { .. } | super::composition::VideoEffect::ConcatSuper8 { .. } | super::composition::VideoEffect::ConcatBleachBypass { .. } | super::composition::VideoEffect::ConcatAutumn { .. } | super::composition::VideoEffect::ConcatNeon { .. } | super::composition::VideoEffect::ConcatPinkFuzz { .. } | super::composition::VideoEffect::ConcatNight { .. } | super::composition::VideoEffect::ConcatSpooky { .. } | super::composition::VideoEffect::ConcatCool { .. } | super::composition::VideoEffect::ConcatWarm { .. } | super::composition::VideoEffect::ConcatTealOrange { .. } | super::composition::VideoEffect::ConcatNoir { .. } | super::composition::VideoEffect::ConcatDreamy { .. } | super::composition::VideoEffect::ConcatHighContrastMono { .. } | super::composition::VideoEffect::ConcatLandscapePop { .. } | super::composition::VideoEffect::ConcatNatural { .. } | super::composition::VideoEffect::ConcatRetro70s { .. } | super::composition::VideoEffect::ConcatSilver { .. } | super::composition::VideoEffect::ConcatTropical { .. } | super::composition::VideoEffect::ConcatVaporwave { .. } | super::composition::VideoEffect::ConcatWinterCool { .. } | super::composition::VideoEffect::ConcatClassicChrome { .. } | super::composition::VideoEffect::ConcatCyberpunk { .. } | super::composition::VideoEffect::ConcatDelicious { .. } | super::composition::VideoEffect::ConcatRosy { .. } | super::composition::VideoEffect::ConcatSciFi { .. } | super::composition::VideoEffect::ConcatMatcha { .. } | super::composition::VideoEffect::ConcatSage { .. } | super::composition::VideoEffect::ConcatIvory { .. } | super::composition::VideoEffect::ConcatMilk { .. } | super::composition::VideoEffect::ConcatBronze { .. } | super::composition::VideoEffect::ConcatCoolBlue { .. } | super::composition::VideoEffect::ConcatFresh { .. } | super::composition::VideoEffect::ConcatVivid { .. } | super::composition::VideoEffect::ConcatSoftLight { .. } | super::composition::VideoEffect::ConcatMoody { .. } | super::composition::VideoEffect::ConcatRain { .. } | super::composition::VideoEffect::ConcatUmber { .. } | super::composition::VideoEffect::ConcatFair { .. } | super::composition::VideoEffect::ConcatPeach { .. } | super::composition::VideoEffect::ConcatPolaroid { .. } | super::composition::VideoEffect::ConcatVintage { .. } | super::composition::VideoEffect::ConcatCloudlessSky { .. } | super::composition::VideoEffect::ConcatLagoon { .. } | super::composition::VideoEffect::ConcatGoldenHour { .. } | super::composition::VideoEffect::ConcatDaydream { .. } | super::composition::VideoEffect::ConcatClean { .. } | super::composition::VideoEffect::ConcatContrastPop { .. } | super::composition::VideoEffect::ConcatVignette { .. } | super::composition::VideoEffect::ConcatAdjust { .. } | super::composition::VideoEffect::ConcatCurves { .. } | super::composition::VideoEffect::ConcatColorWheels { .. } | super::composition::VideoEffect::ConcatWhiteBalance { .. } | super::composition::VideoEffect::ConcatExposure { .. } | super::composition::VideoEffect::ConcatContrast { .. } | super::composition::VideoEffect::ColorSharpen { .. }
 | super::composition::VideoEffect::CreativeAdjust { .. }
 | super::composition::VideoEffect::BasicGrade { .. }
 | super::composition::VideoEffect::ProcAmp { .. } | super::composition::VideoEffect::GammaCorrection { .. } | super::composition::VideoEffect::Posterize { .. } | super::composition::VideoEffect::ColorInvert { .. } | super::composition::VideoEffect::ColorReplace { .. } | super::composition::VideoEffect::UltraKey { .. } | super::composition::VideoEffect::ColorKey { .. } | super::composition::VideoEffect::ColorPass { .. } | super::composition::VideoEffect::ChangeToColor { .. } | super::composition::VideoEffect::LeaveColor { .. } | super::composition::VideoEffect::AlphaAdjust { .. } | super::composition::VideoEffect::Emboss { .. } | super::composition::VideoEffect::Noise { .. } | super::composition::VideoEffect::NoiseLegacy { .. } | super::composition::VideoEffect::Strobe { .. } | super::composition::VideoEffect::Twirl { .. } | super::composition::VideoEffect::TwirlLegacy { .. } | super::composition::VideoEffect::WaveWarp { .. } | super::composition::VideoEffect::LensDistortion { .. } | super::composition::VideoEffect::Grid { .. } | super::composition::VideoEffect::LensFlare { .. } | super::composition::VideoEffect::Ellipse { .. } | super::composition::VideoEffect::PaintBucket { .. } | super::composition::VideoEffect::Gradient { .. } | super::composition::VideoEffect::BrushStrokes { .. } | super::composition::VideoEffect::AlphaGlow { .. } | super::composition::VideoEffect::BlockDissolve { .. } | super::composition::VideoEffect::GradientWipe { .. } | super::composition::VideoEffect::LinearWipe { .. } | super::composition::VideoEffect::Magnify { .. } | super::composition::VideoEffect::Mirror { .. } | super::composition::VideoEffect::Offset { .. } | super::composition::VideoEffect::Transform { .. } | super::composition::VideoEffect::AscCdl { .. } | super::composition::VideoEffect::VignetteEffect { .. } | super::composition::VideoEffect::LightingEffects{..} | super::composition::VideoEffect::LogoCutout{..} | super::composition::VideoEffect::EdgeGlow{..} | super::composition::VideoEffect::WonderGlow{..} | super::composition::VideoEffect::Glint{..} | super::composition::VideoEffect::PosterizeTime{..}|super::composition::VideoEffect::Echo{..}|super::composition::VideoEffect::AutoReframe{..}|super::composition::VideoEffect::VrDigitalGlitch{..}|super::composition::VideoEffect::VrFractalNoise{..}|super::composition::VideoEffect::VrGlow{..}|super::composition::VideoEffect::Lightning{..}|super::composition::VideoEffect::WriteOn{..}|super::composition::VideoEffect::Timecode{..}|super::composition::VideoEffect::ClipName{..}|super::composition::VideoEffect::MetadataBurnin{..}|super::composition::VideoEffect::SimpleText{..}|super::composition::VideoEffect::VrPlaneToSphere{..}|super::composition::VideoEffect::VrColorGradients{..}|super::composition::VideoEffect::VrChromaticAberrations{..}|super::composition::VideoEffect::VrDenoise{..}|super::composition::VideoEffect::VrBlur{..}|super::composition::VideoEffect::VrSharpen{..}|super::composition::VideoEffect::SphericalVideo{..}|super::composition::VideoEffect::VrProjection{..}|super::composition::VideoEffect::VrRotateSphere{..}|super::composition::VideoEffect::LightLeaks{..}|super::composition::VideoEffect::WarpStabilizer{..} | super::composition::VideoEffect::VolumetricRays{..} | super::composition::VideoEffect::EchoGlow{..} | super::composition::VideoEffect::RgbSplit{..} | super::composition::VideoEffect::Spherize { .. } | super::composition::VideoEffect::CornerPin { .. } | super::composition::VideoEffect::Rotate3D { .. } | super::composition::VideoEffect::Grow { .. } | super::composition::VideoEffect::Spin { .. } | super::composition::VideoEffect::Move { .. } | super::composition::VideoEffect::Wiggle { .. } | super::composition::VideoEffect::CameraShake { .. } | super::composition::VideoEffect::Spacer { .. } | super::composition::VideoEffect::RoundedCrop { .. } | super::composition::VideoEffect::Clone { .. } | super::composition::VideoEffect::AutoAlign { .. } | super::composition::VideoEffect::GraphicText { .. } | super::composition::VideoEffect::GraphicShape { .. } | super::composition::VideoEffect::TrackMatte { .. } | super::composition::VideoEffect::CompoundBlur { .. } |super::composition::VideoEffect::Crop { .. } | super::composition::VideoEffect::BrightnessContrast { .. } | super::composition::VideoEffect::UnsharpMask { .. } | super::composition::VideoEffect::BokehBlur { .. } |super::composition::VideoEffect::FocusBlur { .. } | super::composition::VideoEffect::EdgeFeather { .. } | super::composition::VideoEffect::Sharpen { .. } | super::composition::VideoEffect::ReduceInterlaceFlicker { .. } | super::composition::VideoEffect::ChannelBlur { .. } | super::composition::VideoEffect::TurbulentDisplace { .. } | super::composition::VideoEffect::Shrink { .. } | super::composition::VideoEffect::ColorEmboss { .. } | super::composition::VideoEffect::RoughenEdges { .. } | super::composition::VideoEffect::LongShadow { .. } | super::composition::VideoEffect::Stroke { .. } | super::composition::VideoEffect::CellPattern { .. } | super::composition::VideoEffect::Checkerboard { .. } | super::composition::VideoEffect::Circle { .. } | super::composition::VideoEffect::Ramp { .. } | super::composition::VideoEffect::FourColorGradient { .. } | super::composition::VideoEffect::BevelAlpha { .. } | super::composition::VideoEffect::DropShadow { .. } | super::composition::VideoEffect::Basic3D { .. } | super::composition::VideoEffect::Mosaic { .. } | super::composition::VideoEffect::MosaicLegacy { .. } | super::composition::VideoEffect::DirectionalBlur { .. } | super::composition::VideoEffect::DirectionalBlurLegacy { .. } | super::composition::VideoEffect::FindEdges { .. } | super::composition::VideoEffect::Replicate { .. } | super::composition::VideoEffect::Median { .. } | super::composition::VideoEffect::CameraBlur { .. } | super::composition::VideoEffect::GaussianBlur { .. } | super::composition::VideoEffect::GaussianBlurLegacy { .. } | super::composition::VideoEffect::LumaKey { .. } | super::composition::VideoEffect::ChannelMix { .. } | super::composition::VideoEffect::ColorBalance { .. } | super::composition::VideoEffect::ColorTint { .. } | super::composition::VideoEffect::ColorLevels { .. } | super::composition::VideoEffect::ColorExtract { .. } | super::composition::VideoEffect::HorizontalFlip | super::composition::VideoEffect::VerticalFlip | super::composition::VideoEffect::BlackWhite | super::composition::VideoEffect::CineonConverter { .. }
 | super::composition::VideoEffect::VideoLimiter { .. }
 | super::composition::VideoEffect::ColorVignette { .. }
                                    | super::composition::VideoEffect::Lumetri { .. }
                                    | super::composition::VideoEffect::HslSecondary { .. }
                            )
                    })
}

/// None delegates unsupported visible media layers to the existing media decoder.
pub fn render_generated(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<GroupSurface>> {
    render_generated_images(composition, time_us, checkpoint, None)
}

pub fn render_generated_images(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
    image: Option<&mut dyn FnMut(&super::composition::SourceId) -> Result<GroupSurface>>,
) -> Result<Option<GroupSurface>> {
    render_media_layers(composition, time_us, checkpoint, image, None)
}

pub fn render_media_layers(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
    image: Option<&mut dyn FnMut(&super::composition::SourceId) -> Result<GroupSurface>>,
    video: Option<
        &mut dyn FnMut(&super::composition::SourceId, u64, bool) -> Result<GroupSurface>,
    >,
) -> Result<Option<GroupSurface>> {
    render_media_layers_with_library(composition, time_us, checkpoint, image, video, &[])
}

// Upstream temporal prefixes run without an external environment. Track-dependent effects
// therefore use their documented own-picture fallback instead of reading another track.
struct TemporalPrefixContext { burnin:Option<super::metadata_burnin::BurninValues>,duration:u64,fps:Option<f64>,progress:Option<f64>,source_progress:Option<f64> }
impl EffectTrackContext for TemporalPrefixContext{
 fn clip_progress_seconds(&self,seconds:f64)->Option<f64>{
  if !seconds.is_finite(){return None;}if let Some(progress)=self.source_progress{return Some(progress);}
  let duration=self.duration as f64/1_000_000.;Some(if duration<=1e-9{0.}else{(seconds/duration).clamp(0.,1.)})
 }
 fn clip_progress(&self,local:u64)->Option<f64>{Some(self.progress.unwrap_or_else(||(local as f64/self.duration.max(1)as f64).clamp(0.,1.)))}
 fn clip_duration_us(&self)->Option<u64>{Some(self.duration)}
 fn frame_rate(&self)->Option<f64>{self.fps}
 fn burnin_values(&self,_:u64)->Result<Option<super::metadata_burnin::BurninValues>>{Ok(self.burnin.clone())}
 fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>{Ok(None)}
 fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
}
enum TemporalClip<'a>{Video(&'a super::composition::VideoClip),Image(&'a super::composition::ImageClip),Text(&'a super::composition::TextClip),Shape(&'a super::graphic_timeline::GraphicTimelineClip)}
pub type ClipVideoDecoder<'a> = dyn FnMut(&super::composition::SourceId,super::video_clock::FrameTime,bool,&super::composition::CompositionClipId)->Result<GroupSurface> + 'a;

struct CompositionEffectContext<'a> {
 exact_sample:Option<EffectContextSample>,
 temporal_clip:Option<TemporalClip<'a>>,
 clip_duration_us:u64,
 composition:&'a Composition,time_us:u64,depth:u32,library:&'a [super::project_luts::ProjectLut],matrix:GroupAffine,
 image:Option<&'a mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>>,
 video:Option<&'a mut ClipVideoDecoder<'a>>,
}
struct SourceEffectContext<'a>{inner:CompositionEffectContext<'a>,clock:super::concat_project_transitions::EffectClock}
impl EffectTrackContext for SourceEffectContext<'_>{
 fn temporal_frame(&mut self,dt:f64,prefix:&[super::composition::VideoEffect],checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>{self.inner.temporal_frame(dt,prefix,checkpoint)}
 fn timeline_seconds(&self)->Option<f64>{Some(self.clock.timeline_tick as f64/254016000000.)}
 fn clip_duration_us(&self)->Option<u64>{Some((self.clock.duration_tick as f64/254016.).round().max(1.) as u64)}
 fn frame_rate(&self)->Option<f64>{self.inner.frame_rate()}
 fn motion_blur_span(&self)->Option<f64>{Some(1./(self.frame_rate()?*(self.clock.duration_tick as f64/254016000000.)).max(1.))}
 fn clip_progress(&self,_:u64)->Option<f64>{Some(self.clock.progress())}
 fn clip_progress_seconds(&self,_:f64)->Option<f64>{Some(self.clock.progress())}
 fn burnin_values(&self,local:u64)->Result<Option<super::metadata_burnin::BurninValues>>{self.inner.burnin_values(local)}
 fn track(&mut self,index:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>{self.inner.track(index,checkpoint)}
 fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){self.inner.layer_to_output(x,y)}
}
impl CompositionEffectContext<'_> {
 fn burnin_at(&self,time_us:u64,local_us:u64)->Result<Option<super::metadata_burnin::BurninValues>>{
 let(clip_id,source_id,source_us)=match self.temporal_clip.as_ref(){
 Some(TemporalClip::Video(clip))=>{let Some(source)=super::video_clock::source_tick(&clip.placement,clip.playback_mode,time_us)?else{return Ok(None)};(&clip.id,&clip.source_id,source)},
 Some(TemporalClip::Image(clip))=>(&clip.id,&clip.source_id,local_us),Some(TemporalClip::Text(_))|Some(TemporalClip::Shape(_))|None=>return Ok(None)};
 Ok(Some(super::metadata_burnin::BurninValues::from_composition(self.composition,clip_id,source_id,time_us,source_us,local_us)?))
 }
}
impl EffectTrackContext for CompositionEffectContext<'_> {
    fn timeline_seconds(&self)->Option<f64>{Some(self.exact_sample.map(|sample|sample.timeline_tick as f64/254016000000.).unwrap_or(self.time_us as f64/1000000.))}
 fn burnin_values(&self,local_us:u64)->Result<Option<super::metadata_burnin::BurninValues>>{
  if let Some(sample)=self.exact_sample {
   let (clip_id,source_id)=match self.temporal_clip.as_ref(){
    Some(TemporalClip::Video(clip))=>(&clip.id,&clip.source_id),
    Some(TemporalClip::Image(clip))=>(&clip.id,&clip.source_id),
    Some(TemporalClip::Text(_))|Some(TemporalClip::Shape(_))|None=>return Ok(None),
   };
   return Ok(Some(super::metadata_burnin::BurninValues::from_composition_at_ticks(self.composition,clip_id,source_id,sample.timeline_tick,sample.clocks.frame,sample.clocks.generator as f64/254_016_000_000.0)?));
  }
  self.burnin_at(self.time_us,local_us)
 }
 fn stabilizer_path(&mut self,p:&super::warp_stabilizer::WarpStabilizer,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::warp_stabilizer::StabilizerPath>>{
 let(source_id,start)=match self.temporal_clip.as_ref(){Some(TemporalClip::Video(c))=>(&c.source_id,c.placement.timeline_start_tick),Some(TemporalClip::Image(c))=>(&c.source_id,c.timeline_start_tick),Some(TemporalClip::Text(_))|Some(TemporalClip::Shape(_))|None=>return Ok(None)};
 let width=self.composition.sources.get(source_id).ok_or_else(||anyhow::anyhow!("missing Warp Stabilizer source"))?.width;
 let seconds=self.time_us.saturating_sub(start) as f64/1000000.;let duration=self.clip_duration_us as f64/1000000.;let fps=self.frame_rate().unwrap_or(24.);
 let checks=std::cell::RefCell::new(checkpoint);
 super::warp_stabilizer::analyse_clip(seconds,duration,fps,width,p.tracking_method(),p.detailed,&mut |dt,scale| {
 let mut check=||(checks.borrow_mut())();let Some(frame)=self.temporal_frame(dt,&[],&mut check)?else{return Ok(None)};
 let n=super::auto_reframe::analysis_decimation(frame.dimensions().0,width,scale)?;let frame=frame.box_decimated(n,&mut check)?;
 let(w,h)=frame.dimensions();let mut gray=Vec::with_capacity(w*h);
 for row in frame.pixels().chunks(w){check()?;for v in row{let luma=0.2126*v[0]+0.7152*v[1]+0.0722*v[2];gray.push(super::basic_grade::display(luma.clamp(0.,1.)));}}
 Ok(Some(super::motion_tracker::Gray::new(w,h,gray)))
 },&mut ||(checks.borrow_mut())())
 }

 fn auto_reframe_environment(&mut self,preset:u32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::auto_reframe::AutoReframeEnvironment>>{
 let(source_id,start)=match self.temporal_clip.as_ref(){Some(TemporalClip::Video(c))=>(&c.source_id,c.placement.timeline_start_tick),Some(TemporalClip::Image(c))=>(&c.source_id,c.timeline_start_tick),Some(TemporalClip::Text(_))|Some(TemporalClip::Shape(_))|None=>return Ok(None)};
 let source=self.composition.sources.get(source_id).ok_or_else(||anyhow::anyhow!("missing Auto Reframe source"))?;let width=source.width;let sequence=(self.composition.canvas.width,self.composition.canvas.height,width);let seconds=self.time_us.saturating_sub(start)as f64/1000000.;let duration=self.clip_duration_us as f64/1000000.;let fps=self.frame_rate().unwrap_or(24.);
 let checks=std::cell::RefCell::new(checkpoint);let mut analysis=super::auto_reframe::AutoReframeAnalysis::default();
 let mut source_frame=|dt:f64,scale:f32|->Result<Option<super::linear_color_frame::LinearColorFrame>>{
  let mut check=||(checks.borrow_mut())();let Some(frame)=self.temporal_frame(dt,&[],&mut check)?else{return Ok(None)};let n=super::auto_reframe::analysis_decimation(frame.dimensions().0,width,scale)?;Ok(Some(frame.box_decimated(n,&mut check)?))
 };
 let subject=analysis.temporal_subject(preset,seconds,duration,fps,width,&mut source_frame,&mut||(checks.borrow_mut())())?;
 Ok(Some(super::auto_reframe::AutoReframeEnvironment{subject,sequence}))
 }
 fn temporal_frame(&mut self,dt:f64,prefix:&[super::composition::VideoEffect],checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>>{
 if self.exact_sample.is_some()||matches!(self.temporal_clip.as_ref(),Some(TemporalClip::Video(clip)) if clip.placement.concat_retime.is_some()){return self.temporal_frame_exact(dt,prefix,checkpoint);}
 let delta=(dt*1000000.).round();let shifted=self.time_us as i128+delta as i128;if shifted<0||shifted>u64::MAX as i128{return Ok(None);}let shifted=shifted as u64;checkpoint()?;
 let(mut child,local)=match self.temporal_clip.as_ref(){
 Some(TemporalClip::Video(clip))=>{let Some(source)=super::video_clock::source_tick(&clip.placement,clip.playback_mode,shifted)?else{return Ok(None);};let Some(decode)=self.video.as_mut()else{return Ok(None);};(decode(&clip.source_id,super::video_clock::FrameTime::Microseconds(source),true,&clip.id)?,shifted.saturating_sub(clip.placement.timeline_start_tick))},
 Some(TemporalClip::Image(clip))=>{if shifted<clip.timeline_start_tick||shifted>=clip.timeline_start_tick+clip.duration_ticks{return Ok(None);}let Some(decode)=self.image.as_mut()else{return Ok(None);};(decode(&clip.source_id)?,shifted-clip.timeline_start_tick)},
 Some(TemporalClip::Shape(clip))=>{let Some(local)=clip.local_time(shifted)?else{return Ok(None)};let Some(shape)=&clip.concat_shape else{return Ok(None)};(shape.render((clip.width,clip.height),checkpoint)?,local)},
 Some(TemporalClip::Text(clip))=>{let Some(style)=&clip.concat_title else{return Ok(None)};let Some((picture,_))=super::concat_title::picture(clip,style,(self.composition.canvas.width,self.composition.canvas.height),shifted,checkpoint)?else{return Ok(None)};(picture,shifted-clip.timeline_start_tick)},
 None=>return Ok(None),
 };
 let transform=match self.temporal_clip.as_ref(){Some(TemporalClip::Video(clip))=>&clip.transform,Some(TemporalClip::Image(clip))=>&clip.transform,Some(TemporalClip::Text(clip))=>&clip.transform,Some(TemporalClip::Shape(clip))=>&clip.transform,None=>return Ok(None)};
 super::concat_picture::prepare_transform_source(transform,&mut child,checkpoint)?;
 let prefix:Vec<_>=prefix.iter().filter(|e|!matches!(e,super::composition::VideoEffect::PosterizeTime{..}|super::composition::VideoEffect::Echo{..}|super::composition::VideoEffect::AutoReframe{..}|super::composition::VideoEffect::WarpStabilizer{..})).cloned().collect();
 let mut burnin=self.burnin_at(shifted,local)?;if let Some(values)=burnin.as_mut(){values.has_environment=false;}
 Ok(Some(apply_effect_stack_retained(&mut child,&prefix,local,self.library,Some(&mut TemporalPrefixContext{burnin,duration:self.clip_duration_us,fps:self.frame_rate(),progress:None,source_progress:None}),checkpoint)?))
 }

 fn frame_rate(&self)->Option<f64>{let rate=self.composition.canvas.frame_rate();Some(rate.numerator as f64/rate.denominator as f64)}
 fn clip_duration_us(&self)->Option<u64>{Some(self.clip_duration_us)}
 fn track(&mut self,index:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Option<super::linear_color_frame::LinearColorFrame>> {
  let visual=self.composition.tracks.iter().enumerate().filter(|(_,t)|matches!(t,CompositionTrack::Video{..}|CompositionTrack::Image{..}|CompositionTrack::Color{..}|CompositionTrack::Graphic{..})).nth(index).map(|(i,_)|i);
  let Some(index)=visual else{return Ok(None);};
  let mut working=None;
  let _surface=render_media_layers_contextual(self.composition,self.time_us,checkpoint,
   self.image.as_mut().map(|d|&mut **d as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),
   self.video.as_mut().map(|d|&mut **d as &mut ClipVideoDecoder<'_>),self.library,self.depth+1,Some(index),Some(&mut working),None)?;
  Ok(working)
 }
 fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){let p=self.matrix.point([x,y]);(p[0],p[1])}
}

pub fn render_media_layers_with_library(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
    image: Option<&mut dyn FnMut(&super::composition::SourceId) -> Result<GroupSurface>>,
    video: Option<
        &mut dyn FnMut(&super::composition::SourceId, u64, bool) -> Result<GroupSurface>,
    >,
    library: &[super::project_luts::ProjectLut],
) -> Result<Option<GroupSurface>> {
    render_media_layers_context(composition,time_us,checkpoint,image,video,library,0,None,None,None)
}

/// Export supplies an explicit exact caption sample; arbitrary-us preview keeps its clock.
pub fn render_media_layers_with_library_at(
    composition:&Composition,time_us:u64,checkpoint:&mut dyn FnMut()->Result<()>,
    image:Option<&mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>>,
    video:Option<&mut dyn FnMut(&super::composition::SourceId,u64,bool)->Result<GroupSurface>>,
    library:&[super::project_luts::ProjectLut],caption_time:i128,
)->Result<Option<GroupSurface>>{
    render_media_layers_context(composition,time_us,checkpoint,image,video,library,0,None,None,Some(caption_time))
}

/// Renderer admission carrying the exact video clip through temporal and transition reads.
pub fn render_media_layers_with_library_at_contextual(
 composition:&Composition,time_us:u64,checkpoint:&mut dyn FnMut()->Result<()>,
 image:Option<&mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>>,
 video:Option<&mut ClipVideoDecoder<'_>>,library:&[super::project_luts::ProjectLut],caption_time:i128,
)->Result<Option<GroupSurface>>{render_media_layers_contextual(composition,time_us,checkpoint,image,video,library,0,None,None,Some(caption_time))}

// Concat uploads SDR bytes with BT.1886 gamma 2.4; native frames use sRGB.
fn source_stage_gamma(frame:&mut super::linear_color_frame::LinearColorFrame,to_source:bool,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 frame.map_premultiplied(&mut|mut p|{let alpha=p[3];if alpha>0.{for value in &mut p[..3]{let straight=*value/alpha;*value=if to_source{super::basic_grade::display(straight).max(0.).powf(2.4)*alpha}else{super::basic_grade::linear(straight.max(0.).powf(1./2.4))*alpha};}}p},checkpoint)
}
fn composite_source_stage(base:&mut super::linear_color_frame::LinearColorFrame,source:&super::linear_color_frame::LinearColorFrame,blend:BlendMode,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
 anyhow::ensure!(base.dimensions()==source.dimensions(),"source stage dimensions differ");
 anyhow::ensure!(matches!(blend,BlendMode::Normal|BlendMode::Multiply|BlendMode::Screen|BlendMode::Addition|BlendMode::Lighten|BlendMode::Darken),"unsupported source stage blend");
 let mut index=0;
 base.map_premultiplied(&mut|under|{let over=source.pixels()[index];index+=1;let alpha=over[3];if alpha<=0.{return under;}let mut out=under;for k in 0..3{out[k]=match blend{BlendMode::Normal=>over[k]+under[k]*(1.-alpha),BlendMode::Multiply=>over[k]*under[k]+under[k]*(1.-alpha),BlendMode::Screen=>over[k]*(1.-under[k])+under[k],BlendMode::Addition=>over[k]+under[k],BlendMode::Lighten=>(over[k]/alpha).max(under[k])*alpha+under[k]*(1.-alpha),BlendMode::Darken=>(over[k]/alpha).min(under[k])*alpha+under[k]*(1.-alpha),_=>unreachable!()};}out[3]=alpha+under[3]*(1.-alpha);out},checkpoint)
}

fn transition_side<'ctx>(
 composition:&'ctx Composition,clip:Option<&'ctx super::composition::VideoClip>,time_us:u64,exact:i128,transitions:&[super::composition::ClipTransition],clips:&[super::composition::VideoClip],held_tail:bool,timeline_exact:i128,force_opacity:bool,source_override:Option<i128>,
 video:&'ctx mut ClipVideoDecoder<'ctx>,
 image:Option<&'ctx mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>>,
 library:&'ctx [super::project_luts::ProjectLut],depth:u32,
 checkpoint:&mut dyn FnMut()->Result<()>,
)->Result<super::linear_color_frame::LinearColorFrame>{
 use super::{linear_color_frame::LinearColorFrame,composition::AnimatableValue};
 let empty=GroupSurface::transparent(composition.canvas.width as usize,composition.canvas.height as usize)?;
 let mut output=LinearColorFrame::from_surface(&empty,checkpoint)?;
 let Some(clip)=clip.filter(|clip|clip.enabled)else{return Ok(output)};
 let Some(clocks)=super::video_effect_timing::sample(clip,exact)?else{return Ok(output)};
 let source_exact=source_override.unwrap_or(clocks.frame as i128);
 let source=if clip.placement.concat_retime.is_some(){super::video_clock::FrameTime::SourceTicks(i64::try_from(source_exact)?)}else{super::video_clock::FrameTime::Microseconds(u64::try_from(source_exact/254016)?)};
 let mut child=video(&clip.source_id,source,true,&clip.id)?;
 let preparation_offset=super::concat_picture::prepare_transform_source(&clip.transform,&mut child,checkpoint)?;
 let local=clocks.generator as f64/254016000000.;
 let sample=|value:&AnimatableValue|->Result<f64>{Ok(match value{AnimatableValue::Constant{value}=>*value,AnimatableValue::Keyframes{track}=>track.sample_seconds(local)?})};
 let source_pose=super::concat_project_transitions::source_pose(clip,clips,transitions,&composition.canvas,exact,held_tail)?;
 let(x,y,sx,sy,rotation,opacity)=if let Some(pose)=source_pose{(pose.x,pose.y,pose.scale_x,pose.scale_y,pose.rotation_degrees,pose.opacity)}else{let(x,y)=if let Some(position)=&clip.transform.position{let point=position.compile().map_err(anyhow::Error::msg)?.sample_seconds(local).map_err(anyhow::Error::msg)?;(point.x,point.y)}else{(sample(&clip.transform.x)?,sample(&clip.transform.y)?)};(x,y,sample(&clip.transform.scale_x)?,sample(&clip.transform.scale_y)?,sample(&clip.transform.rotation_degrees)?,sample(&clip.opacity)?)};
 let matrix=GroupAffine::new([output.dimensions().0 as f64,output.dimensions().1 as f64],[child.width as f64,child.height as f64],[clip.transform.anchor_x,clip.transform.anchor_y],[x+preparation_offset[0],y+preparation_offset[1]],[sx,sy],rotation)?;
 let matrix=if let Some(motion)=&clip.filmcraft_motion {GroupAffine(motion.parameters.matrix((composition.canvas.width,composition.canvas.height),(child.width as u32,child.height as u32),clocks.parameters,motion.enabled,motion.scale_to_frame)?)}else{matrix};
 let child=if clip.effects.is_empty(){LinearColorFrame::from_surface(&child,checkpoint)?}else{
  let mut context=CompositionEffectContext{
   exact_sample:Some(EffectContextSample{timeline_tick:i64::try_from(exact)?,clocks}),
   temporal_clip:Some(TemporalClip::Video(clip)),clip_duration_us:clip.placement.timeline_duration_ticks()?,composition,time_us,depth,library,matrix,image,video:Some(video),
  };
  if let Some(clock)=super::concat_project_transitions::effect_clock(clip,clips,transitions,&composition.canvas,timeline_exact,held_tail)?{
   let mut source=SourceEffectContext{inner:context,clock};apply_effect_stack_seeded_at_time(&mut child,&clip.effects,0,super::animation_sample_time::AnimationSampleTime::SourceTicks(clock.local_tick()),clock.local_tick() as f64/254016000000.,library,Some(&mut source),checkpoint,None)?
  }else{apply_effect_stack_seeded_at_time(&mut child,&clip.effects,0,super::video_effect_timing::parameter_time(clip,clocks),local,library,Some(&mut context),checkpoint,None)?}
 };
 output.composite_working(&child,matrix,if force_opacity{1.}else{opacity.clamp(0.,1.)as f32},BlendMode::Normal,checkpoint)?;
 if let Some(wipe)=super::concat_project_transitions::source_wipe(clip,clips,transitions,&composition.canvas,timeline_exact,held_tail)?{
  let [a,b,c,d,tx,ty]=matrix.0;let det=a*d-b*c;
  anyhow::ensure!(det.is_finite()&&det!=0.,"invalid source wipe geometry");
  let(width,_)=output.dimensions();let picture_width=child.dimensions().0 as f64;let mut index=0;
  output.map_premultiplied(&mut|pixel|{let x=(index%width)as f64+0.5;let y=(index/width)as f64+0.5;index+=1;
   let u=((d*(x-tx)-c*(y-ty))/det/picture_width)as f32;
   if wipe.keeps(u){pixel}else{[0.;4]}
  },checkpoint)?;
 }

 if let Some(shade)=super::concat_project_transitions::source_colour(clip,clips,transitions,&composition.canvas,timeline_exact,held_tail)?{
  output.map_premultiplied(&mut|mut pixel|{let alpha=pixel[3];if alpha>0.{for value in &mut pixel[..3]{let source=super::basic_grade::display(*value/alpha).max(0.).powf(2.4);let source=source*shade.scale+shade.offset;*value=super::basic_grade::linear(source.max(0.).powf(1./2.4))*alpha;}}pixel},checkpoint)?;
 }
 let _=time_us;
 Ok(output)
}

fn render_media_layers_context(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
    image: Option<&mut dyn FnMut(&super::composition::SourceId) -> Result<GroupSurface>>,
    mut video: Option<
        &mut dyn FnMut(&super::composition::SourceId, u64, bool) -> Result<GroupSurface>,
    >,
    library: &[super::project_luts::ProjectLut],
    depth:u32,
    only:Option<usize>,
    working:Option<&mut Option<super::linear_color_frame::LinearColorFrame>>,
    caption_time:Option<i128>,
) -> Result<Option<GroupSurface>> {
    let present=video.is_some();
    let mut decode=|source:&super::composition::SourceId,time:super::video_clock::FrameTime,hold,_clip:&super::composition::CompositionClipId|video.as_mut().expect("video decoder admitted")(source,time.legacy_microseconds()?,hold);
    render_media_layers_contextual(composition,time_us,checkpoint,image,if present{Some(&mut decode)}else{None},library,depth,only,working,caption_time)
}

fn render_media_layers_contextual(
    composition: &Composition,
    time_us: u64,
    checkpoint: &mut dyn FnMut() -> Result<()>,
    mut image: Option<&mut dyn FnMut(&super::composition::SourceId) -> Result<GroupSurface>>,
    mut video: Option<
        &mut ClipVideoDecoder<'_>,
    >,
    library: &[super::project_luts::ProjectLut],
    depth:u32,
    only:Option<usize>,
    mut working:Option<&mut Option<super::linear_color_frame::LinearColorFrame>>,
    caption_time:Option<i128>,
) -> Result<Option<GroupSurface>> {
    composition.validate_native_surface()?;
    ensure!(
        composition.time_base == 1_000_000,
        "native surface requires microsecond clock"
    );
    if composition.canvas.background_mode != CanvasBackgroundMode::Color {
        return Ok(None);
    }
    for track in &composition.tracks {
        match track {
            // Empty lanes need no decoder; new timelines intentionally retain their empty lanes.
            CompositionTrack::Video { clips, .. } if clips.is_empty() => {},
            CompositionTrack::Image { clips, .. } if clips.is_empty() => {},
            CompositionTrack::Graphic { clips, .. } if clips.iter().all(|clip|supported_effect_stack(&clip.effects)) => {},
            CompositionTrack::Graphic { hidden:true, .. } => {},
            CompositionTrack::Color { .. } | CompositionTrack::Audio { .. } | CompositionTrack::Text { .. } => {}
            CompositionTrack::Image { clips, .. } if image.is_some() && clips.iter().all(|clip|supported_effect_stack(&clip.effects)) => {}
            CompositionTrack::Video {
                clips, transitions, ..
            } if video.is_some()
                && transitions.iter().all(|transition|super::transition_preview_sources::resolved_span(clips,transition).is_ok()&&matches!(transition.kind,super::composition::TransitionKind::ConcatDissolve|super::composition::TransitionKind::ConcatFadeBlack|super::composition::TransitionKind::ConcatFadeWhite|super::composition::TransitionKind::Procedural|super::composition::TransitionKind::Dissolve|super::composition::TransitionKind::FilmDissolve|super::composition::TransitionKind::AdditiveDissolve|super::composition::TransitionKind::FadeWhite|super::composition::TransitionKind::FadeBlack|super::composition::TransitionKind::WipeLeft|super::composition::TransitionKind::WipeRight|super::composition::TransitionKind::WipeUp|super::composition::TransitionKind::WipeDown))
                && clips.iter().all(|clip| {
                    supported_effect_stack(&clip.effects) && clip.stabilization == super::composition::StabilizationSpec::Disabled
                        && clip.frame_interpolation
                            == super::composition::FrameInterpolation::Duplicate
                }) => {}
            CompositionTrack::Video { hidden: true, .. }
            | CompositionTrack::Image { hidden: true, .. } => {}
            _ => return Ok(None),
        }
    }
    let canvas = composition.canvas;
    let mut surface = GroupSurface::transparent(canvas.width as usize, canvas.height as usize)?;
    if depth>8 {if let Some(output)=working.as_deref_mut(){*output=Some(super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?);}return Ok(Some(surface));}
    let mut background = canvas.background;
    if only.is_some(){background.red=0.;background.green=0.;background.blue=0.;background.alpha=0.;}
    let pixel = [
        background.red,
        background.green,
        background.blue,
        background.alpha,
    ]
    .map(|channel| (channel * 255.0).round() as u8);
    for row in surface.rgba.chunks_mut(surface.width * 4) {
        checkpoint()?;
        for destination in row.chunks_exact_mut(4) {
            destination.copy_from_slice(&pixel);
        }
    }
    // Source Concat animation is composed in linear light, including other
    // layers in this scene; retain that canvas until the final encoding.
    let concat_scene=composition.tracks.iter().any(|track|match track{
        CompositionTrack::Video{clips,..}=>clips.iter().any(|clip|(clip.transform.concat_animation.is_some()||clip.transform.concat_preparation.is_some())),
        CompositionTrack::Image{clips,..}=>clips.iter().any(|clip|(clip.transform.concat_animation.is_some()||clip.transform.concat_preparation.is_some())),
        _=>false,
    });
    let mut working_canvas=if concat_scene || working.is_some() || (only.is_none() && composition.caption_tracks.iter().any(|track|!track.hidden && !track.clips.is_empty())){Some(super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?)}else{None};
    let mut source_treated_now=false;
    for(index,track)in composition.tracks.iter().enumerate(){
        if only.is_some_and(|only|only!=index){continue;}
        if let CompositionTrack::Graphic{hidden,clips,..}=track{
            if *hidden&&only.is_none(){continue;}
            for clip in clips{
                if super::concat_treatment::has_shader(&clip.effects)&&super::concat_treatment::sample_clock(clip,&canvas,caption_time.unwrap_or(time_us as i128*254016))?.is_some(){source_treated_now=true;}
            }
        }
    }
    // Track zero is topmost in the authored flat composition.
    for (track_index,track) in composition.tracks.iter().enumerate().rev() {
        if only.is_some_and(|index|index!=track_index){continue;}
        if let CompositionTrack::Text {hidden,clips,..}=track {
            if *hidden && only.is_none(){continue;}
            for clip in clips {
                if let Some(style)=&clip.concat_title {
                    let mut effects=|picture:&mut GroupSurface,stack:&[super::composition::VideoEffect],local:u64,matrix:GroupAffine,check:&mut dyn FnMut()->Result<()>|{
                        let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:Some(TemporalClip::Text(clip)),clip_duration_us:clip.timeline_end_tick-clip.timeline_start_tick,composition,time_us,depth,library,matrix,
                            image:image.as_mut().map(|d|&mut **d as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),video:video.as_mut().map(|d|&mut **d as &mut ClipVideoDecoder<'_>)};
                        apply_effect_stack_retained(picture,stack,local,library,Some(&mut context),check).map(|_|())
                    };
                    super::concat_title::composite(clip,style,&mut surface,time_us,&mut effects,checkpoint)?;
                    if let Some(output)=working_canvas.as_mut(){*output=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                if let Some(frame)=super::legacy_text_surface::render(clip,(canvas.width,canvas.height),time_us,checkpoint)? {
                    if let Some(output)=working_canvas.as_mut(){output.composite_working(&frame,GroupAffine::IDENTITY,1.,clip.blend_mode.unwrap_or(BlendMode::Normal),checkpoint)?;}
                    surface.composite_checked(&frame.to_surface(checkpoint)?,GroupAffine::IDENTITY,1.,clip.blend_mode.unwrap_or(BlendMode::Normal),checkpoint)?;
                }
            }
        }
        if let CompositionTrack::Graphic {hidden,clips,..}=track {
            if *hidden && only.is_none(){continue;}
            for clip in clips {
                if let Some(treatment)=&clip.concat_treatment{
                    let Some(clock)=super::concat_treatment::sample_clock(clip,&canvas,caption_time.unwrap_or(time_us as i128*254016))?else{continue;};
                    let mut apply=|picture:&mut GroupSurface,stack:&[super::composition::VideoEffect],check:&mut dyn FnMut()->Result<()>|{
                        let inner=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:(clock.duration_tick as f64/254016.).round().max(1.)as u64,composition,time_us,depth,library,matrix:GroupAffine::IDENTITY,
                        image:image.as_mut().map(|d|&mut **d as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),video:video.as_mut().map(|d|&mut **d as &mut ClipVideoDecoder<'_>)};
                        let mut context=SourceEffectContext{inner,clock};
                        // Native absolute keyframes retain their authored timeline;
                        // source normalized keys read the quantized progress context.
                        apply_effect_stack_with_context(picture,stack,time_us,library,Some(&mut context),check)
                    };
                    treatment.composite_with_strength(&mut surface,super::concat_treatment::sample_strength(treatment,clock)?,&clip.effects,&mut apply,checkpoint)?;
                    if let Some(output)=working_canvas.as_mut(){*output=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                if clip.local_time(time_us)?.is_none(){continue;}
                if clip.concat_shape.is_some(){
                    let mut effects=|picture:&mut GroupSurface,stack:&[super::composition::VideoEffect],local:u64,matrix:GroupAffine,check:&mut dyn FnMut()->Result<()>|{
                        let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:Some(TemporalClip::Shape(clip)),clip_duration_us:clip.timeline_end_tick-clip.timeline_start_tick,composition,time_us,depth,library,matrix,
                        image:image.as_mut().map(|d|&mut **d as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),
                        video:video.as_mut().map(|d|&mut **d as &mut ClipVideoDecoder<'_>)};
                        apply_effect_stack_with_context(picture,stack,local,library,Some(&mut context),check)
                    };
                    super::concat_shape::composite(clip,&mut surface,time_us,&mut effects,checkpoint)?;
                    if working_canvas.is_some(){working_canvas=Some(super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?);}
                    continue;
                }
                let instance=CompositionInstance{id:clip.id.as_str().into(),animation_offset_us:clip.animation_offset_us,timing:CompositionReferenceTiming{composition:"graphic".into(),timeline_start_us:clip.timeline_start_tick,source_in_us:0,source_out_us:clip.timeline_end_tick-clip.timeline_start_tick},transform:clip.transform.clone(),opacity:clip.opacity.clone(),blend_mode:BlendMode::Normal};
                let pose=instance.pose(time_us)?;
                let matrix=GroupAffine::new([surface.width as f64,surface.height as f64],[clip.width as f64,clip.height as f64],[clip.transform.anchor_x,clip.transform.anchor_y],[pose.x,pose.y],[pose.scale_x,pose.scale_y],pose.rotation_degrees)?;
                let mut effects=|frame:&mut super::linear_color_frame::LinearColorFrame,stack:&[super::composition::VideoEffect],local:u64,check:&mut dyn FnMut()->Result<()>|{
                    let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:clip.timeline_end_tick-clip.timeline_start_tick,composition,time_us,depth,library,matrix,
                    image:image.as_mut().map(|d|&mut **d as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),
                    video:video.as_mut().map(|d|&mut **d as &mut ClipVideoDecoder<'_>)};
                    apply_effect_stack_working(frame,stack,local,library,Some(&mut context),check)
                };
                if let Some(frame)=clip.render_timeline((canvas.width,canvas.height),time_us,&mut effects,checkpoint)? {
                    if let Some(output)=working_canvas.as_mut(){output.composite_working(&frame,GroupAffine::IDENTITY,1.,BlendMode::Normal,checkpoint)?;}
                    surface.composite_checked(&frame.to_surface(checkpoint)?,GroupAffine::IDENTITY,1.,BlendMode::Normal,checkpoint)?;
                }
            }
        }
        if let CompositionTrack::Video {
            hidden,
            clips,
            transitions,
            ..
        } = track
        {
            if *hidden && only.is_none(){continue;}
            let exact=caption_time.unwrap_or(time_us as i128*254016);
            let active=transitions.iter().filter(|transition|transition.concat_overlap.map_or(true,|overlap|overlap.legacy.is_none()&&!source_treated_now)).find(|transition|super::transition_preview_sources::resolved_span(clips,transition).is_ok_and(|(start,duration)|exact>=start as i128&&exact<(start as i128+duration as i128)));
            if let Some(transition)=active{
                let (start,duration)=super::transition_preview_sources::resolved_span(clips,transition)?;
                let decoder=video.as_mut().expect("video renderer admitted");
                let mut above=vec![];
                let(a,b)=if let Some(layers)=super::concat_project_transitions::visual_layers(clips,transitions,&composition.canvas,exact)?{
                    let lane=super::concat_project_transitions::transition_lane(transitions,transition)?;
                    let mut from=if let Some(canvas)=&working_canvas{canvas.clone()}else{super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?};
                    source_stage_gamma(&mut from,true,checkpoint)?;
                    let mut incoming=None;
                    for layer in layers {
                        let clip=&clips[layer.clip_index];
                        let picture_exact=if layer.held{clip.placement.timeline_end_tick()? as i128*254016-1}else{exact};
                        let mut frame=transition_side(composition,Some(clip),time_us,picture_exact,transitions,clips,layer.held,exact,layer.lane==lane,Some(layer.source_tick),&mut **decoder,image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),library,depth,checkpoint)?;
                        source_stage_gamma(&mut frame,true,checkpoint)?;
                        if layer.lane!=lane && layer.ramp<1.{frame.map_premultiplied(&mut|p|p.map(|v|v*layer.ramp),checkpoint)?;}
                        if layer.lane<lane{composite_source_stage(&mut from,&frame,clip.blend_mode,checkpoint)?;}else if layer.lane==lane{incoming=Some((frame,clip.blend_mode));}else{above.push((frame,clip.blend_mode));}
                    }
                    let mut to=from.clone();
                    if let Some((frame,blend))=incoming{composite_source_stage(&mut to,&frame,blend,checkpoint)?;}
                    (from,to)
                }else{
                    let outgoing_exact=super::concat_project_transitions::outgoing_time(composition,transition,clips,exact)?;
                    let a=transition_side(composition,clips.iter().find(|clip|Some(&clip.id)==transition.from_clip_id.as_ref()),time_us,outgoing_exact,transitions,clips,outgoing_exact!=exact,exact,false,None,&mut **decoder,image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),library,depth,checkpoint)?;
                    let b=transition_side(composition,clips.iter().find(|clip|Some(&clip.id)==transition.to_clip_id.as_ref()),time_us,exact,transitions,clips,false,exact,false,None,&mut **decoder,image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),library,depth,checkpoint)?;(a,b)
                };
                let progress=((exact-start as i128)as f64/duration as f64)as f32;
                let mut mixed=match transition.kind {
                    super::composition::TransitionKind::ConcatDissolve => super::concat_transitions::render(&a,&b,progress,transition.reverse,super::concat_transitions::Style::Dissolve,checkpoint)?,
                    super::composition::TransitionKind::ConcatFadeBlack => super::concat_transitions::render(&a,&b,progress,transition.reverse,super::concat_transitions::Style::Black,checkpoint)?,
                    super::composition::TransitionKind::ConcatFadeWhite => super::concat_transitions::render(&a,&b,progress,transition.reverse,super::concat_transitions::Style::White,checkpoint)?,
                    super::composition::TransitionKind::Procedural => super::transition_procedural::render(transition.kernel.as_ref().ok_or_else(||anyhow::anyhow!("procedural transition kernel missing"))?,&a,&b,progress,transition.reverse,checkpoint)?,
                    super::composition::TransitionKind::Dissolve => super::transition_pixels::cross_dissolve(&a,&b,progress,transition.reverse,checkpoint)?,
                    super::composition::TransitionKind::FadeBlack => super::transition_dissolves::render(&a,&b,progress,transition.reverse,super::transition_dissolves::DissolveStyle::Black,checkpoint)?,
                    super::composition::TransitionKind::FilmDissolve => super::transition_dissolves::render(&a,&b,progress,transition.reverse,super::transition_dissolves::DissolveStyle::Film,checkpoint)?,
                    super::composition::TransitionKind::AdditiveDissolve => super::transition_dissolves::render(&a,&b,progress,transition.reverse,super::transition_dissolves::DissolveStyle::Additive,checkpoint)?,
                    super::composition::TransitionKind::FadeWhite => super::transition_dissolves::render(&a,&b,progress,transition.reverse,super::transition_dissolves::DissolveStyle::White,checkpoint)?,
                    kind => {
                        let direction=match kind {
                            super::composition::TransitionKind::WipeDown=>0,
                            super::composition::TransitionKind::WipeLeft=>1,
                            super::composition::TransitionKind::WipeUp=>2,
                            super::composition::TransitionKind::WipeRight=>3,
                            _=>anyhow::bail!("independent transition kernel unavailable"),
                        };
                        super::transition_wipe::render(&a,&b,progress,transition.reverse,direction,checkpoint)?
                    }
                };
                if transition.concat_overlap.is_some(){for(frame,blend)in above{composite_source_stage(&mut mixed,&frame,blend,checkpoint)?;}source_stage_gamma(&mut mixed,false,checkpoint)?;surface=mixed.to_surface(checkpoint)?;working_canvas=Some(mixed);}else{
                    if let Some(canvas)=working_canvas.as_mut(){canvas.composite_working(&mixed,GroupAffine::IDENTITY,1.,BlendMode::Normal,checkpoint)?;}
                    surface.composite_checked(&mixed.to_surface(checkpoint)?,GroupAffine::IDENTITY,1.,BlendMode::Normal,checkpoint)?;
                }
                continue;
            }
            if let Some(layers)=super::concat_project_transitions::visual_layers(clips,transitions,&composition.canvas,exact)?{
                let mut output=if let Some(canvas)=&working_canvas{canvas.clone()}else{super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?};
                source_stage_gamma(&mut output,true,checkpoint)?;
                for layer in layers {
                    let clip=&clips[layer.clip_index];
                    let picture_exact=if layer.held{clip.placement.timeline_end_tick()? as i128*254016-1}else{exact};
                    let decoder=video.as_mut().expect("video renderer admitted");
                    let mut frame=transition_side(composition,Some(clip),time_us,picture_exact,transitions,clips,layer.held,exact,false,Some(layer.source_tick),&mut **decoder,image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),library,depth,checkpoint)?;
                    source_stage_gamma(&mut frame,true,checkpoint)?;
                    if layer.ramp<1.{frame.map_premultiplied(&mut|p|p.map(|v|v*layer.ramp),checkpoint)?;}
                    composite_source_stage(&mut output,&frame,clip.blend_mode,checkpoint)?;
                }
                source_stage_gamma(&mut output,false,checkpoint)?;
                surface=output.to_surface(checkpoint)?;working_canvas=Some(output);continue;
            }
            for clip in clips.iter().filter(|clip| clip.enabled) {
                let Some(source_time) =
                    super::video_clock::source_tick(&clip.placement, clip.playback_mode, time_us)?
                else {
                    continue;
                };
                let precise=if clip.effect_timing.is_some(){let timeline=caption_time.unwrap_or(time_us as i128*254016);super::video_effect_timing::sample(clip,timeline)?.map(|clocks|Ok::<_,anyhow::Error>(EffectContextSample{timeline_tick:i64::try_from(timeline)?,clocks})).transpose()?}else{None};
                // Keep Filmcraft time exact until the decoder's microsecond boundary.
                let source_time = if clip.effect_timing.is_some() {
                    let Some(sample) = precise else { continue; };
                    u64::try_from(sample.clocks.frame / 254016)?
                } else { source_time };
                checkpoint()?;
                let pose=if let Some(source)=super::concat_project_transitions::source_pose(clip,clips,transitions,&composition.canvas,caption_time.unwrap_or(time_us as i128*254016),false)?{
                    super::nested_composition::NestedGroupPose{local_time_us:time_us-clip.placement.timeline_start_tick,x:source.x,y:source.y,scale_x:source.scale_x,scale_y:source.scale_y,rotation_degrees:source.rotation_degrees,opacity:source.opacity}
                }else{
                    let instance = CompositionInstance {
                    id: clip.id.as_str().into(),
                    animation_offset_us: 0,
                    timing: CompositionReferenceTiming {
                        composition: "video".into(),
                        timeline_start_us: clip.placement.timeline_start_tick,
                        source_in_us: 0,
                        source_out_us: clip.placement.timeline_duration_ticks()?,
                    },
                    transform: clip.transform.clone(),
                    opacity: clip.opacity.clone(),
                    blend_mode: clip.blend_mode,
                };
                    instance.pose(time_us)?
                };
                if pose.opacity == 0.0 || pose.scale_x == 0.0 || pose.scale_y == 0.0 {
                    continue;
                }
                let mut child = video.as_mut().expect("video renderer admitted")(
                    &clip.source_id,
                    super::video_clock::FrameTime::Microseconds(source_time),
                    true,
                    &clip.id,
                )?;
                if clip.effects.iter().all(|effect|matches!(effect,super::composition::VideoEffect::ConcatBypass{..}|super::composition::VideoEffect::NativeBypass{..})) && composite_source_picture(&clip.transform,&child,&mut surface,&pose,clip.blend_mode,false,checkpoint)?{
                    if let Some(canvas)=working_canvas.as_mut(){*canvas=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                let preparation_offset=super::concat_picture::prepare_transform_source(&clip.transform,&mut child,checkpoint)?;
                let matrix = GroupAffine::new(
                    [surface.width as f64, surface.height as f64],
                    [child.width as f64, child.height as f64],
                    [clip.transform.anchor_x, clip.transform.anchor_y],
                    [pose.x+preparation_offset[0], pose.y+preparation_offset[1]],
                    [pose.scale_x, pose.scale_y],
                    pose.rotation_degrees,
                )?;
                let matrix=if let Some(motion)=&clip.filmcraft_motion {let sample=precise.ok_or_else(||anyhow::anyhow!("Motion requires source clock"))?;GroupAffine(motion.parameters.matrix((composition.canvas.width,composition.canvas.height),(child.width as u32,child.height as u32),sample.clocks.parameters,motion.enabled,motion.scale_to_frame)?)}else{matrix};
                let mut context=CompositionEffectContext {exact_sample:precise,temporal_clip:Some(TemporalClip::Video(clip)),clip_duration_us:clip.placement.timeline_duration_ticks()?,composition,time_us,depth,library,matrix,
                    image:image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),
                    video:video.as_mut().map(|decoder|&mut **decoder as &mut ClipVideoDecoder<'_>)};
                let source_clock=if clip.effects.is_empty(){None}else{super::concat_project_transitions::effect_clock(clip,clips,transitions,&composition.canvas,caption_time.unwrap_or(time_us as i128*254016),false)?};
                let child_working=if let Some(clock)=source_clock{let mut source=SourceEffectContext{inner:context,clock};apply_effect_stack_seeded_at_time(&mut child,&clip.effects,0,super::animation_sample_time::AnimationSampleTime::SourceTicks(clock.local_tick()),clock.local_tick() as f64/254016000000.,library,Some(&mut source),checkpoint,None)?}else if let Some(sample)=precise{apply_effect_stack_seeded_at_time(&mut child,&clip.effects,0,super::video_effect_timing::parameter_time(clip,sample.clocks),sample.clocks.generator as f64/254016000000.,library,Some(&mut context),checkpoint,None)?}else{apply_effect_stack_retained(&mut child,&clip.effects,time_us-clip.placement.timeline_start_tick,library,Some(&mut context),checkpoint)?};
                if composite_source_picture(&clip.transform,&child,&mut surface,&pose,clip.blend_mode,true,checkpoint)?{
                    if let Some(canvas)=working_canvas.as_mut(){*canvas=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                if let Some(canvas)=working_canvas.as_mut(){canvas.composite_working(&child_working,matrix,pose.opacity as f32,clip.blend_mode,checkpoint)?;}
                surface.composite_checked(
                    &child,
                    matrix,
                    pose.opacity,
                    clip.blend_mode,
                    checkpoint,
                )?;
            }
        }
        if let CompositionTrack::Image {
            hidden,
            clips,
            ..
        } = track
        {
            if *hidden && only.is_none(){continue;}
            for clip in clips.iter().filter(|clip| {
                clip.enabled
                    && time_us >= clip.timeline_start_tick
                    && time_us < clip.timeline_start_tick + clip.duration_ticks
            }) {
                checkpoint()?;
                let instance = CompositionInstance {
                    id: clip.id.as_str().into(),
                    animation_offset_us: 0,
                    timing: CompositionReferenceTiming {
                        composition: "image".into(),
                        timeline_start_us: clip.timeline_start_tick,
                        source_in_us: 0,
                        source_out_us: clip.duration_ticks,
                    },
                    transform: clip.transform.clone(),
                    opacity: clip.opacity.clone(),
                    blend_mode: clip.blend_mode,
                };
                let pose = instance.pose(time_us)?;
                if pose.opacity == 0.0 || pose.scale_x == 0.0 || pose.scale_y == 0.0 {
                    continue;
                }
                let mut child = image.as_mut().expect("image renderer admitted")(&clip.source_id)?;
                if clip.effects.iter().all(|effect|matches!(effect,super::composition::VideoEffect::ConcatBypass{..}|super::composition::VideoEffect::NativeBypass{..})) && composite_source_picture(&clip.transform,&child,&mut surface,&pose,clip.blend_mode,false,checkpoint)?{
                    if let Some(canvas)=working_canvas.as_mut(){*canvas=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                let preparation_offset=super::concat_picture::prepare_transform_source(&clip.transform,&mut child,checkpoint)?;
                let matrix = GroupAffine::new(
                    [surface.width as f64, surface.height as f64],
                    [child.width as f64, child.height as f64],
                    [clip.transform.anchor_x, clip.transform.anchor_y],
                    [pose.x+preparation_offset[0], pose.y+preparation_offset[1]],
                    [pose.scale_x, pose.scale_y],
                    pose.rotation_degrees,
                )?;
                let mut context=CompositionEffectContext {exact_sample:None,temporal_clip:Some(TemporalClip::Image(clip)),clip_duration_us:clip.duration_ticks,composition,time_us,depth,library,matrix,
                    image:image.as_mut().map(|decoder|&mut **decoder as &mut dyn FnMut(&super::composition::SourceId)->Result<GroupSurface>),
                    video:video.as_mut().map(|decoder|&mut **decoder as &mut ClipVideoDecoder<'_>)};
                let child_working=apply_effect_stack_retained(&mut child,&clip.effects,time_us-clip.timeline_start_tick,library,Some(&mut context),checkpoint)?;
                if composite_source_picture(&clip.transform,&child,&mut surface,&pose,clip.blend_mode,true,checkpoint)?{
                    if let Some(canvas)=working_canvas.as_mut(){*canvas=super::linear_color_frame::LinearColorFrame::from_surface(&surface,checkpoint)?;}
                    continue;
                }
                if let Some(canvas)=working_canvas.as_mut(){canvas.composite_working(&child_working,matrix,pose.opacity as f32,clip.blend_mode,checkpoint)?;}
                surface.composite_checked(
                    &child,
                    matrix,
                    pose.opacity,
                    clip.blend_mode,
                    checkpoint,
                )?;
            }
        }
        if let CompositionTrack::Color {
            hidden,
            clips,
            ..
        } = track
        {
            if *hidden && only.is_none(){continue;}
            for clip in clips.iter().filter(|clip| {
                clip.enabled
                    && time_us >= clip.timeline_start_tick
                    && time_us < clip.timeline_end_tick
            }) {
                checkpoint()?;
                let local = time_us - clip.timeline_start_tick;
                let color = if let Some(animation) = &clip.color_animation {
                    PreparedColorTrack::new(ColorSampleRequest {
                        track: animation.clone(),
                        default: clip.color,
                        times_us: vec![],
                    })
                    .map_err(anyhow::Error::msg)?
                    .sample(local as f64)
                    .map_err(anyhow::Error::msg)?
                } else {
                    clip.color
                };
                let instance = CompositionInstance {
                    id: clip.id.as_str().into(),
                    animation_offset_us: 0,
                    timing: CompositionReferenceTiming {
                        composition: "solid".into(),
                        timeline_start_us: clip.timeline_start_tick,
                        source_in_us: 0,
                        source_out_us: clip.timeline_end_tick - clip.timeline_start_tick,
                    },
                    transform: clip.transform.clone(),
                    opacity: clip.opacity.clone(),
                    blend_mode: BlendMode::Normal,
                };
                let pose = instance.pose(time_us)?;
                if pose.opacity == 0.0 || pose.scale_x == 0.0 || pose.scale_y == 0.0 {
                    continue;
                }
                let mut child =
                    GroupSurface::transparent(clip.width as usize, clip.height as usize)?;
                for row in child.rgba.chunks_mut(child.width * 4) {
                    checkpoint()?;
                    for destination in row.chunks_exact_mut(4) {
                        destination.copy_from_slice(&[color.r, color.g, color.b, color.a]);
                    }
                }
                let matrix = GroupAffine::new(
                    [surface.width as f64, surface.height as f64],
                    [child.width as f64, child.height as f64],
                    [clip.transform.anchor_x, clip.transform.anchor_y],
                    [pose.x, pose.y],
                    [pose.scale_x, pose.scale_y],
                    pose.rotation_degrees,
                )?;
                if let Some(canvas)=working_canvas.as_mut(){let child_working=super::linear_color_frame::LinearColorFrame::from_surface(&child,checkpoint)?;canvas.composite_working(&child_working,matrix,pose.opacity as f32,BlendMode::Normal,checkpoint)?;}
                surface.composite_checked(
                    &child,
                    matrix,
                    pose.opacity,
                    BlendMode::Normal,
                    checkpoint,
                )?;
            }
        }
    }
    if only.is_none() && composition.caption_tracks.iter().any(|track|!track.hidden && !track.clips.is_empty()) {
        let frame=working_canvas.as_mut().ok_or_else(||anyhow::anyhow!("missing caption working canvas"))?;
        super::caption_render::render_tracks_at(composition,caption_time.unwrap_or(super::caption_time::CaptionTime::Microseconds(time_us).exact_ticks()),frame,checkpoint)?;
        surface=frame.to_surface(checkpoint)?;
    }
    if concat_scene {
        surface=working_canvas.as_ref().ok_or_else(||anyhow::anyhow!("missing Concat linear canvas"))?.to_surface(checkpoint)?;
    }
    if let Some(output)=working{*output=working_canvas;}
    Ok(Some(surface))
}

#[cfg(test)]
mod tests {
    use super::super::composition::{CanvasSpec, TransformSpec};
    use super::*;
    #[test]
    fn cineon_animation_uses_clip_local_time_after_offset() {
        let mut canvas = CanvasSpec::default(); canvas.width = 2; canvas.height = 2;
        let mut composition = Composition::new(canvas);
        composition.sources = serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":2000000,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"video","id":"videos","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":250000,"sourceOutTick":1250000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"cineon_converter","parameters":{"conversion":0,"rolloff":0},"animation":{"gamma":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1.0},{"tick":1000000,"value":3.0}]}}}}]}]}])).unwrap();
        for (timeline, gamma) in [(1000000, 1.0), (1500000, 2.0)] {
            let input = GroupSurface { width: 2, height: 2, rgba: [64, 128, 192, 255].repeat(4) };
            let mut expected = super::super::linear_color_frame::LinearColorFrame::from_surface(&input, &mut || Ok(())).unwrap();
            super::super::cineon_converter::CineonConverter { gamma, rolloff: 0.0, ..Default::default() }
                .apply_linear(&mut expected, &mut || Ok(())).unwrap();
            let expected = expected.to_surface(&mut || Ok(())).unwrap();
            let mut decoder = |_: &super::super::composition::SourceId, _, _| Ok(input.clone());
            let actual = render_media_layers(&composition, timeline, &mut || Ok(()), None, Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(actual, expected, "timeline {timeline}");
        }
    }
    #[test]
    fn extract_image_threshold_local_clock_and_inversion() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_extract","parameters":{},"animation":{"black":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":255.0}]}}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for inverted in [false,true] {
            let mut document=serde_json::to_value(&composition).unwrap();document["tracks"][0]["clips"][0]["effects"][0]["parameters"]["invert"]=serde_json::json!(inverted);
            let composition:Composition=serde_json::from_value(document).unwrap();composition.validate().unwrap();
            for (time,inside) in [(1000000,true),(1500000,false)] {
                let value=if inside!=inverted {128}else{0};
                let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
                let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
                assert_eq!(frame.rgba,[value,value,value,255].repeat(4),"Extract image at {time} inverted={inverted}");
            }
        }
    }
    #[test]
    fn tint_image_endpoint_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":1}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_tint","parameters":{"black":[0,0,0,0],"white":[0,0,0,0]},"colorAnimation":{"blackR":curve,"whiteR":curve}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[0,0,0,255]),(1250000,[64,0,0,255]),(1500000,[128,0,0,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Tint local image time {time}");
        }
    }
    #[test]
    fn leave_color_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"leave_color","parameters":{},"animation":{"amount":curve}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1250000,[50,63,82,255]),(1500000,[63,63,63,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Leave Color local image time {time}");
        }
    }
    #[test]
    fn color_pass_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_pass","parameters":{},"animation":curve}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[63,63,63,255]),(1250000,[63,63,63,255]),(1500000,[32,64,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Pass local image time {time}");
        }
    }
    #[test]
    fn luma_key_image_independent_curves_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":50}]}});
        for threshold in [0,100] {
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"luma_key","parameters":{},"animation":{"threshold":{"mode":"constant","value":threshold},"cutoff":curve}}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let expected=if threshold==100 {[[16u8,31,47],[10,20,31],[0,0,0]]}else{[[32u8,64,96],[32,64,96],[0,0,0]]};
            for (time,rgb) in [1000000,1250000,1500000].into_iter().zip(expected) {
                let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
                let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
                for pixel in frame.rgba.chunks_exact(4) {for(actual,expected)in pixel[..3].iter().zip(rgb){assert!((i16::from(*actual)-i16::from(expected)).abs()<=1,"Luma Key threshold {threshold} local {time}: {pixel:?}");}assert_eq!(pixel[3],255);}
            }
        }
    }
    #[test]
    fn directional_blur_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::directional_blur::DirectionalBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-directional-blur.json")).unwrap();
        assert_eq!(oracle.cases.len(),100);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"directional_blur","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Directional Blur image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn mosaic_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::mosaic::Mosaic,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-mosaic.json")).unwrap();
        assert_eq!(oracle.cases.len(),96);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"mosaic","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Mosaic image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn find_edges_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::find_edges::FindEdges,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-find-edges.json")).unwrap();
        assert_eq!(oracle.cases.len(),32);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"find_edges","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"FindEdges image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn emboss_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::emboss::Emboss,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-emboss.json")).unwrap();
        assert_eq!(oracle.cases.len(),64);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"emboss","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Emboss image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn twirl_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::twirl::Twirl,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-twirl.json")).unwrap();
        assert_eq!(oracle.cases.len(),216);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"twirl","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Twirl image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn grid_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::grid::Grid,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-grid.json")).unwrap();
        assert_eq!(oracle.cases.len(),128);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"grid","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Grid image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn lens_flare_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::lens_flare::LensFlare,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lens-flare.json")).unwrap();
        assert_eq!(oracle.cases.len(),512);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"lens_flare","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"LensFlare image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn checkerboard_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::checkerboard::Checkerboard,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-checkerboard.json")).unwrap();
        assert_eq!(oracle.cases.len(),864);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"checkerboard","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Checkerboard image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn cell_pattern_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::cell_pattern::CellPattern,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-cell-pattern.json")).unwrap();
        assert_eq!(oracle.cases.len(),1152);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"cell_pattern","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"CellPattern image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn roughen_edges_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::roughen_edges::RoughenEdges,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-roughen-edges.json")).unwrap();
        assert_eq!(oracle.cases.len(),256);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"roughen_edges","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"RoughenEdges image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn brush_strokes_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::brush_strokes::BrushStrokes,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-brush-strokes.json")).unwrap();
        assert_eq!(oracle.cases.len(),192);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"brush_strokes","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"BrushStrokes image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn long_shadow_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::long_shadow::LongShadow,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-long-shadow.json")).unwrap();
        assert_eq!(oracle.cases.len(),512);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"long_shadow","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"LongShadow image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn alpha_glow_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::alpha_glow::AlphaGlow,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-alpha-glow.json")).unwrap();
        assert_eq!(oracle.cases.len(),128);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"alpha_glow","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"AlphaGlow image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn block_dissolve_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::block_dissolve::BlockDissolve,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-block-dissolve.json")).unwrap();
        assert_eq!(oracle.cases.len(),128);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"block_dissolve","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"BlockDissolve image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn stroke_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::stroke::Stroke,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-stroke.json")).unwrap();
        assert_eq!(oracle.cases.len(),192);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"stroke","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Stroke image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn color_emboss_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::color_emboss::ColorEmboss,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-color-emboss.json")).unwrap();
        assert_eq!(oracle.cases.len(),64);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_emboss","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"ColorEmboss image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn gradient_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::gradient::Gradient,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gradient.json")).unwrap();
        assert_eq!(oracle.cases.len(),288);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"gradient","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Gradient image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn paint_bucket_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::paint_bucket::PaintBucket,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-paint-bucket.json")).unwrap();
        assert_eq!(oracle.cases.len(),2880);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"paint_bucket","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"PaintBucket image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn ellipse_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::ellipse::Ellipse,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ellipse.json")).unwrap();
        assert_eq!(oracle.cases.len(),144);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"ellipse","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Ellipse image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn circle_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::circle::Circle,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-circle.json")).unwrap();
        assert_eq!(oracle.cases.len(),160);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"circle","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Circle image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn ramp_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::ramp::Ramp,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ramp.json")).unwrap();
        assert_eq!(oracle.cases.len(),192);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"ramp","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Ramp image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn four_color_gradient_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::four_color_gradient::FourColorGradient,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-four-color-gradient.json")).unwrap();
        assert_eq!(oracle.cases.len(),96);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"four_color_gradient","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"FourColorGradient image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn bevel_alpha_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::bevel_alpha::BevelAlpha,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-bevel-alpha.json")).unwrap();
        assert_eq!(oracle.cases.len(),128);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"bevel_alpha","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"BevelAlpha image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn drop_shadow_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::drop_shadow::DropShadow,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-drop-shadow.json")).unwrap();
        assert_eq!(oracle.cases.len(),128);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"drop_shadow","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"DropShadow image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn basic_3d_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::basic_3d::Basic3D,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-basic-3d.json")).unwrap();
        assert_eq!(oracle.cases.len(),64);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"basic_3d","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Basic3D image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn lens_distortion_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::lens_distortion::LensDistortion,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lens-distortion.json")).unwrap();
        assert_eq!(oracle.cases.len(),64);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"lens_distortion","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"LensDistortion image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn wave_warp_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct Case {local_tick:u64,parameters:super::super::wave_warp::WaveWarp,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wave-warp.json")).unwrap();
        assert_eq!(oracle.cases.len(),144);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"wave_warp","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1000000+case.local_tick,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"WaveWarp image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn replicate_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::replicate::Replicate,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-replicate.json")).unwrap();
        assert_eq!(oracle.cases.len(),24);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"replicate","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Replicate image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn noise_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct Case {local_tick:u64,parameters:super::super::noise::Noise,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-noise.json")).unwrap();
        assert_eq!(oracle.cases.len(),96);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":2000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"noise","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1000000+case.local_tick,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Noise image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn strobe_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]#[serde(rename_all="camelCase")]struct Case {local_tick:u64,parameters:super::super::strobe::Strobe,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-strobe.json")).unwrap();
        assert_eq!(oracle.cases.len(),112);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":2000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"strobe","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1000000+case.local_tick,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Strobe image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn median_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::median::Median,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-median.json")).unwrap();
        assert_eq!(oracle.cases.len(),32);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"median","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Median image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn camera_blur_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::camera_blur::CameraBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-camera-blur.json")).unwrap();
        assert_eq!(oracle.cases.len(),20);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"camera_blur","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Camera Blur image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn gaussian_blur_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::gaussian_blur::GaussianBlur,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gaussian-blur.json")).unwrap();
        assert_eq!(oracle.cases.len(),120);
        for (index,case) in oracle.cases.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"gaussian_blur","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Gaussian Blur image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn ultra_key_image_spatial_original_oracle_and_persistence() {
        #[derive(serde::Deserialize)]struct Case {parameters:super::super::ultra_key::UltraKey,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
        #[derive(serde::Deserialize)]struct Oracle {spatial:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-ultra-key.json")).unwrap();
        assert_eq!(oracle.spatial.len(),384);
        for (index,case) in oracle.spatial.iter().enumerate().filter(|(_,c)|c.width%2==0&&c.height%2==0) {
            let mut canvas=CanvasSpec::default();canvas.width=case.width as u32;canvas.height=case.height as u32;
            let mut composition=Composition::new(canvas);
            composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":case.width,"height":case.height,"hasAudio":false}})).unwrap();
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"ultra_key","parameters":case.parameters}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:case.width,height:case.height,rgba:case.rgba.clone()});
            let frame=render_generated_images(&composition,1250000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            for (pixel,bits) in frame.rgba.chunks_exact(4).zip(case.bits.chunks_exact(4)) {
                let a=f32::from_bits(bits[3]);let byte_alpha=(a*255.0+1e-4).round().clamp(0.0,255.0);
                for c in 0..3 {
                    let expected=if a<=1e-6 {0}else{let straight=super::super::basic_grade::display((f32::from_bits(bits[c])/a).max(0.0));let byte=(straight*255.0+1e-4).round().clamp(0.0,255.0);(byte*byte_alpha/255.0).round() as u8};
                    assert!((i16::from(pixel[c])-i16::from(expected)).abs()<=1,"Ultra Key image oracle {index}: {pixel:?} expected channel {c} {expected}");
                }
                assert_eq!(pixel[3],255);
            }
        }
    }
    #[test]
    fn color_key_image_independent_curves_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":255}]}});
        for selected_color in [false,true] {
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[if selected_color {serde_json::json!({"kind":"color_key","parameters":{},"colorAnimation":{"r":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":64.0/255.0}]}},"g":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":128.0/255.0}]}},"b":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":1},{"tick":500000,"value":192.0/255.0}]}},"a":{"mode":"constant","value":0}}})}else{serde_json::json!({"kind":"color_key","parameters":{"thin":5,"feather":50},"animation":{"tolerance":curve}})}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            let expected=if selected_color {[[32u8,64,96],[32,64,96],[0,0,0]]}else{[[32u8,64,96],[16,33,49],[0,0,0]]};
            for (time,rgb) in [1000000,1250000,1500000].into_iter().zip(expected) {
                let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
                let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
                for pixel in frame.rgba.chunks_exact(4) {for(actual,expected)in pixel[..3].iter().zip(rgb){assert!((i16::from(*actual)-i16::from(expected)).abs()<=1,"Color Key animatedcolor {selected_color} local {time}: {pixel:?}");}assert_eq!(pixel[3],255);}
            }
        }
    }
    #[test]
    fn alpha_adjust_image_flags_animation_local_clock_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100},{"tick":500000,"value":0}]}});
        for ignore in [false,true] {for invert in [false,true] {for mask_only in [false,true] {
            composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"alpha_adjust","parameters":{"ignore":ignore,"invert":invert,"maskOnly":mask_only},"animation":curve}]}]}])).unwrap();
            composition.validate().unwrap();assert!(requires_native_video(&composition));
            let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
            for (time,opacity) in [(1000000,1.0f64),(1250000,0.5),(1500000,0.0)] {
                let original=if ignore {1.0}else{128.0/255.0};let alpha=(if invert {1.0-original}else{original})*opacity;
                let rgb=if mask_only {[(alpha*255.0).round() as u8;3]}else{[64.0,128.0,192.0].map(|v|(v*alpha).round() as u8)};
                let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
                let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
                for pixel in frame.rgba.chunks_exact(4) {for (actual,expected) in pixel[..3].iter().zip(rgb) {assert!((i16::from(*actual)-i16::from(expected)).abs()<=1,"Alpha Adjust flags {ignore}/{invert}/{mask_only} localtime {time}: {pixel:?}");}assert_eq!(pixel[3],255);}
            }
        }}}
    }
    #[test]
    fn channel_mix_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100},{"tick":500000,"value":0}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"channel_mix","parameters":{},"animation":{"rr":curve,"rb":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1250000,[64,64,96,255]),(1500000,[96,64,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Channel Mix local image time {time}");
        }
    }
    #[test]
    fn channel_mix_monochrome_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100},{"tick":500000,"value":0}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"channel_mix","parameters":{"monochrome":true},"animation":{"rr":curve,"rb":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,32,32,255]),(1250000,[64,64,64,255]),(1500000,[96,96,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Channel Mix local image time {time}");
        }
    }
    #[test]
    fn color_pass_reverse_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_pass","parameters":{"reverse":true},"animation":curve}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1250000,[32,64,96,255]),(1500000,[63,63,63,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Pass local image time {time}");
        }
    }
    #[test]
    fn change_to_color_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let component=|start:f32,end:f32|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":start},{"tick":500000,"value":end}]}});
        let colors=serde_json::json!({"toG":component(0.0,1.0),"toB":component(1.0,0.0),"toA":{"mode":"constant","value":0}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"change_to_color","parameters":{"from":[64.0/255.0,128.0/255.0,192.0/255.0,0]},"colorAnimation":colors}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,32,96,255]),(1250000,[32,96,96,255]),(1500000,[32,96,32,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Change to Color local image time {time}");
        }
    }
    #[test]
    fn color_replace_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let component=|start:f32,end:f32|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":start},{"tick":500000,"value":end}]}});
        let colors=serde_json::json!({"replaceG":component(0.0,1.0),"replaceB":component(1.0,0.0),"replaceA":{"mode":"constant","value":0}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_replace","parameters":{"solid":true,"target":[64.0/255.0,128.0/255.0,192.0/255.0,0]},"colorAnimation":colors}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[0,0,128,255]),(1250000,[0,64,64,255]),(1500000,[0,128,0,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Replace local image time {time}");
        }
    }
    #[test]
    fn color_replace_lightness_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let component=|start:f32,end:f32|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":start},{"tick":500000,"value":end}]}});
        let colors=serde_json::json!({"replaceG":component(0.0,1.0),"replaceB":component(1.0,0.0),"replaceA":{"mode":"constant","value":0}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_replace","parameters":{"solid":false,"target":[64.0/255.0,128.0/255.0,192.0/255.0,0]},"colorAnimation":colors}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[1,1,128,255]),(1250000,[1,128,128,255]),(1500000,[1,128,1,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Replace local image time {time}");
        }
    }
    #[test]
    fn leave_color_key_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let component=|start:f32,end:f32|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":start},{"tick":500000,"value":end}]}});
        let colors=serde_json::json!({"r":component(1.0,64.0/255.0),"g":component(0.0,128.0/255.0),"b":component(0.0,192.0/255.0),"a":{"mode":"constant","value":0}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"leave_color","parameters":{"amount":100},"colorAnimation":colors}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[63,63,63,255]),(1500000,[32,64,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Leave Color local image time {time}");
        }
    }
    #[test]
    fn color_pass_key_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let component=|start:f32,end:f32|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":start},{"tick":500000,"value":end}]}});
        let colors=serde_json::json!({"r":component(1.0,64.0/255.0),"g":component(0.0,128.0/255.0),"b":component(0.0,192.0/255.0),"a":{"mode":"constant","value":0}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_pass","parameters":{},"colorAnimation":colors}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[63,63,63,255]),(1500000,[32,64,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Pass animated key image time {time}");
        }
    }
    #[test]
    fn color_balance_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_balance","parameters":{},"animation":{"shadowR":curve}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1250000,[37,64,96,255]),(1500000,[41,64,96,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Balance local image time {time}");
        }
    }
    #[test]
    fn color_balance_preserve_image_animation_local_clock_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":100}]}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_balance","parameters":{"preserve":true},"animation":{"shadowR":curve}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1250000,[36,63,95,255]),(1500000,[39,62,94,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Color Balance local image time {time}");
        }
    }
    #[test]
    fn black_white_image_alpha_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"black_white"}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[63,63,63,255]),(1500000,[63,63,63,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Black & White image time {time}");
        }
    }
    #[test]
    fn levels_image_gamma_uses_local_clock_and_preserves_alpha() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_levels","parameters":{},"animation":{"gamma":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":100.0},{"tick":500000,"value":200.0}]}}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1500000,[64,91,111,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Levels local image time {time}");
        }
    }
    #[test]
    fn invert_alpha_image_reaches_compositor_and_native_route() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{"channel":4,"blend":0}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (source,expected) in [([64,128,192,64],[48,96,144,255]),([64,128,192,255],[0,0,0,255]),([0,0,0,0],[0,0,0,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:source.repeat(4)});
            let frame=render_generated_images(&composition,1500000,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,expected.repeat(4),"alpha image source {source:?}");
        }
    }
    #[test]
    fn posterize_image_alpha_local_clock_and_persistence() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"posterize","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":2.0},{"tick":500000,"value":3.0}]}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[0,128,128,255]),(1500000,[64,64,128,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"Posterize image local time {time}");
        }
    }
    #[test]
    fn image_effects_use_shared_stack_alpha_and_local_animation_clock() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Images","clips":[{"id":"clip","sourceId":"image","timelineStartTick":1000000,"durationTicks":1000000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"gamma_correction","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":10.0},{"tick":500000,"value":20.0}]}}}]}]}])).unwrap();
        composition.validate().unwrap();assert!(requires_native_video(&composition));
        let restored:Composition=serde_json::from_value(serde_json::to_value(&composition).unwrap()).unwrap();assert_eq!(restored,composition);
        for (time,pixel) in [(1000000,[32,64,96,255]),(1500000,[8,32,73,255])] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let frame=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(frame.rgba,pixel.repeat(4),"image local time {time}");
        }
        let mut split=serde_json::to_value(&composition).unwrap();
        let mut right=split["tracks"][0]["clips"][0].clone();
        split["tracks"][0]["clips"][0]["durationTicks"]=serde_json::json!(250000);
        right["id"]=serde_json::json!("right");right["timelineStartTick"]=serde_json::json!(1250000);right["durationTicks"]=serde_json::json!(750000);
        for key in right["effects"][0]["animation"]["track"]["keyframes"].as_array_mut().unwrap() {
            key["tick"]=serde_json::json!(key["tick"].as_i64().unwrap()-250000);
        }
        split["tracks"][0]["clips"].as_array_mut().unwrap().push(right);
        let split:Composition=serde_json::from_value(split).unwrap();split.validate().unwrap();
        for time in [1000000,1249999,1250000,1300000,1500000,1999999] {
            let mut decoder=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:[64,128,192,128].repeat(4)});
            let original=render_generated_images(&composition,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            let divided=render_generated_images(&split,time,&mut||Ok(()),Some(&mut decoder)).unwrap().unwrap();
            assert_eq!(original.rgba,divided.rgba,"split image animation {time}");
        }
        let mut invalid=serde_json::to_value(&composition).unwrap();
        invalid["tracks"][0]["clips"][0]["effects"][0]["parameters"]=serde_json::json!({"gamma":29.0});
        assert!(serde_json::from_value::<Composition>(invalid).unwrap().validate().is_err());
        let mut legacy=serde_json::to_value(&composition).unwrap();legacy["tracks"][0]["clips"][0].as_object_mut().unwrap().remove("effects");
        let legacy:Composition=serde_json::from_value(legacy).unwrap();legacy.validate().unwrap();assert!(!requires_native_video(&legacy));
    }
    #[test]
    fn project_library_reaches_video_effect_renderer() {
        let mut canvas = CanvasSpec::default(); canvas.width = 2; canvas.height = 2;
        let mut composition = Composition::new(canvas);
        composition.sources = serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"video","id":"videos","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"lumetri","parameters":{"lookLutLibrary":"shared"}}]}]}])).unwrap();
        let library = vec![super::super::project_luts::ProjectLut {
            id: "shared".into(), name: "Invert".into(), source_path: None,
            lut: super::super::color_lut::ColorLut {
                format: super::super::color_lut::TableFormat::Cube,
                text: "LUT_1D_SIZE 2\n1 1 1\n0 0 0\n".into(), intensity: 100.0,
            },
        }];
        let mut decoder = |_: &super::super::composition::SourceId, _, _| Ok(GroupSurface { width: 2, height: 2, rgba: [0, 0, 0, 255].repeat(4) });
        let rendered = render_media_layers_with_library(&composition, 0, &mut || Ok(()), None, Some(&mut decoder), &library).unwrap().unwrap();
        assert_eq!(rendered.rgba, [255, 255, 255, 255].repeat(4));
        assert_eq!(render_media_layers(&composition, 0, &mut || Ok(()), None, Some(&mut decoder)).unwrap().unwrap().rgba, [0,0,0,255].repeat(4));
        let project = super::super::nested_composition::NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts: library, schema_version: 1, root: "root".into(),
            compositions: std::collections::BTreeMap::from([("root".into(), super::super::nested_composition::NestedCompositionDocument { duration_us: Some(1_000_000), content: composition, instances: vec![] })]),
        };
        project.dependency_order().unwrap();
        let mut missing = project.clone(); missing.luts.clear();
        missing.dependency_order().unwrap();
        let restored: super::super::nested_composition::NestedCompositionProject = serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        restored.dependency_order().unwrap();
        assert_eq!(restored, project);
    }
    #[test]
    fn consecutive_color_effects_preserve_sub_byte_corrections() {
        let mut canvas=CanvasSpec::default();canvas.width=2;canvas.height=2;
        let mut composition=Composition::new(canvas);
        composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":2,"height":2,"hasAudio":false}})).unwrap();
        let effect=serde_json::json!({"kind":"creative_adjust","parameters":{"fadedFilm":0.9}});
        composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"videos","name":"Video","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[effect,{"kind":"hsl_secondary","parameters":{"enabled":false}},{"kind":"tone_curves","curves":{}},{"kind":"color_vignette","parameters":{}},{"kind":"color_lut","lut":{"format":"cube","text":"LUT_1D_SIZE 2\n0 0 0\n1 1 1"}},effect]}]}])).unwrap();
        let mut decoder=|_:&super::super::composition::SourceId,_,_|Ok(GroupSurface{width:2,height:2,rgba:[0,0,0,255].repeat(4)});
        let frame=render_media_layers(&composition,0,&mut||Ok(()),None,Some(&mut decoder)).unwrap().unwrap();
        assert_eq!(frame.rgba,[1,1,1,255].repeat(4));
        let mut standalone=GroupSurface{width:2,height:2,rgba:[0,0,0,255].repeat(4)};
        for _ in 0..3 {super::super::creative_adjust::CreativeAdjust{faded_film:1.0,..Default::default()}.apply(&mut standalone,&mut||Ok(())).unwrap();}
        assert_eq!(standalone.rgba,[0,0,0,255].repeat(4));
    }
    #[test]
    fn png_image_alpha_is_composed_using_authorized_decoder_callback() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("layer.png");
        image::RgbaImage::from_raw(2, 2, [0, 0, 255, 128].repeat(4))
            .unwrap()
            .save(&path)
            .unwrap();
        let mut canvas = CanvasSpec::default();
        canvas.width = 2;
        canvas.height = 2;
        canvas.background.red = 1.0;
        let mut composition = Composition::new(canvas);
        composition.sources = serde_json::from_value(serde_json::json!({"picture":{"id":"picture","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"image","id":"images","name":"Image","clips":[{"id":"photo","sourceId":"picture","timelineStartTick":100000,"durationTicks":900000,"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"}]}])).unwrap();
        let mut decoder = |id: &super::super::composition::SourceId| {
            assert_eq!(id.as_str(), "picture");
            super::super::group_media::decode_image_surface(&path)
        };
        let frame =
            render_generated_images(&composition, 100000, &mut || Ok(()), Some(&mut decoder))
                .unwrap()
                .unwrap();
        assert_eq!(frame.rgba, [127, 0, 128, 255].repeat(4));
        let frame = render_generated_images(
            &composition,
            1000000,
            &mut || Ok(()),
            Some(&mut |_| panic!("past image endpoint")),
        )
        .unwrap()
        .unwrap();
        assert_eq!(frame.rgba, [255, 0, 0, 255].repeat(4));
        assert!(render_generated(&composition, 100000, &mut || Ok(()))
            .unwrap()
            .is_none());
    }
    #[test]
    fn generated_solid_uses_native_alpha_and_half_open_timing() {
        let mut canvas = CanvasSpec::default();
        canvas.width = 2;
        canvas.height = 2;
        let mut composition = Composition::new(canvas);
        composition.tracks = serde_json::from_value(serde_json::json!([{"kind":"color","id":"colors","name":"Color","clips":[{"id":"solid","timelineStartTick":100000,"timelineEndTick":1000000,"width":2,"height":2,"color":{"r":0,"g":0,"b":255,"a":255},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0.5}}]}])).unwrap();
        assert_eq!(
            render_generated(&composition, 100000, &mut || Ok(()))
                .unwrap()
                .unwrap()
                .rgba,
            [0, 0, 128, 255].repeat(4)
        );
        assert_eq!(
            render_generated(&composition, 1000000, &mut || Ok(()))
                .unwrap()
                .unwrap()
                .rgba,
            [0, 0, 0, 255].repeat(4)
        );
    }
}
#[cfg(test)]mod gradient_working_track_tests {use super::*;
#[test]fn external_track_retains_effect_fraction_before_output_quantization(){
 let mut canvas=super::super::composition::CanvasSpec::default();canvas.width=2;canvas.height=2;let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(serde_json::json!({"image":{"id":"image","kind":"image","durationTicks":0,"width":2,"height":2,"hasAudio":false}})).unwrap();
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"image","timelineStartTick":0,"durationTicks":1000000,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"gradient_wipe","parameters":{"completion":77.,"softness":100.}}]}]}])).unwrap();
 let mut decode=|_:&super::super::composition::SourceId|Ok(GroupSurface{width:2,height:2,rgba:vec![255;16]});
 let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:1000000,composition:&composition,time_us:0,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:Some(&mut decode),video:None};
 let actual=context.track(0,&mut||Ok(())).unwrap().unwrap();let mut expected=super::super::linear_color_frame::LinearColorFrame::from_surface(&GroupSurface{width:2,height:2,rgba:vec![255;16]},&mut||Ok(())).unwrap();
 super::super::gradient_wipe::GradientWipe{completion:77.,softness:100.,..Default::default()}.apply_linear(&mut expected,None,&mut||Ok(())).unwrap();assert_eq!(actual.pixels(),expected.pixels());
 let quantized=super::super::linear_color_frame::LinearColorFrame::from_surface(&expected.to_surface(&mut||Ok(())).unwrap(),&mut||Ok(())).unwrap();assert_ne!(actual.pixels(),quantized.pixels());
}
}
#[cfg(test)]mod original_gradient_track_tests {use super::*;
#[test]fn original_transformed_mixed_alpha_control_track_and_effect_bits(){
 let f:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gradient-wipe.json")).unwrap();let cases=f["compositionCases"].as_array().unwrap();assert_eq!(cases.len(),108);
 for(index,c)in cases.iter().enumerate(){let mut canvas=super::super::composition::CanvasSpec::default();canvas.width=18;canvas.height=10;let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(serde_json::json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":18,"height":10,"hasAudio":false},"control":{"id":"control","kind":"image","durationTicks":0,"width":6,"height":4,"hasAudio":false}})).unwrap();let t=&c["transform"];let constant=|v:&serde_json::Value|serde_json::json!({"mode":"constant","value":v});let transform=serde_json::json!({"x":constant(&t["x"]),"y":constant(&t["y"]),"scaleX":constant(&t["scaleX"]),"scaleY":constant(&t["scaleY"]),"rotationDegrees":constant(&t["rotation"]),"anchorX":0.5,"anchorY":0.5});
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":0,"durationTicks":1000000,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"gradient_wipe","parameters":c["parameters"]}]}]},{"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"control","timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":0.7},"blendMode":"normal","effects":[{"kind":"gradient_wipe","parameters":{"completion":c["controlCompletion"],"softness":100.}}]}]}])).unwrap();
 let main:Vec<u8>=serde_json::from_value(c["rgba"].clone()).unwrap();let control:Vec<u8>=serde_json::from_value(c["controlRgba"].clone()).unwrap();let mut decode=|id:&super::super::composition::SourceId|Ok(if id.as_str()=="main"{GroupSurface{width:18,height:10,rgba:main.clone()}}else{GroupSurface{width:6,height:4,rgba:control.clone()}});
 let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:1000000,composition:&composition,time_us:0,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:Some(&mut decode),video:None};let track=context.track(1,&mut||Ok(())).unwrap().unwrap();let expected_track:Vec<u32>=serde_json::from_value(c["trackBits"].clone()).unwrap();assert_eq!(track.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected_track,"original control track {index}");
 let mut child=GroupSurface{width:18,height:10,rgba:main.clone()};let CompositionTrack::Image{clips,..}=&composition.tracks[0]else{panic!("main")};let working=apply_effect_stack_retained(&mut child,&clips[0].effects,0,&[],Some(&mut context),&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(working.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"original external wipe {index}");
 }
}
}
#[cfg(test)]mod original_magnify_stack_tests {use super::*;
#[test]fn original_magnify_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::magnify::Magnify,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-magnify.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Magnify{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Magnify ordered stack {i}");checked+=1;}assert_eq!(checked,320);
}
}
#[cfg(test)]mod original_spherize_stack_tests {use super::*;
#[test]fn original_spherize_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::spherize::Spherize,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spherize.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Spherize{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Spherize ordered stack {i}");checked+=1;}assert_eq!(checked,48);
}
}

#[cfg(test)]mod original_corner_pin_stack_tests {use super::*;
#[test]fn original_corner_pin_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::corner_pin::CornerPin,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-corner-pin.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::CornerPin{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"CornerPin ordered stack {i}");checked+=1;}assert_eq!(checked,24);
}
}

#[cfg(test)]mod original_rotate_3d_stack_tests {use super::*;
#[test]fn original_rotate_3d_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::rotate_3d::Rotate3D,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-rotate-3d.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Rotate3D{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Rotate3D ordered stack {i}");checked+=1;}assert_eq!(checked,192);
}
}

#[cfg(test)]mod effect_clip_clock_tests {
 use super::*;
 struct Clock(Option<u64>);
 impl EffectTrackContext for Clock {
  fn clip_duration_us(&self)->Option<u64>{self.0}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<super::super::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn normalized_clip_clock_clamps_and_distinguishes_missing_context(){
  assert_eq!(Clock(None).clip_progress(250000),None);
  assert_eq!(Clock(Some(0)).clip_progress(250000),Some(0.));
  assert_eq!(Clock(Some(1000000)).clip_progress(250000),Some(0.25));
  assert_eq!(Clock(Some(500000)).clip_progress(250000),Some(0.5));
  assert_eq!(Clock(Some(500000)).clip_progress(1000000),Some(1.));
 }
}

#[cfg(test)]mod grow_shrink_duration_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn grow_shrink_real_image_duration_and_retimed_video_progress(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["grow","shrink"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,125000),(true,1000000,1.,250000),(true,500000,2.,125000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"from":100.,"to":200.}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"normalized quarter progress {effect}");}
   assert_ne!(outputs[4],outputs[0],"half progress must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_err());
  }
 }
}

#[cfg(test)]mod original_grow_shrink_stack_tests {
 use super::*;
 struct Context;
 impl EffectTrackContext for Context{
  fn clip_duration_us(&self)->Option<u64>{Some(1000000)}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<crate::domain::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn original_grow_shrink_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{kind:String,parameters:serde_json::Value,progress:f64,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-grow-shrink.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":c.kind,"parameters":c.parameters})).unwrap();
   assert!(supported_effect_stack(&[effect.clone()]));
   let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};
   let frame=apply_effect_stack_retained(&mut child,&[effect],(c.progress*1000000.)as u64,&[],Some(&mut Context),&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Grow/Shrink ordered stack {index}");count+=1;
  }assert_eq!(count,320);
 }
}

#[cfg(test)]mod original_spin_stack_tests {
 use super::*;
 struct Context;
 impl EffectTrackContext for Context{
  fn clip_duration_us(&self)->Option<u64>{Some(1000000)}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<crate::domain::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn original_spin_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,progress:f64,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spin.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"spin","parameters":c.parameters})).unwrap();
   assert!(supported_effect_stack(&[effect.clone()]));
   let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};
   let frame=apply_effect_stack_retained(&mut child,&[effect],(c.progress*1000000.)as u64,&[],Some(&mut Context),&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Spin ordered stack {index}");count+=1;
  }assert_eq!(count,1280);
 }
}

#[cfg(test)]mod spin_duration_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn spin_real_image_duration_and_retimed_video_progress(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["spin"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,125000),(true,1000000,1.,250000),(true,500000,2.,125000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"amount":180.,"scale":100.,"easing":0}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"normalized quarter progress {effect}");}
   assert_ne!(outputs[4],outputs[0],"half progress must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_err());
  }
 }
}


#[cfg(test)]mod effect_frame_rate_tests {
 use super::*;
 struct Clock{duration:Option<u64>,fps:Option<f64>}
 impl EffectTrackContext for Clock{
  fn clip_duration_us(&self)->Option<u64>{self.duration}
  fn frame_rate(&self)->Option<f64>{self.fps}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<crate::domain::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn motion_span_uses_rate_and_actual_duration(){
  let clock=|duration,fps|Clock{duration,fps};
  assert_eq!(clock(Some(1000000),Some(10.)).motion_blur_span(),Some(0.1));
  assert_eq!(clock(Some(500000),Some(10.)).motion_blur_span(),Some(0.2));
  assert_eq!(clock(Some(1000000),Some(30000./1001.)).motion_blur_span(),Some(1001./30000.));
  assert_eq!(clock(Some(0),Some(10.)).motion_blur_span(),Some(1.));
  assert_eq!(clock(None,Some(10.)).motion_blur_span(),None);
  assert_eq!(clock(Some(1000000),None).motion_blur_span(),None);
  assert_eq!(clock(Some(1000000),Some(f64::NAN)).motion_blur_span(),None);
 }
}

#[cfg(test)]mod original_move_stack_tests {
 use super::*;
 struct Context(f64);
 impl EffectTrackContext for Context{
  fn clip_duration_us(&self)->Option<u64>{Some(1000000)}
  fn frame_rate(&self)->Option<f64>{Some(self.0)}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<crate::domain::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn original_move_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,progress:f64,span:f64,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-move.json")).unwrap();
  // 960 original cases plus 12 changing-source precision regressions.
  assert_eq!(fixture.cases.len(),972);let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"move","parameters":c.parameters})).unwrap();
   assert!(supported_effect_stack(&[effect.clone()]));
   let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};
   let frame=apply_effect_stack_retained(&mut child,&[effect],(c.progress*1000000.)as u64,&[],Some(&mut Context(1./c.span)),&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Move ordered stack {index}");count+=1;
  }assert_eq!(count,492);
 }
}


#[cfg(test)]mod move_duration_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn move_real_image_duration_and_retimed_video_progress(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["move"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,125000),(true,1000000,1.,250000),(true,500000,2.,125000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"from":[-2.,0.],"to":[2.,0.],"easing":0,"motionBlur":0.}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"normalized quarter progress {effect}");}
   assert_ne!(outputs[4],outputs[0],"half progress must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_err());
  }
 }
}


#[cfg(test)]mod move_blur_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn move_motion_blur_uses_real_duration_and_frame_rate(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["move"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local,fps) in[(false,1000000,1.,250000,10),(false,500000,1.,125000,20),(true,1000000,1.,250000,10),(true,500000,2.,125000,20),(false,500000,1.,125000,10)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;canvas.fps_milli=fps*1000;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"from":[-2.,0.],"to":[2.,0.],"easing":0,"motionBlur":100.}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"normalized quarter progress {effect}");}
   assert_ne!(outputs[4],outputs[0],"duration and rate must affect blur {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_err());
  }
 }
}


#[cfg(test)]mod original_wiggle_stack_tests{
 use super::*;
 #[test]fn original_wiggle_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,seconds:f64,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wiggle.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"wiggle","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));
   let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};
   let frame=apply_effect_stack_retained(&mut child,&[effect],(c.seconds*1000000.)as u64,&[],None,&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Wiggle ordered stack {index}");count+=1;
  }assert_eq!(count,768);
 }
}

#[cfg(test)]mod wiggle_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn wiggle_real_image_and_retimed_video_use_local_seconds(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["wiggle"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"frequency":3.25,"amount":2.5,"rotation":37.,"scale":100.,"seed":42.75,"dimensions":0}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}


#[cfg(test)]mod original_camera_shake_stack_tests{
 use super::*;
 struct Context(f64);
 impl EffectTrackContext for Context{
  fn frame_rate(&self)->Option<f64>{Some(self.0)}
  fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<crate::domain::linear_color_frame::LinearColorFrame>>{Ok(None)}
  fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
 }
 #[test]fn original_camera_shake_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,seconds:f64,fps:f64,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-camera-shake.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"camera_shake","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));
   let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],(c.seconds*1000000.)as u64,&[],Some(&mut Context(c.fps)),&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"CameraShake ordered stack {index}");count+=1;
  }assert_eq!(count,1728);
 }
 #[test]fn missing_motion_blur_rate_is_refused(){
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"camera_shake","parameters":{"motionBlur":50.}})).unwrap();let mut child=GroupSurface{width:1,height:1,rgba:vec![255,255,255,255]};assert!(apply_effect_stack(&mut child,&[effect],0,&[],&mut||Ok(())).is_err());
 }
}

#[cfg(test)]mod camera_shake_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn camera_shake_real_image_and_retimed_video_use_local_seconds(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["camera_shake"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;canvas.fps_milli=29970;canvas.fps_rate=Some(crate::domain::composition::FrameRateRatio{numerator:30000,denominator:1001});let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"frequency":4.,"amount":2.5,"rotation":3.75,"zoom":25.,"complexity":3.75,"motionBlur":50.,"seed":42.75}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}


#[cfg(test)]mod camera_shake_composition_rate_tests{
 use super::*;
 #[test]fn exact_fractional_composition_rate_matches_original_working_bits(){
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-camera-shake.json")).unwrap();let c=fixture["cases"].as_array().unwrap().iter().find(|c|c["width"]==18&&c["height"]==10&&c["scale"]==1.&&c["seconds"]==0.123456&&c["fps"].as_f64()==Some(30000./1001.)&&c["parameters"]["motionBlur"]==100.&&c["parameters"]["complexity"]==3.75&&c["parameters"]["seed"]==42.75&&c["parameters"]["frequency"]==4.&&c["parameters"]["amount"]==2.5).unwrap();
  let mut canvas=crate::domain::composition::CanvasSpec::default();canvas.width=18;canvas.height=10;canvas.fps_milli=29970;canvas.fps_rate=Some(crate::domain::composition::FrameRateRatio{numerator:30000,denominator:1001});let composition=Composition::new(canvas);composition.validate().unwrap();
  let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:1000000,composition:&composition,time_us:1123456,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:None,video:None};assert_eq!(context.frame_rate(),Some(30000./1001.));assert_ne!(context.frame_rate(),Some(29.970));
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"camera_shake","parameters":c["parameters"]})).unwrap();let mut child=GroupSurface{width:18,height:10,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],Some(&mut context),&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits);
 }
}

#[cfg(test)]mod original_spacer_stack_tests{
 use super::*;
 #[test]fn original_spacer_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-spacer.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"spacer","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Spacer ordered stack {index}");count+=1;
  }assert_eq!(count,576);
 }
}

#[cfg(test)]mod spacer_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn spacer_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["spacer"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"uniform":false,"fill":true},"animation":{"left":curve(0.,3.),"top":curve(1.,4.),"right":curve(2.,1.),"bottom":curve(0.5,2.),"radius":curve(0.,4.),"colorR":curve(0.,1.),"colorG":curve(1.,0.),"colorB":curve(0.25,0.75),"colorA":curve(0.,1.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}


#[cfg(test)]mod original_rounded_crop_stack_tests{
 use super::*;
 #[test]fn original_rounded_crop_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-rounded-crop.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"rounded_crop","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"RoundedCrop ordered stack {index}");count+=1;
  }assert_eq!(count,972);
 }
}

#[cfg(test)]mod rounded_crop_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn rounded_crop_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["rounded_crop"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"left":curve(0.,20.),"top":curve(0.,10.),"right":curve(0.,5.),"bottom":curve(0.,15.),"radius":curve(0.,4.),"feather":curve(0.,2.),"border":curve(0.,2.),"borderR":curve(0.,1.),"borderG":curve(1.,0.),"borderB":curve(0.25,0.75),"borderA":curve(0.,1.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}


#[cfg(test)]mod original_clone_effect_stack_tests{
 use super::*;
 #[test]fn original_clone_effect_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-clone.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"clone","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Clone ordered stack {index}");count+=1;
  }assert_eq!(count,576);
 }
}

#[cfg(test)]mod clone_effect_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn clone_effect_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["clone"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"mirror":true},"animation":{"columns":curve(1.,3.75),"rows":curve(1.,2.25),"gap":curve(0.,2.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}



#[cfg(test)]mod original_auto_align_stack_tests{
 use super::*;
 #[test]fn original_auto_align_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-auto-align.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"auto_align","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Auto Align ordered stack {index}");count+=1;
  }assert_eq!(count,1024);
 }
}


#[cfg(test)]mod auto_align_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn auto_align_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["auto_align"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"horizontal":3,"vertical":1},"animation":{"marginX":curve(0.,3.),"marginY":curve(1.,4.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}




#[cfg(test)]mod original_turbulent_displace_stack_tests{
 use super::*;
 #[test]fn original_turbulent_displace_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-turbulent-displace.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"turbulent_displace","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Turbulent Displace ordered stack {index}");count+=1;
  }assert_eq!(count,1728);
 }
}



#[cfg(test)]mod turbulent_displace_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn turbulent_displace_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["turbulent_displace"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"displacement":5,"pinning":0,"cycle":true},"animation":{"amount":curve(-2.,5.),"size":curve(2.,5.),"offsetX":curve(0.,3.),"offsetY":curve(1.,4.),"complexity":curve(1.,6.5),"evolution":curve(-450.,810.),"cycleRevs":curve(1.,2.25),"seed":curve(0.,42.75)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}





#[cfg(test)]mod original_channel_blur_stack_tests{
 use super::*;
 #[test]fn original_channel_blur_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-channel-blur.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"channel_blur","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Channel Blur ordered stack {index}");count+=1;
  }assert_eq!(count,96);
 }
}



#[cfg(test)]mod channel_blur_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn channel_blur_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["channel_blur"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"dimensions":2,"repeatEdge":true},"animation":{"red":curve(0.,2.5),"green":curve(1.,4.),"blue":curve(0.,6.),"alpha":curve(0.,3.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}





#[cfg(test)]mod original_reduce_interlace_flicker_stack_tests{
 use super::*;
 #[test]fn original_reduce_interlace_flicker_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-reduce-interlace-flicker.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"reduce_interlace_flicker","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Reduce Interlace Flicker ordered stack {index}");count+=1;
  }assert_eq!(count,20);
 }
}



#[cfg(test)]mod reduce_interlace_flicker_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn reduce_interlace_flicker_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["reduce_interlace_flicker"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":curve(0.,4.)}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}






#[cfg(test)]mod original_focus_blur_stack_tests{
 use super::*;
 #[test]fn original_focus_blur_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-focus-blur.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"focus_blur","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Focus Blur ordered stack {index}");count+=1;
  }assert_eq!(count,64);
 }
}

#[cfg(test)]mod focus_blur_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn focus_blur_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["focus_blur"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"shape":1,"show":true},"animation":{"amount":curve(0.,8.),"size":curve(0.,5.),"feather":curve(1.,4.),"angle":curve(-37.,74.),"centerX":curve(0.,3.),"centerY":curve(1.,4.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_bokeh_blur_stack_tests{
 use super::*;
 #[test]fn original_bokeh_blur_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-bokeh-blur.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"bokeh_blur","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Bokeh Blur ordered stack {index}");count+=1;
  }assert_eq!(count,96);
 }
}

#[cfg(test)]mod bokeh_blur_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn bokeh_blur_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["bokeh_blur"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"shape":5},"animation":{"amount":curve(0.,26.5),"rotation":curve(-37.,74.),"highlight":curve(0.,100.),"threshold":curve(0.,100.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_compound_blur_stack_tests{
 use super::*;
 #[test]fn original_compound_blur_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-compound-blur.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"compound_blur","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Compound Blur ordered stack {index}");count+=1;
  }assert_eq!(count,48);
 }
}

#[cfg(test)]mod compound_blur_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn compound_blur_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["compound_blur"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"invert":true,"stretch":false},"animation":curve(0.,8.5)}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod compound_blur_original_track_tests {use super::*;
#[test]fn compound_blur_original_transformed_mixed_alpha_control_track_and_effect_bits(){
 let f:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-compound-blur.json")).unwrap();let cases=f["compositionCases"].as_array().unwrap();assert_eq!(cases.len(),72);
 for(index,c)in cases.iter().enumerate(){let mut canvas=super::super::composition::CanvasSpec::default();canvas.width=18;canvas.height=10;let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(serde_json::json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":18,"height":10,"hasAudio":false},"control":{"id":"control","kind":"image","durationTicks":0,"width":6,"height":4,"hasAudio":false}})).unwrap();let t=&c["transform"];let constant=|v:&serde_json::Value|serde_json::json!({"mode":"constant","value":v});let transform=serde_json::json!({"x":constant(&t["x"]),"y":constant(&t["y"]),"scaleX":constant(&t["scaleX"]),"scaleY":constant(&t["scaleY"]),"rotationDegrees":constant(&t["rotation"]),"anchorX":0.5,"anchorY":0.5});
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":0,"durationTicks":1000000,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":c["parameters"]}]}]},{"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"control","timelineStartTick":0,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":0.7},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{"max":c["controlMax"]}}]}]}])).unwrap();
 let main:Vec<u8>=serde_json::from_value(c["rgba"].clone()).unwrap();let control:Vec<u8>=serde_json::from_value(c["controlRgba"].clone()).unwrap();let mut decode=|id:&super::super::composition::SourceId|Ok(if id.as_str()=="main"{GroupSurface{width:18,height:10,rgba:main.clone()}}else{GroupSurface{width:6,height:4,rgba:control.clone()}});
 let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:1000000,composition:&composition,time_us:0,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:Some(&mut decode),video:None};let track=context.track(1,&mut||Ok(())).unwrap().unwrap();let expected_track:Vec<u32>=serde_json::from_value(c["trackBits"].clone()).unwrap();assert_eq!(track.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected_track,"original control track {index}");
 let mut child=GroupSurface{width:18,height:10,rgba:main.clone()};let CompositionTrack::Image{clips,..}=&composition.tracks[0]else{panic!("main")};let working=apply_effect_stack_retained(&mut child,&clips[0].effects,0,&[],Some(&mut context),&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(working.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"original external compound blur {index}");
 }
}
}

#[cfg(test)]mod compound_blur_animated_external_track_tests {use super::*;
#[test]fn compound_blur_animated_external_track_original_bits(){
 let f:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-compound-blur.json")).unwrap();let cases=f["externalMedia"].as_array().unwrap();assert_eq!(cases.len(),36);
 let mut count=0;
 for(index,c)in cases.iter().enumerate(){let frames=c["frames"].as_array().unwrap();for(frame_index,bits)in frames.iter().enumerate(){let local=c["localTick"].as_u64().unwrap()+frame_index as u64*100000;let curve=|b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.},{"tick":500000,"value":b}]}});let mut canvas=super::super::composition::CanvasSpec::default();canvas.width=16;canvas.height=16;let mut composition=Composition::new(canvas);
 composition.sources=serde_json::from_value(serde_json::json!({"main":{"id":"main","kind":"image","durationTicks":0,"width":16,"height":16,"hasAudio":false},"control":{"id":"control","kind":"image","durationTicks":0,"width":6,"height":4,"hasAudio":false}})).unwrap();let t=&c["transform"];let constant=|v:&serde_json::Value|serde_json::json!({"mode":"constant","value":v});let transform=serde_json::json!({"x":constant(&t["x"]),"y":constant(&t["y"]),"scaleX":constant(&t["scaleX"]),"scaleY":constant(&t["scaleY"]),"rotationDegrees":constant(&t["rotation"]),"anchorX":0.5,"anchorY":0.5});
 composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"image","id":"main","name":"Main","clips":[{"id":"main","sourceId":"main","timelineStartTick":1000000,"durationTicks":1000000,"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{"layer":2,"stretch":c["stretch"],"invert":c["invert"]},"animation":curve(8.5)}]}]},{"kind":"image","id":"control","name":"Control","hidden":true,"clips":[{"id":"control","sourceId":"control","timelineStartTick":1000000,"durationTicks":1000000,"transform":transform,"opacity":{"mode":"constant","value":0.7},"blendMode":"normal","effects":[{"kind":"compound_blur","parameters":{},"animation":curve(4.25)}]}]}])).unwrap();
 let main:Vec<u8>=serde_json::from_value(c["rgba"].clone()).unwrap();let control:Vec<u8>=serde_json::from_value(c["controlRgba"].clone()).unwrap();let mut decode=|id:&super::super::composition::SourceId|Ok(if id.as_str()=="main"{GroupSurface{width:16,height:16,rgba:main.clone()}}else{GroupSurface{width:6,height:4,rgba:control.clone()}});
 let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:None,clip_duration_us:1000000,composition:&composition,time_us:1000000+local,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:Some(&mut decode),video:None};
 let mut child=GroupSurface{width:16,height:16,rgba:main.clone()};let CompositionTrack::Image{clips,..}=&composition.tracks[0]else{panic!("main")};let working=apply_effect_stack_retained(&mut child,&clips[0].effects,local,&[],Some(&mut context),&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(working.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"original animated external compound blur {index} frame{frame_index}");
 count+=1;}}assert_eq!(count,276);
}
}

#[cfg(test)]mod original_sharpen_stack_tests{
 use super::*;
 #[test]fn original_sharpen_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-sharpen.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"sharpen","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Sharpen ordered stack {index}");count+=1;
  }assert_eq!(count,24);
 }
}

#[cfg(test)]
mod sharpen_persistence_tests {
    use super::*;
    use crate::domain::sharpen::{Sharpen, sample_animation, validate_animation};
    use crate::domain::linear_color_frame::LinearColorFrame;
    #[test]
    fn sharpen_persistence_local_curve_and_admission() {
        let effect: super::super::composition::VideoEffect = serde_json::from_value(serde_json::json!({"kind":"sharpen","parameters":{},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0},{"tick":500000,"value":4000}]}}})).unwrap();
        assert_eq!(
            effect,
            serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap()
        );
        let super::super::composition::VideoEffect::Sharpen {
            parameters,
            animation: Some(curve),
        } = effect
        else {
            panic!("Sharpen")
        };
        assert_eq!(parameters, Sharpen::default());
        assert_eq!(
            sample_animation(parameters, &curve, 250000).unwrap().amount,
            2000.
        );
        assert_eq!(
            sample_animation(parameters, &curve, 750000).unwrap().amount,
            4000.
        );
        for value in [-1., 4000.1] {
            assert!(
                validate_animation(&super::super::composition::AnimatableValue::Constant { value })
                    .is_err()
            );
        }
        let source = super::super::group_surface::GroupSurface {
            width: 6,
            height: 4,
            rgba: (0..24)
                .flat_map(|i| {
                    if i % 3 == 0 {
                        [255, 255, 255, 128]
                    } else {
                        [64, 128, 192, 255]
                    }
                })
                .collect(),
        };
        let mut expected = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        Sharpen { amount: 2000. }
            .apply_linear(&mut expected, 1., &mut || Ok(()))
            .unwrap();
        let mut child = source;
        let effect = super::super::composition::VideoEffect::Sharpen {
            parameters,
            animation: Some(curve),
        };
        let actual = apply_effect_stack_retained(
            &mut child,
            &[effect],
            250000,
            &[],
            None,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(actual.pixels(), expected.pixels());
    }
}

#[cfg(test)]mod sharpen_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn sharpen_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["sharpen"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":curve(0.,100.)}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_unsharp_mask_stack_tests{
 use super::*;
 #[test]fn original_unsharp_mask_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-unsharp-mask.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"unsharp_mask","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Unsharp Mask ordered stack {index}");count+=1;
  }assert_eq!(count,32);
 }
}

#[cfg(test)]mod unsharp_mask_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn unsharp_mask_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["unsharp_mask"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"amount":curve(0.,100.),"radius":curve(1.,4.25),"threshold":curve(0.,32.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_brightness_contrast_stack_tests{
 use super::*;
 #[test]fn original_brightness_contrast_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-brightness-contrast.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"brightness_contrast","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Brightness & Contrast ordered stack {index}");count+=1;
  }assert_eq!(count,100);
 }
}

#[cfg(test)]mod brightness_contrast_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn brightness_contrast_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["brightness_contrast"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"brightness":curve(0.,37.25),"contrast":curve(0.,-12.5)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_crop_stack_tests{
 use super::*;
 #[test]fn original_crop_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-crop.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"crop","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Crop ordered stack {index}");count+=1;
  }assert_eq!(count,64);
 }
}

#[cfg(test)]mod crop_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn crop_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["crop"]{for zoom in[false,true]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"zoom":zoom},"animation":{"left":curve(0.,12.5),"top":curve(0.,20.),"right":curve(0.,3.25),"bottom":curve(0.,7.5),"feather":curve(0.,4.25)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

}

#[cfg(test)]mod original_edge_feather_stack_tests{
 use super::*;
 #[test]fn original_edge_feather_ordered_stack_bits(){
  #[derive(serde::Deserialize)]struct Case{parameters:serde_json::Value,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
  #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
  let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edge-feather.json")).unwrap();let mut count=0;
  for(index,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"edge_feather","parameters":c.parameters})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let output=apply_effect_stack_retained(&mut child,&[effect],123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Edge Feather ordered stack {index}");count+=1;
  }assert_eq!(count,24);
 }
}

#[cfg(test)]mod edge_feather_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn edge_feather_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["edge_feather"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":curve(0.,100.)}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod flip_stack_tests {
 use super::*;
 #[test]fn flip_original_ordered_stack_and_persistence(){
 #[derive(serde::Deserialize)]struct Case{effects:Vec<String>,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-flip.json")).unwrap();assert_eq!(f.cases.len(),60);
 for(i,c)in f.cases.into_iter().enumerate(){let effects:Vec<super::super::composition::VideoEffect>=c.effects.iter().map(|name|serde_json::from_value(serde_json::json!({"kind":name})).unwrap()).collect();assert_eq!(effects,serde_json::from_value::<Vec<super::super::composition::VideoEffect>>(serde_json::to_value(&effects).unwrap()).unwrap());assert!(supported_effect_stack(&effects));let mut source=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut source,&effects,123456,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original flip stack {i}");}
 }
}

#[cfg(test)]mod flip_routing_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn flip_real_image_and_retimed_video_routing(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  for effect in["horizontal_flip","vertical_flip"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_eq!(outputs[4],outputs[0],"static flip later local time {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_mirror_stack_tests {use super::*;
#[test]fn original_mirror_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::mirror::Mirror,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-mirror.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Mirror{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Mirror ordered stack {i}");checked+=1;}assert_eq!(checked,144);
}
}

#[cfg(test)]mod mirror_media_stack_tests {use super::*;
#[test]fn mirror_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-mirror.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"mirror","parameters":{},"animation":{"angle":curve(-37.25,90.),"centerX":curve(4.,12.),"centerY":curve(4.,12.)}})).unwrap();
 assert!(supported_effect_stack(&[effect.clone()]));let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Mirror animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,23);
}
}

#[cfg(test)]mod mirror_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn mirror_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["mirror"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"angle":curve(-37.25,90.),"centerX":curve(1.,4.),"centerY":curve(1.,3.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_offset_stack_tests {use super::*;
#[test]fn original_offset_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::offset::Offset,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-offset.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Offset{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Offset ordered stack {i}");checked+=1;}assert_eq!(checked,80);
}
}

#[cfg(test)]mod offset_media_stack_tests {use super::*;
#[test]fn offset_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-offset.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"offset","parameters":{},"animation":{"blend":curve(0.,75.),"shiftX":curve(4.,12.),"shiftY":curve(4.,12.)}})).unwrap();
 assert!(supported_effect_stack(&[effect.clone()]));let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Offset animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,23);
}
}

#[cfg(test)]mod offset_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn offset_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["offset"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"blend":curve(0.,75.),"shiftX":curve(1.,4.),"shiftY":curve(1.,3.)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_asc_cdl_stack_tests {use super::*;
#[test]fn original_asc_cdl_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::asc_cdl::AscCdl,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-asc-cdl.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::AscCdl{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"AscCdl ordered stack {i}");checked+=1;}assert_eq!(checked,136);
}
}

#[cfg(test)]mod asc_cdl_media_stack_tests {use super::*;
#[test]fn asc_cdl_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-asc-cdl.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"asc_cdl","parameters":{},"animation":serde_json::json!({"rSlope":curve(1.0,0.75),"rOffset":curve(0.0,0.1),"rPower":curve(1.0,1.25),"gSlope":curve(1.0,1.5),"gOffset":curve(0.0,-0.1),"gPower":curve(1.0,0.5),"bSlope":curve(1.0,2.0),"bOffset":curve(0.0,0.25),"bPower":curve(1.0,2.5),"saturation":curve(1.0,0.625)})})).unwrap();
 assert!(supported_effect_stack(&[effect.clone()]));let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"AscCdl animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,23);
}
}

#[cfg(test)]mod asc_cdl_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn asc_cdl_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["asc_cdl"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"rSlope":curve(1.0,0.75),"rOffset":curve(0.0,0.1),"rPower":curve(1.0,1.25),"gSlope":curve(1.0,1.5),"gOffset":curve(0.0,-0.1),"gPower":curve(1.0,0.5),"bSlope":curve(1.0,2.0),"bOffset":curve(0.0,0.25),"bPower":curve(1.0,2.5),"saturation":curve(1.0,0.625)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_transform_effect_stack_tests {use super::*;
#[test]fn original_transform_effect_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::transform_effect::TransformEffect,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transform-effect.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::Transform{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"Transform ordered stack {i}");checked+=1;}assert_eq!(checked,32);
}
}

#[cfg(test)]mod transform_effect_media_stack_tests {use super::*;
#[test]fn transform_effect_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transform-effect.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"transform","parameters":{"uniformScale":false},"animation":serde_json::json!({"scaleHeight":curve(100.0,25.0),"scaleWidth":curve(100.0,50.0),"skew":curve(0.0,12.5),"skewAxis":curve(0.0,37.25),"rotation":curve(0.0,-37.25),"opacity":curve(100.0,62.5),"shutterAngle":curve(0.0,180.0),"anchorX":curve(4.0,8.0),"anchorY":curve(4.0,8.0),"positionX":curve(4.0,12.0),"positionY":curve(4.0,12.0)})})).unwrap();
 assert!(supported_effect_stack(&[effect.clone()]));let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Transform animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,23);
}
}

#[cfg(test)]mod transform_effect_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn transform_effect_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["transform"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"uniformScale":false},"animation":{"scaleHeight":curve(100.0,25.0),"scaleWidth":curve(100.0,50.0),"skew":curve(0.0,12.5),"skewAxis":curve(0.0,37.25),"rotation":curve(0.0,-37.25),"opacity":curve(100.0,62.5),"shutterAngle":curve(0.0,180.0),"anchorX":curve(1.0,3.0),"anchorY":curve(1.0,3.0),"positionX":curve(1.0,4.0),"positionY":curve(1.0,3.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_vignette_effect_stack_tests {use super::*;
#[test]fn original_vignette_effect_ordered_stack_bits(){
 #[derive(serde::Deserialize)]struct Case{parameters:super::super::vignette_effect::VignetteEffect,width:usize,height:usize,rgba:Vec<u8>,scale:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let fixture:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-vignette-effect.json")).unwrap();let mut checked=0;
 for(i,c)in fixture.cases.into_iter().enumerate(){if c.scale!=1.{continue;}let effect=super::super::composition::VideoEffect::VignetteEffect{parameters:c.parameters,animation:None};assert!(supported_effect_stack(&[effect.clone()]));let mut child=GroupSurface{width:c.width,height:c.height,rgba:c.rgba};let frame=apply_effect_stack_retained(&mut child,&[effect],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"VignetteEffect ordered stack {i}");checked+=1;}assert_eq!(checked,1080);
}
}

#[cfg(test)]mod vignette_effect_media_stack_tests {use super::*;
#[test]fn vignette_effect_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-vignette-effect.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"vignette","parameters":{},"animation":serde_json::json!({"amount":curve(-50.0,-100.0),"midpoint":curve(50.0,75.0),"roundness":curve(0.0,-100.0),"feather":curve(50.0,0.0),"colorRed":curve(0.0,0.25),"colorGreen":curve(0.0,0.75),"colorBlue":curve(0.0,0.125),"colorAlpha":curve(1.0,0.0)})})).unwrap();
 assert!(supported_effect_stack(&[effect.clone()]));let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"VignetteEffect animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,23);
}
}

#[cfg(test)]mod vignette_effect_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn vignette_effect_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  for effect in["vignette"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"amount":curve(-50.0,-100.0),"midpoint":curve(50.0,75.0),"roundness":curve(0.0,-100.0),"feather":curve(50.0,0.0),"colorRed":curve(0.0,0.25),"colorGreen":curve(0.0,0.75),"colorBlue":curve(0.0,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}


#[cfg(test)]
mod legacy_blur_identity_tests {
 use super::*;
 #[test]
 fn legacy_blur_ids_load_animate_render_and_preserve_identity() {
  use crate::domain::composition::VideoEffect;
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,if i%3==0{0}else{128}]).collect()};
  let curve=serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":12.5}]}});
  for (legacy,canonical,parameters,animation) in [
   ("gaussian_blur_legacy","gaussian_blur",serde_json::json!({"blurriness":12.5,"dimensions":1}),curve.clone()),
   ("directional_blur_legacy","directional_blur",serde_json::json!({"direction":37.25,"length":12.5}),serde_json::json!({"length":curve}))
  ] {
   let make=|kind:&str|serde_json::from_value::<VideoEffect>(serde_json::json!({"kind":kind,"parameters":parameters,"animation":animation})).unwrap();
   let a=make(legacy);let b=make(canonical);assert_ne!(a,b);assert_eq!(serde_json::to_value(&a).unwrap()["kind"],legacy);assert_eq!(serde_json::from_value::<VideoEffect>(serde_json::to_value(&a).unwrap()).unwrap(),a);
   for tick in [0,250000,500000,750000] {
    let render=|effect:VideoEffect|apply_effect_stack_retained(&mut input.clone(),&[effect],tick,&[],None,&mut||Ok(())).unwrap();
    assert_eq!(render(a.clone()).pixels(),render(b.clone()).pixels(),"{legacy}/{tick}");
   }
  }
 }
}

#[cfg(test)]mod legacy_blur_original_tests {use super::*;
#[test]fn legacy_blur_original_working_bits(){
 let mut checked=0;
 for (kind,fixture,count) in [
 ("gaussian_blur_legacy",include_str!("../../tests/fixtures/filmcraft-gaussian-blur-legacy.json"),60),
 ("directional_blur_legacy",include_str!("../../tests/fixtures/filmcraft-directional-blur-legacy.json"),100)] {
 let data:serde_json::Value=serde_json::from_str(fixture).unwrap();assert_eq!(data["cases"].as_array().unwrap().len(),count);
 for (index,case) in data["cases"].as_array().unwrap().iter().enumerate(){
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":case["parameters"]})).unwrap();
  let mut surface=GroupSurface{width:case["width"].as_u64().unwrap() as usize,height:case["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};
  let frame=apply_effect_stack_retained(&mut surface,&[effect],0,&[],None,&mut||Ok(())).unwrap();
  let bits:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
  assert_eq!(frame.pixels().iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>(),bits,"{kind}/{index}");checked+=1;
 }
 }
 assert_eq!(checked,160);
}}

#[cfg(test)]mod legacy_distort_identity_tests {use super::*;
#[test]fn legacy_mosaic_noise_twirl_preserve_identity_and_animation(){
 use crate::domain::composition::VideoEffect;
 let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,if i%3==0{0}else{128}]).collect()};
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 for (canonical,parameters,animation) in [
 ("mosaic",serde_json::json!({"horizontal":2.5,"vertical":3.5,"sharp":true}),serde_json::json!({"horizontal":curve(1.0,10.0),"vertical":curve(1.0,5.0)})),
 ("noise",serde_json::json!({"amount":25.0,"color":false,"clip":false}),serde_json::json!({"amount":curve(0.0,75.0)})),
 ("twirl",serde_json::json!({"angle":50.0,"radius":30.0,"center":[2.123456789012,null]}),serde_json::json!({"angle":curve(-45.0,720.0),"radius":curve(0.0,100.0)}))] {
 let legacy=format!("{canonical}_legacy");
 let make=|kind:&str|serde_json::from_value::<VideoEffect>(serde_json::json!({"kind":kind,"parameters":parameters,"animation":animation})).unwrap();
 let a=make(&legacy);let b=make(canonical);assert_ne!(a,b);
 let json=serde_json::to_value(&a).unwrap();assert_eq!(json["kind"],legacy);assert_eq!(serde_json::from_value::<VideoEffect>(json).unwrap(),a);
 for tick in [0,250000,500000,750000]{let render=|effect:VideoEffect|apply_effect_stack_retained(&mut input.clone(),&[effect],tick,&[],None,&mut||Ok(())).unwrap();assert_eq!(render(a.clone()).pixels(),render(b.clone()).pixels(),"{legacy}/{tick}");}
 }
}}

#[cfg(test)]mod legacy_distort_original_tests {use super::*;
#[test]fn legacy_mosaic_noise_twirl_original_working_bits(){
 let mut checked=0;
 for (kind,fixture,count) in [
 ("mosaic_legacy",include_str!("../../tests/fixtures/filmcraft-mosaic-legacy.json"),32),
 ("noise_legacy",include_str!("../../tests/fixtures/filmcraft-noise-legacy.json"),96),
 ("twirl_legacy",include_str!("../../tests/fixtures/filmcraft-twirl-legacy.json"),216)] {
 let data:serde_json::Value=serde_json::from_str(fixture).unwrap();assert_eq!(data["cases"].as_array().unwrap().len(),count);
 for (index,case) in data["cases"].as_array().unwrap().iter().enumerate(){
  let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":case["parameters"]})).unwrap();
  let mut surface=GroupSurface{width:case["width"].as_u64().unwrap() as usize,height:case["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};
  let frame=apply_effect_stack_retained(&mut surface,&[effect],case["localTick"].as_u64().unwrap_or(0),&[],None,&mut||Ok(())).unwrap();
  let bits:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
  assert_eq!(frame.pixels().iter().flatten().map(|x|x.to_bits()).collect::<Vec<_>>(),bits,"{kind}/{index}");checked+=1;
 }
 }
 assert_eq!(checked,344);
}}

#[cfg(test)]mod legacy_effect_media_original_tests {use super::*;
#[test]fn legacy_effects_animated_original_working_frames(){
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let mut checked=0;
 for (kind,fixture,parameters,animation) in [
 ("mosaic_legacy",include_str!("../../tests/fixtures/filmcraft-mosaic-legacy.json"),serde_json::json!({}),serde_json::json!({"horizontal":curve(2.0,8.0),"vertical":curve(3.0,6.0)})),
 ("noise_legacy",include_str!("../../tests/fixtures/filmcraft-noise-legacy.json"),serde_json::json!({"color":true,"clip":false}),serde_json::json!({"amount":curve(25.0,75.0)})),
 ("twirl_legacy",include_str!("../../tests/fixtures/filmcraft-twirl-legacy.json"),serde_json::json!({}),serde_json::json!({"angle":curve(-45.0,720.0),"radius":curve(30.0,100.0),"centerX":curve(8.0,2.0),"centerY":curve(8.0,1.75)})),
 ("directional_blur_legacy",include_str!("../../tests/fixtures/filmcraft-directional-blur-legacy.json"),serde_json::json!({}),serde_json::json!({"direction":curve(0.0,90.0),"length":curve(0.0,12.0)}))] {
 let data:serde_json::Value=serde_json::from_str(fixture).unwrap();
 let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":parameters,"animation":animation})).unwrap();
 for media in data["media"].as_array().unwrap(){
 let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for (index,bits) in media["frames"].as_array().unwrap().iter().enumerate(){
 let mut surface=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut surface,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();
 let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"{kind}/{start}/{index}");checked+=1;
 }
 }
 }
 assert_eq!(checked,92);
}
}

#[cfg(test)]mod legacy_gaussian_media_original_tests {use super::*;
#[test]fn legacy_gaussian_animated_all_original_working_frames(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-gaussian-blur-legacy.json")).unwrap();let mut checked=0;
 for media in data["media"].as_array().unwrap(){
 let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"gaussian_blur_legacy","parameters":{"dimensions":media["dimensions"]},"animation":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":0.0},{"tick":500000,"value":12.0}]}}})).unwrap();
 let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for (index,bits) in media["frames"].as_array().unwrap().iter().enumerate(){let mut surface=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut surface,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Legacy Gaussian dimensions{}/{start}/{index}",media["dimensions"]);checked+=1;}
 }
 assert_eq!(checked,69);
}}

#[cfg(test)]mod original_lighting_effects_stack_tests{use super::*;
#[test]fn original_lighting_effects_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lighting-effects.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate().filter(|(_,c)|c["scale"].as_f64()==Some(1.0)){
 let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"lighting_effects","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));
 let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Lighting stack {index}");checked+=1;}
 assert_eq!(checked,32);
}}

#[cfg(test)]mod lighting_effects_media_original_tests{use super::*;
#[test]fn lighting_effects_animated_all_original_working_frames(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lighting-effects.json")).unwrap();let mut checked=0;
 for media in data["media"].as_array().unwrap(){assert_eq!(media["animation"].as_object().unwrap().len(),64);let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"lighting_effects","parameters":media["parameters"],"animation":media["animation"]})).unwrap();let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut surface=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut surface,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Lighting animated {start}/{index}/{}",media["parameters"]["whiteHigh"]);checked+=1;}}
 assert_eq!(checked,46);
}}

#[cfg(test)]mod lighting_effects_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn lighting_effects_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lighting-effects.json")).unwrap();let media=&fixture["media"][0];
  for effect in["lighting_effects"]{
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":media["parameters"],"animation":media["animation"]}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   assert_ne!(outputs[4],outputs[0],"later local time must differ {effect}");
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 }
}

#[cfg(test)]mod original_logo_cutout_stack_tests{use super::*;
#[test]fn original_logo_cutout_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-logo-cutout.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"logo_cutout","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Logo Cutout stack {index}");checked+=1;}
 assert_eq!(checked,864);
}}

#[cfg(test)]mod logo_cutout_media_stack_tests {use super::*;
#[test]fn logo_cutout_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-logo-cutout.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let animation=serde_json::json!({"threshold":curve(5.0,75.0),"softness":curve(10.0,100.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(1.0,0.75),"colorBlue":curve(1.0,0.125),"colorAlpha":curve(1.0,0.0)});
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"logo_cutout","parameters":media["parameters"],"animation":animation})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Logo Cutout animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,276);
}
}


#[cfg(test)]mod logo_cutout_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn logo_cutout_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for background in 0..3 {for unmultiply in [false,true] {for invert in [false,true] {let effect="logo_cutout";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"background":background,"unmultiply":unmultiply,"invert":invert},"animation":{"threshold":curve(5.0,25.0),"softness":curve(10.0,100.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(1.0,0.75),"colorBlue":curve(1.0,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }}}
 assert!(changed>0,"animated logo matte must change across local time");
 }
}


#[cfg(test)]mod original_rgb_split_stack_tests{use super::*;
#[test]fn original_rgb_split_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-rgb-split.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"rgb_split","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"RGB Split stack {index}");checked+=1;}
 assert_eq!(checked,1920);
}}


#[cfg(test)]mod rgb_split_media_stack_tests {use super::*;
#[test]fn rgb_split_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-rgb-split.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let animation=serde_json::json!({"amount":curve(1.25,10.0),"angle":curve(-37.25,90.0),"blend":curve(0.0,75.0),"centerX":curve(8.0,2.123456789012),"centerY":curve(8.0,1.75)});
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"rgb_split","parameters":media["parameters"],"animation":animation})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"RGB Split animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,46);
}
}



#[cfg(test)]mod rgb_split_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn rgb_split_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for mode in [0,1] {let effect="rgb_split";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"mode":mode},"animation":{"amount":curve(1.25,10.0),"angle":curve(-37.25,90.0),"blend":curve(0.0,75.0),"centerX":curve(8.0,2.123456789012),"centerY":curve(8.0,1.75)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated RGB channels must change across local time");
 }
}



#[cfg(test)]mod original_edge_glow_stack_tests{use super::*;
#[test]fn original_edge_glow_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edge-glow.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"edge_glow","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Edge Glow stack {index}");checked+=1;}
 assert_eq!(checked,32);
}}



#[cfg(test)]mod edge_glow_media_stack_tests {use super::*;
#[test]fn edge_glow_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-edge-glow.json")).unwrap();
 let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
 let animation=serde_json::json!({"threshold":curve(20.0,5.0),"width":curve(2.0,12.0),"radius":curve(10.0,30.0),"intensity":curve(150.0,350.0),"colorRed":curve(0.3,0.7),"colorGreen":curve(0.8,0.2),"colorBlue":curve(1.0,0.0),"colorAlpha":curve(1.0,0.0)});
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"edge_glow","parameters":media["parameters"],"animation":animation})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Edge Glow animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,46);
}
}




#[cfg(test)]mod edge_glow_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn edge_glow_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[(i*10)as u8,(255-i*10)as u8,64,255]).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for only in [false,true] {let effect="edge_glow";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":6,"height":4,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"only":only},"animation":{"threshold":curve(20.0,5.0),"width":curve(2.0,12.0),"radius":curve(10.0,30.0),"intensity":curve(150.0,350.0),"colorRed":curve(0.3,0.7),"colorGreen":curve(0.8,0.2),"colorBlue":curve(1.0,0.0),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated Edge Glow must change across local time");
 }
}




#[cfg(test)]mod original_echo_glow_stack_tests{use super::*;
#[test]fn original_echo_glow_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-echo-glow.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"echo_glow","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Echo Glow stack {index}");checked+=1;}
 assert_eq!(checked,32);
}}




#[cfg(test)]mod echo_glow_media_stack_tests {use super::*;
#[test]fn echo_glow_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-echo-glow.json")).unwrap();
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"echo_glow","parameters":media["parameters"],"animation":media["animation"]})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Echo Glow animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,46);
}
}





#[cfg(test)]mod echo_glow_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn echo_glow_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:16,height:16,rgba:(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for (echo_start,echo_end) in [(4.,1.5),(1.,12.)] {let effect="echo_glow";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=16;canvas.height=16;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":16,"height":16,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{},"animation":{"threshold":curve(60.0,5.0),"echoes":curve(echo_start,echo_end),"radius":curve(12.0,30.0),"spread":curve(1.8,2.5),"intensity":curve(100.0,350.0),"decay":curve(50.0,100.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(0.85,0.75),"colorBlue":curve(0.6,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated Echo Glow must change across local time");
 }
}





#[cfg(test)]mod original_wonder_glow_stack_tests{use super::*;
#[test]fn original_wonder_glow_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wonder-glow.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"wonder_glow","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Wonder Glow stack {index}");checked+=1;}
 assert_eq!(checked,64);
}}




#[cfg(test)]mod wonder_glow_media_stack_tests {use super::*;
#[test]fn wonder_glow_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wonder-glow.json")).unwrap();
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"wonder_glow","parameters":media["parameters"],"animation":media["animation"]})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Wonder Glow animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,92);
}
}






#[cfg(test)]mod wonder_glow_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn wonder_glow_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:16,height:16,rgba:(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for (mode,use_color) in [(0,false),(0,true),(1,false),(1,true)] {let effect="wonder_glow";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=16;canvas.height=16;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":16,"height":16,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"mode":mode,"useColor":use_color},"animation":{"threshold":curve(60.0,5.0),"radius":curve(25.0,75.0),"intensity":curve(100.0,350.0),"saturation":curve(100.0,400.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(0.8,0.75),"colorBlue":curve(0.5,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated Wonder Glow must change across local time");
 }
}

#[cfg(test)]mod original_glint_stack_tests{use super::*;
#[test]fn original_glint_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-glint.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"glint","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Glint stack {index}");checked+=1;}
 assert_eq!(checked,128);
}}





#[cfg(test)]mod glint_media_stack_tests {use super::*;
#[test]fn glint_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-glint.json")).unwrap();
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"glint","parameters":media["parameters"],"animation":media["animation"]})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Glint animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,92);
}
}







#[cfg(test)]mod glint_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn glint_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:16,height:16,rgba:(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for rays in 0..4 {let effect="glint";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=16;canvas.height=16;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":16,"height":16,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"rays":rays},"animation":{"threshold":curve(75.0,5.0),"length":curve(60.0,120.0),"rotation":curve(45.0,-135.0),"intensity":curve(100.0,350.0),"colorize":curve(0.0,100.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(1.0,0.75),"colorBlue":curve(1.0,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated Glint must change across local time");
 }
}


#[cfg(test)]mod original_volumetric_rays_stack_tests{use super::*;
#[test]fn original_volumetric_rays_persisted_stack_bits(){
 let data:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-volumetric-rays.json")).unwrap();let mut checked=0;
 for(index,c)in data["cases"].as_array().unwrap().iter().enumerate(){if c["scale"].as_f64().unwrap()!=1. {continue;}let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"volumetric_rays","parameters":c["parameters"]})).unwrap();let effects:Vec<crate::domain::composition::VideoEffect>=serde_json::from_value(serde_json::to_value(vec![effect]).unwrap()).unwrap();assert!(supported_effect_stack(&effects));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap() as usize,height:c["height"].as_u64().unwrap() as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let frame=apply_effect_stack_retained(&mut surface,&effects,0,&[],None,&mut||Ok(())).unwrap();let bits:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"Volumetric Rays stack {index}");checked+=1;}
 assert_eq!(checked,448);
}}






#[cfg(test)]mod volumetric_rays_media_stack_tests {use super::*;
#[test]fn volumetric_rays_animated_all_original_frames(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-volumetric-rays.json")).unwrap();
 let mut checked=0;
 for media in fixture["media"].as_array().unwrap(){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"volumetric_rays","parameters":media["parameters"],"animation":media["animation"]})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let start=media["localTick"].as_u64().unwrap();let rgba:Vec<u8>=serde_json::from_value(media["rgba"].clone()).unwrap();
 for(index,bits)in media["frames"].as_array().unwrap().iter().enumerate(){let mut child=GroupSurface{width:16,height:16,rgba:rgba.clone()};let frame=apply_effect_stack_retained(&mut child,&[effect.clone()],start+index as u64*100000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Volumetric Rays animated {start}/{index}");checked+=1;}}
 assert_eq!(checked,46);
}
}








#[cfg(test)]mod volumetric_rays_local_clock_tests {
 use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]fn volumetric_rays_real_image_and_retimed_video_use_local_curves(){
  let input=GroupSurface{width:16,height:16,rgba:(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect()};
  let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
  let mut changed=0;for only in [false,true] {let effect="volumetric_rays";
   let mut outputs=Vec::new();
   for (video,duration,speed,local) in[(false,1000000,1.,250000),(false,500000,1.,250000),(true,1000000,1.,250000),(true,500000,2.,250000),(false,1000000,1.,500000)] {
    let mut canvas=CanvasSpec::default();canvas.width=16;canvas.height=16;let mut composition=Composition::new(canvas);
    composition.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":if video{"video"}else{"image"},"durationTicks":if video{1000000}else{0},"width":16,"height":16,"hasAudio":false}})).unwrap();
    let mut clip=serde_json::json!({"id":"clip","sourceId":"source","transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.},"blendMode":"normal","effects":[{"kind":effect,"parameters":{"only":only},"animation":{"threshold":curve(60.0,5.0),"length":curve(50.0,100.0),"centerX":curve(8.0,2.123456789012),"centerY":curve(4.0,-0.75),"intensity":curve(100.0,350.0),"colorRed":curve(1.0,0.25),"colorGreen":curve(0.95,0.75),"colorBlue":curve(0.8,0.125),"colorAlpha":curve(1.0,0.0)}}]});
    if video {clip["placement"]=serde_json::json!({"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":speed});}
    else {clip["timelineStartTick"]=serde_json::json!(1000000);clip["durationTicks"]=serde_json::json!(duration);}
    composition.tracks=serde_json::from_value(serde_json::json!([{"kind":if video{"video"}else{"image"},"id":"track","name":"Track","clips":[clip]}])).unwrap();
    composition.validate().unwrap();
    let mut image_decode=|_:&crate::domain::composition::SourceId|Ok(input.clone());
    let mut video_decode=|_:&crate::domain::composition::SourceId,_:u64,_:bool|Ok(input.clone());
    let output=render_media_layers(&composition,1000000+local,&mut||Ok(()),Some(&mut image_decode),Some(&mut video_decode)).unwrap().unwrap();outputs.push(output);
   }
   for frame in &outputs[1..4]{assert_eq!(frame,&outputs[0],"same local seconds across durations/speeds {effect}");}
   if outputs[4]!=outputs[0] {changed+=1;}
   let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{}})).unwrap();
   assert!(apply_effect_stack(&mut input.clone(),&[effect],250000,&[],&mut||Ok(())).is_ok());
  }
 assert!(changed>0,"animated Volumetric Rays must change across local time");
 }
}



#[cfg(test)]mod posterize_time_media_tests{use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
#[test]fn posterize_time_redecodes_retimed_video_and_reapplies_prefix(){let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":2000000,"width":6,"height":4,"hasAudio":false}})).unwrap();c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":250000,"sourceOutTick":1250000,"speed":2.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{}},{"kind":"posterize_time","parameters":{}}]}]}])).unwrap();c.validate().unwrap();let mut times=vec![];let input=|t:u64|GroupSurface{width:6,height:4,rgba:if t==416666{[32,64,128,255].repeat(24)}else{[200,100,50,255].repeat(24)}};let mut decode=|_:&crate::domain::composition::SourceId,t:u64,_:bool|{times.push(t);Ok(input(t))};let actual=render_media_layers(&c,1125000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();assert_eq!(times,vec![500000,416666]);let invert:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"color_invert","parameters":{}})).unwrap();let mut expected=input(416666);apply_effect_stack(&mut expected,&[invert],83333,&[],&mut||Ok(())).unwrap();assert_eq!(actual.rgba,expected.rgba);}
}
#[cfg(test)]mod posterize_time_prefix_tests{use super::*;
#[test]fn posterize_time_prefix_without_tracks_uses_own_picture(){
let input=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[i*10,255-i*10,i*5,255]).collect()};
for (kind,parameters) in [("gradient_wipe",serde_json::json!({"layer":1,"completion":50.0,"softness":10.0})),("compound_blur",serde_json::json!({"layer":1,"max":10.0}))]{
let external:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":parameters})).unwrap();let mut parameters=parameters;parameters["layer"]=serde_json::json!(0);let own:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":kind,"parameters":parameters})).unwrap();let actual=apply_effect_stack_retained(&mut input.clone(),&[external],0,&[],Some(&mut TemporalPrefixContext{burnin:None,duration:1000000,fps:Some(24.),progress:None,source_progress:None}),&mut||Ok(())).unwrap();let expected=apply_effect_stack_retained(&mut input.clone(),&[own],0,&[],None,&mut||Ok(())).unwrap();assert_eq!(actual.pixels(),expected.pixels(),"source own-picture temporal fallback {kind}");
}
}
}
#[cfg(test)]mod posterize_time_original_media_tests{use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
#[test]fn posterize_time_moving_original_frames_with_animated_rate_and_prefix(){let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-posterize-time.json")).unwrap();let mut checked=0;for case in oracle["media"].as_array().unwrap(){let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":6,"height":4,"hasAudio":false}})).unwrap();let mut effects=vec![];if case["invert"]==true{effects.push(serde_json::json!({"kind":"color_invert","parameters":{}}));}let mut temporal=serde_json::json!({"kind":"posterize_time","parameters":{}});if case["animated"]==true{temporal["animation"]=serde_json::json!({"rate":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":12.0},{"tick":500000,"value":24.0}]}}});}effects.push(temporal);c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":effects}]}])).unwrap();c.validate().unwrap();let start=case["localTick"].as_u64().unwrap();for(index,bits)in case["frames"].as_array().unwrap().iter().enumerate(){let mut decode=|_:&crate::domain::composition::SourceId,t:u64,_:bool|Ok(GroupSurface{width:6,height:4,rgba:(0..24u64).flat_map(|i|{let k=t/10000;[(i*17+k*11)%256,(i*7+k*23)%256,(i*31+k*3)%256,255].map(|v|v as u8)}).collect()});let mut working=None;render_media_layers_context(&c,1000000+start+index as u64*100000,&mut||Ok(()),None,Some(&mut decode),&[],0,None,Some(&mut working),None).unwrap().unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();assert_eq!(working.unwrap().pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"moving original {case:?} frame{index}");checked+=1;}}assert_eq!(checked,92);}
}
#[cfg(test)]mod posterize_time_playback_tests{use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec};
#[test]fn posterize_time_reverse_freeze_multiple_temporal_prefix_and_absent_context(){
for(mode,expected,seek)in[(serde_json::json!({"mode":"forward"}),vec![500000,416666,250000],true),(serde_json::json!({"mode":"reverse"}),vec![999999,1083333,1249999],true),(serde_json::json!({"mode":"freeze","sourceTick":400000}),vec![400000,400000,400000],true)]{
let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":2000000,"width":6,"height":4,"hasAudio":false}})).unwrap();c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","playbackMode":mode,"placement":{"timelineStartTick":1000000,"sourceInTick":250000,"sourceOutTick":1250000,"speed":2.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"color_invert","parameters":{}},{"kind":"posterize_time","parameters":{}},{"kind":"posterize_time","parameters":{"rate":4.0}}]}]}])).unwrap();c.validate().unwrap();let mut calls=vec![];let input=|t:u64|GroupSurface{width:6,height:4,rgba:[(t/1000%256)as u8,64,128,255].repeat(24)};let mut decode=|_:&crate::domain::composition::SourceId,t:u64,s:bool|{calls.push((t,s));Ok(input(t))};let output=render_media_layers(&c,1125000,&mut||Ok(()),None,Some(&mut decode)).unwrap().unwrap();assert_eq!(calls,expected.iter().map(|t|(*t,seek)).collect::<Vec<_>>());let invert:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"color_invert","parameters":{}})).unwrap();let mut reference=input(*expected.last().unwrap());apply_effect_stack(&mut reference,&[invert],0,&[],&mut||Ok(())).unwrap();assert_eq!(output.rgba,reference.rgba);
}
let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"posterize_time","parameters":{}})).unwrap();let mut input=GroupSurface{width:6,height:4,rgba:[32,64,128,255].repeat(24)};let original=input.clone();apply_effect_stack(&mut input,&[effect],125000,&[],&mut||Ok(())).unwrap();assert_eq!(input,original);
}
}
#[cfg(test)]
mod echo_original_media_tests {
 use super::*;
 use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]
 fn echo_video_layer_matches_original_seven_operators(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-echo.json")).unwrap();
  for (index,case) in oracle["cases"].as_array().unwrap().iter().enumerate(){
   let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);
   c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":6,"height":4,"hasAudio":false}})).unwrap();
   let mut parameters=case.clone();parameters.as_object_mut().unwrap().remove("bits");parameters.as_object_mut().unwrap().remove("alpha");let alpha=case["alpha"].as_u64().unwrap();
   c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[{"kind":"echo","parameters":parameters}]}]}])).unwrap();c.validate().unwrap();
   let mut decode=|_:&crate::domain::composition::SourceId,t:u64,_:bool|Ok(GroupSurface{width:6,height:4,rgba:(0..24u64).flat_map(|i|{let k=t/10000;[(i*17+k*11)%256,(i*7+k*23)%256,(i*31+k*3)%256,alpha].map(|v|v as u8)}).collect()});
   let mut working=None;render_media_layers_context(&c,1500000,&mut||Ok(()),None,Some(&mut decode),&[],0,Some(0),Some(&mut working),None).unwrap().unwrap();
   let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
   assert_eq!(working.unwrap().pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"original Echo video-layer case{index}");
  }
 }
 #[test]
 fn echo_without_temporal_context_is_identity(){
  let effect=serde_json::from_value(serde_json::json!({"kind":"echo","parameters":{}})).unwrap();
  let mut input=GroupSurface{width:6,height:4,rgba:[32,64,128,255].repeat(24)};let original=input.clone();apply_effect_stack(&mut input,&[effect],500000,&[],&mut||Ok(())).unwrap();assert_eq!(input,original);
 }
}
#[cfg(test)]
mod echo_animated_original_media_tests {
 use super::*;
 use crate::domain::composition::{CanvasSpec,TransformSpec};
 #[test]
 fn echo_all_animated_original_frames_with_prefix_and_local_clock(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-echo.json")).unwrap();let mut checked=0;
  for case in oracle["media"].as_array().unwrap(){
   let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);
   c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":6,"height":4,"hasAudio":false}})).unwrap();
   let mut animation=serde_json::Map::new();for(channel,from,to)in[("time",-0.1,0.1),("count",1.,6.5),("start",1.,0.25),("decay",0.5,1.5)]{animation.insert(channel.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":from},{"tick":500000,"value":to}]}}));}
   let mut effects=vec![];if case["invert"]==true{effects.push(serde_json::json!({"kind":"color_invert","parameters":{}}));}effects.push(serde_json::json!({"kind":"echo","parameters":{"operator":case["operator"]},"animation":animation}));
   c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":effects}]}])).unwrap();c.validate().unwrap();let alpha=case["alpha"].as_u64().unwrap();let start=case["localTick"].as_u64().unwrap();
   for(index,bits)in case["frames"].as_array().unwrap().iter().enumerate(){
    let mut decode=|_:&crate::domain::composition::SourceId,t:u64,_:bool|Ok(GroupSurface{width:6,height:4,rgba:(0..24u64).flat_map(|i|{let k=t/10000;[(i*17+k*11)%256,(i*7+k*23)%256,(i*31+k*3)%256,alpha].map(|v|v as u8)}).collect()});
    let mut working=None;render_media_layers_context(&c,1000000+start+index as u64*100000,&mut||Ok(()),None,Some(&mut decode),&[],0,Some(0),Some(&mut working),None).unwrap().unwrap();let expected:Vec<u32>=serde_json::from_value(bits.clone()).unwrap();
    assert_eq!(working.unwrap().pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"animated Echo alpha{alpha} invert{} operator{} start{start} frame{index}",case["invert"],case["operator"]);checked+=1;
   }
  }
  assert_eq!(checked,644);
 }
}
#[cfg(test)]
mod echo_playback_tests {
 use super::*;
 use crate::domain::{composition::{CanvasSpec,TransformSpec,VideoEffect},echo::Echo};
 #[test]
 fn echo_reverse_freeze_speed_and_temporal_prefix_requests(){
  for(mode,source,seek)in[(serde_json::json!({"mode":"forward"}),[500000,416666,450000,400000,350000],true),(serde_json::json!({"mode":"reverse"}),[999999,1083333,1049999,1099999,1149999],true),(serde_json::json!({"mode":"freeze","sourceTick":400000}),[400000;5],true)]{
   for operator in 0..7{for echo_first in [false,true]{
    let p=Echo{time:-0.025,count:3.,start:0.5,decay:0.5,operator};
    let invert:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"color_invert","parameters":{}})).unwrap();
    let poster:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"posterize_time","parameters":{}})).unwrap();
    let echo=VideoEffect::Echo{parameters:p,animation:None};
    let effects=if echo_first{vec![invert.clone(),echo,poster]}else{vec![invert.clone(),poster,echo]};
    let mut canvas=CanvasSpec::default();canvas.width=6;canvas.height=4;let mut c=Composition::new(canvas);
    c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":2000000,"width":6,"height":4,"hasAudio":false}})).unwrap();
    c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","playbackMode":mode,"placement":{"timelineStartTick":1000000,"sourceInTick":250000,"sourceOutTick":1250000,"speed":2.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":effects}]}])).unwrap();c.validate().unwrap();
    let input=|t:u64|GroupSurface{width:6,height:4,rgba:[(t/1000%256)as u8,64,128,255].repeat(24)};let mut calls=vec![];
    let mut decode=|_:&crate::domain::composition::SourceId,t:u64,s:bool|{calls.push((t,s));Ok(input(t))};let mut working=None;
    render_media_layers_context(&c,1125000,&mut||Ok(()),None,Some(&mut decode),&[],0,Some(0),Some(&mut working),None).unwrap().unwrap();
    let expected_calls=if echo_first{vec![source[0],source[2],source[3],source[4],source[1]]}else{source.to_vec()};assert_eq!(calls,expected_calls.iter().map(|t|(*t,seek)).collect::<Vec<_>>(),"{mode} operator{operator} echo_first{echo_first}");
    let inverted=|t|{let mut frame=input(t);apply_effect_stack_retained(&mut frame,&[invert.clone()],0,&[],None,&mut||Ok(())).unwrap()};
    let expected=if echo_first{inverted(source[1])}else{let layers=vec![(inverted(source[1]),0.5),(inverted(source[2]),0.25),(inverted(source[3]),0.125),(inverted(source[4]),0.0625)];p.combine(&layers,&mut||Ok(())).unwrap()};
    assert_eq!(working.unwrap().pixels(),expected.pixels(),"playback pixels {mode} operator{operator} echo_first{echo_first}");
   }}
  }
 }
}
#[cfg(test)]mod auto_reframe_stack_tests{use super::*;
#[test]fn auto_reframe_original_persisted_standalone_stack(){
 let f:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-auto-reframe.json")).unwrap();let mut count=0;
 for(index,c)in f["cases"].as_array().unwrap().iter().enumerate().filter(|(_,c)|c["scale"]==1.){let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"auto_reframe","parameters":c["parameters"]})).unwrap();assert!(supported_effect_stack(&[effect.clone()]));let mut surface=GroupSurface{width:c["width"].as_u64().unwrap()as usize,height:c["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(c["rgba"].clone()).unwrap()};let output=apply_effect_stack_retained(&mut surface,&[effect],250000,&[],None,&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(c["bits"].clone()).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Auto Reframe persisted stack{index}");count+=1;}assert_eq!(count,120);
}}
#[cfg(test)]mod auto_reframe_clip_context_tests{use super::*;use crate::domain::composition::{CanvasSpec,TransformSpec,VideoEffect};
fn picture(t:u64)->GroupSurface{let rgba=(0..180usize).flat_map(|i|{let x=(t/100000%12)as usize+2;if(i%18).abs_diff(x)<=1&&(i/18).abs_diff(4)<=1{[255,192,64,128]}else{[(i*7%32)as u8,(i*3%24)as u8,(i*11%16)as u8,128]}}).collect();GroupSurface{width:18,height:10,rgba}}
#[test]fn auto_reframe_original_moving_frames_through_clip_context(){
 let f:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-auto-reframe.json")).unwrap();assert_eq!(f["moving"].as_array().unwrap().len(),60);
 for(index,case)in f["moving"].as_array().unwrap().iter().enumerate(){let tick=case["tick"].as_u64().unwrap();let effect:VideoEffect=serde_json::from_value(serde_json::json!({"kind":"auto_reframe","parameters":case["parameters"]})).unwrap();let mut canvas=CanvasSpec::default();canvas.width=10;canvas.height=16;canvas.fps_milli=29970;let mut c=Composition::new(canvas);c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":18,"height":10,"hasAudio":false}})).unwrap();c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[effect.clone()]}]}])).unwrap();c.validate().unwrap();let clip=match &c.tracks[0]{CompositionTrack::Video{clips,..}=>&clips[0],_=>unreachable!()};let mut decode=|_:&crate::domain::composition::SourceId,t:crate::domain::video_clock::FrameTime,_:bool,_clip:&crate::domain::composition::CompositionClipId|Ok(picture(t.legacy_microseconds()?));let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:Some(TemporalClip::Video(clip)),clip_duration_us:1000000,composition:&c,time_us:1000000+tick,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:None,video:Some(&mut decode)};let mut input=picture(tick);let output=apply_effect_stack_retained(&mut input,&[effect],tick,&[],Some(&mut context),&mut||Ok(())).unwrap();let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Auto Reframe actual clip context{index}");}
}
}

#[cfg(test)] mod warp_stabilizer_clip_context_tests {
 use super::*;
 use crate::domain::composition::{CanvasSpec,TransformSpec,VideoEffect};
 fn picture(t:u64,alpha:u8)->GroupSurface {
  let k=(t as f64/1000000.*6.).round()as i64;
  let rgba=(0..96*64).flat_map(|i|{let x=(i%96)as i64-2*k;let y=(i/96)as i64-k;let u=x.rem_euclid(96)as usize;let v=y.rem_euclid(64)as usize;let a=((u*17+v*31+(u/7)*(v/5)*53)%256)as u8;[a,((u*3+v*11)%256)as u8,((u*29+v*7)%256)as u8,alpha]}).collect();
  GroupSurface{width:96,height:64,rgba}
 }
 #[test] fn warp_stabilizer_original_moving_frames_through_clip_context() {
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-warp-stabilizer.json")).unwrap();
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),32);
  for animated in [false,true] { for(index,case)in cases.iter().enumerate(){
   let alpha=case["alpha"].as_u64().unwrap()as u8;
   let mut authored=serde_json::json!({"kind":"warp_stabilizer","parameters":case["parameters"]});
   if animated {
    let controls=[("smoothness",100.,25.,75.),("maxScale",300.,100.,200.),("actionSafe",30.,0.,0.),("additionalScale",150.,75.,125.),("cropLess",90.,25.,75.)];
    let mut animation=serde_json::Map::new();
    for(channel,base,left,right)in controls {authored["parameters"][channel]=serde_json::json!(base);animation.insert(channel.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":250000,"value":left},{"tick":750000,"value":right}]}}));}
    authored["animation"]=serde_json::Value::Object(animation);
   }
   let effect:VideoEffect=serde_json::from_value(authored).unwrap();
   assert!(supported_effect_stack(&[effect.clone()]));
   let mut canvas=CanvasSpec::default();canvas.width=96;canvas.height=64;canvas.fps_milli=6000;
   let mut c=Composition::new(canvas);
   c.sources=serde_json::from_value(serde_json::json!({"source":{"id":"source","kind":"video","durationTicks":1000000,"width":96,"height":64,"hasAudio":false}})).unwrap();
   c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"v","name":"V","clips":[{"id":"clip","sourceId":"source","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","effects":[effect.clone()]}]}])).unwrap();c.validate().unwrap();
   let clip=match &c.tracks[0]{CompositionTrack::Video{clips,..}=>&clips[0],_=>unreachable!()};
   let mut calls=vec![];
   let mut decode=|_:&crate::domain::composition::SourceId,t:crate::domain::video_clock::FrameTime,_:bool,_clip:&crate::domain::composition::CompositionClipId|{let t=t.legacy_microseconds()?;calls.push(t);Ok(picture(t,alpha))};
   let mut context=CompositionEffectContext{exact_sample:None,temporal_clip:Some(TemporalClip::Video(clip)),clip_duration_us:1000000,composition:&c,time_us:1500000,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:None,video:Some(&mut decode)};
   let mut input=picture(500000,alpha);
   let output=apply_effect_stack_retained(&mut input,&[effect],500000,&[],Some(&mut context),&mut||Ok(())).unwrap();
   let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
   assert_eq!(output.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"Warp Stabilizer actual clip context {index}");
   assert_eq!(calls,vec![0,166667,333333,500000,666667,833333]);
  }}
 }
}

#[cfg(test)]
#[path = "track_matte_composition_tests.rs"]
mod track_matte_composition_tests;

#[cfg(test)]mod independent_transition_sequence_tests{
 use super::*;
 #[test]fn independent_transition_sequence_matches_original_756_frames(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-sequence.json")).unwrap();
  assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
  let black:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-dissolve-sequence.json")).unwrap();
  let wipe:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wipe-sequence.json")).unwrap();
  let cases:Vec<_>=oracle["cases"].as_array().unwrap().iter().chain(black["cases"].as_array().unwrap().iter().filter(|case|case["effect"]!="cross_dissolve")).chain(wipe["cases"].as_array().unwrap()).collect();assert_eq!(cases.len(),756);
  for case in cases{
   let mode=case["mode"].as_u64().unwrap();let mut composition=Composition::new(Default::default());
   composition.canvas.width=4;composition.canvas.height=4;composition.canvas.background.alpha=0.;
   composition.sources=serde_json::from_value(serde_json::json!({"red":{"id":"red","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false},"blue":{"id":"blue","kind":"video","durationTicks":100000000,"width":4,"height":4,"hasAudio":false}})).unwrap();
   let kind=if let Some(direction)=case["direction"].as_u64(){["wipe_down","wipe_left","wipe_up","wipe_right"][direction as usize]}else{match case["effect"].as_str(){Some("dip_to_black")=>"fade_black",Some("dip_to_white")=>"fade_white",Some("film_dissolve")=>"film_dissolve",Some("additive_dissolve")=>"additive_dissolve",_=>"dissolve"}};
   let mut transitions=vec![serde_json::json!({"id":"first","fromClipId":if mode==1{None}else{Some("a")},"toClipId":if mode==2{None}else{Some("b")},"timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind,"reverse":case["reverse"]})];
   if mode==5{transitions.push(serde_json::json!({"id":"second","fromClipId":null,"toClipId":"b","timeline":{"startTick":"2286144000000","durationTick":"508032000000"},"kind":kind}));}
   composition.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","transitions":transitions,"clips":[{"id":"a","sourceId":"red","placement":{"timelineStartTick":0,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","enabled":mode!=3},{"id":"b","sourceId":"blue","placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":super::super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal","enabled":mode!=4}]}])).unwrap();
   let exact=case["at"].as_str().unwrap().parse::<i128>().unwrap();let time=(exact/254016)as u64;
   let mut decode=|id:&super::super::composition::SourceId,_:u64,_:bool|Ok(GroupSurface{width:4,height:4,rgba:if id.as_str()=="red"{[255,0,0,255].repeat(16)}else{[0,0,255,255].repeat(16)}});
   let mut working=None;
   render_media_layers_context(&composition,time,&mut||Ok(()),None,Some(&mut decode),&[],0,None,Some(&mut working),Some(exact)).unwrap().expect("native transition support");
   let bits:Vec<u32>=working.unwrap().pixels().iter().flatten().map(|value|value.to_bits()).collect();
   assert_eq!(bits,serde_json::from_value::<Vec<u32>>(case["bits"].clone()).unwrap(),"{case}");
  }
 }
}

#[cfg(test)]
#[path="flat_surface_procedural_tests.rs"]
mod procedural_transition_scene_tests;

include!("effect_stack_clock_tests.rs");

include!("effect_context_sample.rs");

include!("effect_progress_scene_tests.rs");

#[cfg(test)]
#[path="transition_effect_default_scene_tests.rs"]
mod transition_effect_default_scene_tests;

include!("transition_temporal_context.rs");

#[cfg(test)]
#[path="transition_effect_animation_scene_tests.rs"]
mod transition_effect_animation_scene_tests;

#[cfg(test)]
#[path="transition_effect_persisted_scene_tests.rs"]
mod transition_effect_persisted_scene_tests;

#[cfg(test)]
#[path="transition_effect_varying_source_tests.rs"]
mod transition_effect_varying_source_tests;

#[cfg(test)]
#[path="transition_effect_varying_animation_tests.rs"]
mod transition_effect_varying_animation_tests;

#[cfg(test)]
#[path = "transition_lumetri_animation_tests.rs"]
mod transition_lumetri_animation_tests;

#[cfg(test)]
#[path = "transition_lumetri_wheel_tests.rs"]
mod transition_lumetri_wheel_tests;

#[cfg(test)]
#[path = "transition_tint_color_tests.rs"]
mod transition_tint_color_tests;

#[cfg(test)]
#[path = "transition_keying_color_tests.rs"]
mod transition_keying_color_tests;

#[cfg(test)]
#[path = "transition_generator_color_tests.rs"]
mod transition_generator_color_tests;

#[cfg(test)]
#[path = "transition_vr_color_tests.rs"]
mod transition_vr_color_tests;

#[cfg(test)]
#[path = "transition_glow_color_tests.rs"]
mod transition_glow_color_tests;

#[cfg(test)]
#[path = "transition_fill_color_tests.rs"]
mod transition_fill_color_tests;

#[cfg(test)]
#[path = "transition_lighting_color_tests.rs"]
mod transition_lighting_color_tests;

#[cfg(test)]
#[path = "transition_lumetri_color_tests.rs"]
mod transition_lumetri_color_tests;

#[cfg(test)]
#[path = "transition_auto_generator_point_tests.rs"]
mod transition_auto_generator_point_tests;
#[path = "transition_auto_distortion_point_tests.rs"]
mod transition_auto_distortion_point_tests;
#[path = "transition_auto_layout_point_tests.rs"]
mod transition_auto_layout_point_tests;
#[path = "transition_key_auto_generator_point_tests.rs"]
mod transition_key_auto_generator_point_tests;
#[path = "transition_key_auto_distortion_point_tests.rs"]
mod transition_key_auto_distortion_point_tests;
#[path = "transition_key_auto_layout_point_tests.rs"]
mod transition_key_auto_layout_point_tests;
#[path = "transition_key_auto_move_point_tests.rs"]
mod transition_key_auto_move_point_tests;
#[path = "transition_generator_point_tests.rs"]
mod transition_generator_point_tests;

#[cfg(test)]
#[path = "transition_distortion_point_tests.rs"]
mod transition_distortion_point_tests;

#[cfg(test)]
#[path = "transition_layout_point_tests.rs"]
mod transition_layout_point_tests;

#[cfg(test)]
#[path = "transition_key_auto_motion_tests.rs"]
mod transition_key_auto_motion_tests;
#[path = "ordinary_key_auto_motion_tests.rs"]
mod ordinary_key_auto_motion_tests;
#[path = "transition_motion_import_tests.rs"]
mod transition_motion_import_tests;

#[cfg(test)]
#[path = "ordinary_motion_import_tests.rs"]
mod ordinary_motion_import_tests;

#[cfg(test)]
#[path = "ordinary_placement_tests.rs"]
mod ordinary_placement_tests;
