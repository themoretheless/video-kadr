//! Stateful clip DSP before host gain/pan. Caller supplies the full ancestor-clipped layer
//! (not the selection-clipped export layer) so seek preroll can read preceding audio.
use super::{composition::SourceId, filmcraft_dsp::TimelineChain, nested_audio::NestedAudioLayer};
use anyhow::{Context, Result, ensure};
pub struct ClipProcessor {
    layer: NestedAudioLayer,
    dsp: TimelineChain,
    rate: u32,
    channels: usize,
}
impl ClipProcessor {
    pub fn new(
        layer: NestedAudioLayer,
        rate: u32,
        channels: usize,
        preroll: usize,
    ) -> Result<Self> {
        ensure!(
            matches!(channels, 1 | 2),
            "clip DSP supports conformed mono/stereo"
        );
        let origin=i64::try_from((u128::from(layer.root_range_us.start)*u128::from(rate)).div_ceil(1_000_000))?;
        let end=i64::try_from((u128::from(layer.root_range_us.end)*u128::from(rate)).div_ceil(1_000_000))?;
        let dsp = TimelineChain::new(rate, channels, &layer.clip.dsp_effects, preroll)?.with_input_bounds(origin,end)?;
        Ok(Self {
            layer,
            dsp,
            rate,
            channels,
        })
    }
    /// Render raw processed planar samples for a root timeline block. Gain/pan/envelopes
    /// are deliberately left to the mixer at the output clock, after latency compensation.
    pub fn render(
        &mut self,
        start_sample: i64,
        frames: usize,
        read: &mut dyn FnMut(&SourceId, u64) -> Result<(u32, [f64; 2])>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        let layer = &self.layer;
        let rate = self.rate;
        let channels = self.channels;
        let checkpoint = std::cell::RefCell::new(checkpoint);
        self.dsp.render_automated(
            start_sample,
            frames,
            &mut |start, count| {
                let mut planar = vec![vec![0.0; count]; channels];
                for index in 0..count {
                    if index % 256 == 0 {
                        checkpoint.borrow_mut()()?;
                    }
                    let clock = start
                        .checked_add(index as i64)
                        .context("clip DSP clock overflow")?;
                    if clock < 0 {
                        continue;
                    }
                    let time =
                        u64::try_from(u128::from(clock as u64) * 1_000_000 / u128::from(rate))?;
                    let Some(sample) = layer.sample(time)? else {
                        continue;
                    };
                    let (actual_channels, values) = read(layer.source_id(), sample.source_time_us)?;
                    ensure!(
                        actual_channels as usize == channels,
                        "clip DSP source channel mismatch"
                    );
                    for channel in 0..channels {
                        let value = values[channel] as f32;
                        ensure!(value.is_finite(), "clip DSP input overflow");
                        planar[channel][index] = value;
                    }
                }
                Ok(planar)
            },
            &mut |grid, chain| {
                let seconds = grid as f64 / f64::from(rate)
                    + layer.clip_local_start_us as f64 / 1_000_000.0
                    - layer.crossfade.head_ticks as f64 / 1_000_000.0
                    - layer.root_range_us.start as f64 / 1_000_000.0;
                for (slot, spec) in layer
                    .clip
                    .dsp_effects
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| !e.animation.is_empty())
                {
                    chain.update_params(slot, &spec.params_at_seconds(seconds)?)?;
                }
                Ok(())
            },
            &mut || checkpoint.borrow_mut()(),
        )
    }
}

