//! Bounded host adapter over the unchanged Filmcraft audio DSP algorithms.
use anyhow::{Context, Result, ensure};
use filmcraft_audio_dsp::{AudioEffect, Unit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub fn effect_info(id: &str) -> Option<&'static filmcraft_audio_dsp::EffectInfo> {
    filmcraft_audio_dsp::effect_info(id).or_else(|| super::concat_audio::info(id))
}
pub fn create_effect(id: &str, rate: f32, channels: usize) -> Option<Box<dyn AudioEffect>> {
    effect_info(id).map(|info| (info.create)(rate, channels))
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectSpec {
    /// Inactive source metadata: pinned Concat audio chains read static params.
    #[serde(default,skip_serializing_if="BTreeMap::is_empty")]
    pub concat_source_keys:BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>,
    pub id: String,
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
    #[serde(default)]
    pub bypass: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub animation: BTreeMap<String, super::composition::AnimatableValue>,
}
pub fn validate_specs(specs: &[EffectSpec]) -> Result<()> {
    ensure!(specs.len() <= 16, "DSP chain exceeds budget");
    // Validate the entire authored configuration before constructing any state.
    for spec in specs {
        if !spec.concat_source_keys.is_empty(){super::concat_audio_import::validate_source_keys(&spec.id,&spec.concat_source_keys)?;}
        ensure!(
            spec.id.len() <= 128 && spec.params.len() <= 128,
            "invalid DSP effect budget"
        );
        if spec
            .id
            .starts_with(super::filmcraft_audio_project::PROJECT_PREFIX)
        {
            super::filmcraft_audio_project::validate_authored(spec)?;
            continue;
        }
        if crate::tools::packet_clip::is_effect(&spec.id) {
            crate::tools::packet_clip::validate_spec(spec)?;
            continue;
        }
        let info = effect_info(&spec.id).context("unknown DSP effect")?;
        ensure!(!spec.id.starts_with("concat.") || spec.animation.is_empty(), "Concat audio parameter animation is not supported");
        ensure!(
            spec.animation.len() <= 128,
            "DSP animation parameter budget exceeded"
        );
        let mut keys = 0usize;
        for (id, curve) in &spec.animation {
            let param = info
                .params
                .iter()
                .find(|p| p.id == id)
                .context("unknown DSP animation parameter")?;
            curve
                .validate()
                .map_err(|error| anyhow::anyhow!("invalid DSP curve: {error:?}"))?;
            let values: Vec<f64> = match curve {
                super::composition::AnimatableValue::Constant { value } => vec![*value],
                super::composition::AnimatableValue::Keyframes { track } => {
                    keys = keys
                        .checked_add(track.keyframes.len())
                        .context("DSP keyframe overflow")?;
                    ensure!(keys <= 8192, "DSP keyframe budget exceeded");
                    track.keyframes.iter().map(|key| key.value).collect()
                }
            };
            ensure!(
                values.iter().all(|v| v.is_finite()
                    && *v >= f64::from(param.min)
                    && *v <= f64::from(param.max)
                    && (!matches!(param.unit, Unit::Choice | Unit::Toggle) || v.fract() == 0.0)),
                "DSP curve value out of range"
            );
        }

        for (id, value) in &spec.params {
            let param = info
                .params
                .iter()
                .find(|p| p.id == id)
                .context("unknown DSP parameter")?;
            ensure!(
                value.is_finite() && *value >= param.min && *value <= param.max,
                "DSP parameter out of range"
            );
            if matches!(param.unit, Unit::Toggle | Unit::Choice) {
                ensure!(value.fract() == 0.0, "DSP choice must be integral");
            }
        }
    }
    Ok(())
}
impl EffectSpec {
    /// Authored project scalars retain source units until dispatcher conversion.
    pub fn authored_params_at_seconds(&self, seconds: f64) -> Result<BTreeMap<String, f64>> {
        ensure!(seconds.is_finite(), "invalid DSP curve clock");
        let mut values: BTreeMap<String, f64> = self
            .params
            .iter()
            .map(|(k, v)| (k.clone(), f64::from(*v)))
            .collect();
        for (id, curve) in &self.animation {
            let value = match curve {
                super::composition::AnimatableValue::Constant { value } => *value,
                super::composition::AnimatableValue::Keyframes { track } => {
                    track.sample_seconds(seconds)?
                }
            };
            ensure!(value.is_finite(), "nonfinite DSP animation sample");
            values.insert(id.clone(), value);
        }
        Ok(values)
    }
    pub fn native_at_seconds(&self, seconds: f64, rate: u32) -> Result<EffectSpec> {
        if let Some(id) = self
            .id
            .strip_prefix(super::filmcraft_audio_project::PROJECT_PREFIX)
        {
            return Ok(super::filmcraft_audio_project::resolve(
                id,
                &self.authored_params_at_seconds(seconds)?,
                self.bypass,
                rate,
            )?
            .0);
        }
        Ok(EffectSpec { concat_source_keys: Default::default(),
            id: self.id.clone(),
            params: self.params_at_seconds(seconds)?,
            bypass: self.bypass,
            animation: BTreeMap::new(),
        })
    }
    /// Resolve validated native curves at the clip/source-local microsecond clock.
    pub fn params_at(&self, local_us: u64) -> Result<BTreeMap<String, f32>> {
        self.params_at_seconds(local_us as f64 / 1_000_000.0)
    }
    pub fn params_at_seconds(&self, seconds: f64) -> Result<BTreeMap<String, f32>> {
        ensure!(seconds.is_finite(), "invalid DSP curve clock");
        if self
            .id
            .starts_with(super::filmcraft_audio_project::PROJECT_PREFIX)
        {
            return Ok(self.native_at_seconds(seconds, 48000)?.params);
        }
        let info = effect_info(&self.id).context("unknown DSP effect")?;
        let mut values = self.params.clone();
        for (id, curve) in &self.animation {
            let param = info
                .params
                .iter()
                .find(|p| p.id == id)
                .context("unknown DSP animation parameter")?;
            let sampled = match curve {
                super::composition::AnimatableValue::Constant { value } => *value,
                super::composition::AnimatableValue::Keyframes { track } => {
                    track.sample_seconds(seconds)?
                }
            };
            ensure!(sampled.is_finite(), "nonfinite DSP animation sample");
            values.insert(id.clone(), param.sanitize(sampled as f32));
        }
        Ok(values)
    }
}
enum HostedEffect { Spectral(super::concat_enhance_voice::Stream), Native(Box<dyn AudioEffect>), Concat(super::concat_audio::Reflections), Distorted(super::concat_distorted::Distorted), Tone(super::concat_tone::Tone) }
impl std::ops::Deref for HostedEffect {
    type Target=dyn AudioEffect;
    fn deref(&self)->&Self::Target{match self{Self::Spectral(effect)=>effect,Self::Native(effect)=>effect.as_ref(),Self::Concat(effect)=>effect,Self::Distorted(effect)=>effect,Self::Tone(effect)=>effect}}
}
impl std::ops::DerefMut for HostedEffect {
    fn deref_mut(&mut self)->&mut Self::Target{match self{Self::Spectral(effect)=>effect,Self::Native(effect)=>effect.as_mut(),Self::Concat(effect)=>effect,Self::Distorted(effect)=>effect,Self::Tone(effect)=>effect}}
}
pub struct Chain {
    effects: Vec<HostedEffect>,
    input_cursor:i64,
    source_bounds:Option<(i64,i64)>,
    channels: usize,
    latency: usize,
    specs: Vec<EffectSpec>,
    slots: Vec<Option<usize>>,
    parameter_order: Vec<Vec<String>>,
}
impl Chain {
    pub fn new(sample_rate: u32, channels: usize, specs: &[EffectSpec]) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&sample_rate)
                && (1..=16).contains(&channels)
                && specs.len() <= 16,
            "invalid DSP chain budget"
        );
        validate_specs(specs)?;
        let native_specs = specs
            .iter()
            .map(|spec| {
                if spec
                    .id
                    .starts_with(super::filmcraft_audio_project::PROJECT_PREFIX)
                {
                    let mut static_spec = spec.clone();
                    static_spec.animation.clear();
                    static_spec.native_at_seconds(0.0, sample_rate)
                } else {
                    Ok(spec.clone())
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let parameter_order: Vec<Vec<String>> = specs
            .iter()
            .zip(&native_specs)
            .map(|(authored, native)| {
                let direct = authored
                    .id
                    .strip_prefix(super::filmcraft_audio_project::PROJECT_PREFIX)
                    .filter(|id| {
                        super::filmcraft_audio_mapping::DIRECT_PROJECT_EFFECTS
                            .iter()
                            .any(|(project, dsp, _)| project == id && *dsp == native.id)
                    });
                direct
                    .map(|id| {
                        super::filmcraft_audio_project::catalog()["effects"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|definition| definition["id"] == id)
                            .unwrap()["params"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|parameter| parameter["id"].as_str().unwrap().to_owned())
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect();
        let mut effects = Vec::new();
        let mut latency = 0usize;
        let mut slots = Vec::with_capacity(specs.len());
        for (slot, spec) in native_specs.iter().enumerate() {
            if spec.bypass {
                slots.push(None);
                continue;
            }
            slots.push(Some(effects.len()));
            let mut effect=if spec.id=="concat.denoise"{HostedEffect::Spectral(super::concat_enhance_voice::Stream::denoise(sample_rate,channels))}else if spec.id=="concat.enhance-voice"{HostedEffect::Spectral(super::concat_enhance_voice::Stream::new(sample_rate,channels))}else if let Some(kind)=super::concat_tone::kind(&spec.id){HostedEffect::Tone(super::concat_tone::Tone::new(kind,sample_rate,channels))}else if spec.id=="concat.distorted"{HostedEffect::Distorted(super::concat_distorted::Distorted::new(sample_rate,channels))}else if let Some(effect)=super::concat_audio::create_runtime(&spec.id,sample_rate,channels){HostedEffect::Concat(effect)}else{HostedEffect::Native(create_effect(&spec.id,sample_rate as f32,channels).context("DSP factory unavailable")?)};
            // Direct source dispatch follows definition order and skips unchanged targets.
            // Setter order affects coupled smoothing state, even with identical final targets.
            let project_id = specs[slot]
                .id
                .strip_prefix(super::filmcraft_audio_project::PROJECT_PREFIX);
            let direct = project_id.filter(|id| {
                super::filmcraft_audio_mapping::DIRECT_PROJECT_EFFECTS
                    .iter()
                    .any(|(project, native, _)| project == id && *native == spec.id)
            });
            if let Some(id) = direct {
                let definition = super::filmcraft_audio_project::catalog()["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|definition| definition["id"] == id)
                    .unwrap();
                for parameter in definition["params"].as_array().unwrap() {
                    let name = parameter["id"].as_str().unwrap();
                    if let Some(value) = spec.params.get(name) {
                        if effect.param(name) != Some(*value) {
                            ensure!(effect.set_param(name, *value), "DSP parameter not accepted");
                        }
                    }
                }
            } else {
                for (id, value) in &spec.params {
                    ensure!(effect.set_param(id, *value), "DSP parameter not accepted");
                }
            }
            latency = latency
                .checked_add(effect.latency())
                .context("DSP latency overflow")?;
            effects.push(effect);
        }
        Ok(Self {
            effects,
            input_cursor:0,
            source_bounds:None,
            channels,
            latency,
            specs: native_specs,
            slots,
            parameter_order,
        })
    }
    /// Declare canonical source PCM frame bounds before the first processed sample.
    pub fn set_input_frames(&mut self,frames:usize)->Result<()>{
        ensure!(self.input_cursor==0,"source frame bounds must precede processing");
        self.configure_frame_flow(0,i64::try_from(frames)?,0)
    }
    fn configure_frame_flow(&mut self,origin:i64,end:i64,cursor:i64)->Result<()>{
        ensure!(end>=origin&&end.checked_add(192000*28).is_some(),"invalid source frame bounds");
        self.source_bounds=Some((origin,end));self.input_cursor=cursor;
        let mut first=origin;let mut next=end;let mut odd=if (end-origin)%2!=0{vec![end]}else{vec![]};
        for effect in &mut self.effects{match effect{
            HostedEffect::Spectral(effect)=>{(next,odd)=effect.frame_flow(first,next,&odd,cursor);first+=effect.latency() as i64;},
            HostedEffect::Concat(effect)=>{(next,odd)=effect.frame_flow(next,&odd,cursor)?;},
            HostedEffect::Distorted(effect)=>{effect.frame_end(next,cursor);},
            HostedEffect::Tone(effect)=>{effect.frame_flow(next,&odd,cursor);},
            HostedEffect::Native(effect)=>{let delay=i64::try_from(effect.latency())?;first=first.checked_add(delay).context("DSP frame origin overflow")?;next=next.checked_add(delay).context("DSP frame latency overflow")?;for end in &mut odd{*end=end.checked_add(delay).context("DSP frame latency overflow")?;}},
        }}
        Ok(())
    }
    /// Caller owns preroll/look-ahead and must compensate this latency when placing audio.
    pub fn latency_samples(&self) -> usize {
        self.latency
    }
    /// Apply an authored parameter batch between blocks, preserving native smoothing/state.
    /// Indices refer to authored slots, including bypassed slots.
    pub fn update_params(&mut self, slot: usize, params: &BTreeMap<String, f32>) -> Result<()> {
        let spec = self.specs.get(slot).context("unknown DSP slot")?;
        ensure!(params.len() <= 128, "DSP parameter batch exceeds budget");
        let info = effect_info(&spec.id).context("unknown DSP effect")?;
        // Reject the whole batch before any setter mutates recursive state.
        for (id, value) in params {
            let param = info
                .params
                .iter()
                .find(|p| p.id == id)
                .context("unknown DSP parameter")?;
            ensure!(
                value.is_finite() && *value >= param.min && *value <= param.max,
                "DSP parameter out of range"
            );
            ensure!(
                !matches!(param.unit, Unit::Toggle | Unit::Choice) || value.fract() == 0.0,
                "DSP choice must be integral"
            );
        }
        if let Some(index) = self.slots[slot] {
            let order = &self.parameter_order[slot];
            let ordered = order
                .iter()
                .filter_map(|id| params.get_key_value(id))
                .chain(params.iter().filter(|(id, _)| !order.contains(id)));
            for (id, value) in ordered {
                if self.effects[index].param(id) != Some(*value) {
                    ensure!(
                        self.effects[index].set_param(id, *value),
                        "DSP parameter not accepted"
                    );
                }
            }
            self.latency = self.effects.iter().try_fold(0usize, |sum, e| {
                sum.checked_add(e.latency()).context("DSP latency overflow")
            })?;
        }
        self.specs[slot]
            .params
            .extend(params.iter().map(|(k, v)| (k.clone(), *v)));
        if let Some((origin,end))=self.source_bounds{self.configure_frame_flow(origin,end,self.input_cursor)?;}
        Ok(())
    }
    pub fn reset(&mut self) {
        self.input_cursor=self.source_bounds.map_or(0,|b|b.0);
        for effect in &mut self.effects {
            effect.reset();match effect{HostedEffect::Spectral(effect)=>effect.set_cursor(self.input_cursor),HostedEffect::Concat(effect)=>effect.set_cursor(self.input_cursor),HostedEffect::Distorted(effect)=>effect.set_cursor(self.input_cursor),HostedEffect::Tone(effect)=>effect.set_cursor(self.input_cursor),_=>{}}
        }
    }
    pub fn process(
        &mut self,
        audio: &mut [&mut [f32]],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.process_inner(audio, 0, self.input_cursor, &mut |_, _| Ok(()), checkpoint)
    }
    /// Native source absolute256-sample control grid, independent of request cuts.
    pub fn process_at(
        &mut self,
        audio: &mut [&mut [f32]],
        start: i64,
        update: &mut dyn FnMut(i64, &mut Chain) -> Result<()>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.process_inner(audio,start,start,update,checkpoint)
    }
    fn process_inner(&mut self,audio:&mut [&mut [f32]],start:i64,input_clock:i64,update:&mut dyn FnMut(i64,&mut Chain)->Result<()>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        checkpoint()?;
        ensure!(audio.len() == self.channels, "DSP channel mismatch");
        let frames = audio.first().map_or(0, |c| c.len());
        ensure!(
            frames <= 65536
                && audio
                    .iter()
                    .all(|c| c.len() == frames && c.iter().all(|v| v.is_finite())),
            "invalid DSP PCM block"
        );
        start
            .checked_add(frames as i64)
            .context("DSP processing clock overflow")?;
        input_clock.checked_add(frames as i64).context("DSP input clock overflow")?;
        for effect in &mut self.effects{match effect{HostedEffect::Spectral(effect)=>effect.set_cursor(input_clock),HostedEffect::Concat(effect)=>effect.set_cursor(input_clock),HostedEffect::Distorted(effect)=>effect.set_cursor(input_clock),HostedEffect::Tone(effect)=>effect.set_cursor(input_clock),_=>{}}}
        let mut offset = 0usize;
        while offset < frames {
            let position = start + offset as i64;
            let remainder = position.rem_euclid(256);
            let grid = position - remainder;
            let end = (offset + (256 - remainder) as usize).min(frames);
            checkpoint()?;
            self.input_cursor=input_clock+offset as i64;
            update(grid, self)?;
            let mut scratch: [&mut [f32]; 16] = std::array::from_fn(|_| &mut [] as &mut [f32]);
            for (dest, source) in scratch.iter_mut().zip(audio.iter_mut()) {
                *dest = &mut source[offset..end];
            }
            let slices = &mut scratch[..self.channels];
            let mut first=0;
            while first<self.effects.len(){
                let mut last=first;
                while last<self.effects.len() && !matches!(self.effects[last],HostedEffect::Native(_)){last+=1;}
                if last>first && self.effects[first..last].iter().any(|e|matches!(e,HostedEffect::Tone(_)|HostedEffect::Distorted(_)|HostedEffect::Spectral(_))){
                    for frame in 0..end-offset{
                        if frame%64==0{checkpoint()?;}
                        let mut sample=[0f64;16];
                        for ch in 0..self.channels{sample[ch]=f64::from(slices[ch][frame]);}
                        let mut double=self.effects[first..last].iter().take_while(|e|!matches!(e,HostedEffect::Tone(_)|HostedEffect::Distorted(_))).any(|e|matches!(e,HostedEffect::Spectral(effect) if effect.requires_double()));
                        let clock=input_clock+offset as i64+frame as i64;
                        let mut stage_origin=self.source_bounds.map_or(0,|b|b.0)+self.effects[..first].iter().map(|e|e.latency() as i64).sum::<i64>();
                        for effect in &mut self.effects[first..last]{
                            let delay=effect.latency() as i64;
                            if clock<stage_origin {sample[..self.channels].fill(0.);stage_origin+=delay;continue;}
                            match effect{HostedEffect::Spectral(e)=>e.set_cursor(clock),HostedEffect::Tone(e)=>e.set_cursor(clock),HostedEffect::Distorted(e)=>e.set_cursor(clock),HostedEffect::Concat(e)=>e.set_cursor(clock),_=>{}}
                            match effect{
                            HostedEffect::Spectral(effect)=>{effect.process_frame(&mut sample[..self.channels],double);double|=effect.requires_double();},
                            HostedEffect::Tone(effect)=>{effect.process_frame(&mut sample[..self.channels],double);double=true;},
                            HostedEffect::Distorted(effect)=>{effect.process_frame(&mut sample[..self.channels]);double=true;},
                            HostedEffect::Concat(effect)=>{
                                if double {effect.process_frame_double(&mut sample[..self.channels]);continue;}
                                let mut floats=[0f32;16];for ch in 0..self.channels{floats[ch]=sample[ch] as f32;}
                                let mut refs:[&mut[f32];16]=std::array::from_fn(|_|&mut[]as &mut[f32]);
                                for (out,value) in refs.iter_mut().zip(floats.iter_mut()).take(self.channels){*out=std::slice::from_mut(value);}
                                effect.process(&mut refs[..self.channels]);
                                for ch in 0..self.channels{sample[ch]=f64::from(floats[ch]);}double=false;
                            },HostedEffect::Native(_)=>unreachable!(),
                        }stage_origin+=delay;}
                        for ch in 0..self.channels{slices[ch][frame]=sample[ch] as f32;}
                    }
                    first=last;
                }else{checkpoint()?;self.effects[first].process(slices);first+=1;}
            }
            ensure!(
                slices.iter().all(|c| c.iter().all(|v| v.is_finite())),
                "DSP produced nonfinite PCM"
            );
            offset = end;
        }
        self.input_cursor=input_clock+frames as i64;
        checkpoint()
    }
}
/// Sequential timeline reader with source-style look-ahead and seek preroll.
/// The reader supplies silence outside the clip and owns decoding/conform.
pub struct TimelineChain {
    chain: Chain,
    specs: Vec<EffectSpec>,
    rate: u32,
    channels: usize,
    preroll: usize,
    next: Option<i64>,
    input_bounds:Option<(i64,i64)>,
    full_history:bool,
}
impl TimelineChain {
    pub fn new(rate: u32, channels: usize, specs: &[EffectSpec], preroll: usize) -> Result<Self> {
        let full_history=super::concat_audio::requires_full_history(specs,rate);
        let history_budget = if full_history {6*60*60}else if specs.iter().any(|s| s.id.starts_with("concat.")) { 28 } else { 8 };
        ensure!(preroll <= rate as usize * history_budget, "DSP preroll exceeds budget");
        Ok(Self {
            chain: Chain::new(rate, channels, specs)?,
            specs: specs.to_vec(),
            rate,
            channels,
            preroll,
            next: None,
            input_bounds:None,
            full_history,
        })
    }
    pub fn with_input_bounds(mut self,origin:i64,end:i64)->Result<Self>{ensure!(end>=origin,"invalid DSP input bounds");self.input_bounds=Some((origin,end));self.next=None;Ok(self)}
    pub fn render(
        &mut self,
        start: i64,
        frames: usize,
        read: &mut dyn FnMut(i64, usize) -> Result<Vec<Vec<f32>>>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_automated(start, frames, read, &mut |_, _| Ok(()), checkpoint)
    }
    /// Resolve authored automation at the source absolute input grid, including preroll.
    pub fn render_automated(
        &mut self,
        start: i64,
        frames: usize,
        read: &mut dyn FnMut(i64, usize) -> Result<Vec<Vec<f32>>>,
        update: &mut dyn FnMut(i64, &mut Chain) -> Result<()>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(frames <= 65536, "DSP timeline block exceeds budget");
        let end = start
            .checked_add(frames as i64)
            .context("DSP timeline overflow")?;
        let contiguous = self.next == Some(start);
        // Reconstruct instead of resetting: reset jumps parameter smoothing to its target,
        // whereas a source seek starts a fresh factory with native initial smoothing.
        if !contiguous {
            self.chain = Chain::new(self.rate, self.channels, &self.specs)?;
        }
        let skip = if contiguous {0}else if self.full_history{
            let origin=self.input_bounds.map_or(0,|b|b.0);
            let age=start.checked_sub(origin).context("DSP source age overflow")?.max(0) as usize;
            // With canonical source bounds, replay begins at the source origin.
            // Extra silence before that origin advances time-varying modulation.
            let replay=age.checked_add(self.chain.latency_samples()).context("DSP full history overflow")?;
            let needed=if self.input_bounds.is_some(){replay}else{replay.max(self.preroll)};
            ensure!(needed<=self.rate as usize*6*60*60+self.chain.latency_samples(),"full-history DSP seek exceeds six-hour budget");needed
        }else{self.preroll};
        let input_start = start
            .checked_sub(skip as i64)
            .and_then(|v| v.checked_add(self.chain.latency_samples() as i64))
            .context("DSP look-ahead overflow")?;
        input_start
            .checked_add((skip + frames) as i64)
            .context("DSP input clock overflow")?;
        self.next = None; // Failed reads/cancellation cannot leave reusable recursive state.
        if !contiguous{if let Some((origin,end))=self.input_bounds{self.chain.configure_frame_flow(origin,end,input_start)?;}}
        let mut output: Vec<Vec<f32>> = (0..self.channels)
            .map(|_| Vec::with_capacity(frames))
            .collect();
        let mut offset = 0usize;
        while offset < skip + frames {
            checkpoint()?;
            let count = (skip + frames - offset).min(65536);
            let mut input = read(input_start + offset as i64, count)?;
            ensure!(
                input.len() == self.channels && input.iter().all(|c| c.len() == count),
                "DSP reader shape mismatch"
            );
            let mut slices: Vec<_> = input.iter_mut().map(Vec::as_mut_slice).collect();
            self.chain
                .process_at(&mut slices, input_start + offset as i64, update, checkpoint)?;
            let discard = skip.saturating_sub(offset).min(count);
            for (dst, src) in output.iter_mut().zip(input.iter()) {
                dst.extend_from_slice(&src[discard..]);
            }
            offset += count;
        }
        checkpoint()?;
        self.next = Some(end);
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_channel_mixer_updates_preserve_source_setter_order() {
        let spec = EffectSpec { concat_source_keys: Default::default(),
            id: "filmcraft:channel_mixer_a".into(),
            ..Default::default()
        };
        let mut chain = Chain::new(48000, 2, &[spec]).unwrap();
        let mut reference =
            filmcraft_audio_dsp::create_effect("channel_mixer", 48000.0, 2).unwrap();
        let params = BTreeMap::from([
            ("l_from_l".into(), 0.0),
            ("l_from_r".into(), 0.0),
            ("r_from_l".into(), 0.0),
            ("r_from_r".into(), 0.0),
            ("invert_l".into(), 1.0),
            ("invert_r".into(), 1.0),
        ]);
        chain.update_params(0, &params).unwrap();
        // Original direct dispatcher iterates source definition order, not map order.
        for id in [
            "l_from_l", "l_from_r", "r_from_l", "r_from_r", "invert_l", "invert_r",
        ] {
            let value = params[id];
            if reference.param(id) != Some(value) {
                assert!(reference.set_param(id, value));
            }
        }
        let mut actual = [vec![0.125; 8192], vec![-0.25; 8192]];
        let mut expected = actual.clone();
        let [left, right] = &mut actual;
        chain.process(&mut [left, right], &mut || Ok(())).unwrap();
        let [left, right] = &mut expected;
        reference.process(&mut [left, right]);
        for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
    #[test]
    fn authored_parameter_curves_validate_sample_and_serialize() {
        let spec: EffectSpec = serde_json::from_value(serde_json::json!({"id":"amplify","params":{"gain":12.0},"animation":{"gain":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":-6.0},{"tick":1000000,"value":0.0}]}}}})).unwrap();
        validate_specs(std::slice::from_ref(&spec)).unwrap();
        assert_eq!(spec.params_at(500000).unwrap()["gain"], -3.0);
        assert_eq!(spec.params_at(2000000).unwrap()["gain"], 0.0);
        let restored: EffectSpec =
            serde_json::from_value(serde_json::to_value(&spec).unwrap()).unwrap();
        assert_eq!(restored, spec);
        let mut invalid = spec;
        invalid.animation.insert(
            "gain".into(),
            super::super::composition::AnimatableValue::constant(25.0),
        );
        assert!(validate_specs(&[invalid]).is_err());
    }
    #[test]
    fn timeline_automation_includes_preroll_and_compensated_input_clock() {
        let spec = EffectSpec { concat_source_keys: Default::default(),
            id: "fft_filter".into(),
            ..Default::default()
        };
        let mut timeline = TimelineChain::new(48000, 1, &[spec], 300).unwrap();
        let mut grids = Vec::new();
        let mut update = |grid: i64, _: &mut Chain| {
            grids.push(grid);
            Ok(())
        };
        let output = timeline
            .render_automated(
                100,
                600,
                &mut |_, n| Ok(vec![vec![0.0; n]]),
                &mut update,
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(output[0].len(), 600);
        assert_eq!(grids, vec![1792, 2048, 2304, 2560]); //100-300+2048 input start.
        grids.clear();
        let mut update = |grid: i64, _: &mut Chain| {
            grids.push(grid);
            Ok(())
        };
        timeline
            .render_automated(
                700,
                100,
                &mut |_, n| Ok(vec![vec![0.0; n]]),
                &mut update,
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(grids, vec![2560, 2816]);
    }
    #[test]
    fn absolute_parameter_grid_is_request_cut_invariant() {
        let spec = EffectSpec { concat_source_keys: Default::default(),
            id: "amplify".into(),
            ..Default::default()
        };
        let mut whole = Chain::new(48000, 1, &[spec.clone()]).unwrap();
        let mut split = Chain::new(48000, 1, &[spec]).unwrap();
        let mut a = vec![0.5; 600];
        let mut b = a.clone();
        let mut updates = Vec::new();
        let mut apply = |grid: i64, chain: &mut Chain| {
            updates.push(grid);
            chain.update_params(
                0,
                &BTreeMap::from([("gain".into(), if grid < 256 { -6.0 } else { 3.0 })]),
            )
        };
        whole
            .process_at(&mut [&mut a], -10, &mut apply, &mut || Ok(()))
            .unwrap();
        assert_eq!(updates, vec![-256, 0, 256, 512]);
        updates.clear();
        let mut apply = |grid: i64, chain: &mut Chain| {
            chain.update_params(
                0,
                &BTreeMap::from([("gain".into(), if grid < 256 { -6.0 } else { 3.0 })]),
            )
        };
        split
            .process_at(&mut [&mut b[..257]], -10, &mut apply, &mut || Ok(()))
            .unwrap();
        split
            .process_at(&mut [&mut b[257..]], 247, &mut apply, &mut || Ok(()))
            .unwrap();
        assert_eq!(a, b);
    }
    #[test]
    fn audio_clip_serialization_preserves_chain_and_legacy_defaults() {
        use super::super::composition::AudioClip;
        let legacy = serde_json::json!({"id":"clip","sourceId":"source","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":48000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0});
        let old: AudioClip = serde_json::from_value(legacy.clone()).unwrap();
        assert!(old.dsp_effects.is_empty());
        assert!(
            serde_json::to_value(old)
                .unwrap()
                .get("dspEffects")
                .is_none()
        );
        let mut document = legacy;
        document["dspEffects"] = serde_json::json!([{"id":"amplify","params":{"gain":-6.0}},{"id":"invert","bypass":true}]);
        let clip: AudioClip = serde_json::from_value(document).unwrap();
        validate_specs(&clip.dsp_effects).unwrap();
        let restored: AudioClip =
            serde_json::from_value(serde_json::to_value(&clip).unwrap()).unwrap();
        assert_eq!(clip, restored);
        let mut invalid = restored.dsp_effects;
        invalid[0].params.insert("gain".into(), 25.0);
        assert!(validate_specs(&invalid).is_err());
    }
    #[test]
    fn parameter_batches_preserve_slots_and_native_smoothing() {
        let specs = [
            EffectSpec { concat_source_keys: Default::default(),
                id: "invert".into(),
                bypass: true,
                animation: BTreeMap::new(),
                ..Default::default()
            },
            EffectSpec { concat_source_keys: Default::default(),
                id: "amplify".into(),
                ..Default::default()
            },
        ];
        let mut chain = Chain::new(48000, 1, &specs).unwrap();
        let mut reference = filmcraft_audio_dsp::create_effect("amplify", 48000.0, 1).unwrap();
        assert!(chain.update_params(2, &BTreeMap::new()).is_err());
        let invalid = BTreeMap::from([("gain".into(), -6.0), ("unknown".into(), 1.0)]);
        assert!(chain.update_params(1, &invalid).is_err());
        assert_eq!(chain.specs[1].params.len(), 0);
        for gain in [-6.0, 3.0, -12.0] {
            chain
                .update_params(1, &BTreeMap::from([("gain".into(), gain)]))
                .unwrap();
            assert!(reference.set_param("gain", gain));
            let mut actual = vec![0.5; 777];
            let mut expected = actual.clone();
            chain.process(&mut [&mut actual], &mut || Ok(())).unwrap();
            reference.process(&mut [&mut expected]);
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn timeline_lookahead_seek_and_failure_state() {
        let specs = [EffectSpec { concat_source_keys: Default::default(),
            id: "pitch_shifter".into(),
            ..Default::default()
        }];
        let latency = Chain::new(48000, 1, &specs).unwrap().latency_samples() as i64;
        let mut chain = TimelineChain::new(48000, 1, &specs, 100).unwrap();
        let mut requests = Vec::new();
        let mut reader = |start, count| {
            requests.push((start, count));
            Ok(vec![vec![0.0; count]])
        };
        assert_eq!(
            chain.render(0, 20, &mut reader, &mut || Ok(())).unwrap()[0].len(),
            20
        );
        chain.render(20, 20, &mut reader, &mut || Ok(())).unwrap();
        chain.render(5, 20, &mut reader, &mut || Ok(())).unwrap();
        assert_eq!(
            requests,
            vec![
                (latency - 100, 120),
                (latency + 20, 20),
                (latency - 95, 120)
            ]
        );
        assert!(
            chain
                .render(25, 20, &mut |_, _| Ok(vec![vec![0.0; 1]]), &mut || Ok(()))
                .is_err()
        );
        assert_eq!(chain.next, None);
        assert!(
            chain
                .render(i64::MAX, 1, &mut |_, _| unreachable!(), &mut || Ok(()))
                .is_err()
        );
    }
    #[test]
    fn admission_is_strict_and_bypass_has_no_latency() {
        for spec in [
            EffectSpec { concat_source_keys: Default::default(),
                id: "unknown".into(),
                ..Default::default()
            },
            EffectSpec { concat_source_keys: Default::default(),
                id: "amplify".into(),
                params: BTreeMap::from([("gain".into(), f32::NAN)]),
                bypass: false,
                animation: BTreeMap::new(),
            },
            EffectSpec { concat_source_keys: Default::default(),
                id: "amplify".into(),
                params: BTreeMap::from([("gain".into(), 25.0)]),
                bypass: false,
                animation: BTreeMap::new(),
            },
        ] {
            assert!(Chain::new(48000, 2, &[spec]).is_err())
        }
        let bypass = EffectSpec { concat_source_keys: Default::default(),
            id: "pitch_shifter".into(),
            bypass: true,
            animation: BTreeMap::new(),
            ..Default::default()
        };
        assert_eq!(
            Chain::new(48000, 2, &[bypass]).unwrap().latency_samples(),
            0
        );
        assert!(
            Chain::new(
                48000,
                2,
                &[EffectSpec { concat_source_keys: Default::default(),
                    id: "pitch_shifter".into(),
                    ..Default::default()
                }]
            )
            .unwrap()
            .latency_samples()
                > 0
        );
    }
    #[test]
    fn native_gain_is_streaming_and_checkpoints_cancel() {
        let spec = EffectSpec { concat_source_keys: Default::default(),
            id: "amplify".into(),
            params: BTreeMap::from([("gain".into(), -6.0)]),
            bypass: false,
            animation: BTreeMap::new(),
        };
        let mut a = Chain::new(48000, 1, &[spec.clone()]).unwrap();
        let mut b = Chain::new(48000, 1, &[spec]).unwrap();
        a.reset();
        b.reset();
        let mut whole = vec![1.0; 1000];
        a.process(&mut [&mut whole], &mut || Ok(())).unwrap();
        let mut split = vec![1.0; 1000];
        for block in split.chunks_mut(37) {
            b.process(&mut [block], &mut || Ok(())).unwrap()
        }
        assert_eq!(whole, split);
        assert!((whole[999] - 10.0f32.powf(-6.0 / 20.0)).abs() < 1e-6);
        assert!(
            a.process(&mut [&mut whole], &mut || anyhow::bail!("cancelled"))
                .is_err()
        );
    }
}
