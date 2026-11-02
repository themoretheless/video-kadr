//! Source-owned packaged transition placement, following pinned Concat export overlap rules.
use super::{
    composition::CompositionSource,
    concat_effect_upgrade::{native_transition, AppliedFilter},
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn number(clip: &Value, name: &str, default: f64) -> Result<f64> {
    let value = clip
        .get(name)
        .map_or(Some(default), Value::as_f64)
        .context("Invalid transition clip number")?;
    ensure!(value.is_finite(), "Nonfinite transition number");
    Ok(value)
}
pub(crate) fn frame_tick(seconds: f64, num: u64, den: u64) -> Result<i64> {
    // Keep original Concat quantise: multiply by the already-divided fps.
    // Reassociation changes half-frame ties at NTSC rates (e.g. 0.65065 s).
    let frames = (seconds * (num as f64 / den as f64)).round().max(0.) as i128;
    let ticks = frames
        .checked_mul(254016000000i128)
        .and_then(|v| v.checked_mul(den as i128))
        .context("Transition frame clock overflow")?;
    ensure!(
        ticks % num as i128 == 0,
        "Frame rate cannot be represented by exact source ticks"
    );
    i64::try_from(ticks / num as i128).context("Transition frame clock overflow")
}
#[derive(Clone, Copy)]
struct Window {
    start: f64,
    duration: f64,
    source_start: f64,
}
fn clip_window(clip:&super::composition::VideoClip,canvas:&super::composition::CanvasSpec)->Result<Window>{
    if let Some(clock)=&clip.placement.concat_retime {
        clock.validate_placement(&clip.placement)?;
        let(num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
        ensure!(num>0&&den>0&&u128::from(clock.rate_num)*u128::from(den)==u128::from(num)*u128::from(clock.rate_den),"retained document clock frame rate differs from canvas");
        let(start,duration,source_start)=clock.document_times(clip.placement.timeline_start_tick,clip.placement.source_in_tick)?;
        Ok(Window{start,duration,source_start})
    }else{Ok(Window{start:clip.placement.timeline_start_tick as f64/1e6,duration:clip.placement.timeline_duration_ticks()? as f64/1e6,source_start:clip.placement.source_in_tick as f64/1e6})}
}
impl Window {
    fn extend(&mut self, head: f64, tail: f64) {
        if head <= 0. && tail <= 0. {
            return;
        }
        self.duration = self.duration + head + tail;
        self.start -= head;
        self.source_start = (self.source_start - head).max(0.);
    }
}
// Preserve the original f64 operation order until video frame quantization.
// Integer source metadata validates ownership; it must not replace these raw
// sequential calculations at half-frame rounding boundaries.
struct RawOverlap {from:usize,to:usize,lead:f64,length:f64,held:bool,from_window:Window,to_window:Window,colour:Option<bool>}
fn raw_overlaps(clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec)->Result<Vec<RawOverlap>>{
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let frame=den as f64/num as f64;
    let mut windows=clips.iter().map(|clip|clip_window(clip,canvas)).collect::<Result<Vec<_>>>()?;
    let mut result=vec![];
    for transition in transitions {
        let overlap=transition.concat_overlap.context("Mixed source transition ownership")?;
        let from=clips.iter().position(|clip|Some(&clip.id)==transition.from_clip_id.as_ref()).context("Source outgoing clip missing")?;
        let to=clips.iter().position(|clip|Some(&clip.id)==transition.to_clip_id.as_ref()).context("Source incoming clip missing")?;
        let requested=overlap.authored_duration_ticks.unwrap_or(overlap.duration_ticks) as f64/1e6;
        if let Some(white)=colour_target(overlap.legacy){result.push(RawOverlap{from,to,lead:0.,length:requested,held:false,from_window:windows[from],to_window:windows[to],colour:Some(white)});continue;}
        ensure!(clips[from].placement.concat_retime.is_none()&&clips[to].placement.concat_retime.is_none(),"retained document clock overlap needs separate source window qualification");
        let length=requested.min(windows[from].duration).min(windows[to].duration);
        let mut lead=(length/2.).min(windows[to].source_start);
        if lead<frame/2.{lead=0.;}
        result.push(RawOverlap{from,to,lead,length,held:overlap.hold_outgoing,from_window:windows[from],to_window:windows[to],colour:None});
        windows[to].extend(lead,0.);
        if !overlap.hold_outgoing{windows[from].extend(0.,length-lead);}
    }
    Ok(result)
}

pub fn import(
    clips: &[Value],
    track: &Value,
    bindings: &BTreeMap<String, CompositionSource>,
    num: u64,
    den: u64,
) -> Result<Vec<Value>> {
    let mut result = vec![];
    let frame = den as f64 / num as f64;
    let mut windows = BTreeMap::<String, Window>::new();
    let mut from_ids = std::collections::BTreeSet::new();
    let mut to_ids = std::collections::BTreeSet::new();
    for incoming in clips
        .iter()
        .filter(|clip| clip["trackId"] == track["id"] && clip["kind"] == "video")
    {
        let Some(transition) = incoming.get("transitionIn").filter(|v| !v.is_null()) else {
            continue;
        };
        let object = transition
            .as_object()
            .context("transitionIn must be an object")?;
        ensure!(
            object
                .keys()
                .all(|key| matches!(key.as_str(), "id" | "duration")),
            "Unknown transitionIn field"
        );
        let id = transition["id"]
            .as_str()
            .context("Transition package missing")?;
        let legacy=match id{"cross-fade"=>Some(SourceLegacy::CrossFade),"push"=>Some(SourceLegacy::Push),"zoom"=>Some(SourceLegacy::Zoom),"wipe-left"=>Some(SourceLegacy::WipeLeft),"wipe-right"=>Some(SourceLegacy::WipeRight),"fade-black"=>Some(SourceLegacy::FadeBlack),"fade-white"=>Some(SourceLegacy::FadeWhite),_=>None};
        let descriptor_id=if legacy.is_some(){"concat.dissolve"}else{id};
        let descriptor =
            native_transition(&serde_json::from_value::<AppliedFilter>(json!({"id":descriptor_id}))?)?
                .context("Legacy/unknown source transition needs its original overlap renderer")?;
        let start = number(incoming, "start", 0.)?;
        let outgoing = clips
            .iter()
            .find(|clip| {
                clip["trackId"] == track["id"]
                    && clip["kind"] == "video"
                    && clip["id"] != incoming["id"]
                    && number(clip, "start", 0.)
                        .and_then(|s| {
                            Ok((s + number(clip, "duration", 1.)? - start).abs() < frame / 2.)
                        })
                        .unwrap_or(false)
            })
            .context("Orphan source transition needs retained inactive ownership")?;
        let from_id = outgoing["id"]
            .as_str()
            .context("Outgoing clip ID missing")?
            .to_owned();
        let to_id = incoming["id"]
            .as_str()
            .context("Incoming clip ID missing")?
            .to_owned();
        ensure!(
            from_ids.insert(from_id.clone()) && to_ids.insert(to_id.clone()),
            "Branching source transition ownership needs resolution"
        );
        let mut from_window = windows.get(&from_id).copied().unwrap_or(Window {
            start: number(outgoing, "start", 0.)?,
            duration: number(outgoing, "duration", 1.)?,
            source_start: number(outgoing, "sourceStart", 0.)?,
        });
        let mut to_window = windows.get(&to_id).copied().unwrap_or(Window {
            start: number(incoming, "start", 0.)?,
            duration: number(incoming, "duration", 1.)?,
            source_start: number(incoming, "sourceStart", 0.)?,
        });
        let requested = number(transition, "duration", 1.)?;
        ensure!(
            requested > 0. && requested * 1e6 <= 9007199254740991.,
            "Source transition authored duration outside exact native clock"
        );
        let length = requested.min(from_window.duration).min(to_window.duration);
        ensure!(
            colour_target(legacy).is_some()||length >= frame,
            "Subframe source transition needs retained inactive ownership"
        );
        for clip in [outgoing, incoming] {
            ensure!(
                colour_target(legacy).is_some() || number(clip, "speed", 1.)? == 1.,
                "Retimed transition sides need separately quantized source clocks"
            );
            for field in ["start", "sourceStart", "duration"] {
                if colour_target(legacy).is_some(){continue;}
                let position = number(clip, field, if field == "duration" { 1. } else { 0. })?
                    * num as f64
                    / den as f64;
                ensure!(
                    (position - position.round()).abs() < 1e-8,
                    "Off-grid transition sides need original frame/source clock quantization"
                );
            }
        }
        if colour_target(legacy).is_some(){
            let(begin,duration)=colour_span(from_window,to_window,requested,num,den)?;
            let mut native=serde_json::to_value(descriptor)?;
            native["id"]=json!(format!("concat-transition-{}",to_id));native["fromClipId"]=json!(from_id);native["toClipId"]=json!(to_id);
            native["concatOverlap"]=json!({"leadTicks":0,"durationTicks":(requested*1e6).round()as u64,"legacy":legacy});
            native["timeline"]=json!({"startTick":begin.to_string(),"durationTick":duration.to_string()});result.push(native);continue;
        }
        let speed = number(incoming, "speed", 1.)?;
        let mut lead = (length / 2.).min(to_window.source_start / speed);
        if lead < frame / 2. {
            lead = 0.;
        }
        let outgoing_source = &bindings[outgoing["mediaId"].as_str().unwrap()];
        let outgoing_speed = number(outgoing, "speed", 1.)?;
        let handle = ((outgoing_source.duration_ticks as f64 / 1e6 - from_window.source_start)
            / outgoing_speed
            - from_window.duration
            - frame)
            .max(0.);
        let hold_outgoing = handle + frame / 2. < length - lead;
        let begin = frame_tick(to_window.start - lead, num, den)?;
        let duration = frame_tick(length, num, den)?;
        let mut native = serde_json::to_value(descriptor)?;
        native["id"] = json!(format!(
            "concat-transition-{}",
            incoming["id"]
                .as_str()
                .context("Transition clip ID missing")?
        ));
        native["fromClipId"] = outgoing["id"].clone();
        native["toClipId"] = incoming["id"].clone();
        native["concatOverlap"] = json!({"leadTicks":(lead*1e6).round() as u64,"durationTicks":(length*1e6).round() as u64});
        if let Some(legacy)=legacy{native["concatOverlap"]["legacy"]=serde_json::to_value(legacy)?;}
        if (requested * 1e6).round() as u64 != (length * 1e6).round() as u64 {
            native["concatOverlap"]["authoredDurationTicks"] =
                json!((requested * 1e6).round() as u64);
        }
        if hold_outgoing {
            native["concatOverlap"]["holdOutgoing"] = json!(true);
        }
        native["timeline"] =
            json!({"startTick":begin.to_string(),"durationTick":duration.to_string()});
        to_window.extend(lead, 0.);
        if !hold_outgoing {
            from_window.extend(0., length - lead);
        }
        windows.insert(from_id, from_window);
        windows.insert(to_id, to_window);
        result.push(native);
    }
    Ok(result)
}
pub fn export(transition: &Value) -> Result<Value> {
    let native: super::composition::ClipTransition = serde_json::from_value(transition.clone())?;
    ensure!(
        !native.reverse && native.duration_ticks == 0,
        "Source transitions need forward explicit intervals"
    );
    let (_, duration) = native
        .timeline
        .context("Explicit source transition interval missing")?
        .source_span()
        .map_err(|e| anyhow::anyhow!("Invalid source transition interval: {e:?}"))?;
    let id = match native.kind {
        super::composition::TransitionKind::ConcatDissolve => "concat.dissolve",
        super::composition::TransitionKind::ConcatFadeBlack => "concat.fade-black",
        super::composition::TransitionKind::ConcatFadeWhite => "concat.fade-white",
        super::composition::TransitionKind::Procedural => {
            &native
                .kernel
                .as_ref()
                .context("Transition kernel missing")?
                .effect
        }
        _ => anyhow::bail!("Native transition has no exact source package"),
    };
    let descriptor =
        native_transition(&serde_json::from_value::<AppliedFilter>(json!({"id":id}))?)?
            .context("Source transition package missing")?;
    ensure!(
        descriptor.kind == native.kind && descriptor.kernel == native.kernel,
        "Source transitionIn cannot represent custom package parameters"
    );
    let source_id=if let Some(legacy)=native.concat_overlap.and_then(|overlap|overlap.legacy){
        ensure!(native.kind==super::composition::TransitionKind::ConcatDissolve&&native.kernel.is_none(),"Modified legacy transition kernel");
        match legacy{SourceLegacy::CrossFade=>"cross-fade",SourceLegacy::Push=>"push",SourceLegacy::Zoom=>"zoom",SourceLegacy::WipeLeft=>"wipe-left",SourceLegacy::WipeRight=>"wipe-right",SourceLegacy::FadeBlack=>"fade-black",SourceLegacy::FadeWhite=>"fade-white"}
    }else{id};
    Ok(
        json!({"id":source_id,"duration":native.concat_overlap.map_or(duration as f64/254016000000.,|overlap|overlap.authored_duration_ticks.unwrap_or(overlap.duration_ticks) as f64/1e6)}),
    )
}

/// Legacy cross-fade owns the incoming opacity ramp rather than a shader.
#[derive(Debug,Clone,Copy,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum SourceLegacy{CrossFade,Push,Zoom,WipeLeft,WipeRight,FadeBlack,FadeWhite}

fn colour_target(legacy:Option<SourceLegacy>)->Option<bool>{match legacy{Some(SourceLegacy::FadeBlack)=>Some(false),Some(SourceLegacy::FadeWhite)=>Some(true),_=>None}}
fn colour_span(a:Window,b:Window,requested:f64,num:u64,den:u64)->Result<(i64,i64)>{
    let fps=num as f64/den as f64;let unit=frame_tick(den as f64/num as f64,num,den)? as i128;
    let out=((requested/2.).min(a.duration)*fps).round().max(1.)as i128;
    let incoming=((requested/2.).min(b.duration)*fps).round().max(1.)as i128;
    let total=(a.duration*fps).round()as i128;
    let start=frame_tick(a.start,num,den)? as i128+(total-out).max(0)*unit;
    let end=frame_tick(b.start,num,den)? as i128+incoming*unit;
    Ok((i64::try_from(start)?,i64::try_from(end-start)?))
}

/// Raw (unquantized) source overlap. Concat video is frame-quantized; sound is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceOverlap {
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub legacy:Option<SourceLegacy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored_duration_ticks: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hold_outgoing: bool,
    pub lead_ticks: u64,
    pub duration_ticks: u64,
}

/// Build source-owned expanded audio clips. Fades are maxima, never multiplied twice.
pub fn audio_clip(
    video: &super::composition::VideoClip,
    transitions: &[super::composition::ClipTransition],
    mut clip: super::composition::AudioClip,
) -> Result<super::composition::AudioClip> {
    if clip.placement.concat_retime.is_some(){
        ensure!(!transitions.iter().any(|t|t.concat_overlap.is_some_and(|overlap|colour_target(overlap.legacy).is_none())&&(t.to_clip_id.as_ref()==Some(&video.id)||t.from_clip_id.as_ref()==Some(&video.id))),"retimed source overlap sound is not migrated");
        // Delay rounding happens separately for each audio piece, after slicing.
        return Ok(clip);
    }

    let source_owned=transitions.iter().any(|t|t.concat_overlap.is_some()&&(t.to_clip_id.as_ref()==Some(&video.id)||t.from_clip_id.as_ref()==Some(&video.id)));
    let source_delay=|clip:&mut super::composition::AudioClip|{
        // Original Concat audio::filtergraph uses adelay in whole milliseconds.
        // Keep this derived sound clock separate from authored video placement.
        if source_owned{clip.placement.timeline_start_tick=(((clip.placement.timeline_start_tick as f64/1e6)*1000.).round().max(0.)as u64)*1000;}
    };
    if !transitions.iter().any(|t| {
        t.concat_overlap.is_some_and(|overlap|colour_target(overlap.legacy).is_none())
            && (t.to_clip_id.as_ref() == Some(&video.id)
                || t.from_clip_id.as_ref() == Some(&video.id))
    }) {
        source_delay(&mut clip);
        return Ok(clip);
    }
    let mut head = 0;
    let mut tail = 0;
    let mut key_duration = clip.placement.timeline_duration_ticks()? as f64 / 1e6;
    for transition in transitions {
        let Some(overlap) = transition.concat_overlap else {
            continue;
        };
        if colour_target(overlap.legacy).is_some(){continue;}
        ensure!(
            overlap.duration_ticks > 0 && overlap.lead_ticks <= overlap.duration_ticks,
            "Invalid Concat overlap"
        );
        if transition.to_clip_id.as_ref() == Some(&video.id) {
            extend_keys(
                &mut clip.concat_gain_keys,
                &mut key_duration,
                overlap.lead_ticks as f64 / 1e6,
                0.,
            );
            head = head.max(overlap.lead_ticks);
            clip.fade_in_ticks = clip.fade_in_ticks.max(overlap.duration_ticks);
        }
        if transition.from_clip_id.as_ref() == Some(&video.id) && !overlap.hold_outgoing {
            extend_keys(
                &mut clip.concat_gain_keys,
                &mut key_duration,
                0.,
                (overlap.duration_ticks - overlap.lead_ticks) as f64 / 1e6,
            );
            tail = tail.max(overlap.duration_ticks - overlap.lead_ticks);
            clip.fade_out_ticks = clip.fade_out_ticks.max(overlap.duration_ticks);
        }
    }
    ensure!(
        clip.placement.speed == 1. && clip.placement.speed_ramp.is_none() && !clip.reversed,
        "Concat overlap sound needs forward unit speed"
    );
    ensure!(
        matches!(
            clip.gain,
            super::composition::AnimatableValue::Constant { .. }
        ),
        "Concat overlap sound gain animation needs rewindowing"
    );
    clip.placement.timeline_start_tick = clip
        .placement
        .timeline_start_tick
        .checked_sub(head)
        .context("Concat sound timeline head underflow")?;
    clip.placement.source_in_tick = clip
        .placement
        .source_in_tick
        .checked_sub(head)
        .context("Concat sound source head underflow")?;
    clip.placement.source_out_tick = clip
        .placement
        .source_out_tick
        .checked_add(tail)
        .context("Concat sound source tail overflow")?;
    source_delay(&mut clip);
    Ok(clip)
}

/// Recompute source sequential windows before either preview or audio planning.
pub fn validate_audio_overlaps(
    clips: &[super::composition::VideoClip],
    transitions: &[super::composition::ClipTransition],
    sources: &BTreeMap<super::composition::SourceId, CompositionSource>,
    canvas: &super::composition::CanvasSpec,
) -> Result<()> {
    if !transitions
        .iter()
        .any(|transition| transition.concat_overlap.is_some())
    {
        return Ok(());
    }
    ensure!(
        transitions
            .iter()
            .all(|transition| transition.concat_overlap.is_some()),
        "Mixed source transition ownership"
    );
    let (num, den) = canvas
        .fps_rate
        .map(|rate| (rate.numerator as u64, rate.denominator as u64))
        .unwrap_or((canvas.fps_milli as u64, 1000));
    let frame = den as f64 / num as f64;
    let mut from_ids = std::collections::BTreeSet::new();
    let mut to_ids = std::collections::BTreeSet::new();
    let mut windows = BTreeMap::<super::composition::CompositionClipId, Window>::new();
    for transition in transitions {
        let overlap = transition.concat_overlap.unwrap();
        ensure!(overlap.legacy.is_none()||(transition.kind==super::composition::TransitionKind::ConcatDissolve&&transition.kernel.is_none()),"Modified legacy transition kernel");
        ensure!(
            !transition.reverse && transition.duration_ticks == 0,
            "Modified source transition direction"
        );
        let from = clips
            .iter()
            .find(|clip| Some(&clip.id) == transition.from_clip_id.as_ref())
            .context("Source outgoing clip missing")?;
        let to = clips
            .iter()
            .find(|clip| Some(&clip.id) == transition.to_clip_id.as_ref())
            .context("Source incoming clip missing")?;
        ensure!(
            from_ids.insert(from.id.clone()) && to_ids.insert(to.id.clone()),
            "Branching source transition ownership needs resolution"
        );
        ensure!(
            from.placement.timeline_end_tick()? == to.placement.timeline_start_tick,
            "Source overlap clips no longer touch"
        );
        for clip in [from, to] {
            validate_side_effects(&clip.effects)?;
            if matches!(overlap.legacy,Some(SourceLegacy::Push|SourceLegacy::Zoom)){
                ensure!(clip.transform.concat_animation.as_ref().is_some_and(|animation|animation.duration_ticks==clip.placement.timeline_duration_ticks().unwrap_or(0)as i64&&animation.rest.scale_x>0.),"Legacy movement needs its retained authored pose clock");
            }
            ensure!(
                (colour_target(overlap.legacy).is_some() || (clip.placement.speed == 1. && clip.placement.timeline_duration_tick.is_none()))
                    && clip.placement.speed_ramp.is_none()
                    && clip.placement.source_timeline_offset_tick.is_none()
                    && clip.placement.source_head_timeline_tick.is_none()
                    && clip.playback_mode == super::composition::PlaybackMode::Forward,
                "Modified source overlap playback clock"
            );
        }
        let base = |clip: &super::composition::VideoClip| -> Result<Window> {
            Ok(Window {
                start: clip.placement.timeline_start_tick as f64 / 1e6,
                duration: clip.placement.timeline_duration_ticks()? as f64 / 1e6,
                source_start: clip.placement.source_in_tick as f64 / 1e6,
            })
        };
        let mut from_window = windows.get(&from.id).copied().unwrap_or(base(from)?);
        let mut to_window = windows.get(&to.id).copied().unwrap_or(base(to)?);
        let requested = overlap
            .authored_duration_ticks
            .unwrap_or(overlap.duration_ticks);
        ensure!(
            requested > 0 && requested <= 9007199254740991,
            "Invalid source authored duration"
        );
        if colour_target(overlap.legacy).is_some(){
            ensure!(overlap.lead_ticks==0&&!overlap.hold_outgoing&&overlap.authored_duration_ticks.is_none(),"Modified colour fade overlap metadata");
            let expected=colour_span(from_window,to_window,requested as f64/1e6,num,den)?;
            ensure!(transition.timeline.context("Colour fade visual span missing")?.source_span().map_err(|e|anyhow::anyhow!("{e:?}"))?==expected,"Modified source colour fade span");continue;
        }
        let length = (requested as f64 / 1e6)
            .min(from_window.duration)
            .min(to_window.duration);
        ensure!(
            overlap.duration_ticks == (length * 1e6).round() as u64 && overlap.duration_ticks > 0,
            "Source overlap duration differs from sequential clip windows"
        );
        let mut lead = (length / 2.).min(to_window.source_start);
        if lead < frame / 2. {
            lead = 0.;
        }
        ensure!(
            overlap.lead_ticks == (lead * 1e6).round() as u64,
            "Modified source audio lead"
        );
        let expected = (
            frame_tick(to_window.start - lead, num, den)?,
            frame_tick(length, num, den)?,
        );
        ensure!(
            transition
                .timeline
                .context("Source visual window missing")?
                .source_span()
                .map_err(|e| anyhow::anyhow!("{e:?}"))?
                == expected,
            "Source video/audio windows disagree"
        );
        let handle = ((sources[&from.source_id].duration_ticks as f64 / 1e6
            - from_window.source_start)
            - from_window.duration
            - frame)
            .max(0.);
        ensure!(
            overlap.hold_outgoing == (handle + frame / 2. < length - lead),
            "Stale source outgoing hold ownership"
        );
        // Original Concat selects the first covering span in document order.
        // The shared source lane plan supplies the other live clips (including
        // their fallback ramps), so shader windows may overlap.
        to_window.extend(lead, 0.);
        if !overlap.hold_outgoing {
            from_window.extend(0., length - lead);
        }
        windows.insert(from.id.clone(), from_window);
        windows.insert(to.id.clone(), to_window);
    }
    Ok(())
}

/// Original held copy reads the last complete picture, with no sound of its own.
pub fn outgoing_time(
    composition: &super::composition::Composition,
    transition: &super::composition::ClipTransition,
    clips: &[super::composition::VideoClip],
    exact: i128,
) -> Result<i128> {
    if !transition
        .concat_overlap
        .is_some_and(|overlap| overlap.hold_outgoing)
    {
        return Ok(exact);
    }
    let clip = clips
        .iter()
        .find(|clip| Some(&clip.id) == transition.from_clip_id.as_ref())
        .context("Held outgoing clip missing")?;
    let cut = clip.placement.timeline_end_tick()? as i128 * 254016;
    if exact < cut {
        return Ok(exact);
    }
    let (num, den) = composition
        .canvas
        .fps_rate
        .map(|r| (r.numerator as u64, r.denominator as u64))
        .unwrap_or((composition.canvas.fps_milli as u64, 1000));
    Ok(cut - frame_tick(den as f64 / num as f64, num, den)? as i128)
}

fn extend_keys(
    keys: &mut [super::concat_effect_upgrade::ParamKey],
    duration: &mut f64,
    head: f64,
    tail: f64,
) {
    if head <= 0. && tail <= 0. {
        return;
    }
    let next = *duration + head + tail;
    for key in keys {
        key.at = ((key.at * *duration + head) / next).clamp(0., 1.);
    }
    *duration = next;
}
/// Source pose on the final frame grid. Authored keys remain on the original clip.
pub fn source_pose(
    clip: &super::composition::VideoClip,
    clips: &[super::composition::VideoClip],
    transitions: &[super::composition::ClipTransition],
    canvas: &super::composition::CanvasSpec,
    exact: i128,
    held_tail: bool,
) -> Result<Option<super::concat_clip_animation::Pose>> {
    let Some(animation) = &clip.transform.concat_animation else {
        return Ok(None);
    };
    if clip.placement.concat_retime.is_none() && !transitions.iter().any(|t| {
        t.concat_overlap.is_some()
            && (t.from_clip_id.as_ref() == Some(&clip.id)
                || t.to_clip_id.as_ref() == Some(&clip.id))
    }) {
        return Ok(None);
    }
    let state=resolved_visual_state(clips,transitions,canvas)?;
    let index=clips.iter().position(|candidate|candidate.id==clip.id).context("Source pose clip missing")?;
    if held_tail{return Ok(state.holds.iter().find(|hold|hold.index==index).and_then(|hold|hold.pose));}
    let window=state.windows[index];
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let duration=frame_tick(window.duration,num,den)?;
    if duration==0{return Ok(None);}
    let progress=(exact-frame_tick(window.start,num,den)? as i128)as f64/duration as f64;
    Ok(Some(super::concat_clip_animation::ClipAnimation::new(&state.keys[index])?.sample_progress(progress,animation.rest,animation.stretch,animation.canvas)?))
}

/// Source filters are one-frame Concat packages; native temporal/environment effects
/// retain their separate independent-transition admission gate.
pub fn validate_side_effects(effects: &[super::composition::VideoEffect]) -> Result<()> {
    for effect in effects {
        if matches!(
            effect,
            super::composition::VideoEffect::ConcatBypass { .. }
                | super::composition::VideoEffect::NativeBypass { .. }
        ) {
            continue;
        }
        let wire = serde_json::to_value(effect)?;
        ensure!(
            wire["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("concat_")),
            "Non-Concat transition side effects require their native clock qualification"
        );
    }
    Ok(())
}
#[derive(Debug, Clone, Copy)]
pub struct EffectClock {
    pub start_tick: i64,
    pub duration_tick: i64,
    pub timeline_tick: i64,
}
impl EffectClock {
    pub fn progress(self) -> f64 {
        ((self.timeline_tick as i128 - self.start_tick as i128) as f64 / self.duration_tick as f64)
            .clamp(0., 1.)
    }
    pub fn local_tick(self) -> i64 {
        self.timeline_tick - self.start_tick
    }
}
/// Filter keys keep their normalized source positions. The clip's expanded,
/// quantized window changes; held copies have their own fresh filter interval.
pub fn effect_clock(
    clip: &super::composition::VideoClip,
    clips: &[super::composition::VideoClip],
    transitions: &[super::composition::ClipTransition],
    canvas: &super::composition::CanvasSpec,
    exact: i128,
    held_tail: bool,
) -> Result<Option<EffectClock>> {
    if clip.placement.concat_retime.is_none() && !transitions.iter().any(|t| {
        t.concat_overlap.is_some()
            && (t.from_clip_id.as_ref() == Some(&clip.id)
                || t.to_clip_id.as_ref() == Some(&clip.id))
    }) {
        return Ok(None);
    }
    let (_,_,window,_,_)=visual_windows(clips,transitions,canvas)?.into_iter().find(|(index,_,_,held,_)|clips[*index].id==clip.id&&*held==held_tail).context("Source effect window missing")?;
    let start=window.start;let duration=window.duration;
    let (num, den) = canvas
        .fps_rate
        .map(|r| (r.numerator as u64, r.denominator as u64))
        .unwrap_or((canvas.fps_milli as u64, 1000));
    let duration_tick=frame_tick(duration,num,den)?;
    if duration_tick==0{return Ok(None);}
    Ok(Some(EffectClock {
        start_tick: frame_tick(start, num, den)?,
        duration_tick,
        timeline_tick: i64::try_from(exact)?,
    }))
}

/// One source-engine lane at an exact timeline instant. Held copies are appended
/// after original clips, and the original engine chooses the last active clip
/// on each lane (it does not composite overlapping clips within one lane).
#[derive(Debug, Clone)]
pub struct VisualLayer {
    pub clip_index: usize,
    pub lane: usize,
    pub held: bool,
    pub source_tick: i128,
    pub ramp: f32,
}
pub fn visual_layers(
    clips: &[super::composition::VideoClip],
    transitions: &[super::composition::ClipTransition],
    canvas: &super::composition::CanvasSpec,
    exact: i128,
) -> Result<Option<Vec<VisualLayer>>> {
    if !transitions.iter().any(|t| t.concat_overlap.is_some()) && !clips.iter().any(|clip|clip.placement.concat_retime.is_some()) { return Ok(None); }
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let mut visible=BTreeMap::new();
    for (index,lane,window,held,ramp) in visual_windows(clips,transitions,canvas)? {
        if !clips[index].enabled { continue; }
        let start=frame_tick(window.start,num,den)? as i128;
        let duration=frame_tick(window.duration,num,den)? as i128;
        if exact<start || exact>=start+duration { continue; }
        let source=if clips[index].placement.concat_retime.is_none() && transitions.iter().any(|t|t.from_clip_id.as_ref()==Some(&clips[index].id)||t.to_clip_id.as_ref()==Some(&clips[index].id)) {
            // Concat Rational::approximate retains six decimal places for speed.
            let rate=(clips[index].placement.speed*1_000_000.).round() as i128;
            frame_tick(window.source_start,num,den)? as i128+if held{0}else{(exact-start)*rate/1_000_000}
        }else{let Some(source)=super::video_clock::transition_source_sample_tick(&clips[index].placement,clips[index].playback_mode,exact)?else{continue;};source as i128};
        let ramp_duration=frame_tick(ramp,num,den)?;
        visible.insert(lane,VisualLayer{clip_index:index,lane,held,source_tick:source,ramp:if ramp_duration>0{((exact-start)as f64/ramp_duration as f64).clamp(0.,1.)as f32}else{1.}});
    }
    Ok(Some(visible.into_values().collect()))
}

struct ColourShape{white:bool,head:bool,start:i64,frames:i64}
type VisualKeys=BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>;
struct HeldVisual{index:usize,lane:usize,window:Window,pose:Option<super::concat_clip_animation::Pose>}
struct VisualState{windows:Vec<Window>,keys:Vec<VisualKeys>,lanes:Vec<usize>,ramps:Vec<f64>,holds:Vec<HeldVisual>,wipes:Vec<Option<(i64,bool)>>,fades:Vec<Vec<ColourShape>>}
fn append_ride(keys:&mut VisualKeys,clip:&super::composition::VideoClip,duration:f64,property:&str,from:f64,to:f64,seconds:f64,head:bool,ease:[f64;4])->Result<()>{
    let animation=clip.transform.concat_animation.as_ref().context("Legacy movement needs retained source pose")?;
    let absolute=|value:f64|if property=="scale"{animation.rest.scale_x*value}else{animation.rest.x+value};
    let fraction=(seconds/duration).clamp(0.,1.);let(start,end)=if head{(0.,fraction)}else{(1.-fraction,1.)};
    keys.insert(property.into(),vec![super::concat_effect_upgrade::ParamKey{at:start,value:absolute(from),ease:[0.,0.,1.,1.],derived:true},super::concat_effect_upgrade::ParamKey{at:end,value:absolute(to),ease,derived:true}]);Ok(())
}
fn extend_visual(keys:&mut VisualKeys,window:&mut Window,head:f64,tail:f64){
    let old=window.duration;for run in keys.values_mut(){let mut duration=old;extend_keys(run,&mut duration,head,tail);}window.extend(head,tail);
}
fn resolved_visual_state(clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec)->Result<VisualState>{
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));let frame=den as f64/num as f64;
    let mut windows=clips.iter().map(|clip|clip_window(clip,canvas)).collect::<Result<Vec<_>>>()?;
    let mut keys=clips.iter().map(|clip|clip.transform.concat_animation.as_ref().map(|animation|animation.keys.clone()).unwrap_or_default()).collect::<Vec<_>>();
    let mut generated=vec![std::collections::BTreeSet::<String>::new();clips.len()];
    let mut lanes=vec![0;clips.len()];let mut ramps=vec![0.;clips.len()];let mut holds=vec![];let mut wipes=vec![None;clips.len()];let mut fades=(0..clips.len()).map(|_|vec![]).collect::<Vec<Vec<ColourShape>>>();
    for(step,transition)in raw_overlaps(clips,transitions,canvas)?.into_iter().zip(transitions){
        let(from,to,lead,length)=(step.from,step.to,step.lead,step.length);let tail=length-lead;
        if let Some(white)=step.colour{
            let fps=num as f64/den as f64;let out=((length/2.).min(step.from_window.duration)*fps).round().max(1.)as i64;
            let incoming=((length/2.).min(step.to_window.duration)*fps).round().max(1.)as i64;
            fades[from].push(ColourShape{white,head:false,start:((step.from_window.duration*fps).round()as i64-out).max(0),frames:out});
            fades[to].push(ColourShape{white,head:true,start:0,frames:incoming});continue;
        }
        extend_visual(&mut keys[to],&mut windows[to],lead,0.);lanes[to]=lanes[from]+1;
        if step.held{
            let pose=if let Some(animation)=&clips[from].transform.concat_animation{
                let mut pose=super::concat_clip_animation::ClipAnimation::new(&keys[from])?.sample_progress(1.,animation.rest,animation.stretch,animation.canvas)?;
                // Original last_frame_held copies the generated key's relative
                // tail into the constant, rather than retaining the old base.
                if generated[from].contains("scale"){pose.scale_x/=animation.rest.scale_x;pose.scale_y/=animation.rest.scale_x;}
                if generated[from].contains("offsetX"){pose.x-=animation.rest.x*animation.canvas[0];}
                Some(pose)
            }else{None};
            holds.push(HeldVisual{index:from,lane:lanes[from],window:Window{start:step.from_window.start+step.from_window.duration,duration:tail,source_start:step.from_window.source_start+(step.from_window.duration-frame).max(0.)},pose});
        }else{extend_visual(&mut keys[from],&mut windows[from],0.,tail);}
        let legacy=transition.concat_overlap.and_then(|overlap|overlap.legacy);
        let property=match legacy{Some(SourceLegacy::Push)=>Some("offsetX"),Some(SourceLegacy::Zoom)=>Some("scale"),_=>None};
        let shaped=property.is_some_and(|property|!step.held&&!keys[from].get(property).is_some_and(|run|!run.is_empty())&&!keys[to].get(property).is_some_and(|run|!run.is_empty()));
        if shaped{
            let property=property.unwrap();
            let(a,b,ease_in,ease_out)=if legacy==Some(SourceLegacy::Push){((1.,0.),(0.,-1.),[0.42,0.,0.58,1.],[0.42,0.,0.58,1.])}else{((1.25,1.),(1.,1.4),[0.,0.,0.58,1.],[0.42,0.,1.,1.])};
            append_ride(&mut keys[to],&clips[to],windows[to].duration,property,a.0,a.1,length,true,ease_in)?;
            append_ride(&mut keys[from],&clips[from],windows[from].duration,property,b.0,b.1,length,false,ease_out)?;
            generated[from].insert(property.into());generated[to].insert(property.into());
        }
        let wipe=matches!(legacy,Some(SourceLegacy::WipeLeft|SourceLegacy::WipeRight));
        if wipe{wipes[to]=Some(((length*(num as f64/den as f64)).round().max(1.)as i64,legacy==Some(SourceLegacy::WipeLeft)));}
        ramps[to]=if wipe||(shaped&&legacy==Some(SourceLegacy::Push)){0.}else{length};
    }
    Ok(VisualState{windows,keys,lanes,ramps,holds,wipes,fades})
}
fn visual_windows(clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec)->Result<Vec<(usize,usize,Window,bool,f64)>>{
    let state=resolved_visual_state(clips,transitions,canvas)?;
    let mut order=(0..clips.len()).collect::<Vec<_>>();
    order.sort_by_key(|&i|transitions.iter().position(|t|t.to_clip_id.as_ref()==Some(&clips[i].id)).map_or(0,|i|i+1));
    Ok(order.into_iter().map(|i|(i,state.lanes[i],state.windows[i],false,state.ramps[i])).chain(state.holds.into_iter().map(|hold|(hold.index,hold.lane,hold.window,true,0.))).collect())
}
/// Conservative authorization over the source engine's expanded/held lifetimes.
pub fn visual_range_clip_indices(clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec,start:i128,end:i128)->Result<Option<std::collections::BTreeSet<usize>>>{
    if !transitions.iter().any(|t|t.concat_overlap.is_some()) && !clips.iter().any(|clip|clip.placement.concat_retime.is_some()){return Ok(None);}
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let mut result=std::collections::BTreeSet::new();
    for(index,_,window,_,_)in visual_windows(clips,transitions,canvas)?{
        if !clips[index].enabled{continue;}
        let first=frame_tick(window.start,num,den)? as i128;
        let duration=frame_tick(window.duration,num,den)? as i128;
        if duration==0{continue;}
        let finish=first+duration;
        if start<finish&&first<end{result.insert(index);}
    }
    Ok(Some(result))
}

