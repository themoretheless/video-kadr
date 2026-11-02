//! Exact caption timestamps with backward-compatible microsecond wire values.
//! Source ticks remain decimal strings, never JSON floating-point numbers.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;

pub const SOURCE_TICKS_PER_MICROSECOND: i128 = 254_016;
/// Signed edit distance: numeric legacy microseconds or exact source tick string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CaptionDelta {
    Microseconds(i64),
    SourceTicks(#[serde(with = "signed_tick_text")] i64),
}
mod signed_tick_text {
    use super::*;
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        String::deserialize(deserializer)?
            .parse::<i64>()
            .map_err(serde::de::Error::custom)
    }
}
impl CaptionDelta {
    pub fn from_exact_ticks(ticks:i128)->Result<Self,&'static str>{
        if ticks%SOURCE_TICKS_PER_MICROSECOND==0 {return i64::try_from(ticks/SOURCE_TICKS_PER_MICROSECOND).map(Self::Microseconds).map_err(|_|"caption delta overflow");}
        i64::try_from(ticks).map(Self::SourceTicks).map_err(|_|"caption delta overflow")
    }
    pub fn exact_ticks(self) -> i128 {
        match self {
            Self::Microseconds(us) => us as i128 * SOURCE_TICKS_PER_MICROSECOND,
            Self::SourceTicks(ticks) => ticks as i128,
        }
    }
    pub fn validate_wire(self) -> Result<(), &'static str> {
        if matches!(self,Self::Microseconds(us) if us.unsigned_abs()>9_007_199_254_740_991) {
            Err("unsafe JSON caption delta")
        } else {
            Ok(())
        }
    }
}
impl PartialEq for CaptionDelta {
    fn eq(&self, other: &Self) -> bool {
        self.exact_ticks() == other.exact_ticks()
    }
}
impl Eq for CaptionDelta {}

