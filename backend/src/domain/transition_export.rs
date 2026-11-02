//! Admission shared by independent-window export routing and trusted preparation.
use super::composition::{Composition,CompositionTrack,TransitionKind,StabilizationSpec,FrameInterpolation};
use anyhow::{Result,ensure};

pub fn has_independent_windows(c:&Composition)->bool {
    c.tracks.iter().any(|track|matches!(track,CompositionTrack::Video{hidden:false,transitions,..} if transitions.iter().any(|t|t.timeline.is_some())))
}

/// Reject combinations whose clocks/processing groups have not reached native parity yet.
/// This does not replace composition validation or ordinary export resource admission.
pub fn validate(c:&Composition)->Result<()> {
    c.validate()?;
    if !has_independent_windows(c){return Ok(());}
    let primary=c.tracks.iter().rposition(|track|match track {
        CompositionTrack::Video{hidden:false,clips,transitions,..}=>clips.iter().any(|clip|clip.enabled)||transitions.iter().any(|t|t.timeline.is_some()),
        CompositionTrack::Image{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Text{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Color{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Graphic{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        _=>false,
    });
    for (index,track) in c.tracks.iter().enumerate(){
        let CompositionTrack::Video{hidden:false,clips,transitions,muted,..}=track else{continue};
        if !transitions.iter().any(|t|t.timeline.is_some()){continue;}
        ensure!(transitions.iter().all(|t|t.timeline.is_some()),"mixed legacy and independent transitions require unified window rendering");
        ensure!(Some(index)==primary||*muted||clips.iter().all(|clip|!clip.source_audio_enabled),
            "independent overlay transition audio requires multitrack embedded audio planning");
        for transition in transitions{
            ensure!(matches!(transition.kind,TransitionKind::ConcatDissolve|TransitionKind::ConcatFadeBlack|TransitionKind::ConcatFadeWhite|TransitionKind::Procedural|TransitionKind::Dissolve|TransitionKind::FilmDissolve|TransitionKind::AdditiveDissolve|TransitionKind::FadeBlack|TransitionKind::FadeWhite|TransitionKind::WipeLeft|TransitionKind::WipeRight|TransitionKind::WipeUp|TransitionKind::WipeDown),
                "independent transition kernel is not migrated");
            for id in [transition.from_clip_id.as_ref(),transition.to_clip_id.as_ref()].into_iter().flatten(){
                let clip=clips.iter().find(|clip|&clip.id==id).expect("validated endpoint");
                if !clip.enabled{continue;}
                if !clip.effects.is_empty(){ensure!(transition.concat_overlap.is_some(),"independent transition side effects require signed effect clocks");super::concat_project_transitions::validate_side_effects(&clip.effects)?;}
                ensure!(clip.placement.speed_ramp.is_none(),"independent transition speed ramp requires extrapolated ramp clock");
                ensure!(clip.stabilization==StabilizationSpec::Disabled&&clip.frame_interpolation==FrameInterpolation::Duplicate,
                    "independent transition stabilization/interpolation requires source-handle qualification");
            }
        }
    }
    Ok(())
}

/// Timing-only graph for streams already owned by native transition rendering.
/// Call only after validating the authored composition and arranging native PCM/video.
/// Legacy adjacency/gap rules must never reinterpret the authored independent windows.
pub fn replace_independent_tracks_with_timing(c: &mut Composition) -> Result<()> {
    use super::composition::{SolidColorClip, TransformSpec, AnimatableValue};
    let duration = c.duration_ticks()?;
    for track in &mut c.tracks {
        if let CompositionTrack::Video {id,name,hidden,locked,clips,transitions,..} = track {
            if !transitions.iter().any(|transition| transition.timeline.is_some()) { continue; }
            let clip_id = clips.first().ok_or_else(|| anyhow::anyhow!("independent transition track lacks clips"))?.id.clone();
            *track = CompositionTrack::Color {
                id: id.clone(), name: name.clone(), hidden: *hidden, locked: *locked,
                clips: vec![SolidColorClip {
                    id: clip_id, timeline_start_tick: 0, timeline_end_tick: duration,
                    width: c.canvas.width, height: c.canvas.height,
                    color: super::animated_color::Color {r:0,g:0,b:0,a:0},
                    color_animation: None, transform: TransformSpec::default(),
                    opacity: AnimatableValue::constant(1.0), enabled: true,
                }],
            };
        }
    }
    Ok(())
}

/// A sparse primary video needs native PCM as well as video: the discarded legacy
/// concat graph cannot express its leading/internal silence without shifting audio.
pub fn sparse_primary_video_track(c:&Composition)->Option<usize>{
    let index=c.tracks.iter().rposition(|track|match track{
        CompositionTrack::Video{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Image{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Color{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Graphic{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),
        CompositionTrack::Text{hidden:false,clips,..}=>clips.iter().any(|clip|clip.enabled),_=>false})?;
    let CompositionTrack::Video{clips,..}=&c.tracks[index]else{return None};
    let mut enabled=clips.iter().filter(|clip|clip.enabled).collect::<Vec<_>>();enabled.sort_by_key(|clip|(clip.placement.timeline_start_tick,clip.id.as_str()));
    let mut cursor=0;for clip in enabled{if clip.placement.timeline_start_tick>cursor{return Some(index)}cursor=clip.placement.timeline_end_tick().ok()?;}None
}
/// Only for a discarded delivery graph after native video/PCM have been arranged.
pub fn replace_sparse_primary_with_timing(c:&mut Composition)->Result<()>{
    let Some(index)=sparse_primary_video_track(c)else{return Ok(())};let duration=c.native_duration_ticks()?;
    let CompositionTrack::Video{id,name,hidden,locked,clips,..}=&c.tracks[index]else{unreachable!()};
    let clip_id=clips.iter().find(|clip|clip.enabled).ok_or_else(||anyhow::anyhow!("sparse video lacks an enabled clip"))?.id.clone();
    c.tracks[index]=CompositionTrack::Color{id:id.clone(),name:name.clone(),hidden:*hidden,locked:*locked,clips:vec![super::composition::SolidColorClip{id:clip_id,timeline_start_tick:0,timeline_end_tick:duration,width:c.canvas.width,height:c.canvas.height,color:super::animated_color::Color{r:0,g:0,b:0,a:0},color_animation:None,transform:super::composition::TransformSpec::default(),opacity:super::composition::AnimatableValue::constant(1.),enabled:true}]};Ok(())
}

/// Native graphics are already painted into the prepared movie. The discarded
/// legacy graph needs only their duration, not graphic effects or overlaps.
pub fn replace_native_graphics_with_timing(c:&mut Composition){
    let(width,height)=(c.canvas.width,c.canvas.height);
    for track in &mut c.tracks{
        let CompositionTrack::Graphic{id,name,hidden,locked,clips}=track else{continue;};
        let timing=clips.first().map(|first|super::composition::SolidColorClip{
            id:first.id.clone(),timeline_start_tick:0,timeline_end_tick:clips.iter().map(|clip|clip.timeline_end_tick).max().unwrap(),width,height,
            color:super::animated_color::Color{r:0,g:0,b:0,a:0},color_animation:None,
            transform:super::composition::TransformSpec::default(),opacity:super::composition::AnimatableValue::constant(1.),enabled:clips.iter().any(|clip|clip.enabled),
        }).into_iter().collect();
        *track=CompositionTrack::Color{id:id.clone(),name:name.clone(),hidden:*hidden,locked:*locked,clips:timing};
    }
}