/// Current export binding for source zero-preroll effects; remaining DSP stays explicit.
pub fn validate_export_processing(layers: &[NestedAudioLayer]) -> Result<()> {
    for layer in layers {
        ensure!(layer.concat_retime_piece.is_none()&&layer.concat_retime_owner.is_none(),"source curve audio piece needs whole-frame preparation before native DSP");
        super::filmcraft_dsp::validate_specs(&layer.clip.dsp_effects)?;
        if crate::tools::packet_clip::resolve_chain(&layer.clip.dsp_effects)?.is_some() {
            let mut packet=layer.clone();packet.clip.dsp_effects.clear();
            crate::tools::packet_clip::source_bounds(&packet,48000)?;
            continue;
        }
        super::filmcraft_audio_mapping::native_export_preroll_at(
            &layer.clip.dsp_effects,
            48000,
            (layer.clip_local_start_us as f64 - layer.crossfade.head_ticks as f64) / 1_000_000.0,
        )?;
        let mut legacy = layer.clone();
        legacy.clip.dsp_effects.clear();
        if legacy.clip.placement.speed != 1.0 || legacy.clip.placement.speed_ramp.is_some() {
            ensure!(legacy.crossfade.head_ticks == 0 && legacy.crossfade.tail_ticks == 0, "tempo crossfade handles require separate conform");
            crate::tools::native_tempo_source_filter(&legacy.clip.placement, legacy.clip.reversed)?;
            legacy.clip.placement.speed = 1.0;
            legacy.clip.placement.speed_ramp = None;
            legacy.clip.reversed = false;
        }
        super::nested_pcm::validate_processing(&[legacy])?;
    }
    Ok(())
}
/// Shared output-clock host gain/pan/envelopes for fixed and variable-frame DSP.
pub(crate) fn mix_processed_layer(
    layer: &NestedAudioLayer,
    channels: usize,
    rate: u32,
    start: u64,
    planar: &[Vec<f32>],
    output: &mut [[f64; 2]],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    ensure!(
        matches!(channels, 1 | 2) && (8000..=192000).contains(&rate),
        "invalid processed layer shape"
    );
    ensure!(
        planar.len() == channels
            && output.len() <= 65536
            && planar
                .iter()
                .all(|ch| ch.len() == output.len() && ch.iter().all(|v| v.is_finite())),
        "invalid processed layer block"
    );
    start
        .checked_add(output.len() as u64)
        .context("processed layer clock overflow")?;
    for (index, out) in output.iter_mut().enumerate() {
        if index % 256 == 0 {
            checkpoint()?;
        }
        let time = u64::try_from(u128::from(start + index as u64) * 1_000_000 / u128::from(rate))?;
        let Some(sample) = layer.sample(time)? else {
            continue;
        };
        let matrix =
            super::nested_pcm::pan_matrix(&layer.clip.pan, channels as u32, sample.clip_local_us)?;
        let left = f64::from(planar[0][index]);
        let right = if channels == 2 {
            f64::from(planar[1][index])
        } else {
            0.0
        };
        out[0] += sample.gain * (matrix[0] * left + matrix[1] * right);
        out[1] += sample.gain * (matrix[2] * left + matrix[3] * right);
        ensure!(out.iter().all(|v| v.is_finite()), "DSP mix overflow");
    }
    Ok(())
}
/// Owned per-layer recursive DSP state; decode/conform remains caller-owned.
pub struct Mixer {
    processors: Vec<ClipProcessor>,
    rate: u32,
}
impl Mixer {
    pub fn new(rate: u32, layers: Vec<(NestedAudioLayer, usize, usize)>) -> Result<Self> {
        ensure!(layers.len() <= 4096, "DSP mixer layer budget exceeded");
        ensure!(
            layers
                .iter()
                .map(|(l, _, _)| l.clip.dsp_effects.iter().filter(|e| !e.bypass).count())
                .sum::<usize>()
                <= 128,
            "DSP mixer active effect budget exceeded"
        );
        let mut processors = Vec::with_capacity(layers.len());
        for (layer, channels, preroll) in layers {
            // Legacy voice/tempo/ducking requires its distinct conform implementation.
            let mut admission = layer.clone();
            admission.clip.dsp_effects.clear();
            super::nested_pcm::validate_processing(&[admission])?;
            processors.push(ClipProcessor::new(layer, rate, channels, preroll)?);
        }
        ensure!((8000..=192000).contains(&rate), "invalid DSP mixer rate");
        Ok(Self { processors, rate })
    }
    pub fn mix(
        &mut self,
        start: u64,
        frames: usize,
        read: &mut dyn FnMut(&SourceId, u64) -> Result<(u32, [f64; 2])>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<[f64; 2]>> {
        self.mix_with_transition_sides(start,frames,read,checkpoint,Vec::new())
    }
    pub(crate) fn mix_with_transition_sides(
        &mut self,start:u64,frames:usize,
        read:&mut dyn FnMut(&SourceId,u64)->Result<(u32,[f64;2])>,
        checkpoint:&mut dyn FnMut()->Result<()>,
        additional:Vec<(NestedAudioLayer,Vec<[f64;2]>)>,
    )->Result<Vec<[f64;2]>>{
        checkpoint()?;
        ensure!(
            frames <= 65536 && self.processors.len()+additional.len()<=4096 && frames.saturating_mul(self.processors.len()+additional.len()) <= 8_388_608,
            "DSP mixer block budget exceeded"
        );
        let end = start
            .checked_add(frames as u64)
            .context("DSP mix clock overflow")?;
        ensure!(
            u128::from(end) * 1_000_000 / u128::from(self.rate) <= 9_007_199_254_740_991,
            "DSP mix clock exceeds exact range"
        );
        let signed = i64::try_from(start)?;
        let mut output = vec![[0.0; 2]; frames];
        let mut groups=std::collections::BTreeMap::<String,Vec<(NestedAudioLayer,Vec<[f64;2]>)>>::new();
        for(layer,side)in additional{
            let metadata=layer.video_transitions.as_ref().context("additional processed side lacks transition metadata")?;
            ensure!(side.len()==frames&&side.iter().all(|frame|frame.iter().all(|sample|sample.is_finite())),"invalid additional processed transition side");
            groups.entry(metadata.group.clone()).or_default().push((layer,side));
        }
        for processor in &mut self.processors {
            let planar = processor.render(signed, frames, read, checkpoint)?;
            if let Some(metadata)=&processor.layer.video_transitions {
                let mut side=vec![[0.;2];frames];
                mix_processed_layer(&processor.layer,processor.channels,self.rate,start,&planar,&mut side,checkpoint)?;
                groups.entry(metadata.group.clone()).or_default().push((processor.layer.clone(),side));
            }else{
                mix_processed_layer(&processor.layer,processor.channels,self.rate,start,&planar,&mut output,checkpoint)?;
            }
        }
        for group in groups.values(){
            let metadata=group[0].0.video_transitions.as_ref().context("transition DSP group missing")?;
            let mut ordinary=vec![[0f64;2];frames];
            for(layer,side)in group{if layer.video_transitions.as_ref().context("transition DSP metadata missing")?.ordinary_enabled{
                for(index,frame)in side.iter().enumerate(){for channel in 0..2{ordinary[index][channel]+=frame[channel];}}
            }}
            let mut planar=vec![ordinary.iter().map(|frame|frame[0]as f32).collect(),ordinary.iter().map(|frame|frame[1]as f32).collect()];
            let mut transitions=metadata.transitions.clone();
            for transition in &mut transitions{
                let timeline=transition.timeline.as_mut().context("legacy transition DSP window not migrated")?;
                let offset=metadata.root_offset_us.checked_mul(254016).context("transition DSP offset overflow")?;
                let tick=timeline.start_tick.exact_ticks().checked_add(offset).context("transition DSP clock overflow")?;
                timeline.start_tick=super::caption_time::CaptionDelta::SourceTicks(i64::try_from(tick)?);
            }
            super::transition_audio::replace_transition_blocks(&mut planar,signed,self.rate,&transitions,&mut|id,first,n|{
                let Some((_,side))=group.iter().find(|(layer,_)|&layer.clip.id==id)else{return Ok(vec![vec![0.;n];2])};
                let offset=usize::try_from(first.checked_sub(signed).context("transition DSP slice overflow")?)?;
                let end=offset.checked_add(n).context("transition DSP slice overflow")?;
                let slice=side.get(offset..end).context("transition DSP slice outside block")?;
                Ok(vec![slice.iter().map(|frame|frame[0]as f32).collect(),slice.iter().map(|frame|frame[1]as f32).collect()])
            },checkpoint)?;
            for(index,frame)in output.iter_mut().enumerate(){for channel in 0..2{frame[channel]+=f64::from(planar[channel][index]);ensure!(frame[channel].is_finite(),"transition DSP mix overflow");}}
        }
        checkpoint()?;
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn layer() -> NestedAudioLayer {
        NestedAudioLayer {
            video_transitions: None, concat_retime_piece:None, concat_retime_owner:None,export_fade_clock:false,gain_track:None,
            composition: "root".into(), instance_path: vec![], root_range_us: 10000..20000,
            clip_local_start_us: 0,
            crossfade: Default::default(),
            clip: serde_json::from_value(serde_json::json!({
                "id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":20000,"sourceOutTick":30000,"speed":1.0},
                "gain":{"mode":"constant","value":0.1},"pan":{"mode":"constant","value":0.5},
                "fadeInTicks":0,"fadeOutTicks":0,"dspEffects":[{"id":"amplify","params":{"gain":-6.0}}]
            })).unwrap(),
        }
    }
    #[test]
    fn crossfade_dsp_mix_matches_source_envelope_with_selected_seek() {
        for curve in [super::super::audio_transition::AudioTransitionCurve::ConstantGain, super::super::audio_transition::AudioTransitionCurve::ConstantPower, super::super::audio_transition::AudioTransitionCurve::ExponentialFade] {
            let mut incoming=layer();
            incoming.clip.placement.timeline_start_tick=10000;
            incoming.clip.gain=super::super::composition::AnimatableValue::constant(1.0);
            incoming.clip.pan=super::super::composition::AnimatableValue::constant(0.0);
            incoming.clip.dsp_effects=vec![serde_json::from_value(serde_json::json!({"id":"filmcraft:invert_a"})).unwrap()];
            incoming.clip.crossfade_in_ticks=2000;
            incoming.crossfade=super::super::audio_transition::AudioCrossfadePlan {head_ticks:1000,fade_in_ticks:2000,in_curve:curve,..Default::default()};
            incoming.root_range_us=9000..20000;
            validate_export_processing(&[incoming.clone()]).unwrap();
            let make=|| Mixer::new(48000,vec![(incoming.clone(),2,0)]).unwrap();
            let read=&mut |_: &SourceId,_| Ok((2,[0.25,-0.125]));
            let full=make().mix(432,528,read,&mut || Ok(())).unwrap();
            let selected=make().mix(480,48,read,&mut || Ok(())).unwrap();
            assert_eq!(&full[48..96],selected.as_slice());
            for (index,frame) in full.iter().enumerate() {
                let time=(432+index as u64)*1000000/48000;
                let p=((time-9000) as f64/2000.0).min(1.0) as f32;
                let gain=f64::from(curve.gains(p).unwrap().1);
                assert!((frame[0]+0.25*gain).abs()<1e-12);
                assert!((frame[1]-0.125*gain).abs()<1e-12);
            }
        }
    }
    #[test]
    fn authored_gain_curve_exports_with_contiguous_request_invariance() {
        let mut layer = layer();
        layer.clip.dsp_effects[0].params.clear();
        layer.clip.dsp_effects[0].animation.insert("gain".into(), serde_json::from_value(serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-6.0},{"tick":10000,"value":3.0}]}})).unwrap());
        validate_export_processing(std::slice::from_ref(&layer)).unwrap();
        let mut whole = ClipProcessor::new(layer.clone(), 48000, 1, 0).unwrap();
        let mut split = ClipProcessor::new(layer, 48000, 1, 0).unwrap();
        let mut read = |_: &SourceId, _| Ok((1, [0.25, 0.0]));
        let expected = whole
            .render(0, 1000, &mut read, &mut || Ok(()))
            .unwrap()
            .remove(0);
        let mut actual = split
            .render(0, 511, &mut read, &mut || Ok(()))
            .unwrap()
            .remove(0);
        actual.extend(
            split
                .render(511, 489, &mut read, &mut || Ok(()))
                .unwrap()
                .remove(0),
        );
        assert_eq!(actual, expected);
        let mut flat = ClipProcessor::new(self::layer(), 48000, 1, 0).unwrap();
        let baseline = flat
            .render(0, 1000, &mut read, &mut || Ok(()))
            .unwrap()
            .remove(0);
        assert!(actual[900] > baseline[900]);
    }
    #[test]
    fn active_export_chain_matches_direct_upstream_before_host_pan() {
        let mut layer = layer();
        layer.clip.dsp_effects = serde_json::from_value(serde_json::json!([
            {"id":"amplify","params":{"gain":-6.0}},
            {"id":"invert"},
            {"id":"stereo_width","params":{"width":50.0}}
        ]))
        .unwrap();
        validate_export_processing(std::slice::from_ref(&layer)).unwrap();
        let mut mixer = Mixer::new(48000, vec![(layer.clone(), 2, 0)]).unwrap();
        let mut reader = |_: &SourceId, _| Ok((2, [0.25, -0.125]));
        let mut actual = mixer.mix(0, 511, &mut reader, &mut || Ok(())).unwrap();
        actual.extend(mixer.mix(511, 489, &mut reader, &mut || Ok(())).unwrap());
        let mut left = vec![0.0f32; 1000];
        let mut right = left.clone();
        left[480..960].fill(0.25);
        right[480..960].fill(-0.125);
        for spec in &layer.clip.dsp_effects {
            let mut native = filmcraft_audio_dsp::create_effect(&spec.id, 48000.0, 2).unwrap();
            for (id, value) in &spec.params {
                assert!(native.set_param(id, *value));
            }
            native.process(&mut [&mut left, &mut right]);
        }
        let matrix = super::super::audio_pan::equal_power_pan(0.5, 2);
        let expected: Vec<[f64; 2]> = (0..1000)
            .map(|i| {
                if (480..960).contains(&i) {
                    [
                        0.1 * (matrix[0] * f64::from(left[i]) + matrix[1] * f64::from(right[i])),
                        0.1 * (matrix[2] * f64::from(left[i]) + matrix[3] * f64::from(right[i])),
                    ]
                } else {
                    [0.0; 2]
                }
            })
            .collect();
        assert_eq!(actual, expected);
        layer.clip.dsp_effects[0].id = "pitch_shifter".into();
        assert!(validate_export_processing(&[layer]).is_err());
    }
    #[test]
    fn bypass_mixer_matches_host_gain_pan_and_sums_layers() {
        let mut layer = layer();
        layer.clip.dsp_effects[0].bypass = true;
        let mut mixer =
            Mixer::new(48000, vec![(layer.clone(), 1, 0), (layer.clone(), 1, 0)]).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let expected = super::super::nested_pcm::mix_block(
            &[layer.clone(), layer],
            0,
            1000,
            48000,
            &cancel,
            |_, _| Ok((1, [0.25, 0.0])),
        )
        .unwrap();
        let actual = mixer
            .mix(0, 1000, &mut |_, _| Ok((1, [0.25, 0.0])), &mut || Ok(()))
            .unwrap();
        assert_eq!(actual, expected);
        assert!(
            mixer
                .mix(u64::MAX, 1, &mut |_, _| unreachable!(), &mut || Ok(()))
                .is_err()
        );
    }
    #[test]
    fn placed_source_matches_upstream_and_block_boundaries() {
        let mut processor = ClipProcessor::new(layer(), 48000, 1, 0).unwrap();
        let mut reads = Vec::new();
        let mut reader = |id: &SourceId, time| {
            assert_eq!(id.as_str(), "source");
            reads.push(time);
            Ok((1, [time as f64 / 100000.0, 0.0]))
        };
        let mut actual = processor
            .render(0, 257, &mut reader, &mut || Ok(()))
            .unwrap()
            .remove(0);
        actual.extend(
            processor
                .render(257, 743, &mut reader, &mut || Ok(()))
                .unwrap()
                .remove(0),
        );
        let mut expected: Vec<f32> = (0..1000u64)
            .map(|frame| {
                let root = frame * 1000000 / 48000;
                if (10000..20000).contains(&root) {
                    ((root - 10000 + 20000) as f64 / 100000.0) as f32
                } else {
                    0.0
                }
            })
            .collect();
        let mut native = filmcraft_audio_dsp::create_effect("amplify", 48000.0, 1).unwrap();
        assert!(native.set_param("gain", -6.0));
        native.process(&mut [&mut expected]);
        assert_eq!(actual, expected);
        assert_eq!(reads.len(), 480);
        assert_eq!(reads[0], 20000);
        assert!(reads.iter().all(|t| (20000..30000).contains(t)));
        // A seek reconstructs smoothing state exactly like a new processor.
        let mut fresh = ClipProcessor::new(layer(), 48000, 1, 100).unwrap();
        let mut seek = ClipProcessor::new(layer(), 48000, 1, 100).unwrap();
        let mut read = |_: &SourceId, _: u64| Ok((1, [0.25, 0.0]));
        seek.render(500, 50, &mut read, &mut || Ok(())).unwrap();
        assert_eq!(
            seek.render(600, 50, &mut read, &mut || Ok(())).unwrap(),
            fresh.render(600, 50, &mut read, &mut || Ok(())).unwrap()
        );
    }
    #[test]
    fn invalid_sources_and_cancelled_reads_are_refused() {
        let mut processor = ClipProcessor::new(layer(), 48000, 1, 0).unwrap();
        assert!(
            processor
                .render(480, 1, &mut |_, _| Ok((2, [0.0, 0.0])), &mut || Ok(()))
                .is_err()
        );
        assert!(
            processor
                .render(480, 1, &mut |_, _| Ok((1, [f64::MAX, 0.0])), &mut || Ok(()))
                .is_err()
        );
        assert!(
            processor
                .render(
                    480,
                    1000,
                    &mut |_, _| unreachable!(),
                    &mut || anyhow::bail!("cancelled")
                )
                .is_err()
        );
        let output = processor
            .render(-10, 10, &mut |_, _| unreachable!(), &mut || Ok(()))
            .unwrap();
        assert_eq!(output, vec![vec![0.0; 10]]);
    }
}
