//! Concat adjustment-layer strength, ramps and backdrop composition.
use super::{
    composition::{BlendMode, VideoEffect},
    concat_clip_animation::Pose,
    group_surface::GroupSurface,
};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Treatment {
    pub strength: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub duration_ticks: u64,
}
impl Treatment {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.strength.is_finite() && (0.0..=1.0).contains(&self.strength),
            "invalid Concat treatment strength"
        );
        ensure!(
            [self.fade_in, self.fade_out]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.),
            "invalid Concat treatment ramps"
        );
        ensure!(
            self.duration_ticks > 0 && self.duration_ticks <= 9_007_199_254_740_991,
            "invalid Concat treatment clock"
        );
        Ok(())
    }
    pub fn strength_at(&self, local: u64) -> Result<f64> {
        self.validate()?;
        if local >= self.duration_ticks {
            return Ok(0.);
        }
        let at = local as f64 / 1_000_000.;
        let left = (self.duration_ticks - local) as f64 / 1_000_000.;
        let mut ramp = 1f64;
        if self.fade_in > 0. && at < self.fade_in {
            ramp = ramp.min(at / self.fade_in);
        }
        if self.fade_out > 0. && left < self.fade_out {
            ramp = ramp.min(left / self.fade_out);
        }
        Ok(self.strength * ramp.clamp(0., 1.))
    }
    pub fn composite(
        &self,
        surface: &mut GroupSurface,
        local: u64,
        effects: &[VideoEffect],
        apply: &mut dyn FnMut(
            &mut GroupSurface,
            &[VideoEffect],
            &mut dyn FnMut() -> Result<()>,
        ) -> Result<()>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.composite_with_strength(surface,self.strength_at(local)?,effects,apply,checkpoint)
    }
    pub(crate) fn composite_with_strength(
        &self,surface:&mut GroupSurface,strength:f64,effects:&[VideoEffect],
        apply:&mut dyn FnMut(&mut GroupSurface,&[VideoEffect],&mut dyn FnMut()->Result<()>)->Result<()>,
        checkpoint:&mut dyn FnMut()->Result<()>,
    )->Result<()>{
        ensure!(strength.is_finite()&&(0.0..=1.0).contains(&strength),"invalid treatment sample strength");
        if strength == 0. || effects.is_empty() {
            return Ok(());
        }
        checkpoint()?;
        let mut treated = surface.clone();
        apply(&mut treated, effects, checkpoint)?;
        ensure!(
            treated.width == surface.width && treated.height == surface.height,
            "treatment effect changed canvas dimensions"
        );
        super::concat_picture::Preparation {
            crop: [0.; 4],
            canvas: [surface.width, surface.height],
            flip_h: false,
            flip_v: false,
        }
        .composite_reference(
            &treated,
            surface,
            Pose {
                scale_x: 1.,
                scale_y: 1.,
                x: 0.,
                y: 0.,
                rotation_degrees: 0.,
                opacity: strength,
            },
            BlendMode::Normal,
            checkpoint,
        )
    }
}

/// Retained source treatment clock on the original video frame grid. A zero
/// strength (including the first fade frame) still owns a live treatment.
pub fn sample_clock(clip:&super::graphic_timeline::GraphicTimelineClip,canvas:&super::composition::CanvasSpec,exact:i128)->Result<Option<super::concat_project_transitions::EffectClock>>{
    let Some(treatment)=&clip.concat_treatment else{return Ok(None)};
    if !clip.enabled{return Ok(None);}
    let (num,den)=canvas.fps_rate.map(|r|(r.numerator as u64,r.denominator as u64)).unwrap_or((canvas.fps_milli as u64,1000));
    let quantise=|us:u64|super::concat_project_transitions::frame_tick(us as f64/1e6,num,den);
    let start=quantise(clip.timeline_start_tick)?;
    let duration=quantise(clip.timeline_end_tick-clip.timeline_start_tick)?;
    if exact<start as i128||exact>=start as i128+duration as i128{return Ok(None);}
    let retained=quantise(treatment.duration_ticks)?;
    if retained<=0{return Ok(None);}
    let origin=start.checked_sub(quantise(clip.animation_offset_us)?).ok_or_else(||anyhow::anyhow!("treatment clock overflow"))?;
    let timeline=i64::try_from(exact)?;
    ensure!(timeline.checked_sub(origin).is_some(),"treatment local clock overflow");
    Ok(Some(super::concat_project_transitions::EffectClock{start_tick:origin,duration_tick:retained,timeline_tick:timeline}))
}
pub fn sample_strength(treatment:&Treatment,clock:super::concat_project_transitions::EffectClock)->Result<f64>{
    treatment.validate()?;
    let local=clock.local_tick()as f64/254016000000.;
    let left=(clock.duration_tick as i128-clock.local_tick() as i128)as f64/254016000000.;
    let mut ramp=1f64;
    if treatment.fade_in>0.&&local<treatment.fade_in{ramp=ramp.min(local/treatment.fade_in);}
    if treatment.fade_out>0.&&left<treatment.fade_out{ramp=ramp.min(left/treatment.fade_out);}
    Ok(treatment.strength*ramp.clamp(0.,1.))
}
/// Source build_timeline retains only enabled shader links. Weight zero and
/// zero-valued parameters do not remove that treatment from the frame plan.
pub fn has_shader(effects:&[VideoEffect])->bool{effects.iter().any(|effect|!matches!(effect,VideoEffect::ConcatBypass{..}|VideoEffect::NativeBypass{..}))}
