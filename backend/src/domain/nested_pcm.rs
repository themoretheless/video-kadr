//! Bounded nested stereo mixer over trusted, conformed PCM supplied by the caller.
use super::{
    audio_pan::equal_power_pan,
    composition::{AnimatableValue, AudioVoiceEffect, SourceId},
    nested_audio::NestedAudioLayer,
    nested_composition::sample_value,
};
use anyhow::{ensure, Result};
use std::sync::atomic::{AtomicBool, Ordering};

/// PCM callback is responsible for source decoding/resampling and channel layout.
/// This core supports authored audio tracks without tempo-preserving or voice DSP;
/// unsupported processing is refused, never silently omitted.
pub fn mix_block<F>(
    layers: &[NestedAudioLayer],
    start_sample: u64,
    frames: usize,
    sample_rate: u32,
    cancel: &AtomicBool,
    mut read: F,
) -> Result<Vec<[f64; 2]>>
where
    F: FnMut(&SourceId, u64) -> Result<(u32, [f64; 2])>,
{
    ensure!(!cancel.load(Ordering::Relaxed), "nested PCM cancelled");
    ensure!(
        (8000..=192000).contains(&sample_rate)
            && frames <= 65536
            && layers.len() <= 4096
            && layers.len().saturating_mul(frames) <= 8_388_608,
        "invalid nested PCM block budget"
    );
    let end = start_sample
        .checked_add(frames as u64)
        .ok_or_else(|| anyhow::anyhow!("nested PCM sample clock overflow"))?;
    ensure!(
        u128::from(end) * 1_000_000 / u128::from(sample_rate) <= 9_007_199_254_740_991,
        "nested PCM clock exceeds exact range"
    );
    validate_processing(layers)?;
    let mut output = vec![[0.0; 2]; frames];
    for (index, out) in output.iter_mut().enumerate() {
        ensure!(!cancel.load(Ordering::Relaxed), "nested PCM cancelled");
        let time_us =
            (u128::from(start_sample + index as u64) * 1_000_000 / u128::from(sample_rate)) as u64;
        let mut contributed=false;
        for layer in layers {
            ensure!(!cancel.load(Ordering::Relaxed), "nested PCM cancelled");
            let Some(sample) = layer.sample(time_us)? else {
                continue;
            };
            if sample.gain == 0.0 {
                continue;
            }
            let (channels, input) = read(layer.source_id(), sample.source_time_us)?;
            ensure!(
                matches!(channels, 1 | 2) && input.iter().all(|value| value.is_finite()),
                "invalid nested PCM source frame"
            );
            let matrix = pan_matrix(&layer.clip.pan, channels, sample.clip_local_us)?;
            let [left, right] = if channels == 1 {
                [input[0], 0.0]
            } else {
                input
            };
            // Identity stereo routing must retain the sign of digital zero.
            // Adding a synthetic +0 accumulator/crossfeed would erase -0 PCM.
            let routed=if channels==2&&matrix==[1.,0.,0.,1.]{[left,right]}else{[matrix[0]*left+matrix[1]*right,matrix[2]*left+matrix[3]*right]};
            let contribution=[sample.gain*routed[0],sample.gain*routed[1]];
            if contributed{out[0]+=contribution[0];out[1]+=contribution[1];}else{*out=contribution;contributed=true;}
            ensure!(
                out.iter().all(|value| value.is_finite()),
                "nested PCM mix overflow"
            );
        }
    }
    Ok(output)
}
/// Admit processing that the PCM core implements without changing authored sound.
pub fn validate_processing(layers: &[NestedAudioLayer]) -> Result<()> {
    for layer in layers {
        ensure!(layer.concat_retime_piece.is_none()&&layer.concat_retime_owner.is_none(),"source curve audio piece needs whole-frame preparation before PCM mixing");
        let clip = &layer.clip;
        ensure!(
            clip.dsp_effects.iter().all(|effect| effect.bypass)
                && clip.voice_effect == AudioVoiceEffect::None
                && clip.pitch_semitones == 0.0
                && clip.tone_db == 0.0
                && clip.ducking.is_none()
                && clip.crossfade_in_ticks == layer.crossfade.fade_in_ticks
                && ((layer.crossfade.head_ticks == 0 && layer.crossfade.tail_ticks == 0) || !clip.reversed)
                && clip.placement.speed == 1.0
                && clip.placement.speed_ramp.is_none(),
            "nested PCM processing requires DSP/conform stage"
        );
    }
    Ok(())
}
/// Match WeftCut's ten-millisecond coefficient grid, rather than interpolating pan values.
pub(super) fn pan_matrix(pan: &AnimatableValue, channels: u32, local_us: u64) -> Result<[f64; 4]> {
    if let AnimatableValue::Keyframes { track } = pan {
        if let Some(clock) = track.sample_clock {
            clock.prepare(track)?;
            let (num, den, offset) = clock.parts();
            let position = i128::from(local_us) * i128::from(num) + i128::from(offset);
            let step = i128::from(den) * 10000;
            let cell = position.div_euclid(step);
            let grid = cell * 10000;
            let at = |us: i128| -> Result<[f64; 4]> {
                // Preserve the legacy unclocked author-grid rounding at coefficient nodes.
                let tick = (us as f64 / 1e6 * track.time_base as f64).round();
                ensure!(tick.is_finite() && tick.abs() <= 9_007_199_254_740_991., "pan coefficient clock overflow");
                let value = track.sample_tick(tick as i64);
                ensure!(value.is_finite() && (-1.0..=1.0).contains(&value), "invalid nested PCM pan sample");
                Ok(equal_power_pan(value, channels))
            };
            let a = at(grid)?;
            let b = at(grid + 10000)?;
            let fraction = (position - cell * step) as f64 / step as f64;
            return Ok(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * fraction));
        }
    }
    let checked = |time| -> Result<[f64; 4]> {
        let value = sample_value(pan, time)?;
        ensure!(
            value.is_finite() && (-1.0..=1.0).contains(&value),
            "invalid nested PCM pan sample"
        );
        Ok(equal_power_pan(value, channels))
    };
    if matches!(pan, AnimatableValue::Constant { .. }) {
        return checked(local_us);
    }
    let grid = local_us / 10000 * 10000;
    let a = checked(grid)?;
    let b = checked(grid + 10000)?;
    let u = (local_us - grid) as f64 / 10000.0;
    Ok(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * u))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layer() -> NestedAudioLayer {
        let clip = serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":100000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0})).unwrap();
        NestedAudioLayer {
            video_transitions: None, concat_retime_piece:None, concat_retime_owner:None,export_fade_clock:false,gain_track:None,
            composition: "child".into(),
            instance_path: vec!["group".into()],
            root_range_us: 0..100000,
            clip_local_start_us: 0,
            crossfade: Default::default(),
            clip,
        }
    }
    #[test]
    fn mono_fanout_and_stereo_crossfeed_sum_without_normalizing_or_clipping() {
        let layer = layer();
        let cancel = AtomicBool::new(false);
        let mono = mix_block(&[layer.clone()], 0, 2, 48000, &cancel, |_, _| {
            Ok((1, [1.0, 0.0]))
        })
        .unwrap();
        let center = std::f64::consts::FRAC_1_SQRT_2;
        assert!((mono[0][0] - center).abs() < 1e-12);
        assert!((mono[0][1] - center).abs() < 1e-12);
        let mut left = layer.clone();
        left.clip.pan = AnimatableValue::constant(-1.0);
        let stereo = mix_block(&[left.clone(), left], 0, 1, 48000, &cancel, |_, _| {
            Ok((2, [0.75, 0.5]))
        })
        .unwrap();
        assert_eq!(stereo, [[2.5, 0.0]]);
    }
    #[test]
    fn animated_pan_lerps_coefficients_and_preserves_unclipped_envelope_time() {
        let mut layer = layer();
        layer.clip_local_start_us = 15000;
        layer.root_range_us = 0..50000;
        layer.clip.pan=serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-1.0},{"tick":20000,"value":1.0}]}})).unwrap();
        let cancel = AtomicBool::new(false);
        let mut times = vec![];
        let block = mix_block(&[layer], 0, 1, 48000, &cancel, |_, time| {
            times.push(time);
            Ok((1, [1.0, 0.0]))
        })
        .unwrap();
        let center = std::f64::consts::FRAC_1_SQRT_2;
        assert_eq!(times, [15000]);
        assert!((block[0][0] - center * 0.5).abs() < 1e-12);
        assert!((block[0][1] - (center + 1.0) * 0.5).abs() < 1e-12);
    }
    #[test]
    fn reverse_boundaries_cancel_bad_pcm_and_unsupported_dsp_are_explicit() {
        let mut layer = layer();
        layer.clip.reversed = true;
        let cancel = AtomicBool::new(false);
        let mut times = vec![];
        mix_block(&[layer.clone()], 0, 2, 48000, &cancel, |_, time| {
            times.push(time);
            Ok((2, [0.0; 2]))
        })
        .unwrap();
        assert_eq!(times, [99999, 99979]);
        assert!(
            mix_block(&[layer.clone()], 0, 1, 48000, &cancel, |_, _| Ok((
                2,
                [f64::NAN, 0.0]
            )))
            .is_err()
        );
        assert!(
            mix_block(&[layer.clone()], 0, 1, 48000, &cancel, |_, _| Ok((
                6, [0.0; 2]
            )))
            .is_err()
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(
            mix_block(&[layer.clone()], 0, 1, 48000, &cancel, |_, _| panic!(
                "cancelled before read"
            ))
            .is_err()
        );
        cancel.store(false, Ordering::Relaxed);
        layer.clip.voice_effect = AudioVoiceEffect::Echo;
        assert!(mix_block(&[layer], 0, 1, 48000, &cancel, |_, _| panic!(
            "unsupported DSP before read"
        ))
        .is_err());
        assert!(mix_block(&[], u64::MAX, 1, 48000, &cancel, |_, _| unreachable!()).is_err());
    }
}

