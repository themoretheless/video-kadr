//! Independently authored finite Concat reflections hosted by the existing DSP chain.
use super::{concat_audio_filter::Lowpass, filmcraft_dsp::EffectSpec};
use anyhow::{Context, Result};
use filmcraft_audio_dsp::{AudioEffect, Category, EffectInfo, ParamSpec, Unit};
const ECHO_PARAMS: &[ParamSpec] = &[
    ParamSpec::new("delay", "Delay", 0.05, 1., 0.25, Unit::Seconds),
    ParamSpec::new("decay", "Decay", 0.1, 0.9, 0.4, Unit::None),
];
const STADIUM_PARAMS: &[ParamSpec] = &[ParamSpec::new("size", "Size", 1., 10., 6., Unit::None)];
const ROOM_PARAMS: &[ParamSpec] = &[ParamSpec::new("size", "Size", 0., 100., 40., Unit::Percent)];
const HALL_PARAMS: &[ParamSpec] = &[ParamSpec::new("size", "Size", 0., 100., 50., Unit::Percent)];
const PLATE_PARAMS: &[ParamSpec] = &[ParamSpec::new("size", "Size", 0., 100., 40., Unit::Percent)];
const CATHEDRAL_PARAMS: &[ParamSpec] =
    &[ParamSpec::new("size", "Size", 0., 100., 55., Unit::Percent)];
