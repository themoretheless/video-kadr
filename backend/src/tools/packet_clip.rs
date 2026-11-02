//! Per-clip binding for source-typed variable-frame audio before host gain/pan.
use super::{
    packet_pcm::{Decoded, Spec},
    packet_pitch::Block,
    packet_timeline::{Source, Timeline},
};
use crate::domain::{concat_format_plan::Format, nested_audio::NestedAudioLayer};
use anyhow::{ensure, Context, Result};
use std::ops::Range;
#[derive(Clone, Copy)]
pub(crate) enum Effect {
    Pitch(f32),
    MixedPitchLoudness { stages:[super::packet_pitch_loudness_chain::Setting;16],len:usize },
    Loudness(f32),
    LoudnessPitch { target:f32, pitch:f32 },
    PitchLoudness { pitch:f32, target:f32 },
    LoudnessChain { targets: [f32;16], len:usize },
    PitchChain {
        semitones: [f32; 16],
        len: usize,
    },
    Voice(super::packet_voice::Preset),
    Sweet {
        amount: f32,
        pitch: f32,
    },
    VoiceChain {
        stages: [super::packet_voice_chain::Setting; 16],
        len: usize,
    },
}
enum Reader {
    Pitch(super::packet_pitch_frames::Reader),
    Voice(super::packet_voice::Reader),
    Ordered(Box<dyn Source>),
}
impl Source for Reader {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        match self {
            Self::Pitch(r) => r.next_packet(check),
            Self::Voice(r) => r.next_packet(check),
            Self::Ordered(r) => r.next_packet(check),
        }
    }
    fn reset(&mut self) {
        match self {
            Self::Pitch(r) => r.reset(),
            Self::Voice(r) => r.reset(),
            Self::Ordered(r) => r.reset(),
        }
    }
}
fn spec(decoded: &Decoded) -> Spec {
    match decoded {
        Decoded::U8(s) => s.spec(),
        Decoded::S16(s) => s.spec(),
        Decoded::S32(s) => s.spec(),
        Decoded::F32(s) => s.spec(),
        Decoded::F64(s) => s.spec(),
    }
}
fn sample(time_us: i128, rate: u32) -> Result<i64> {
    let ticks = time_us
        .checked_mul(i128::from(rate))
        .context("packet clip clock overflow")?;
    i64::try_from(ticks.div_euclid(1_000_000) + i128::from(ticks.rem_euclid(1_000_000) != 0))
        .context("packet clip clock exceeds signed sample range")
}
/// Explicit source trim for the currently supported forward, unit-speed path.
/// Packet tempo/reverse still needs separate source mapping. Transition windows
/// are applied to processed, clip-bounded sides by the host mixer.
pub(crate) fn source_bounds(layer: &NestedAudioLayer, rate: u32) -> Result<Range<u64>> {
    ensure!(
        layer.clip.dsp_effects.iter().all(|s| s.bypass),
        "authored variable-frame effect chain binding is not implemented"
    );
    let mut admission = layer.clone();
    admission.clip.dsp_effects.clear();
    crate::domain::nested_pcm::validate_processing(&[admission])?;
    let p = &layer.clip.placement;
    ensure!(
        p.speed == 1. && p.speed_ramp.is_none() && !layer.clip.reversed,
        "variable-frame clip tempo/reverse routing is not implemented"
    );
    ensure!(
        (8000..=192000).contains(&rate),
        "invalid packet clip source rate"
    );
    let head = p.source_head_timeline_ticks()?;
    let start = p
        .source_in_tick
        .checked_sub(layer.crossfade.head_ticks)
        .and_then(|v| v.checked_sub(head))
        .context("packet clip crossfade source head underflow")?;
    let end = p
        .source_out_tick
        .checked_add(layer.crossfade.tail_ticks)
        .context("packet clip crossfade source tail overflow")?;
    let bounds = u64::try_from(sample(i128::from(start), rate)?)?
        ..u64::try_from(sample(i128::from(end), rate)?)?;
    ensure!(bounds.start < bounds.end, "empty packet clip source trim");
    Ok(bounds)
}
pub(crate) struct Processor {
    layer: NestedAudioLayer,
    timeline: Timeline<Reader>,
    channels: usize,
    failed: bool,
}
impl Processor {
    pub(crate) fn layer(&self)->&NestedAudioLayer{&self.layer}
    pub(crate) fn new(layer: NestedAudioLayer, decoded: Decoded, effect: Effect) -> Result<Self> {
        let spec = spec(&decoded);
        ensure!(
            matches!(spec.channels, 1 | 2),
            "packet clip requires mono/stereo"
        );
        let bounds = source_bounds(&layer, spec.rate)?;
        ensure!(
            spec.start == bounds.start && spec.end == bounds.end,
            "packet clip decoder trim mismatch"
        );
        let p = &layer.clip.placement;
        let head = p.source_head_timeline_ticks()?;
        let origin = i128::from(layer.root_range_us.start)
            - i128::from(layer.clip_local_start_us)
            - i128::from(p.source_timeline_offset_tick.unwrap_or(0))
            - i128::from(head);
        let duration = p
            .source_out_tick
            .checked_sub(p.source_in_tick)
            .and_then(|n| n.checked_add(head))
            .and_then(|n| n.checked_add(layer.crossfade.head_ticks))
            .and_then(|n| n.checked_add(layer.crossfade.tail_ticks))
            .context("packet clip duration overflow")?;
        let output = sample(origin, 48000)?..sample(origin + i128::from(duration), 48000)?;
        let reader = match effect {
            Effect::MixedPitchLoudness{stages,len}=>Reader::Ordered(super::packet_pitch_loudness_chain::build(decoded,&stages[..len])?),
            Effect::PitchLoudness { pitch, target } => {
                let pitch=super::packet_pitch_frames::Reader::new(decoded,&[crate::domain::concat_format_plan::Filter::Resample,crate::domain::concat_format_plan::Filter::Loudness],Format::F32,pitch)?;
                let format=pitch.plan.stages[3].output;
                let up=super::packet_typed_rate::reader(Box::new(pitch),format,48000,192000,spec.channels)?;
                let loud=super::packet_loudness::Reader::new(up,192000,spec.channels,f64::from(target))?;
                Reader::Ordered(Box::new(super::packet_double_rate::Reader::new(Box::new(loud),192000,48000,spec.channels)?))
            }
            Effect::LoudnessPitch { target, pitch } => {
                let up=super::packet_rate_frames::reader(decoded,192000)?;
                let loud=super::packet_loudness::Reader::new(up,192000,spec.channels,f64::from(target))?;
                let down=super::packet_double_rate::Reader::new(Box::new(loud),192000,48000,spec.channels)?;
                Reader::Ordered(Box::new(super::packet_pitch_node::Node::new(Box::new(down),Format::F64,Format::F32,spec.channels,pitch)?))
            }
            Effect::LoudnessChain { targets, len } => {
                ensure!((2..=16).contains(&len),"invalid Loudness chain length");
                let mut source=super::packet_rate_frames::reader(decoded,192000)?;
                for target in &targets[..len] {source=Box::new(super::packet_loudness::Reader::new(source,192000,spec.channels,f64::from(*target))?);}
                Reader::Ordered(Box::new(super::packet_double_rate::Reader::new(source,192000,48000,spec.channels)?))
            }
            Effect::Loudness(target) => {
                let up=super::packet_rate_frames::reader(decoded,192000)?;
                let core=super::packet_loudness::Reader::new(up,192000,spec.channels,f64::from(target))?;
                Reader::Ordered(Box::new(super::packet_double_rate::Reader::new(Box::new(core),192000,48000,spec.channels)?))
            }
            Effect::Pitch(pitch) => Reader::Pitch(super::packet_pitch_frames::Reader::new(
                decoded,
                &[],
                Format::F32,
                pitch,
            )?),
            Effect::PitchChain { semitones, len } => {
                ensure!(
                    (2..=16).contains(&len),
                    "invalid ordered Pitch chain length"
                );
                use crate::domain::concat_format_plan::Filter;
                let following = (1..len)
                    .flat_map(|_| {
                        [
                            Filter::Resample,
                            Filter::Pass,
                            Filter::Resample,
                            Filter::Tempo,
                        ]
                    })
                    .collect::<Vec<_>>();
                let first = super::packet_pitch_frames::Reader::new(
                    decoded,
                    &following,
                    Format::F32,
                    semitones[0],
                )?;
                let mut formats = Vec::new();
                for i in 1..len {
                    let offset = i * 4;
                    ensure!(
                        first.plan.stages[offset].input == first.plan.stages[offset - 1].output
                            && first.plan.stages[offset].input == first.plan.stages[offset].output,
                        "ordered Pitch normalizer conversion is not implemented"
                    );
                    formats.push((
                        first.plan.stages[offset].input,
                        first.plan.stages[offset + 3].output,
                    ));
                }
                let mut source: Box<dyn Source> = Box::new(first);
                for (i, (input, output)) in formats.into_iter().enumerate() {
                    source = Box::new(super::packet_pitch_node::Node::new(
                        source,
                        input,
                        output,
                        spec.channels,
                        semitones[i + 1],
                    )?);
                }
                Reader::Ordered(source)
            }
            Effect::VoiceChain { stages, len } => {
                ensure!(
                    (2..=16).contains(&len),
                    "invalid ordered voice chain length"
                );
                Reader::Ordered(super::packet_voice_chain::build(decoded, &stages[..len])?)
            }
            Effect::Sweet { amount, pitch } => Reader::Ordered(super::packet_voice_chain::build(
                decoded,
                &[super::packet_voice_chain::Setting::Sweet { amount, pitch }],
            )?),
            Effect::Voice(preset) => {
                Reader::Voice(super::packet_voice::Reader::new(decoded, preset)?)
            }
        };
        Ok(Self {
            layer,
            timeline: Timeline::new(reader, spec.channels, output)?,
            channels: spec.channels,
            failed: false,
        })
    }
    pub(crate) fn reset(&mut self) {
        self.timeline.reset();
        self.failed = false;
    }
    pub(crate) fn mix(
        &mut self,
        start: u64,
        frames: usize,
        check: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<[f64; 2]>> {
        ensure!(!self.failed, "packet clip requires reset after failure");
        self.failed = true;
        let raw = self.timeline.render(i64::try_from(start)?, frames, check)?;
        // Match the existing host's f32 processing boundary, after the complete
        // source-typed chain, while gain/pan accumulation remains f64.
        let planar = raw
            .into_iter()
            .map(|ch| ch.into_iter().map(|v| v as f32).collect())
            .collect::<Vec<Vec<f32>>>();
        let mut output = vec![[0.; 2]; frames];
        crate::domain::nested_dsp::mix_processed_layer(
            &self.layer,
            self.channels,
            48000,
            start,
            &planar,
            &mut output,
            check,
        )?;
        check()?;
        self.failed = false;
        Ok(output)
    }
}

/// Preparation boundary shared with export: trusted probe -> exact source trim
/// -> decoded packet spool -> complete owned chain -> existing host mixer law.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare(
    runtime: &crate::process_control::ProcessRuntime,
    ffmpeg: &std::path::Path,
    staging: &std::path::Path,
    layer: NestedAudioLayer,
    source: &super::native_audio_export::PcmSource,
    effect: Effect,
    staging_bytes: u64,
    timeout: std::time::Duration,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Processor> {
    ensure!(
        !cancel.is_cancelled() && !timeout.is_zero(),
        "packet clip preparation cancelled or zero timeout"
    );
    let probe = source
        .probe
        .as_ref()
        .context("packet clip requires fresh source metadata")?;
    let (_, rate, channels) = super::packet_pcm::Format::from_probe(probe)?;
    ensure!(
        channels as u32 == source.channels,
        "packet clip source metadata channel mismatch"
    );
    let bounds = source_bounds(&layer, rate)?;
    let decoded = super::packet_pcm::decode_probed(
        runtime,
        ffmpeg,
        &source.path,
        staging,
        probe,
        bounds,
        staging_bytes,
        timeout,
        cancel,
    )
    .await?;
    ensure!(!cancel.is_cancelled(), "packet clip preparation cancelled");
    Processor::new(layer, decoded, effect)
}

/// Resolve packet effects without constructing a fixed-frame AudioEffect.
/// Step sizes are UI hints; finite fractional values within source ranges remain legal.
pub(crate) fn is_effect(id: &str) -> bool {
    matches!(
        id,
        "concat.pitch" | "concat.chipmunk" | "concat.deep" | "concat.sweet" | "concat.loudness"
    )
}
pub(crate) fn validate_spec(spec: &crate::domain::filmcraft_dsp::EffectSpec) -> Result<()> {
    crate::domain::concat_audio_import::validate_source_keys(&spec.id,&spec.concat_source_keys)?;
    resolve(spec)?.context("unknown packet effect")?;
    Ok(())
}
fn parameters(id: &str) -> Option<&'static [(&'static str, f32, f32, f32)]> {
    Some(match id {
        "concat.loudness" => &[("target", -30., -12., -23.)],
        "concat.pitch" => &[("semitones", -12., 12., 0.)],
        "concat.chipmunk" => &[("pitch", 3., 12., 7.)],
        "concat.deep" => &[("pitch", -8., -1., -3.), ("body", 0., 8., 3.)],
        "concat.sweet" => &[("amount", 0., 100., 65.), ("pitch", 0., 8., 0.)],
        _ => return None,
    })
}
pub(crate) fn catalog() -> Vec<serde_json::Value> {
    use serde_json::json;
    [
        ("concat.pitch", "Pitch"),
        ("concat.chipmunk", "Chipmunk"),
        ("concat.deep", "Deep"),
        ("concat.sweet", "Sweet"),
        ("concat.loudness", "Normalize Loudness"),
    ]
    .into_iter()
    .map(|(id, name)| {
        json!({"id":id,"name":name,"category":if id=="concat.loudness" {"clean"} else {"pitch"},"nativeExport":true,
            "params":parameters(id).unwrap().iter().map(|(key,min,max,default)| json!({
                "id":key,"name":match *key {"target"=>"Target","semitones"=>"Semitones","body"=>"Body","amount"=>"Amount",_=>"Pitch"},
                "min":min,"max":max,"default":default,"step":if *key=="amount" || *key=="target" {1.0} else {0.5},
                "unit":if *key=="body" || *key=="target" {"decibels"} else if *key=="amount" {"percent"} else {"semitones"},
                "logScale":false,"choices":[],"animatable":false,
            })).collect::<Vec<_>>()})
    })
    .collect()
}
fn resolve(spec: &crate::domain::filmcraft_dsp::EffectSpec) -> Result<Option<Effect>> {
    use super::packet_voice::Preset;
    let Some(parameters) = parameters(&spec.id) else {
        return Ok(None);
    };
    ensure!(
        spec.animation.is_empty(),
        "variable-frame parameter animation is not implemented"
    );
    for (key, value) in &spec.params {
        let (_, min, max, _) = parameters
            .iter()
            .find(|(id, _, _, _)| *id == key)
            .context("unknown variable-frame audio parameter")?;
        ensure!(
            value.is_finite() && value >= min && value <= max,
            "variable-frame audio parameter out of range"
        );
    }
    let value = |key: &str| {
        spec.params.get(key).copied().unwrap_or_else(|| {
            parameters
                .iter()
                .find(|(id, _, _, _)| *id == key)
                .unwrap()
                .3
        })
    };
    Ok(Some(match spec.id.as_str() {
        "concat.loudness" => Effect::Loudness(value("target")),
        "concat.pitch" => Effect::Pitch(value("semitones")),
        "concat.chipmunk" => Effect::Voice(Preset::Chipmunk {
            pitch: value("pitch"),
        }),
        "concat.deep" => Effect::Voice(Preset::Deep {
            pitch: value("pitch"),
            body: f64::from(value("body")),
        }),
        "concat.sweet" => Effect::Sweet {
            amount: value("amount"),
            pitch: value("pitch"),
        },
        _ => unreachable!(),
    }))
}
pub(crate) fn resolve_chain(
    specs: &[crate::domain::filmcraft_dsp::EffectSpec],
) -> Result<Option<Effect>> {
    ensure!(specs.len() <= 16, "packet clip effect chain exceeds budget");
    let mut active = Vec::new();
    let mut fixed = false;
    for spec in specs {
        if let Some(resolved) = resolve(spec)? {
            if !spec.bypass {
                active.push(resolved);
            }
        } else {
            crate::domain::filmcraft_dsp::validate_specs(std::slice::from_ref(spec))?;
            fixed |= !spec.bypass;
        }
    }
    ensure!(
        active.is_empty() || !fixed,
        "mixed fixed/variable audio effect chains are not implemented"
    );
    match active.len() {
        0 => Ok(None),
        1 => Ok(Some(active[0])),
        len => {
            if len>2 && active.iter().all(|e|matches!(e,Effect::Pitch(_)|Effect::Loudness(_))) && active.iter().any(|e|matches!(e,Effect::Loudness(_))) && active.iter().any(|e|matches!(e,Effect::Pitch(_))) {
                use super::packet_pitch_loudness_chain::Setting;
                let mut stages=[Setting::Pitch(0.);16];
                for (i,e) in active.iter().enumerate() {stages[i]=match e {Effect::Pitch(p)=>Setting::Pitch(*p),Effect::Loudness(t)=>Setting::Loudness(*t),_=>unreachable!()};}
                return Ok(Some(Effect::MixedPitchLoudness{stages,len}));
            }
            if let [Effect::Pitch(pitch),Effect::Loudness(target)] = active.as_slice() {
                return Ok(Some(Effect::PitchLoudness{pitch:*pitch,target:*target}));
            }
            if let [Effect::Loudness(target),Effect::Pitch(pitch)] = active.as_slice() {
                return Ok(Some(Effect::LoudnessPitch{target:*target,pitch:*pitch}));
            }
            if active.iter().any(|e|matches!(e,Effect::Loudness(_))) {
                ensure!(active.iter().all(|e|matches!(e,Effect::Loudness(_))),"mixed Loudness/voice chains are not implemented");
                let mut targets=[-23.;16];
                for (i,effect) in active.iter().enumerate() {let Effect::Loudness(target)=effect else {unreachable!()};targets[i]=*target;}
                return Ok(Some(Effect::LoudnessChain{targets,len}));
            }
            if active.iter().any(|e| !matches!(e, Effect::Pitch(_))) {
                use super::{packet_voice::Preset, packet_voice_chain::Setting};
                let mut stages = [Setting::Pitch(0.); 16];
                for (i, effect) in active.into_iter().enumerate() {
                    stages[i] = match effect {
                        Effect::Pitch(p) => Setting::Pitch(p),
                        Effect::Sweet { amount, pitch } => Setting::Sweet { amount, pitch },
                        Effect::Voice(Preset::Chipmunk { pitch }) => Setting::Chipmunk(pitch),
                        Effect::Voice(Preset::Deep { pitch, body }) => {
                            Setting::Deep { pitch, body }
                        }
                        _ => unreachable!(),
                    };
                }
                return Ok(Some(Effect::VoiceChain { stages, len }));
            }
            let mut semitones = [0.; 16];
            for (i, effect) in active.into_iter().enumerate() {
                let Effect::Pitch(pitch) = effect else {
                    anyhow::bail!("ordered voice-preset chains are not implemented")
                };
                semitones[i] = pitch;
            }
            Ok(Some(Effect::PitchChain { semitones, len }))
        }
    }
}
/// Authored-chain entry point used by export and preview after clip admission.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare_authored(
    runtime: &crate::process_control::ProcessRuntime,
    ffmpeg: &std::path::Path,
    staging: &std::path::Path,
    mut layer: NestedAudioLayer,
    source: &super::native_audio_export::PcmSource,
    staging_bytes: u64,
    timeout: std::time::Duration,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Processor> {
    let effect = resolve_chain(&layer.clip.dsp_effects)?
        .context("authored clip has no active packet effect")?;
    layer.clip.dsp_effects.clear();
    prepare(
        runtime,
        ffmpeg,
        staging,
        layer,
        source,
        effect,
        staging_bytes,
        timeout,
        cancel,
    )
    .await
}
#[cfg(test)]
mod authored_tests {
    use super::*;
    use crate::domain::filmcraft_dsp::EffectSpec;
    fn authored(id: &str) -> EffectSpec {
        EffectSpec { concat_source_keys: Default::default(),
            id: id.into(),
            ..Default::default()
        }
    }
    #[test]
    fn source_defaults_ranges_fractional_values_and_bypass() {
        assert!(matches!(
            resolve_chain(&[authored("concat.pitch")]).unwrap(),
            Some(Effect::Pitch(0.))
        ));
        assert!(matches!(
            resolve_chain(&[authored("concat.chipmunk")]).unwrap(),
            Some(Effect::Voice(
                super::super::packet_voice::Preset::Chipmunk { pitch: 7. }
            ))
        ));
        assert!(matches!(
            resolve_chain(&[authored("concat.deep")]).unwrap(),
            Some(Effect::Voice(super::super::packet_voice::Preset::Deep {
                pitch: -3.,
                body: 3.
            }))
        ));
        assert!(matches!(
            resolve_chain(&[authored("concat.sweet")]).unwrap(),
            Some(Effect::Sweet {
                amount: 65.,
                pitch: 0.
            })
        ));
        for (id, key, min, max) in [
            ("concat.pitch", "semitones", -12., 12.),
            ("concat.sweet", "amount", 0., 100.),
            ("concat.sweet", "pitch", 0., 8.),
            ("concat.chipmunk", "pitch", 3., 12.),
            ("concat.deep", "pitch", -8., -1.),
            ("concat.deep", "body", 0., 8.),
        ] {
            for value in [min, max, (min + max) / 2. + 0.055] {
                let mut spec = authored(id);
                spec.params.insert(key.into(), value);
                assert!(resolve_chain(&[spec.clone()]).unwrap().is_some());
                spec.bypass = true;
                assert!(resolve_chain(&[spec]).unwrap().is_none());
            }
            for value in [min - 0.01, max + 0.01, f32::NAN, f32::INFINITY] {
                let mut spec = authored(id);
                spec.params.insert(key.into(), value);
                spec.bypass = true;
                assert!(resolve_chain(&[spec]).is_err());
            }
        }
    }
    #[test]
    fn authored_chain_refuses_omitted_stages_and_unknown_settings() {
        let packet = authored("concat.deep");
        let mut fixed = authored("amplify");
        assert!(resolve_chain(&[packet.clone(), fixed.clone()]).is_err());
        fixed.bypass = true;
        assert!(resolve_chain(&[packet.clone(), fixed]).unwrap().is_some());
        assert!(resolve_chain(&[packet.clone(), packet.clone()])
            .unwrap()
            .is_some());
        let mut invalid = packet.clone();
        invalid.params.insert("typo".into(), 1.);
        assert!(resolve_chain(&[invalid]).is_err());
        let mut animated = packet;
        animated.animation.insert(
            "pitch".into(),
            crate::domain::composition::AnimatableValue::Constant { value: -3. },
        );
        assert!(resolve_chain(&[animated]).is_err());
        assert!(resolve_chain(&vec![authored("concat.pitch"); 17]).is_err());
        assert!(resolve_chain(&[authored("unknown")]).is_err());
    }
}

