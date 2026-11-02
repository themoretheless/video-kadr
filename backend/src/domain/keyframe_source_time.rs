// Included in keyframes.rs after exact-time sampling qualification.
impl KeyframeTrack<f64> {
    /// Sample exact original source ticks without quantizing to the authored track grid.
    pub fn sample_source_ticks(&self, tick: i64) -> f64 {
        if let Some(clock)=self.sample_clock{return clock.prepare(self).and_then(|prepared|prepared.sample_source_ticks(tick,false)).unwrap_or(f64::NAN)}
        self.sample_source_numerator(i128::from(tick) * i128::from(self.time_base),false)
    }
    /// Exact Filmcraft parameter evaluator, retaining its original Bézier iteration semantics.
    pub fn sample_filmcraft_source_ticks(&self,tick:i64)->f64 {
        if let Some(clock)=self.sample_clock{return clock.prepare(self).and_then(|prepared|prepared.sample_source_ticks(tick,true)).unwrap_or(f64::NAN)}
        self.sample_source_numerator(i128::from(tick)*i128::from(self.time_base),true)
    }
    fn sample_source_numerator(&self, numerator: i128,filmcraft:bool) -> f64 {
        self.sample_source_ratio(numerator,254016000000,filmcraft)
    }
    pub(crate) fn sample_source_ratio(&self,numerator:i128,denominator:i128,filmcraft:bool)->f64 {
        // Preserve the existing exact-grid path, including its floating-point ordering.
        if !filmcraft&&numerator%denominator==0{return self.sample_tick((numerator/denominator)as i64);}
        let first=&self.keyframes[0];let last=self.keyframes.last().expect("non-empty track");
        let position=|tick:i64|i128::from(tick)*denominator;
        let before=numerator<position(first.tick);
        if before||numerator>position(last.tick) {
            let endpoint=if before{first}else{last};
            let modes=self.extrapolation.unwrap_or_default();
            let mode=if before{modes.before}else{modes.after};
            if mode==ExtrapolationMode::Hold||self.keyframes.len()<2{return endpoint.value;}
            if matches!(mode,ExtrapolationMode::Loop|ExtrapolationMode::PingPong|ExtrapolationMode::Offset){
                let period=position(last.tick)-position(first.tick);
                let relative=numerator-position(first.tick);
                let cycle=relative.div_euclid(period);let remainder=relative.rem_euclid(period);
                let folded=if mode==ExtrapolationMode::PingPong&&cycle.rem_euclid(2)==1{position(last.tick)-remainder}else{position(first.tick)+remainder};
                return self.sample_source_ratio(folded,denominator,filmcraft)+if mode==ExtrapolationMode::Offset{cycle as f64*(last.value-first.value)}else{0.};
            }
            let delta=numerator-position(endpoint.tick);
            let distance=(delta/denominator)as f64+(delta%denominator)as f64/denominator as f64;
            return endpoint.value+self.endpoint_slope(before)*distance;
        }
        if numerator==position(last.tick){return last.value;}
        let next=self.keyframes.partition_point(|key|position(key.tick)<=numerator);
        let(left,right)=(&self.keyframes[next-1],&self.keyframes[next]);
        if numerator==position(left.tick){return left.value;}
        let(delta,span)=(numerator-position(left.tick),position(right.tick)-position(left.tick));
        let unit=i128::from(self.time_base);
        let progress=if filmcraft&&delta%unit==0&&span%unit==0{(delta/unit)as f64/(span/unit)as f64}else{delta as f64/span as f64};
        if filmcraft {
            if let Interpolation::Procedural(ProceduralInterpolation::Bezier{p1,p2})=segment_interpolation(left,right,self.interpolation){
                return left.value+(right.value-left.value)*super::filmcraft_timing_bezier::sample(p1,p2,progress);
            }
        }
        self.interpolate_segment(left,right,progress)
    }
}

#[cfg(test)]
mod source_time_tests {
    use super::*;
    fn track(mode:ExtrapolationMode)->KeyframeTrack<f64>{
        let mut track:KeyframeTrack<f64>=serde_json::from_value(serde_json::json!({"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":5.0},{"tick":1000000,"value":10.0}]})).unwrap();
        track.extrapolation=Some(Extrapolation{before:mode,after:mode});track
    }
    #[test]
    fn exact_source_grid_preserves_integral_samples_and_all_extrapolation_modes(){
        for mode in [ExtrapolationMode::Hold,ExtrapolationMode::Continue,ExtrapolationMode::Loop,ExtrapolationMode::PingPong,ExtrapolationMode::Offset]{
            let track=track(mode);
            for tick in [-2500000,-1500000,-500000,0,125000,500000,1000000,1500000,2500000]{
                assert_eq!(track.sample_source_ticks(tick*254016),track.sample_tick(tick),"{mode:?}/{tick}");
            }
        }
    }
    #[test]
    fn submicrosecond_and_large_origin_offsets_preserve_fractional_progress(){
        let mut track=track(ExtrapolationMode::Continue);
        assert_eq!(track.sample_source_ticks(1),5.+5.*(1./254016000000.));
        assert!(track.sample_source_ticks(-1)<5.);
        let base=35000000000000i64;
        for key in &mut track.keyframes{key.tick+=base;}
        assert_eq!(track.sample_source_ticks(base*254016+1),5.+5.*(1./254016000000.));
    }
    #[test]
    fn original_authored_submicrosecond_keys_keep_all_378_parameter_values() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-exact-authored-parameter-time.json"
        )).unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 378);
        for (index, case) in cases.iter().enumerate() {
            let origin = case["origin"].as_str().unwrap().parse::<i64>().unwrap();
            assert_ne!(origin % 254016, 0, "fixture must exercise authored submicrosecond keys");
            let track: KeyframeTrack<f64> = serde_json::from_value(serde_json::json!({
                "timeBase":254016000000u64,"interpolation":case["nativeInterpolation"],
                "keyframes":[{"tick":origin,"value":5.0},{"tick":origin+254016000000i64,"value":10.0}]
            })).unwrap();
            KeyframeTrack::new(track.time_base, track.interpolation, track.keyframes.clone()).unwrap();
            let restored: KeyframeTrack<f64> = serde_json::from_value(serde_json::to_value(&track).unwrap()).unwrap();
            assert_eq!(restored, track);
            let at = case["at"].as_str().unwrap().parse::<i64>().unwrap();
            assert_eq!(restored.sample_filmcraft_source_ticks(at).to_bits(),case["bits"].as_u64().unwrap(),"original authored curve {index}");
        }
    }
    #[test]
    fn original_parameter_values_at_exact_source_ticks(){
        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-exact-parameter-time.json")).unwrap();
        let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),378);
        let mut mismatches=vec![];
        for case in cases{
            let origin=case["origin"].as_str().unwrap().parse::<i64>().unwrap()/254016;
            let track:KeyframeTrack<f64>=serde_json::from_value(serde_json::json!({"timeBase":1000000,"interpolation":case["nativeInterpolation"],"keyframes":[{"tick":origin,"value":5.0},{"tick":origin+1000000,"value":10.0}]})).unwrap();
            let at=case["at"].as_str().unwrap().parse::<i64>().unwrap();
            let actual=track.sample_filmcraft_source_ticks(at);let expected=case["bits"].as_u64().unwrap();
            if actual.to_bits()!=expected{mismatches.push(format!("{} custom{} at{} actual{} expected{}",case["sourceInterpolation"],case["custom"],at,actual,case["value"]));}
        }
        assert!(mismatches.is_empty(),"{} mismatches: {:?}",mismatches.len(),&mismatches[..mismatches.len().min(12)]);
    }

}