#[derive(Debug, Clone, Copy)]
pub enum CaptionTime {
    Microseconds(u64),
    SourceTicks(i64),
}
impl CaptionTime {
    pub const ZERO: Self = Self::Microseconds(0);
    pub fn exact_ticks(self) -> i128 {
        match self {
            Self::Microseconds(us) => us as i128 * SOURCE_TICKS_PER_MICROSECOND,
            Self::SourceTicks(ticks) => ticks as i128,
        }
    }
    /// Explicit projection for UI/legacy consumers; never use to persist source time.
    pub fn microseconds_floor(self) -> Result<u64, &'static str> {
        u64::try_from(self.exact_ticks().div_euclid(SOURCE_TICKS_PER_MICROSECOND))
            .map_err(|_| "caption time outside nonnegative host range")
    }
    pub fn source_tick(self) -> Result<filmcraft_time::Tick, &'static str> {
        i64::try_from(self.exact_ticks())
            .map(filmcraft_time::Tick)
            .map_err(|_| "caption time outside source clock range")
    }
    /// Preserve the representation when possible; fail instead of truncating source ticks.
    pub fn with_exact_ticks(self, ticks:i128)->Result<Self,&'static str>{
        let value=if matches!(self,Self::Microseconds(_)) && ticks%SOURCE_TICKS_PER_MICROSECOND==0 {
            Self::Microseconds(u64::try_from(ticks/SOURCE_TICKS_PER_MICROSECOND).map_err(|_|"caption time overflow")?)
        }else{Self::SourceTicks(i64::try_from(ticks).map_err(|_|"caption source time overflow")?)};
        value.validate_timeline_point()?;Ok(value)
    }
    pub fn exact_microseconds(self)->Result<u64,&'static str>{
        if self.exact_ticks()%SOURCE_TICKS_PER_MICROSECOND!=0 {return Err("fractional caption placement requires exact graphic timeline support");}
        self.microseconds_floor()
    }
    pub fn checked_add_source_ticks(self, delta: i64) -> Result<Self, &'static str> {
        self.checked_add_delta(CaptionDelta::SourceTicks(delta))
    }
    pub fn checked_add_delta(self, delta: CaptionDelta) -> Result<Self, &'static str> {
        let exact = self
            .exact_ticks()
            .checked_add(delta.exact_ticks())
            .ok_or("caption time overflow")?;
        if matches!(self, Self::Microseconds(_)) && exact % SOURCE_TICKS_PER_MICROSECOND == 0 {
            return u64::try_from(exact / SOURCE_TICKS_PER_MICROSECOND)
                .map(Self::Microseconds)
                .map_err(|_| "caption time outside nonnegative host range");
        }
        i64::try_from(exact)
            .map(Self::SourceTicks)
            .map_err(|_| "caption time outside source clock range")
    }
    /// Conservative host duration/output bound; does not change authored time.
    pub fn microseconds_ceil(self) -> Result<u64, &'static str> {
        let ticks = self.exact_ticks();
        let quotient = ticks.div_euclid(SOURCE_TICKS_PER_MICROSECOND);
        let ceil = quotient + i128::from(ticks.rem_euclid(SOURCE_TICKS_PER_MICROSECOND) != 0);
        if ticks < 0 {
            return Err("negative caption timestamp");
        }
        u64::try_from(ceil).map_err(|_| "caption time outside host range")
    }
    pub fn checked_source_delta_from(self, earlier: Self) -> Result<i64, &'static str> {
        i64::try_from(self.exact_ticks() - earlier.exact_ticks())
            .map_err(|_| "caption delta outside source clock range")
    }
    pub fn snap_nearest_frame(self, rate: filmcraft_time::FrameRate) -> Result<Self, &'static str> {
        let snapped = rate.snap_nearest(self.source_tick()?).0;
        if matches!(self, Self::Microseconds(_))
            && snapped >= 0
            && snapped as i128 % SOURCE_TICKS_PER_MICROSECOND == 0
        {
            return Ok(Self::Microseconds(
                (snapped as i128 / SOURCE_TICKS_PER_MICROSECOND) as u64,
            ));
        }
        Ok(Self::SourceTicks(snapped))
    }
    pub fn validate_timeline_point(self) -> Result<(), &'static str> {
        match self {
            Self::Microseconds(us) if us > 9_007_199_254_740_991 => {
                Err("unsafe JSON microsecond timestamp")
            }
            Self::SourceTicks(ticks) if ticks < 0 => Err("negative caption timeline timestamp"),
            _ => Ok(()),
        }
    }
}
/// Source extract_range semantics without projecting authored timestamps to microseconds.
/// Returns surviving joined placement, or None when entirely removed.
pub fn extract_span(start: CaptionTime, end: CaptionTime, a: CaptionTime, b: CaptionTime)
    -> Result<Option<(CaptionTime, CaptionTime)>, &'static str> {
    for point in [start,end,a,b] {point.validate_timeline_point()?;}
    if start>=end || a>=b {return Err("invalid caption extraction range");}
    if end<=a {return Ok(Some((start,end)));}
    fn encode(ticks:i128, original:CaptionTime)->Result<CaptionTime,&'static str>{
        let result=if matches!(original,CaptionTime::Microseconds(_)) && ticks%SOURCE_TICKS_PER_MICROSECOND==0 {
            CaptionTime::Microseconds(u64::try_from(ticks/SOURCE_TICKS_PER_MICROSECOND).map_err(|_|"caption time overflow")?)
        }else{CaptionTime::SourceTicks(i64::try_from(ticks).map_err(|_|"caption source time overflow")?)};
        result.validate_timeline_point()?;Ok(result)
    }
    let (s,e,a,b)=(start.exact_ticks(),end.exact_ticks(),a.exact_ticks(),b.exact_ticks());
    if s>=b {return Ok(Some((encode(s-(b-a),start)?,encode(e-(b-a),end)?)));}
    let kept=(a-s).max(0)+(e-b).max(0);
    if kept==0 {return Ok(None);}
    let new_start=s.min(a);
    Ok(Some((encode(new_start,start)?,encode(new_start+kept,end)?)))
}

