//! Filmcraft audio transition envelopes, pinned to render/transitions/mod.rs
//! 5231852443363f001c3f6b396dd9b1e6461ae2be (MIT).
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioTransitionCurve {
    ConstantPower,
    #[default]
    ConstantGain,
    ExponentialFade,
}
impl AudioTransitionCurve {
    /// Output/incoming gains, using the upstream f32 operation order.
    pub fn gains(self, progress: f32) -> Result<(f32, f32)> {
        ensure!(
            progress.is_finite() && (0.0..=1.0).contains(&progress),
            "audio transition progress outside [0,1]"
        );
        Ok(match self {
            Self::ConstantGain => (1.0 - progress, progress),
            Self::ExponentialFade => ((1.0 - progress).powi(3), 1.0 - (1.0 - progress).powi(3)),
            Self::ConstantPower => (
                (progress * std::f32::consts::FRAC_PI_2).cos(),
                (progress * std::f32::consts::FRAC_PI_2).sin(),
            ),
        })
    }
    /// Multiply two equally sized planar blocks in place; validate all shapes before mutation.
    pub fn mix(
        self,
        outgoing: &mut [Vec<f32>],
        incoming: &[Vec<f32>],
        start_progress: f32,
        progress_per_frame: f32,
    ) -> Result<()> {
        ensure!(
            !outgoing.is_empty() && outgoing.len() <= 16 && outgoing.len() == incoming.len(),
            "invalid audio transition channels"
        );
        let frames = outgoing[0].len();
        ensure!(
            frames <= 65536
                && outgoing
                    .iter()
                    .chain(incoming)
                    .all(|channel| channel.len() == frames
                        && channel.iter().all(|sample| sample.is_finite())),
            "invalid audio transition PCM shape/sample"
        );
        ensure!(
            progress_per_frame.is_finite() && progress_per_frame >= 0.0,
            "invalid audio transition increment"
        );
        self.gains(start_progress)?;
        if frames > 0 {
            self.gains(start_progress + (frames - 1) as f32 * progress_per_frame)?;
        }
        // Check arithmetic too, so an overflowing mix cannot leave a partly changed block.
        for frame in 0..frames {
            let (a, b) = self.gains(start_progress + frame as f32 * progress_per_frame)?;
            ensure!(
                outgoing
                    .iter()
                    .zip(incoming)
                    .all(|(out, input)| (out[frame] * a + input[frame] * b).is_finite()),
                "audio transition mix overflow"
            );
        }
        for frame in 0..frames {
            let (a, b) = self.gains(start_progress + frame as f32 * progress_per_frame)?;
            for (out, input) in outgoing.iter_mut().zip(incoming) {
                out[frame] = out[frame] * a + input[frame] * b;
            }
        }
        Ok(())
    }
}
/// Source-handle extension and envelope for one clip, in authored timeline ticks.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioCrossfadePlan {
    pub head_ticks: u64,
    pub tail_ticks: u64,
    pub fade_in_ticks: u64,
    pub fade_out_ticks: u64,
    pub in_curve: AudioTransitionCurve,
    pub out_curve: AudioTransitionCurve,
}
/// Plan independent incoming/outgoing joins; disabled clips cannot become neighbors.
/// Composition admission owns source-handle availability validation.
pub fn plan_crossfades(
    clips: &[super::composition::AudioClip],
) -> Result<std::collections::BTreeMap<super::composition::CompositionClipId, AudioCrossfadePlan>> {
    let mut plan = std::collections::BTreeMap::<_, AudioCrossfadePlan>::new();
    let mut ordered: Vec<_> = clips.iter().filter(|clip| clip.enabled).collect();
    ordered.sort_by_key(|clip| (clip.placement.timeline_start_tick, clip.id.as_str()));
    for pair in ordered.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        let duration = current.crossfade_in_ticks;
        if duration == 0
            || previous.placement.timeline_end_tick()? != current.placement.timeline_start_tick
        {
            continue;
        }
        let before = duration / 2;
        let after = duration - before;
        let outgoing = plan.entry(previous.id.clone()).or_default();
        outgoing.tail_ticks = after;
        outgoing.fade_out_ticks = duration;
        outgoing.out_curve = current.crossfade_curve;
        let incoming = plan.entry(current.id.clone()).or_default();
        incoming.head_ticks = before;
        incoming.fade_in_ticks = duration;
        incoming.in_curve = current.crossfade_curve;
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_transition_invariants_and_pcm() {
        for i in 0..=1024 {
            let p = i as f32 / 1024.0;
            let (a, b) = AudioTransitionCurve::ConstantPower.gains(p).unwrap();
            assert!((a * a + b * b - 1.0).abs() < 2e-7);
            for curve in [
                AudioTransitionCurve::ConstantGain,
                AudioTransitionCurve::ExponentialFade,
            ] {
                let (a, b) = curve.gains(p).unwrap();
                assert_eq!(a + b, 1.0);
            }
        }
        assert_eq!(
            AudioTransitionCurve::ExponentialFade.gains(0.5).unwrap(),
            (0.125, 0.875)
        );
        let mut a = vec![vec![1.0; 3], vec![-1.0; 3]];
        let b = vec![vec![-1.0; 3], vec![1.0; 3]];
        AudioTransitionCurve::ExponentialFade
            .mix(&mut a, &b, 0.0, 0.5)
            .unwrap();
        assert_eq!(a, vec![vec![1.0, -0.75, -1.0], vec![-1.0, 0.75, 1.0]]);
    }
    #[test]
    fn source_handles_preserve_odd_ticks_and_independent_neighbor_curves() {
        let clip = |id: &str,
                    start: u64,
                    crossfade: u64,
                    curve: &str|
         -> super::super::composition::AudioClip {
            serde_json::from_value(serde_json::json!({"id":id,"sourceId":"tone","placement":{"timelineStartTick":start,"sourceInTick":10,"sourceOutTick":110,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0,"crossfadeInTicks":crossfade,"crossfadeCurve":curve})).unwrap()
        };
        let a = clip("a", 0, 0, "constant_gain");
        let b = clip("b", 100, 5, "constant_power");
        let c = clip("c", 200, 7, "exponential_fade");
        let mut disabled = clip("disabled", 100, 1, "constant_gain");
        disabled.enabled = false;
        let plan = plan_crossfades(&[c, b, a, disabled]).unwrap();
        let find = |id: &str| plan.iter().find(|(key, _)| key.as_str() == id).unwrap().1;
        assert_eq!(find("a").tail_ticks, 3);
        assert_eq!(find("b").head_ticks, 2);
        assert_eq!(find("b").tail_ticks, 4);
        assert_eq!(find("c").head_ticks, 3);
        assert_eq!(find("b").in_curve, AudioTransitionCurve::ConstantPower);
        assert_eq!(find("b").out_curve, AudioTransitionCurve::ExponentialFade);
        assert_eq!(find("b").fade_in_ticks, 5);
        assert_eq!(find("b").fade_out_ticks, 7);
        assert_eq!(plan.len(), 3);
    }
    #[test]
    fn clip_curve_wire_roundtrip_and_legacy_default() {
        let legacy = serde_json::json!({"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":100000,"speed":1.0},"gain":{"mode":"constant","value":1.0},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0});
        let clip: super::super::composition::AudioClip =
            serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(clip.crossfade_curve, AudioTransitionCurve::ConstantGain);
        for curve in [
            AudioTransitionCurve::ConstantGain,
            AudioTransitionCurve::ConstantPower,
            AudioTransitionCurve::ExponentialFade,
        ] {
            let mut clip = clip.clone();
            clip.crossfade_curve = curve;
            let restored: super::super::composition::AudioClip =
                serde_json::from_value(serde_json::to_value(clip).unwrap()).unwrap();
            assert_eq!(restored.crossfade_curve, curve);
        }
        let mut invalid = legacy;
        invalid["crossfadeCurve"] = serde_json::json!("unknown");
        assert!(serde_json::from_value::<super::super::composition::AudioClip>(invalid).is_err());
    }
    #[test]
    fn invalid_batch_does_not_mutate_pcm() {
        let original = vec![vec![1.0; 3]];
        let mut pcm = original.clone();
        for input in [vec![vec![0.0; 2]], vec![vec![f32::NAN; 3]]] {
            assert!(
                AudioTransitionCurve::ConstantPower
                    .mix(&mut pcm, &input, 0.0, 0.5)
                    .is_err()
            );
            assert_eq!(pcm, original);
        }
        assert!(
            AudioTransitionCurve::ConstantPower
                .mix(&mut pcm, &original, 0.0, 1.0)
                .is_err()
        );
        assert_eq!(pcm, original);
    }
}
