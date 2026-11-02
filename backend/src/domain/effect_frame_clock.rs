//! Exact, independent clocks for decoded frames, parameter keyframes and generators.
//! Input/output units are original 254016000000 Hz source ticks, including preroll.
#[derive(Clone, Copy, Debug)]
pub struct EffectClockInput {
    pub timeline_start: i64,
    pub source_in: i64,
    pub duration: i64,
    pub speed: f64,
    pub reverse: bool,
    pub frame_hold: Option<i64>,
    pub hold_filters: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectFrameClock {
    pub frame: i64,
    pub parameters: i64,
    pub generator: i64,
}
impl EffectClockInput {
    pub fn sample(self, timeline: i64) -> Result<EffectFrameClock, &'static str> {
        if self.duration <= 0 || !self.speed.is_finite() || self.speed == 0.0 {
            return Err("invalid effect clock placement");
        }
        self.sample_source_item(timeline)
    }
    /// Source-project preservation also admits stationary and legacy zero/negative
    /// duration items. Native authored placements retain sample()'s stricter gate.
    pub fn sample_source_item(self, timeline: i64) -> Result<EffectFrameClock, &'static str> {
        if !self.speed.is_finite() { return Err("invalid source item speed"); }
        let local = timeline.checked_sub(self.timeline_start).ok_or("effect local clock overflow")?;
        // Original source/effect clocks return the held tick without evaluating
        // moving media time when Hold Filters is enabled.
        if let Some(held) = self.frame_hold.filter(|_| self.hold_filters) {
            return Ok(EffectFrameClock { frame:held, parameters:held, generator:local });
        }
        let scale = |tick: i64| -> Result<i64, &'static str> {
            let value = (tick as f64 * self.speed.abs()).round();
            // i64::MAX rounds to 2^63 as f64; reject that endpoint before casting.
            if !value.is_finite() || value < i64::MIN as f64 || value >= -(i64::MIN as f64) {
                return Err("effect scaled clock overflow");
            }
            Ok(value as i64)
        };
        let offset = scale(local)?;
        let moving = if self.reverse {
            self.source_in.checked_add(scale(self.duration)?)
                .and_then(|end|end.checked_sub(offset)).and_then(|tick|tick.checked_sub(1))
        } else {
            self.source_in.checked_add(offset)
        }.ok_or("effect media clock overflow")?;
        Ok(EffectFrameClock {
            frame: self.frame_hold.unwrap_or(moving),
            parameters: if self.hold_filters { self.frame_hold.unwrap_or(moving) } else { moving },
            generator: local,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_side_animation_clocks_remain_distinct_and_signed() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-side-animation.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),4284);
        for case in cases {
            let side=case["mode"].as_i64().unwrap();let hold=case["hold"].as_i64().unwrap();
            let input=EffectClockInput{timeline_start:side*10*254016000000,source_in:20*254016000000,duration:10*254016000000,speed:1.,reverse:false,frame_hold:if hold>0{Some(25*254016000000)}else{None},hold_filters:hold==2};
            let parse=|key:&str|case[key].as_str().unwrap().parse::<i64>().unwrap();
            let actual=input.sample(parse("at")).unwrap();
            assert_eq!(actual,EffectFrameClock{frame:parse("sourceTime"),parameters:parse("effectTime"),generator:parse("localTime")},"{case}");
        }
    }
    #[test]
    fn original_reverse_speed_and_hold_matrix() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-effect-frame-clock.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),126);
        for case in cases {
            let v=&case["input"];
            let input=EffectClockInput{timeline_start:v["timeline_start"].as_i64().unwrap(),source_in:v["source_in"].as_i64().unwrap(),duration:v["duration"].as_i64().unwrap(),speed:v["speed"].as_f64().unwrap(),reverse:v["reverse"].as_bool().unwrap(),frame_hold:v["frame_hold"].as_i64(),hold_filters:v["hold_filters"].as_bool().unwrap()};
            assert_eq!(input.sample(case["at"].as_i64().unwrap()).unwrap(),EffectFrameClock{frame:case["frame"].as_i64().unwrap(),parameters:case["parameters"].as_i64().unwrap(),generator:case["generator"].as_i64().unwrap()},"{case}");
        }
    }
    #[test]
    fn invalid_and_overflow_clocks_are_rejected() {
        let input=EffectClockInput{timeline_start:0,source_in:0,duration:1,speed:1.,reverse:false,frame_hold:None,hold_filters:false};
        assert!(EffectClockInput{speed:f64::NAN,..input}.sample(0).is_err());
        assert!(EffectClockInput{speed:0.,..input}.sample(0).is_err());
        assert!(EffectClockInput{timeline_start:i64::MIN,..input}.sample(i64::MAX).is_err());
        assert!(EffectClockInput{source_in:i64::MAX,..input}.sample(1).is_err());
        assert!(input.sample(i64::MAX).is_err());
        assert_eq!(EffectClockInput{reverse:true,..input}.sample(-1).unwrap().frame,1);
    }
}