/// Source lane assigned to the incoming clip when this transition was resolved.
/// Later transitions may move that clip again; the span retains this original lane.
pub fn transition_lane(transitions:&[super::composition::ClipTransition],target:&super::composition::ClipTransition)->Result<usize>{
    let mut lanes=BTreeMap::new();
    for transition in transitions {
        let from=transition.from_clip_id.as_ref().context("Source outgoing clip missing")?;
        let to=transition.to_clip_id.as_ref().context("Source incoming clip missing")?;
        if transition.concat_overlap.is_some_and(|overlap|colour_target(overlap.legacy).is_some()){
            if transition.id==target.id{return Ok(lanes.get(to).copied().unwrap_or(0));}continue;
        }
        let lane=lanes.get(from).copied().unwrap_or(0)+1;
        lanes.insert(to.clone(),lane);
        if transition.id==target.id{return Ok(lane);}
    }
    anyhow::bail!("Source transition missing")
}

/// Original per-picture wipe shading, independent of canvas placement and flips.
#[derive(Debug,Clone,Copy)]
pub struct LegacyWipe{pub uncovered:f32,pub from_right:bool}
impl LegacyWipe{
    pub fn keeps(self,u:f32)->bool{
        u<if self.from_right{2.}else{self.uncovered}&&u>=if self.from_right{1.-self.uncovered}else{-1.}
    }
}
pub fn source_wipe(clip:&super::composition::VideoClip,clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec,exact:i128,held:bool)->Result<Option<LegacyWipe>>{
    if held||!transitions.iter().any(|t|t.to_clip_id.as_ref()==Some(&clip.id)&&t.concat_overlap.is_some_and(|o|matches!(o.legacy,Some(SourceLegacy::WipeLeft|SourceLegacy::WipeRight)))){return Ok(None);}
    let state=resolved_visual_state(clips,transitions,canvas)?;
    let index=clips.iter().position(|candidate|candidate.id==clip.id).context("Source wipe endpoint missing")?;
    let Some((frames,from_right))=state.wipes[index]else{return Ok(None)};
    let(num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let start=frame_tick(state.windows[index].start,num,den)? as i128;
    let duration=frame_tick(state.windows[index].duration,num,den)? as i128;
    if exact<start||exact>=start+duration{return Ok(None);}
    let frame=((exact-start)as f64/254016000000.*(num as f64/den as f64)).round()as i64;
    if frame>=frames{return Ok(None);}
    Ok(Some(LegacyWipe{uncovered:((frame.max(0)+1)as f32/frames as f32).min(1.),from_right}))
}

/// Original linear-light colour shading, folded in source transition order.
#[derive(Debug,Clone,Copy)]pub struct LegacyColour{pub scale:f32,pub offset:f32}
pub fn source_colour(clip:&super::composition::VideoClip,clips:&[super::composition::VideoClip],transitions:&[super::composition::ClipTransition],canvas:&super::composition::CanvasSpec,exact:i128,held:bool)->Result<Option<LegacyColour>>{
    if held||!transitions.iter().any(|t|t.concat_overlap.is_some_and(|o|colour_target(o.legacy).is_some())&&(t.from_clip_id.as_ref()==Some(&clip.id)||t.to_clip_id.as_ref()==Some(&clip.id))){return Ok(None);}
    let state=resolved_visual_state(clips,transitions,canvas)?;let index=clips.iter().position(|c|c.id==clip.id).context("Colour fade clip missing")?;
    let(num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let start=frame_tick(state.windows[index].start,num,den)? as i128;let duration=frame_tick(state.windows[index].duration,num,den)? as i128;
    if exact<start||exact>=start+duration{return Ok(None);}
    let frame=((exact-start)as f64/254016000000.*(num as f64/den as f64)).round()as i64;
    let mut shade=LegacyColour{scale:1.,offset:0.};
    for shape in &state.fades[index]{let amount=if shape.head&&frame<shape.frames{1.-frame.max(0)as f32/shape.frames as f32}else if !shape.head&&frame>=shape.start{((frame-shape.start)as f32/shape.frames as f32).min(1.)}else{continue;};let amount=amount.clamp(0.,1.);shade.scale*=1.-amount;shade.offset=shade.offset*(1.-amount)+if shape.white{amount}else{0.};}
    Ok(if shade.scale==1.&&shade.offset==0.{None}else{Some(shade)})
}