/// Mix each explicit video-audio track independently before summing other tracks.
/// Original transition windows replace track samples, never unrelated audio layers.
pub fn mix_block_with_video_transitions<F>(
 layers:&[NestedAudioLayer],start_sample:u64,frames:usize,sample_rate:u32,cancel:&AtomicBool,mut read:F,
)->Result<Vec<[f64;2]>>where F:FnMut(&SourceId,u64)->Result<(u32,[f64;2])>{
 use std::collections::BTreeMap;
 let mut groups=BTreeMap::<&str,Vec<&NestedAudioLayer>>::new();
 let mut ordinary=vec![];
 for layer in layers{
  if let Some(metadata)=&layer.video_transitions{groups.entry(&metadata.group).or_default().push(layer);}else{ordinary.push(layer.clone());}
 }
 let mut result=mix_block(&ordinary,start_sample,frames,sample_rate,cancel,&mut read)?;
 for group in groups.values(){
  let metadata=group[0].video_transitions.as_ref().expect("video audio group");
  let ordinary:Vec<_>=group.iter().filter(|layer|layer.video_transitions.as_ref().expect("group metadata").ordinary_enabled).map(|layer|(**layer).clone()).collect();
  let track=mix_block(&ordinary,start_sample,frames,sample_rate,cancel,&mut read)?;
  let mut planar=vec![track.iter().map(|frame|frame[0]as f32).collect(),track.iter().map(|frame|frame[1]as f32).collect()];
  let mut transitions=metadata.transitions.clone();
  for transition in &mut transitions{
   let timeline=transition.timeline.as_mut().ok_or_else(||anyhow::anyhow!("legacy video audio window not migrated"))?;
   let start=timeline.start_tick.exact_ticks().checked_add(metadata.root_offset_us.checked_mul(254016).ok_or_else(||anyhow::anyhow!("audio transition offset overflow"))?).ok_or_else(||anyhow::anyhow!("audio transition clock overflow"))?;
   timeline.start_tick=super::caption_time::CaptionDelta::SourceTicks(i64::try_from(start)?);
  }
  super::transition_audio::replace_transition_blocks(&mut planar,i64::try_from(start_sample)?,sample_rate,&transitions,&mut|id,first,n|{
   let Some(layer)=group.iter().find(|layer|&layer.clip.id==id)else{return Ok(vec![vec![0.;n];2])};
   let pcm=mix_block(&[(**layer).clone()],u64::try_from(first)?,n,sample_rate,cancel,&mut read)?;
   Ok(vec![pcm.iter().map(|frame|frame[0]as f32).collect(),pcm.iter().map(|frame|frame[1]as f32).collect()])
  },&mut||{ensure!(!cancel.load(Ordering::Relaxed),"video transition audio cancelled");Ok(())})?;
  for i in 0..frames{for channel in 0..2{result[i][channel]+=f64::from(planar[channel][i]);ensure!(result[i][channel].is_finite(),"video transition audio sum overflow");}}
 }
 Ok(result)
}

