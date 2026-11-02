// Transition handles retain source precision and signed clip-local time in temporal reads.
impl CompositionEffectContext<'_> {
    fn temporal_frame_exact(
        &mut self,
        dt: f64,
        prefix: &[super::composition::VideoEffect],
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Option<super::linear_color_frame::LinearColorFrame>> {
        use super::{composition::VideoEffect};
        let Some(TemporalClip::Video(clip)) = self.temporal_clip.as_ref() else { return Ok(None) };
        let origin = match self.exact_sample { Some(sample) => sample.timeline_tick, None => i64::try_from(i128::from(self.time_us)*254_016)? };
        let Some(sample) = super::video_effect_timing::temporal_sample(clip,origin,dt)? else { return Ok(None) };
        checkpoint()?;
        let timeline=sample.timeline_tick;
        let clocks=sample.clocks;
        let source=clocks.frame;
        let Some(decode) = self.video.as_mut() else { return Ok(None) };
        let mut child = decode(&clip.source_id, sample.frame_time, !matches!(clip.playback_mode, super::composition::PlaybackMode::Freeze { .. }), &clip.id)?;
        super::concat_picture::prepare_transform_source(&clip.transform,&mut child,checkpoint)?;
        let seconds = sample.local_seconds;
        let prefix: Vec<_> = prefix.iter().filter(|effect| !matches!(effect,
            VideoEffect::PosterizeTime { .. } | VideoEffect::Echo { .. } |
            VideoEffect::AutoReframe { .. } | VideoEffect::WarpStabilizer { .. }
        )).cloned().collect();
        let mut values = super::metadata_burnin::BurninValues::from_composition_at_ticks(
            self.composition, &clip.id, &clip.source_id, timeline, source, seconds,
        )?;
        values.has_environment = false;
        Ok(Some(apply_effect_stack_seeded_at_time(
            &mut child, &prefix, 0, super::video_effect_timing::parameter_time(clip,clocks), seconds,
            self.library, Some(&mut TemporalPrefixContext { burnin: Some(values), duration:sample.duration_us, fps:self.frame_rate(), progress:Some(sample.progress), source_progress:clip.placement.concat_retime.as_ref().map(|_|sample.progress) }), checkpoint, None,
        )?))
    }
}
