// Included in flat_surface after precise context qualification.
#[derive(Clone,Copy,Debug)]
struct EffectContextSample {
    timeline_tick:i64,
    clocks:super::effect_frame_clock::EffectFrameClock,
}
impl EffectContextSample {
    fn shifted(self,dt:f64,input:super::effect_frame_clock::EffectClockInput)->Result<Self>{
        let delta=(dt*254_016_000_000.0).round();
        ensure!(delta.is_finite()&&delta>=i64::MIN as f64&&delta < -(i64::MIN as f64),"effect temporal offset overflow");
        let timeline_tick=self.timeline_tick.checked_add(delta as i64).ok_or_else(||anyhow::anyhow!("effect temporal timeline overflow"))?;
        let clocks=input.sample(timeline_tick).map_err(anyhow::Error::msg)?;
        Ok(Self{timeline_tick,clocks})
    }
}
#[cfg(test)]
mod exact_context_tests {
    use super::*;
    use crate::domain::{composition::{CanvasSpec,VideoClip,TransformSpec},effect_frame_clock::EffectFrameClock};
    #[test]
    fn transition_handle_metadata_uses_frame_clock_and_signed_generator_time() {
        let mut canvas=CanvasSpec::default();canvas.fps_milli=30000;
        let composition=Composition::new(canvas);
        let clip:VideoClip=serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"source","placement":{"timelineStartTick":10000000,"sourceInTick":20000000,"sourceOutTick":30000000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1.0},"blendMode":"normal"})).unwrap();
        let sample=EffectContextSample{timeline_tick:9_500_000*254_016,clocks:EffectFrameClock{frame:19_500_000*254_016,parameters:25_000_000*254_016,generator:-500_000*254_016}};
        let context=CompositionEffectContext{exact_sample:Some(sample),temporal_clip:Some(TemporalClip::Video(&clip)),clip_duration_us:10_000_000,composition:&composition,time_us:9_500_000,depth:0,library:&[],matrix:GroupAffine::IDENTITY,image:None,video:None};
        // The ordinary placement query has no active clip at this handle time.
        assert!(context.burnin_at(9_500_000,0).unwrap().is_none());
        let values=context.burnin_values(0).unwrap().unwrap();
        assert_eq!(values.sequence_timecode,"00:00:09:15");
        assert_eq!(values.media_timecode,"00:00:19:15");
        assert_eq!(values.seconds,-0.5);
        assert_eq!(context.clip_progress_seconds(values.seconds),Some(0.0));
    }
}

#[cfg(test)]
#[test]
fn temporal_context_shift_preserves_one_source_tick_and_recomputes_hold_clocks(){
    use super::effect_frame_clock::EffectClockInput;
    let hz=254_016_000_000i64;
    let input=EffectClockInput{timeline_start:10*hz,source_in:20*hz,duration:10*hz,speed:1.,reverse:false,frame_hold:Some(25*hz),hold_filters:false};
    let current=EffectContextSample{timeline_tick:10*hz,clocks:input.sample(10*hz).unwrap()};
    for delta in [-1i64,1]{
        let shifted=current.shifted(delta as f64/hz as f64,input).unwrap();
        assert_eq!(shifted.timeline_tick,10*hz+delta);
        assert_eq!(shifted.clocks.frame,25*hz);
        assert_eq!(shifted.clocks.parameters,20*hz+delta);
        assert_eq!(shifted.clocks.generator,delta);
    }
    assert!(current.shifted(f64::NAN,input).is_err());
    assert!(current.shifted(f64::MAX,input).is_err());
    let edge=EffectContextSample{timeline_tick:i64::MAX,..current};
    assert!(edge.shifted(1.0/hz as f64,input).is_err());
}
