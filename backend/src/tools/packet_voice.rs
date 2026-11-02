//! Source-typed Concat voice presets with natural EOF and original limiter delay.
use super::{
    packet_biquad, packet_limiter, packet_pcm::Decoded, packet_pitch::Block, packet_pitch_frames,
};
use crate::domain::{
    concat_biquad_stream::Design,
    concat_format_plan::{Filter, Format},
};
use anyhow::{ensure, Result};
#[derive(Clone, Copy)]
pub(crate) enum Preset {
    Chipmunk { pitch: f32 },
    Deep { pitch: f32, body: f64 },
}
pub(crate) struct Reader {
    source: packet_pitch_frames::Reader,
    tone: Vec<packet_biquad::Stage>,
    limiter: packet_limiter::Stage,
    failed: bool,
}
impl Reader {
    pub(crate) fn new(source: Decoded, preset: Preset) -> Result<Self> {
        let channels = match &source {
            Decoded::U8(s) => s.spec().channels,
            Decoded::S16(s) => s.spec().channels,
            Decoded::S32(s) => s.spec().channels,
            Decoded::F32(s) => s.spec().channels,
            Decoded::F64(s) => s.spec().channels,
        };
        let (pitch, specs, release) = match preset {
            Preset::Chipmunk { pitch } => {
                ensure!(
                    pitch.is_finite() && (3. ..=12.).contains(&pitch),
                    "invalid Chipmunk pitch"
                );
                (pitch, vec![(Design::Highpass, 120., 0.707, 0.)], 50.)
            }
            Preset::Deep { pitch, body } => {
                ensure!(
                    pitch.is_finite()
                        && (-8. ..=-1.).contains(&pitch)
                        && body.is_finite()
                        && (0. ..=8.).contains(&body),
                    "invalid Deep parameters"
                );
                (
                    pitch,
                    vec![
                        (Design::Highpass, 55., 0.707, 0.),
                        (Design::Peak, 140., 1., body),
                        (Design::Peak, 2600., 0.9, -1.2),
                    ],
                    60.,
                )
            }
        };
        let mut following = vec![Filter::Biquad; specs.len()];
        following.push(Filter::Limiter);
        let source = packet_pitch_frames::Reader::new(source, &following, Format::F64, pitch)?;
        let tone = specs
            .into_iter()
            .enumerate()
            .map(|(i, (design, freq, width, gain))| {
                packet_biquad::Stage::new(
                    source.plan.stages[4 + i].input,
                    design,
                    48000,
                    channels,
                    freq,
                    width,
                    gain,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let limiter = packet_limiter::Stage::new(
            source.plan.stages[4 + tone.len()].input,
            48000,
            channels,
            0.94,
            release,
            true,
        )?;
        Ok(Self {
            source,
            tone,
            limiter,
            failed: false,
        })
    }
    pub(crate) fn reset(&mut self) {
        self.source.reset();
        for stage in &mut self.tone {
            stage.reset();
        }
        self.limiter.reset();
        self.failed = false;
    }
    pub(crate) fn next_packet(
        &mut self,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Option<Block>> {
        ensure!(!self.failed, "voice stream requires reset after failure");
        self.failed = true;
        let output = if let Some(mut packet) = self.source.next_packet(checkpoint)? {
            for stage in &mut self.tone {
                checkpoint()?;
                packet = stage.push_packet(packet)?;
            }
            checkpoint()?;
            Some(self.limiter.push_packet(packet)?)
        } else {
            None
        };
        checkpoint()?;
        self.failed = false;
        Ok(output)
    }
}
