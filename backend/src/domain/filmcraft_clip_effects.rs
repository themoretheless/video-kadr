//! Source clip DSP rack: independent of mixer rack limits and post-fader flags.
use super::{
    filmcraft_audio_inserts::{InsertHost, SourceAudioInserts, SourceInsertRuntime},
    filmcraft_sequence_source::SourceTrackItem,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Serialize, Deserialize)]
pub struct SourceClipEffects {
    source: Value,
    plan: SourceAudioInserts,
}
impl SourceClipEffects {
    pub fn compile(item: &SourceTrackItem<'_>) -> Result<Self> {
        Ok(Self {
            source: item.raw.clone(),
            plan: SourceAudioInserts::compile(
                item.raw.get("effects").context("missing clip effects")?,
                &json!({"mode":"Read"}),
                InsertHost::Clip,
            )?,
        })
    }
    pub fn is_empty(&self) -> bool {
        self.plan.is_empty()
    }
}
struct Chain {
    inserts: SourceInsertRuntime,
    next: i64,
}
pub struct SourceClipEffectsRuntime {
    plan: SourceClipEffects,
    rate: u32,
    channels: usize,
    chains: Vec<Chain>,
}
impl SourceClipEffectsRuntime {
    pub fn new(plan: SourceClipEffects, rate: u32, channels: usize) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "invalid clip DSP format"
        );
        Ok(Self {
            plan,
            rate,
            channels,
            chains: vec![],
        })
    }
    pub fn render(
        &mut self,
        start: i64,
        frames: usize,
        mut read: impl FnMut(i64, usize) -> Result<Vec<Vec<f32>>>,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(frames <= 4_194_304, "clip DSP output exceeds budget");
        if self.plan.is_empty() {
            return read(start, frames);
        }
        let source = SourceTrackItem::read(&self.plan.source)?;
        let clock = |sample: i64| -> Result<i64> {
            Ok(source
                .clocks(i64::try_from(
                    sample as i128 * 254016000000i128 / self.rate as i128,
                )?)?
                .frame)
        };
        let taken = self
            .chains
            .iter()
            .position(|chain| chain.next == start)
            .map(|i| self.chains.swap_remove(i));
        let (mut chain, skip) = match taken {
            Some(chain) => (chain, 0),
            None => {
                let preroll = self.plan.plan.preroll_samples(clock(start)?, self.rate)?;
                ensure!(preroll <= 4_194_304, "clip DSP preroll exceeds budget");
                (
                    Chain {
                        inserts: SourceInsertRuntime::new(
                            self.plan.plan.clone(),
                            self.rate,
                            self.channels,
                        )?,
                        next: start
                            .checked_sub(preroll as i64)
                            .context("clip preroll clock overflow")?,
                    },
                    preroll,
                )
            }
        };
        let latency = chain.inserts.latency_samples(false);
        let input_start = chain
            .next
            .checked_add(latency as i64)
            .context("clip latency clock overflow")?;
        let total = skip
            .checked_add(frames)
            .context("clip DSP buffer overflow")?;
        ensure!(
            total <= 4_194_304,
            "clip DSP expanded buffer exceeds budget"
        );
        let mut input = read(input_start, total)?;
        ensure!(
            input.len() == self.channels
                && input
                    .iter()
                    .all(|c| c.len() == total && c.iter().all(|v| v.is_finite())),
            "invalid clip DSP input"
        );
        let mut offset = 0;
        while offset < total {
            let position = input_start
                .checked_add(offset as i64)
                .context("clip processing clock overflow")?;
            let count = (total - offset).min(65536 - position.rem_euclid(256) as usize);
            let mut bus: Vec<Vec<f32>> = input
                .iter()
                .map(|c| c[offset..offset + count].to_vec())
                .collect();
            chain
                .inserts
                .process_with_clock(false, &mut bus, position, 256, clock)?;
            for (out, channel) in input.iter_mut().zip(bus) {
                out[offset..offset + count].copy_from_slice(&channel);
            }
            offset += count;
        }
        chain.next = start
            .checked_add(frames as i64)
            .context("clip DSP output clock overflow")?;
        let output = input.into_iter().map(|c| c[skip..].to_vec()).collect();
        self.chains.push(chain);
        if self.chains.len() > 4 {
            self.chains.remove(0);
        }
        Ok(output)
    }
}