#[cfg(test)]
mod pitch_chain_tests {
    use super::*;
    #[test]
    fn authored_order_bypass_and_maximum_length_are_preserved() {
        use crate::domain::filmcraft_dsp::EffectSpec;
        let pitch = |v| EffectSpec { concat_source_keys: Default::default(),
            id: "concat.pitch".into(),
            params: std::collections::BTreeMap::from([("semitones".into(), v)]),
            ..Default::default()
        };
        let chain = vec![
            pitch(7.),
            EffectSpec { concat_source_keys: Default::default(),
                id: "concat.deep".into(),
                bypass: true,
                ..Default::default()
            },
            pitch(-3.),
            pitch(1.5),
        ];
        let Some(Effect::PitchChain { semitones, len }) = resolve_chain(&chain).unwrap() else {
            panic!()
        };
        assert_eq!(len, 3);
        assert_eq!(semitones[..3], [7., -3., 1.5]);
        assert!(matches!(
            resolve_chain(&vec![pitch(0.); 16]).unwrap(),
            Some(Effect::PitchChain { len: 16, .. })
        ));
        assert!(resolve_chain(&vec![pitch(0.); 17]).is_err());
        assert!(resolve_chain(&[
            pitch(0.),
            EffectSpec { concat_source_keys: Default::default(),
                id: "concat.chipmunk".into(),
                ..Default::default()
            }
        ])
        .unwrap()
        .is_some());
    }
}

