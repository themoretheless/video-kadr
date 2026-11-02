//! Source-typed ordered Concat voice graphs with one complete format plan.
use super::{
    packet_biquad, packet_limiter, packet_pcm::Decoded, packet_pitch::Block,
    packet_timeline::Source,
};
use crate::domain::{
    concat_biquad_stream::Design,
    concat_format_plan::{Filter, Format},
};
use anyhow::{ensure, Result};
#[derive(Clone, Copy)]
pub(crate) enum Setting {
    Pitch(f32),
    Chipmunk(f32),
    Deep { pitch: f32, body: f64 },
    Sweet { amount: f32, pitch: f32 },
}
enum Op {
    Pitch(f64),
    Compand,
    Deesser(f64),
    Echo(f32),
    Tone(Design, f64, f64, f64),
    Limiter(f64),
}
impl Op {
    fn caps(&self) -> &'static [Filter] {
        match self {
            Self::Pitch(_) => &[
                Filter::Resample,
                Filter::Pass,
                Filter::Resample,
                Filter::Tempo,
            ],
            Self::Tone(..) => &[Filter::Biquad],
            Self::Compand => &[Filter::Compand],
            Self::Deesser(_) => &[Filter::Deesser],
            Self::Echo(_) => &[Filter::Echo],
            Self::Limiter(_) => &[Filter::Limiter],
        }
    }
}
enum Static {
    Tone(packet_biquad::Stage),
    Limiter(packet_limiter::Stage),
    Dynamics(super::packet_dynamics::Stage),
}
struct Node {
    source: Box<dyn Source>,
    stage: Static,
    failed: bool,
}
impl Source for Node {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(
            !self.failed,
            "ordered voice stage requires reset after failure"
        );
        self.failed = true;
        check()?;
        let packet = if let Some(packet) = self.source.next_packet(check)? {
            Some(match &mut self.stage {
                Static::Tone(f) => f.push_packet(packet)?,
                Static::Limiter(f) => f.push_packet(packet)?,
                Static::Dynamics(f) => f.push_packet(packet)?,
            })
        } else {
            None
        };
        check()?;
        self.failed = false;
        Ok(packet)
    }
    fn reset(&mut self) {
        self.source.reset();
        match &mut self.stage {
            Static::Tone(f) => f.reset(),
            Static::Limiter(f) => f.reset(),
            Static::Dynamics(f) => f.reset(),
        };
        self.failed = false;
    }
}
struct EchoNode {
    source: Box<dyn Source>,
    stage: super::packet_echo::Stage,
    upstream_ended: bool,
    ended: bool,
    failed: bool,
}
impl Source for EchoNode {
    fn next_packet(&mut self, check: &mut dyn FnMut() -> Result<()>) -> Result<Option<Block>> {
        ensure!(!self.failed, "ordered Echo requires reset after failure");
        self.failed = true;
        check()?;
        let packet = if self.ended {
            None
        } else if !self.upstream_ended {
            if let Some(packet) = self.source.next_packet(check)? {
                Some(self.stage.push_packet(packet)?)
            } else {
                self.upstream_ended = true;
                self.stage.finish_packet()
            }
        } else {
            self.stage.finish_packet()
        };
        if self.upstream_ended && packet.is_none() {
            self.ended = true;
        }
        check()?;
        self.failed = false;
        Ok(packet)
    }
    fn reset(&mut self) {
        self.source.reset();
        self.stage.reset();
        self.upstream_ended = false;
        self.ended = false;
        self.failed = false;
    }
}
pub(crate) fn build(decoded: Decoded, settings: &[Setting]) -> Result<Box<dyn Source>> {
    ensure!(
        !settings.is_empty() && settings.len() <= 16,
        "invalid ordered voice chain length"
    );
    let channels = match &decoded {
        Decoded::U8(s) => s.spec().channels,
        Decoded::S16(s) => s.spec().channels,
        Decoded::S32(s) => s.spec().channels,
        Decoded::F32(s) => s.spec().channels,
        Decoded::F64(s) => s.spec().channels,
    };
    let mut ops = Vec::new();
    for setting in settings {
        match *setting {
            Setting::Pitch(p) => {
                ensure!(
                    p.is_finite() && (-12. ..=12.).contains(&p),
                    "invalid ordered Pitch parameter"
                );
                ops.push(Op::Pitch(f64::from(p)));
            }
            Setting::Chipmunk(p) => {
                ensure!(
                    p.is_finite() && (3. ..=12.).contains(&p),
                    "invalid ordered Chipmunk parameter"
                );
                ops.extend([
                    Op::Pitch(f64::from(p)),
                    Op::Tone(Design::Highpass, 120., 0.707, 0.),
                    Op::Limiter(50.),
                ]);
            }
            Setting::Sweet { amount, pitch } => {
                ensure!(
                    amount.is_finite()
                        && (0. ..=100.).contains(&amount)
                        && pitch.is_finite()
                        && (0. ..=8.).contains(&pitch),
                    "invalid Sweet parameters"
                );
                let strength = f64::from(amount) / 100.;
                let shift = if pitch > 0. {
                    f64::from(pitch)
                } else {
                    1.2 + 3.8 * strength
                };
                let fixed =
                    |value: f64, places: usize| format!("{value:.places$}").parse::<f64>().unwrap();
                ops.extend([
                    Op::Pitch(shift),
                    Op::Tone(Design::Highpass, 85., 0.707, 0.),
                    Op::Tone(Design::Peak, 300., 1.1, -1.4),
                    Op::Tone(Design::Peak, 4200., 0.8, fixed(0.8 + 5.2 * strength, 3)),
                    Op::Tone(Design::Peak, 10500., 0.7, fixed(1. + 7. * strength, 3)),
                    Op::Compand,
                    Op::Deesser(fixed(0.18 + 0.32 * strength, 3)),
                    Op::Echo(fixed(0.012 + 0.048 * strength, 4) as f32),
                    Op::Limiter(60.),
                ]);
            }
            Setting::Deep { pitch, body } => {
                ensure!(
                    pitch.is_finite()
                        && (-8. ..=-1.).contains(&pitch)
                        && body.is_finite()
                        && (0. ..=8.).contains(&body),
                    "invalid ordered Deep parameters"
                );
                ops.extend([
                    Op::Pitch(f64::from(pitch)),
                    Op::Tone(Design::Highpass, 55., 0.707, 0.),
                    Op::Tone(Design::Peak, 140., 1., body),
                    Op::Tone(Design::Peak, 2600., 0.9, -1.2),
                    Op::Limiter(60.),
                ]);
            }
        }
    }
    let Op::Pitch(pitch) = ops[0] else {
        unreachable!()
    };
    let following = ops
        .iter()
        .skip(1)
        .flat_map(|op| op.caps().iter().copied())
        .collect::<Vec<_>>();
    let first =
        super::packet_pitch_frames::Reader::new_derived(decoded, &following, Format::F32, pitch)?;
    let formats = first
        .plan
        .stages
        .iter()
        .map(|s| (s.input, s.output))
        .collect::<Vec<_>>();
    let mut source: Box<dyn Source> = Box::new(first);
    let mut offset = 4;
    for op in ops.into_iter().skip(1) {
        let input = formats[offset].0;
        source = match op {
            Op::Pitch(pitch) => {
                ensure!(
                    input == formats[offset - 1].1 && input == formats[offset].1,
                    "ordered voice Pitch normalizer conversion is not implemented"
                );
                Box::new(super::packet_pitch_node::Node::new_derived(
                    source,
                    input,
                    formats[offset + 3].1,
                    channels,
                    pitch,
                )?)
            }
            Op::Compand => Box::new(Node {
                source,
                stage: Static::Dynamics(super::packet_dynamics::Stage::compand(
                    input, 48000, channels,
                )?),
                failed: false,
            }),
            Op::Deesser(intensity) => Box::new(Node {
                source,
                stage: Static::Dynamics(super::packet_dynamics::Stage::deesser(
                    input, 48000, channels, intensity, 0.45, 0.55,
                )?),
                failed: false,
            }),
            Op::Echo(decay) => Box::new(EchoNode {
                source,
                stage: super::packet_echo::Stage::new(
                    input,
                    48000,
                    channels,
                    0.8,
                    0.82,
                    &[38.],
                    &[decay],
                )?,
                upstream_ended: false,
                ended: false,
                failed: false,
            }),
            Op::Tone(design, frequency, width, gain) => Box::new(Node {
                source,
                stage: Static::Tone(packet_biquad::Stage::new(
                    input, design, 48000, channels, frequency, width, gain,
                )?),
                failed: false,
            }),
            Op::Limiter(release) => Box::new(Node {
                source,
                stage: Static::Limiter(packet_limiter::Stage::new(
                    input, 48000, channels, 0.94, release, true,
                )?),
                failed: false,
            }),
        };
        offset += match op {
            Op::Pitch(_) => 4,
            _ => 1,
        };
    }
    ensure!(
        offset == formats.len(),
        "ordered voice format plan mismatch"
    );
    Ok(source)
}
