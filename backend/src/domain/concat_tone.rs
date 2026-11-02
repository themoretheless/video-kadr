//! Static owned Concat tone/voice biquads followed by the linked limiter.
use super::{
    concat_limiter::LookaheadLimiter, concat_log_crusher::LogCrusher, concat_tone_filter::Shelf,
};
use filmcraft_audio_dsp::{AudioEffect, Category, EffectInfo, ParamSpec, Unit};
const BASS_PARAMS: &[ParamSpec] = &[ParamSpec::new("gain", "Gain", 0., 12., 5., Unit::Decibels)];
const TREBLE_PARAMS: &[ParamSpec] = &[ParamSpec::new("gain", "Gain", 0., 12., 4., Unit::Decibels)];
const DEPTH_PARAMS: &[ParamSpec] = &[ParamSpec::new("depth", "Depth", 1., 10., 5., Unit::None)];
const TELEPHONE_PARAMS: &[ParamSpec] = &[ParamSpec::new("drive", "Drive", 0., 10., 3., Unit::None)];
const MEGAPHONE_PARAMS: &[ParamSpec] = &[ParamSpec::new("drive", "Drive", 1., 10., 5., Unit::None)];
const WARMTH_PARAMS: &[ParamSpec] = &[ParamSpec::new("warmth", "Warmth", 0., 10., 5., Unit::None)];
const AIR_PARAMS: &[ParamSpec] = &[ParamSpec::new("air", "Air", 1., 10., 5., Unit::None)];
const NOISE_PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "noise",
    "Noise",
    0.,
    100.,
    30.,
    Unit::Percent,
)];
const BITS_PARAMS: &[ParamSpec] = &[ParamSpec::new("bits", "Bits", 4., 12., 8., Unit::None)];
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Bass,
    Treble,
    Bright,
    Underwater,
    Telephone,
    Megaphone,
    Vinyl,
    Whisper,
    Radio,
    Lofi,
    Robot,
}
fn fixed(value: f64, digits: usize) -> f64 {
    format!("{value:.digits$}")
        .parse()
        .expect("finite validated parameter")
}
#[derive(Clone,Copy)]
pub(crate) struct SourceBiquad {pub design:super::concat_biquad_stream::Design,pub frequency:f64,pub width:f64,pub gain:f64}
#[derive(Clone,Copy)]
pub(crate) struct SourceCrusher {pub bits:f64,pub mix:f64}
pub(crate) struct SourceToneRecipe {pub filters:Vec<SourceBiquad>,pub crusher:Option<SourceCrusher>,pub modulation:Option<f64>,pub limit:f64}
pub(crate) fn source_tone_recipe(spec:&super::filmcraft_dsp::EffectSpec)->anyhow::Result<SourceToneRecipe>{
    let kind=kind(&spec.id).ok_or_else(||anyhow::anyhow!("unknown source tone effect"))?;
    let param=&kind.params()[0];let value=f64::from(spec.params.get(param.id).copied().unwrap_or(param.default));
    Ok(SourceToneRecipe{filters:kind.source_biquads(value).ok_or_else(||anyhow::anyhow!("source tone needs additional packet stages"))?,crusher:kind.source_crusher(value),modulation:kind.modulation(value),limit:kind.limit()})
}
impl Kind {
    fn id(self) -> &'static str {
        match self {
            Self::Bass => "concat.bass",
            Self::Treble => "concat.treble",
            Self::Bright => "concat.bright",
            Self::Underwater => "concat.underwater",
            Self::Telephone => "concat.telephone",
            Self::Megaphone => "concat.megaphone",
            Self::Vinyl => "concat.vinyl",
            Self::Whisper => "concat.whisper",
            Self::Radio => "concat.radio",
            Self::Lofi => "concat.lofi",
            Self::Robot => "concat.robot",
        }
    }
    fn params(self) -> &'static [ParamSpec] {
        match self {
            Self::Bass | Self::Bright => BASS_PARAMS,
            Self::Treble => TREBLE_PARAMS,
            Self::Underwater | Self::Robot => DEPTH_PARAMS,
            Self::Telephone => TELEPHONE_PARAMS,
            Self::Megaphone => MEGAPHONE_PARAMS,
            Self::Vinyl => WARMTH_PARAMS,
            Self::Whisper => AIR_PARAMS,
            Self::Radio => NOISE_PARAMS,
            Self::Lofi => BITS_PARAMS,
        }
    }
    fn limit(self) -> f64 {
        match self {
            Self::Underwater | Self::Telephone | Self::Whisper => 0.92,
            Self::Megaphone => 0.88,
            Self::Robot => 0.94,
            Self::Radio | Self::Lofi => 0.9,
            _ => 0.95,
        }
    }
    fn source_biquads(self,value:f64)->Option<Vec<SourceBiquad>>{
        use super::concat_biquad_stream::Design;
        let make=|design,frequency,width,gain|SourceBiquad{design,frequency,width,gain};
        Some(match self{
            Self::Bass=>vec![make(Design::LowShelf,110.,0.6,fixed(value,2))],
            Self::Treble=>vec![make(Design::HighShelf,9000.,0.7,fixed(value,2))],
            Self::Bright=>vec![make(Design::HighShelf,6000.,0.8,fixed(value,2))],
            Self::Underwater=>vec![make(Design::Lowpass,fixed(800.-value*40.,1),0.707,0.),make(Design::Peak,200.,2.,fixed(2.+value*0.3,2))],
            Self::Telephone=>vec![make(Design::Highpass,400.,0.707,0.),make(Design::Lowpass,3400.,0.707,0.),make(Design::Peak,1600.,1.4,fixed(3.+value/2.,2))],
            Self::Megaphone=>vec![make(Design::Highpass,550.,0.707,0.),make(Design::Lowpass,2800.,0.707,0.),make(Design::Peak,1600.,2.,fixed(3.+value*0.8,2))],
            Self::Vinyl=>vec![make(Design::Highpass,250.,0.707,0.),make(Design::Lowpass,5000.,0.707,0.),make(Design::Peak,1200.,1.5,fixed(2.+value*0.4,2))],
            Self::Whisper=>vec![make(Design::Highpass,500.,0.707,0.),make(Design::Peak,3200.,1.4,fixed(2.+value*0.6,2)),make(Design::Peak,7500.,1.2,fixed(1.+value*0.5,2))],
            Self::Radio=>vec![make(Design::Highpass,300.,0.707,0.),make(Design::Lowpass,3400.,0.707,0.)],
            Self::Lofi=>vec![make(Design::Lowpass,3200.,0.707,0.)],
            Self::Robot=>vec![make(Design::Peak,1800.,1.2,3.)],
        })
    }
    fn filters(self, rate: u32, channels: usize, value: f64) -> Vec<Shelf> {
        if let Some(filters)=self.source_biquads(value){
            use super::concat_biquad_stream::Design;
            return filters.into_iter().filter_map(|f|match f.design{
                Design::LowShelf=>Shelf::new(rate,channels,f.frequency,f.width,f.gain,false),
                Design::HighShelf=>Shelf::new(rate,channels,f.frequency,f.width,f.gain,true),
                Design::Lowpass=>Shelf::pass_with_width(rate,channels,f.frequency,f.width,false),
                Design::Peak=>Shelf::peaking(rate,channels,f.frequency,f.width,f.gain),
                Design::Highpass=>Shelf::pass_with_width(rate,channels,f.frequency,f.width,true),
            }).collect();
        }

        unreachable!("all tone biquads use shared recipes")
    }
}
impl Kind {
    fn modulation(self,value:f64)->Option<f64>{matches!(self,Self::Robot).then(||fixed(0.4+value/10.*1.6,2))}
    fn source_crusher(self,value:f64)->Option<SourceCrusher>{
        match self {
            Self::Radio=>Some(SourceCrusher{bits:(16.-value/100.*12.).round(),mix:0.6}),
            Self::Lofi=>Some(SourceCrusher{bits:value.round(),mix:0.8}),
            _=>None,
        }
    }
    fn crusher(self, value: f64) -> Option<LogCrusher> {
        self.source_crusher(value).map(|recipe|LogCrusher::new(recipe.bits,recipe.mix))
    }
}
pub(crate) fn step(id: &str) -> Option<f64> {
    kind(id).map(|kind| {
        if matches!(kind, Kind::Radio | Kind::Lofi) {
            1.
        } else {
            0.5
        }
    })
}
macro_rules! info {
    ($kind:ident,$id:literal,$name:literal,$params:ident) => {
        EffectInfo {
            id: $id,
            name: $name,
            category: Category::Filter,
            params: $params,
            create: |rate, ch| Box::new(Tone::new(Kind::$kind, rate as u32, ch)),
        }
    };
}
pub(crate) const BASS: EffectInfo = info!(Bass, "concat.bass", "Concat Bass", BASS_PARAMS);
pub(crate) const TREBLE: EffectInfo =
    info!(Treble, "concat.treble", "Concat Treble", TREBLE_PARAMS);