/// Conservative live-DSP reservation, separate from decoded disk staging.
/// This is admission accounting, not a physical RSS measurement.
pub(crate) fn live_memory_bytes(effect: Effect) -> u64 {
    match effect {Effect::MixedPitchLoudness{stages,len}=>stages[..len].iter().map(|s|match s {super::packet_pitch_loudness_chain::Setting::Loudness(_)=>256*1024*1024,super::packet_pitch_loudness_chain::Setting::Pitch(_)=>64*1024*1024}).sum(),Effect::Loudness(_)=>256*1024*1024,Effect::LoudnessPitch{..}|Effect::PitchLoudness{..}=>320*1024*1024,Effect::LoudnessChain{len,..}=>256*1024*1024*len as u64,_=>0}
}

#[cfg(test)]
mod loudness_admission_tests {
    use super::*;
    use crate::domain::filmcraft_dsp::EffectSpec;
    #[test]
    fn metadata_parameters_and_chain_rejection() {
        let mut spec=EffectSpec{ concat_source_keys: Default::default(),id:"concat.loudness".into(),..Default::default()};
        assert!(matches!(resolve_chain(&[spec.clone()]).unwrap(),Some(Effect::Loudness(-23.))));
        for target in [-30.,-23.,-12.] {spec.params.insert("target".into(),target);assert!(resolve_chain(&[spec.clone()]).is_ok());}
        spec.params.insert("target".into(),-31.);assert!(resolve_chain(&[spec.clone()]).is_err());
        spec.params.clear();
        assert!(matches!(resolve_chain(&[spec.clone(),spec.clone()]).unwrap(),Some(Effect::LoudnessChain{len:2,..})));
        assert!(matches!(resolve_chain(&[spec.clone(),EffectSpec{ concat_source_keys: Default::default(),id:"concat.pitch".into(),..Default::default()}]).unwrap(),Some(Effect::LoudnessPitch{..})));
        assert!(matches!(resolve_chain(&[EffectSpec{ concat_source_keys: Default::default(),id:"concat.pitch".into(),..Default::default()},spec.clone()]).unwrap(),Some(Effect::PitchLoudness{..})));
        spec.bypass=true;assert!(resolve_chain(&[spec]).unwrap().is_none());
        let entry=catalog().into_iter().find(|v|v["id"]=="concat.loudness").unwrap();
        assert_eq!(entry["params"][0]["default"],-23.);assert_eq!(entry["params"][0]["step"],1.);
        assert_eq!(entry["params"][0]["unit"],"decibels");assert_eq!(entry["params"][0]["animatable"],false);
        assert_eq!(live_memory_bytes(Effect::Loudness(-23.)),256*1024*1024);
    }
}