#[derive(Clone, Copy)]
enum Kind {
    Echo,
    Room,
    Hall,
    Plate,
    Cathedral,
    Stadium,
}
impl Kind {
    fn id(self) -> &'static str {
        match self {
            Self::Echo => "concat.echo",
            Self::Room => "concat.room",
            Self::Hall => "concat.hall",
            Self::Plate => "concat.plate",
            Self::Cathedral => "concat.cathedral",
            Self::Stadium => "concat.stadium",
        }
    }
    fn params(self) -> &'static [ParamSpec] {
        match self {
            Self::Echo => ECHO_PARAMS,
            Self::Room => ROOM_PARAMS,
            Self::Hall => HALL_PARAMS,
            Self::Plate => PLATE_PARAMS,
            Self::Cathedral => CATHEDRAL_PARAMS,
            Self::Stadium => STADIUM_PARAMS,
        }
    }
    fn from_id(id: &str) -> Option<Self> {
        [
            Self::Echo,
            Self::Room,
            Self::Hall,
            Self::Plate,
            Self::Cathedral,
            Self::Stadium,
        ]
        .into_iter()
        .find(|k| k.id() == id)
    }
    fn configure(self,first:f32,second:f32,rate:u32)->Configuration{
        let recipe=self.recipe(first,second);
        Configuration{delays:recipe.delays_ms.map(|ms|(f64::from(ms*rate as f32)/1000.)as usize),weights:recipe.weights,count:recipe.count,gain:recipe.gain}
    }
    fn recipe(self, first: f32, second: f32) -> ReflectionRecipe {
        let frames = |ms: f64| ms.round() as f32;
        if matches!(self, Self::Echo) {
            return ReflectionRecipe {
                delays_ms: [frames(f64::from(first) * 1000.), 0., 0., 0.],
                weights: [
                    format!("{:.2}", f64::from(second))
                        .parse()
                        .expect("finite validated coefficient"),
                    0.,
                    0.,
                    0.,
                ],
                count: 1,
                gain: 0.85,
            };
        }
        if matches!(self, Self::Stadium) {
            return ReflectionRecipe {
                delays_ms: [
                    frames(120. + f64::from(first) * 18.),
                    frames(240. + f64::from(first) * 36.),
                    0.,
                    0.,
                ],
                weights: [0.45, 0.28, 0., 0.],
                count: 2,
                gain: 0.88,
            };
        }
        let size = f64::from(first) / 100.;
        let (offset, scale, times, weights, gain) = match self {
            Self::Room => (
                0.4,
                1.6,
                [23., 41., 67., 97.],
                [0.32, 0.24, 0.17, 0.11],
                0.88,
            ),
            Self::Hall => (
                0.5,
                1.5,
                [60., 110., 180., 270.],
                [0.5, 0.4, 0.3, 0.2],
                0.85,
            ),
            Self::Plate => (0.5, 1.5, [8., 17., 29., 43.], [0.45, 0.35, 0.25, 0.15], 0.9),
            Self::Cathedral => (
                0.5,
                1.5,
                [120., 230., 390., 600.],
                [0.6, 0.5, 0.4, 0.3],
                0.9,
            ),
            Self::Echo | Self::Stadium => unreachable!(),
        };
        ReflectionRecipe {
            delays_ms: times.map(|ms| frames(ms * (offset + size * scale))),
            weights,
            count: 4,
            gain,
        }
    }
}
macro_rules! reflection {
    ($id:literal,$name:literal,$kind:ident,$params:ident,$category:ident) => {
        EffectInfo {
            id: $id,
            name: $name,
            category: Category::$category,
            params: $params,
            create: |rate, channels| Box::new(Reflections::new(Kind::$kind, rate, channels)),
        }
    };
}
static EFFECTS: &[EffectInfo] = &[
    super::concat_enhance_voice::INFO,
    super::concat_enhance_voice::DENOISE_INFO,
    super::concat_distorted::INFO,
    super::concat_tone::BASS,
    super::concat_tone::TREBLE,
    super::concat_tone::BRIGHT,
    super::concat_tone::UNDERWATER,
    super::concat_tone::TELEPHONE,
    super::concat_tone::MEGAPHONE,
    super::concat_tone::VINYL,
    super::concat_tone::WHISPER,
    super::concat_tone::RADIO,
    super::concat_tone::LOFI,
    super::concat_tone::ROBOT,
    reflection!("concat.echo", "Concat Echo", Echo, ECHO_PARAMS, Time),
    reflection!("concat.room", "Concat Room", Room, ROOM_PARAMS, Reverb),
    reflection!("concat.hall", "Concat Hall", Hall, HALL_PARAMS, Reverb),
    reflection!("concat.plate", "Concat Plate", Plate, PLATE_PARAMS, Reverb),
    reflection!(
        "concat.cathedral",
        "Concat Cathedral",
        Cathedral,
        CATHEDRAL_PARAMS,
        Reverb
    ),
    reflection!(
        "concat.stadium",
        "Concat Stadium",
        Stadium,
        STADIUM_PARAMS,
        Reverb
    ),
];
pub fn effects() -> &'static [EffectInfo] {
    EFFECTS
}
pub fn info(id: &str) -> Option<&'static EffectInfo> {
    EFFECTS.iter().find(|e| e.id == id)
}
pub fn preroll(specs: &[EffectSpec], rate: u32) -> Result<usize> {
    let mut frames = 0usize;
    for spec in specs
        .iter()
        .filter(|s| s.id.starts_with("concat.") && !s.bypass)
    {
        if spec.id == "concat.denoise" || spec.id == "concat.enhance-voice" || spec.id == "concat.distorted" || super::concat_tone::kind(&spec.id).is_some() { continue; }
        let kind = Kind::from_id(&spec.id)
            .ok_or_else(|| anyhow::anyhow!("unknown Concat audio package"))?;
        let params = kind.params();
        let first = spec
            .params
            .get(params[0].id)
            .copied()
            .unwrap_or(params[0].default);
        let second = spec.params.get("decay").copied().unwrap_or(0.4);
        let configuration = kind.configure(first, second, rate);
        frames = frames
            .checked_add(
                configuration.delays[..configuration.count]
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(0),
            )
            .ok_or_else(|| anyhow::anyhow!("Concat audio history overflow"))?;
    }
    Ok(frames)
}
/// Source-rounded milliseconds, before rate-specific packet/filter execution.
pub(crate) struct ReflectionRecipe {
    pub delays_ms:[f32;4],pub weights:[f64;4],pub count:usize,pub gain:f64,
}
pub(crate) fn source_reflection_recipe(spec:&EffectSpec)->Result<ReflectionRecipe>{
    let kind=Kind::from_id(&spec.id).context("unknown source reflection effect")?;
    let first=spec.params.get(kind.params()[0].id).copied().unwrap_or(kind.params()[0].default);
    Ok(kind.recipe(first,spec.params.get("decay").copied().unwrap_or(0.4)))
}
struct Configuration {
    delays: [usize; 4],
    weights: [f64; 4],
    count: usize,
    gain: f64,
}
pub(crate) struct Reflections {
    kind: Kind,
    rate: u32,
    first: f32,
    second: f32,
    configuration: Configuration,
    history: Vec<Vec<f64>>,
    position: usize,
    filter: Option<Lowpass>,
    filter_double: Option<super::concat_tone_filter::Shelf>,
    cursor: i64,
    odd_frame_ends: Vec<i64>,
    output_end: Option<i64>,
}
impl Reflections {
    fn new(kind: Kind, rate: f32, channels: usize) -> Self {
        let rate = rate as u32;
        let first = kind.params()[0].default;
        let second = 0.4;
        let maximum = kind
            .configure(kind.params()[0].max, 0.9, rate)
            .delays
            .into_iter()
            .max()
            .unwrap()
            .max(1);
        Self {
            kind,
            rate,
            first,
            second,
            configuration: kind.configure(first, second, rate),
            history: vec![vec![0.; maximum]; channels],
            position: 0,
            cursor: 0,
            odd_frame_ends: Vec::new(),
            output_end: None,
            filter_double: if matches!(kind,Kind::Stadium) {super::concat_tone_filter::Shelf::lowpass(rate,channels,6500.)}else{None},
            filter: if matches!(kind, Kind::Stadium) {
                Lowpass::new(rate, channels, 6500.)
            } else {
                None
            },
        }
    }
}
pub(crate) fn create_runtime(id: &str, rate: u32, channels: usize) -> Option<Reflections> {
    Some(Reflections::new(Kind::from_id(id)?, rate as f32, channels))
}
pub(crate) fn requires_full_history(specs: &[EffectSpec], rate: u32) -> bool {
    specs.iter().any(|s| !s.bypass && (s.id == "concat.denoise" || s.id == "concat.enhance-voice" || s.id == "concat.distorted" || super::concat_tone::kind(&s.id).is_some() || (rate >= 13000 && s.id == "concat.stadium")))
}
impl Reflections {
    pub(crate) fn process_frame_double(&mut self,samples:&mut[f64]){
        if self.output_end.is_some_and(|end|self.cursor>=end){samples.fill(0.);self.cursor+=1;return;}
        let size=self.history.first().map_or(0,Vec::len);if size==0{return;}
        let odd=self.odd_frame_ends.contains(&(self.cursor+1));
        for (ch,sample) in samples.iter_mut().enumerate(){
            let input=*sample;let history=&mut self.history[ch];
            let reflected=super::concat_echo_kernel::mix(input,(0..self.configuration.count).map(|tap|{
                let read=(self.position+size-self.configuration.delays[tap])%size;
                (history[read],self.configuration.weights[tap] as f32)
            }),0.8,self.configuration.gain as f32);
            *sample=if let Some(filter)=&mut self.filter_double{filter.process(ch,reflected,true,odd)}else{reflected};history[self.position]=input;
        }
        self.position=(self.position+1)%size;self.cursor+=1;
    }
    pub(crate) fn set_cursor(&mut self, cursor: i64) {
        self.cursor = cursor;
    }
    pub(crate) fn frame_flow(
        &mut self,
        end: i64,
        odd: &[i64],
        cursor: i64,
    ) -> Result<(i64, Vec<i64>)> {
        let extension = *self.configuration.delays[..self.configuration.count]
            .iter()
            .max()
            .unwrap() as i64;
        let next = end
            .checked_add(extension)
            .context("Concat frame-flow overflow")?;
        let mut ends = odd.to_vec();
        if extension % 2 != 0 {
            ends.push(next);
        }
        self.odd_frame_ends = ends.clone();
        self.output_end = Some(next);
        self.cursor = cursor;
        Ok((next, ends))
    }
}
impl AudioEffect for Reflections {
    fn id(&self) -> &'static str {
        self.kind.id()
    }
    fn params(&self) -> &'static [ParamSpec] {
        self.kind.params()
    }
    fn set_param(&mut self, id: &str, value: f32) -> bool {
        let Some(parameter) = self.params().iter().find(|p| p.id == id) else {
            return false;
        };
        if !value.is_finite() || !(parameter.min..=parameter.max).contains(&value) {
            return false;
        }
        if id == self.params()[0].id {
            self.first = value
        } else {
            self.second = value
        }
        self.configuration = self.kind.configure(self.first, self.second, self.rate);
        true
    }
    fn param(&self, id: &str) -> Option<f32> {
        if id == self.params()[0].id {
            Some(self.first)
        } else if matches!(self.kind, Kind::Echo) && id == "decay" {
            Some(self.second)
        } else {
            None
        }
    }
    fn reset(&mut self) {
        for channel in &mut self.history {
            channel.fill(0.)
        }
        self.position = 0;
        self.cursor = 0;
        if let Some(filter) = self.filter.as_mut() { filter.reset(); }
        if let Some(filter) = self.filter_double.as_mut() { filter.reset(); }
    }
    fn process(&mut self, channels: &mut [&mut [f32]]) {
        let count = channels.iter().map(|c| c.len()).min().unwrap_or(0);
        let size = self.history.first().map_or(0, Vec::len);
        if size == 0 {
            return;
        }
        for frame in 0..count {
            if self.output_end.is_some_and(|end| self.cursor >= end) {
                for channel in channels.iter_mut() {
                    channel[frame] = 0.;
                }
                self.cursor += 1;
                continue;
            }
            for (index, (channel, history)) in
                channels.iter_mut().zip(&mut self.history).enumerate()
            {
                let input = f64::from(channel[frame]);
                let reflected = super::concat_echo_kernel::mix(channel[frame],(0..self.configuration.count).map(|tap|{
                    let read=(self.position+size-self.configuration.delays[tap])%size;
                    (history[read] as f32,self.configuration.weights[tap] as f32)
                }),0.8,self.configuration.gain as f32);
                channel[frame] = if let Some(filter) = self.filter.as_mut() {
                    filter.process(
                        index,
                        reflected,
                        self.odd_frame_ends.contains(&(self.cursor + 1)),
                    )
                } else {
                    reflected
                };
                history[self.position] = input;
            }
            self.position = (self.position + 1) % size;
            self.cursor += 1;
        }
    }
}