impl From<u64> for CaptionTime {
    fn from(us: u64) -> Self {
        Self::Microseconds(us)
    }
}
impl PartialEq for CaptionTime {
    fn eq(&self, other: &Self) -> bool {
        self.exact_ticks() == other.exact_ticks()
    }
}
impl Eq for CaptionTime {}
impl PartialOrd for CaptionTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for CaptionTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.exact_ticks().cmp(&other.exact_ticks())
    }
}
impl Serialize for CaptionTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Microseconds(us) => serializer.serialize_u64(*us),
            Self::SourceTicks(ticks) => serializer.serialize_str(&ticks.to_string()),
        }
    }
}
impl<'de> Deserialize<'de> for CaptionTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Microseconds(u64),
            SourceTicks(String),
        }
        match Wire::deserialize(deserializer)? {
            Wire::Microseconds(us) => Ok(Self::Microseconds(us)),
            Wire::SourceTicks(text) => text
                .parse::<i64>()
                .map(Self::SourceTicks)
                .map_err(serde::de::Error::custom),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_time_preserves_legacy_numbers_and_exact_signed_source_strings() {
        for json in [
            "0",
            "9007199254740991",
            "\"-9007199254740993\"",
            "\"9007199254740993\"",
            "\"137729592000\"",
        ] {
            let time: CaptionTime = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&time).unwrap(), json);
        }
        for invalid in [
            "0.1",
            "-1",
            "\"9223372036854775808\"",
            "\"0.1\"",
            "null",
            "{}",
        ] {
            assert!(
                serde_json::from_str::<CaptionTime>(invalid).is_err(),
                "{invalid}"
            );
        }
        assert_eq!(
            CaptionTime::Microseconds(1),
            CaptionTime::SourceTicks(254016)
        );
        assert!(CaptionTime::SourceTicks(254015) < CaptionTime::Microseconds(1));
        assert!(CaptionTime::SourceTicks(254017) > CaptionTime::Microseconds(1));
        assert!(CaptionTime::SourceTicks(-1)
            .validate_timeline_point()
            .is_err());
        assert!(CaptionTime::Microseconds(9007199254740992)
            .validate_timeline_point()
            .is_err());
    }
    #[test]
    fn caption_time_arithmetic_never_rounds_fractional_frames() {
        let frame = filmcraft_time::FrameRate::FPS_23_976.frame_duration().0;
        let time = CaptionTime::ZERO
            .checked_add_source_ticks(frame * 13)
            .unwrap();
        assert_eq!(serde_json::to_string(&time).unwrap(), "\"137729592000\"");
        assert_eq!(time.microseconds_floor().unwrap(), 542208);
        assert_ne!(time, CaptionTime::Microseconds(542208));
        assert_eq!(time.source_tick().unwrap().0, frame * 13);
        assert_eq!(
            time.checked_add_source_ticks(-frame * 13).unwrap(),
            CaptionTime::ZERO
        );
        assert_eq!(
            serde_json::to_string(
                &CaptionTime::Microseconds(1)
                    .checked_add_source_ticks(254016)
                    .unwrap()
            )
            .unwrap(),
            "2"
        );
        assert!(CaptionTime::SourceTicks(i64::MAX)
            .checked_add_source_ticks(1)
            .is_err());
        assert!(CaptionTime::SourceTicks(-1).microseconds_floor().is_err());
    }
    #[test]
    fn caption_time_deltas_frame_snap_and_output_bounds_stay_exact() {
        let fractional = CaptionTime::SourceTicks(137729592000);
        assert_eq!(fractional.microseconds_floor().unwrap(), 542208);
        assert_eq!(fractional.microseconds_ceil().unwrap(), 542209);
        assert_eq!(
            CaptionTime::Microseconds(542208)
                .microseconds_ceil()
                .unwrap(),
            542208
        );
        assert_eq!(
            CaptionTime::Microseconds(1)
                .checked_source_delta_from(CaptionTime::SourceTicks(254015))
                .unwrap(),
            1
        );
        assert_eq!(
            CaptionTime::SourceTicks(9007199254740993)
                .checked_source_delta_from(CaptionTime::SourceTicks(9007199254740992))
                .unwrap(),
            1
        );
        assert!(CaptionTime::SourceTicks(i64::MAX)
            .checked_source_delta_from(CaptionTime::SourceTicks(i64::MIN))
            .is_err());
        assert_eq!(
            fractional
                .checked_add_source_ticks(1)
                .unwrap()
                .snap_nearest_frame(filmcraft_time::FrameRate::FPS_23_976)
                .unwrap(),
            fractional
        );
        assert_eq!(
            serde_json::to_string(
                &CaptionTime::Microseconds(1000000)
                    .snap_nearest_frame(filmcraft_time::FrameRate::FPS_25)
                    .unwrap()
            )
            .unwrap(),
            "1000000"
        );
        assert!(CaptionTime::SourceTicks(-1).microseconds_ceil().is_err());
    }
    #[test]
    fn caption_edit_deltas_preserve_signed_units_and_fractional_moves() {
        for wire in ["-1", "1", "\"-1\"", "\"9007199254740993\""] {
            let delta: CaptionDelta = serde_json::from_str(wire).unwrap();
            assert_eq!(serde_json::to_string(&delta).unwrap(), wire);
        }
        assert_eq!(
            CaptionDelta::Microseconds(-1),
            CaptionDelta::SourceTicks(-254016)
        );
        assert_ne!(
            CaptionDelta::Microseconds(-1),
            CaptionDelta::SourceTicks(-1)
        );
        let frame = filmcraft_time::FrameRate::FPS_23_976.frame_duration().0;
        let moved = CaptionTime::ZERO
            .checked_add_delta(CaptionDelta::SourceTicks(frame * 13))
            .unwrap();
        assert_eq!(serde_json::to_string(&moved).unwrap(), "\"137729592000\"");
        assert_eq!(
            moved
                .checked_add_delta(CaptionDelta::SourceTicks(-frame * 13))
                .unwrap(),
            CaptionTime::ZERO
        );
        assert_eq!(
            CaptionTime::Microseconds(1)
                .checked_add_delta(CaptionDelta::Microseconds(1))
                .unwrap(),
            CaptionTime::Microseconds(2)
        );
        assert!(CaptionDelta::Microseconds(i64::MIN)
            .validate_wire()
            .is_err());
        assert!(serde_json::from_str::<CaptionDelta>("\"9223372036854775808\"").is_err());
    }
    #[test]
    fn exact_extraction_matches_original_and_preserves_fractional_ticks() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-ripple.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap(){
            for (before,after) in case["before"].as_array().unwrap().iter().zip(case["after"].as_array().unwrap()) {
                let cue=&before["captions"][0];
                let start=cue["start"].as_i64().unwrap();let end=start+cue["duration"].as_i64().unwrap();
                let mut span=Some((CaptionTime::SourceTicks(start),CaptionTime::SourceTicks(end)));
                if !before["locked"].as_bool().unwrap() && before["sync_lock"].as_bool().unwrap(){
                    for hole in case["holes"].as_array().unwrap(){
                        span=span.and_then(|(s,e)|extract_span(s,e,CaptionTime::SourceTicks(hole[0].as_i64().unwrap()),CaptionTime::SourceTicks(hole[1].as_i64().unwrap())).unwrap());
                    }
                }
                let expected=after["captions"].as_array().unwrap().first().map(|cue|{let s=cue["start"].as_i64().unwrap();(CaptionTime::SourceTicks(s),CaptionTime::SourceTicks(s+cue["duration"].as_i64().unwrap()))});
                assert_eq!(span,expected);
            }
        }
        // Single source tick deletion must not disappear through host rounding.
        let result=extract_span(CaptionTime::Microseconds(0),CaptionTime::Microseconds(2),CaptionTime::SourceTicks(1),CaptionTime::SourceTicks(2)).unwrap().unwrap();
        assert_eq!(result.1,CaptionTime::SourceTicks(508031));
        assert_eq!(serde_json::to_string(&result.1).unwrap(),"\"508031\"");
        assert!(extract_span(CaptionTime::ZERO,CaptionTime::Microseconds(1),CaptionTime::ZERO,CaptionTime::ZERO).is_err());
    }

}
