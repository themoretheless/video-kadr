// Exact metadata context, included after signed-clock qualification.
impl BurninValues {
    pub fn from_composition_at_ticks(
        composition: &super::composition::Composition,
        clip_id: &super::composition::CompositionClipId,
        source_id: &super::composition::SourceId,
        timeline_tick: i64,
        source_tick: i64,
        generator_seconds: f64,
    ) -> Result<Self> {
        let rate = composition.canvas.frame_rate();
        let rate = filmcraft_time::FrameRate::new(rate.numerator as i64, rate.denominator as i64);
        ensure!(generator_seconds.is_finite(), "invalid metadata generator clock");
        let sequence_frame = rate
            .frame_at(filmcraft_time::Tick(timeline_tick))
            .checked_add(
                composition
                    .metadata
                    .as_ref()
                    .map_or(0, |m| m.sequence_start_frame),
            )
            .ok_or_else(|| anyhow::anyhow!("sequence timecode frame overflow"))?;
        let source_frame = rate.frame_at(filmcraft_time::Tick(source_tick));
        let drop = composition.metadata.as_ref().is_some_and(|m| m.drop_frame);
        Ok(Self {
            has_environment: true,
            sequence_timecode: filmcraft_time::format_timecode_frames(sequence_frame, rate, drop),
            media_timecode: filmcraft_time::format_timecode_frames(source_frame, rate, drop),
            clip_name: composition
                .metadata
                .as_ref()
                .and_then(|m| m.clip_names.get(clip_id))
                .cloned()
                .unwrap_or_default(),
            file_name: composition
                .metadata
                .as_ref()
                .and_then(|m| m.source_names.get(source_id))
                .cloned()
                .unwrap_or_default(),
            sequence_name: composition
                .metadata
                .as_ref()
                .map(|m| m.sequence_name.clone())
                .unwrap_or_default(),
            frame_rate: rate.num as f64 / rate.den as f64,
            seconds: generator_seconds,
        })
    }
}
#[cfg(test)]
mod exact_metadata_tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec,Composition,CompositionClipId,SourceId};
    #[test]
    fn signed_media_time_and_single_source_tick_frame_boundary() {
        let mut canvas=CanvasSpec::default();canvas.fps_milli=30000;
        let composition=Composition::new(canvas);
        let clip=CompositionClipId::parse("clip").unwrap();let source=SourceId::parse("source").unwrap();
        let boundary=254_016_000_000i64/30;
        let before=BurninValues::from_composition_at_ticks(&composition,&clip,&source,boundary-1,-1,-0.5).unwrap();
        let after=BurninValues::from_composition_at_ticks(&composition,&clip,&source,boundary,0,-0.5).unwrap();
        assert_eq!(before.sequence_timecode,"00:00:00:00");
        assert_eq!(after.sequence_timecode,"00:00:00:01");
        assert_ne!(before.media_timecode,after.media_timecode);
        assert_eq!(before.seconds,-0.5);assert_eq!(after.seconds,-0.5);
        assert_eq!(after.media_timecode,"00:00:00:00");
        for invalid in [f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
            assert!(BurninValues::from_composition_at_ticks(&composition,&clip,&source,0,0,invalid).is_err());
        }
    }
    #[test]
    fn exact_metadata_retains_legacy_names_and_positive_frame_values() {
        let composition=Composition::new(CanvasSpec::default());
        let clip=CompositionClipId::parse("clip").unwrap();let source=SourceId::parse("source").unwrap();
        for time in [0u64,1,12345,500000,1000000,20_000_001] {
            let legacy=BurninValues::from_composition(&composition,&clip,&source,time,time/2,time/3).unwrap();
            let exact=BurninValues::from_composition_at_ticks(&composition,&clip,&source,(time*254016)as i64,(time/2*254016)as i64,(time/3)as f64/1000000.).unwrap();
            assert_eq!(legacy,exact);
        }
    }
}