pub(crate) const BRIGHT: EffectInfo = info!(Bright, "concat.bright", "Concat Bright", BASS_PARAMS);
pub(crate) const UNDERWATER: EffectInfo = info!(
    Underwater,
    "concat.underwater", "Concat Underwater", DEPTH_PARAMS
);
pub(crate) const TELEPHONE: EffectInfo = info!(
    Telephone,
    "concat.telephone", "Concat Telephone", TELEPHONE_PARAMS
);
pub(crate) const MEGAPHONE: EffectInfo = info!(
    Megaphone,
    "concat.megaphone", "Concat Megaphone", MEGAPHONE_PARAMS
);
pub(crate) const VINYL: EffectInfo = info!(Vinyl, "concat.vinyl", "Concat Vinyl", WARMTH_PARAMS);
pub(crate) const WHISPER: EffectInfo =
    info!(Whisper, "concat.whisper", "Concat Whisper", AIR_PARAMS);
pub(crate) const RADIO: EffectInfo = info!(Radio, "concat.radio", "Concat Radio", NOISE_PARAMS);
pub(crate) const ROBOT: EffectInfo = info!(Robot, "concat.robot", "Concat Robot", DEPTH_PARAMS);
pub(crate) const LOFI: EffectInfo = info!(Lofi, "concat.lofi", "Concat Lo-fi", BITS_PARAMS);
pub(crate) fn kind(id: &str) -> Option<Kind> {
    match id {
        "concat.bass" => Some(Kind::Bass),
        "concat.treble" => Some(Kind::Treble),
        "concat.bright" => Some(Kind::Bright),
        "concat.underwater" => Some(Kind::Underwater),
        "concat.telephone" => Some(Kind::Telephone),
        "concat.megaphone" => Some(Kind::Megaphone),
        "concat.vinyl" => Some(Kind::Vinyl),
        "concat.whisper" => Some(Kind::Whisper),
        "concat.radio" => Some(Kind::Radio),
        "concat.lofi" => Some(Kind::Lofi),
        "concat.robot" => Some(Kind::Robot),
        _ => None,
    }
}
pub(crate) struct Tone {
    kind: Kind,
    rate: u32,
    channels: usize,
    value: f32,
    filters: Vec<Shelf>,
    limiter: LookaheadLimiter,
    crusher: Option<LogCrusher>,
    robot: Option<super::concat_robot::Robot>,
    cursor: i64,
    end: Option<i64>,
    odd: Vec<i64>,
}
impl Tone {
    pub(crate) fn new(kind: Kind, rate: u32, channels: usize) -> Self {
        let value = kind.params()[0].default;
        Self {
            kind,
            rate,
            channels,
            value,
            filters: kind.filters(rate, channels, f64::from(value)),
            limiter: LookaheadLimiter::new(rate, channels, kind.limit(), true),
            crusher: kind.crusher(f64::from(value)),
            robot: kind.modulation(f64::from(value)).map(|speed|super::concat_robot::Robot::new(rate,channels,speed)),
            cursor: 0,
            end: None,
            odd: vec![],
        }
    }
    pub(crate) fn set_cursor(&mut self, cursor: i64) {
        self.cursor = cursor;
    }
    pub(crate) fn frame_flow(&mut self, end: i64, odd: &[i64], cursor: i64) {
        self.end = Some(end);
        self.odd = odd.to_vec();
        self.cursor = cursor;
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64], double: bool) {
        if self.end.is_some_and(|end| self.cursor >= end) {
            samples.fill(0.);
        } else {
            let double = if let Some(robot) = &mut self.robot {
                robot.process_frame(samples, double);
                true
            } else {
                double
            };
            let odd = self.odd.contains(&(self.cursor + 1));
            for (ch, sample) in samples.iter_mut().enumerate() {
                if !double {
                    *sample = f64::from(*sample as f32);
                }
                for filter in &mut self.filters {
                    *sample = filter.process(ch, *sample, double, odd);
                }
                if let Some(crusher) = &self.crusher {
                    *sample = crusher.process(*sample);
                }
            }
            self.limiter.process_frame(samples);
        }
        self.cursor += 1;
    }
}
impl AudioEffect for Tone {
    fn id(&self) -> &'static str {
        self.kind.id()
    }
    fn params(&self) -> &'static [ParamSpec] {
        self.kind.params()
    }
    fn set_param(&mut self, id: &str, value: f32) -> bool {
        let param = self.params()[0];
        if id != param.id || !value.is_finite() || !(param.min..=param.max).contains(&value) {
            return false;
        }
        self.value = value;
        self.filters = self
            .kind
            .filters(self.rate, self.channels, f64::from(value));
        self.crusher = self.kind.crusher(f64::from(value));
        self.robot=self.kind.modulation(f64::from(value)).map(|speed|super::concat_robot::Robot::new(self.rate,self.channels,speed));
        true
    }
    fn param(&self, id: &str) -> Option<f32> {
        (id == self.params()[0].id).then_some(self.value)
    }
    fn reset(&mut self) {
        for filter in &mut self.filters {
            filter.reset();
        }
        self.limiter.reset();
        if let Some(robot) = &mut self.robot {
            robot.reset();
        }
        self.cursor = 0;
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        if channels.len() < self.channels {
            return;
        }
        let frames = channels
            .iter()
            .take(self.channels)
            .map(|v| v.len())
            .min()
            .unwrap_or(0);
        for i in 0..frames {
            let mut sample = [0.; 16];
            for ch in 0..self.channels {
                sample[ch] = f64::from(channels[ch][i]);
            }
            self.process_frame(&mut sample[..self.channels], false);
            for ch in 0..self.channels {
                channels[ch][i] = sample[ch] as f32;
            }
        }
    }
}