#[cfg(test)]mod video_transition_bridge_tests{
 use super::*;
 use crate::domain::{composition::{Composition,TransformSpec},nested_composition::{NestedCompositionDocument,NestedCompositionProject},nested_audio::plan_audio_tracks};
 use std::collections::BTreeMap;
 #[test]fn video_transition_audio_keeps_other_tracks_and_disabled_side_window(){
  for disabled in [false,true]{
   let mut c=Composition::new(Default::default());
   c.sources=serde_json::from_value(serde_json::json!({"a":{"id":"a","kind":"video","durationTicks":2000000,"width":4,"height":4,"hasAudio":true,"audioChannels":2},"b":{"id":"b","kind":"video","durationTicks":2000000,"width":4,"height":4,"hasAudio":true,"audioChannels":2},"other":{"id":"other","kind":"audio","durationTicks":2000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}})).unwrap();
   c.tracks=serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","transitions":[{"id":"fade","fromClipId":"a","toClipId":"b","timeline":{"startTick":"127008000000","durationTick":"254016000000"},"kind":"dissolve"}],"clips":[{"id":"a","sourceId":"a","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"blendMode":"normal","opacity":{"mode":"constant","value":1.0},"enabled":!disabled},{"id":"b","sourceId":"b","placement":{"timelineStartTick":1000000,"sourceInTick":0,"sourceOutTick":1000000,"speed":1.0},"transform":TransformSpec::default(),"blendMode":"normal","opacity":{"mode":"constant","value":1.0}}]},{"kind":"audio","id":"audio","name":"Other","clips":[{"id":"ordinary","sourceId":"other","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":2000000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0}]}])).unwrap();
   let project=NestedCompositionProject{fonts:vec![],timeline_order: vec![],schema_version:1,root:"root".into(),luts:vec![],compositions:BTreeMap::from([("root".into(),NestedCompositionDocument{duration_us:None,content:c,instances:vec![]})])};
   let layers=plan_audio_tracks(&project,"root",0..2000000).unwrap();
   assert_eq!(layers.len(),3);assert_eq!(layers.iter().filter(|layer|layer.video_transitions.is_some()).count(),2);
   let cancel=AtomicBool::new(false);
   for start in [0,24000,48000,72000]{
    let actual=mix_block_with_video_transitions(&layers,start,16,48000,&cancel,|id,_|Ok((2,match id.as_str(){"a"=>[0.25,0.5],"b"=>[-0.25,0.25],_=>[0.125,0.125]}))).unwrap();
    for(i,actual)in actual.iter().enumerate(){
     let sample=start+i as u64;let mut expected=[0.125f64;2];
     if (24000..72000).contains(&sample){
      let(ga,gb)=crate::domain::audio_transition::AudioTransitionCurve::ConstantPower.gains((sample-24000)as f32/48000.).unwrap();
      for channel in 0..2{let a=if sample<48000{[0.25f32,0.5][channel]}else{0.};let b=if sample>=48000{[-0.25f32,0.25][channel]}else{0.};expected[channel]+=f64::from(a*ga+b*gb);}
     }else if sample<48000&&!disabled{expected[0]+=0.25;expected[1]+=0.5;}else if sample>=48000{expected[0]-=0.25;expected[1]+=0.25;}
     for channel in 0..2{assert!((actual[channel]-expected[channel]).abs()<1e-7,"disabled={disabled} sample={sample}: {actual:?} != {expected:?}");}
    }
   }
  }
 }
}
