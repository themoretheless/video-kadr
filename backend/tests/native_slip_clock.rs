use serde_json::json;
use video_kadr_backend::domain::{composition::{ClipPlacement,PlaybackMode},video_clock::source_tick};
#[test]
fn slipped_source_windows_shift_samples_without_moving_presentation_or_retained_phase(){
 let original:ClipPlacement=serde_json::from_value(json!({"timelineStartTick":2000000,"sourceInTick":1000000,"sourceOutTick":3000000,"speed":1.0,"sourceTimelineOffsetTick":-250000,"sourceHeadTimelineTick":250000,"timelineDurationTick":2250000})).unwrap();
 let mut slipped=original.clone();slipped.source_in_tick+=1000000;slipped.source_out_tick+=1000000;
 assert_eq!(original.timeline_duration_ticks().unwrap(),slipped.timeline_duration_ticks().unwrap());assert_eq!(original.timeline_end_tick().unwrap(),slipped.timeline_end_tick().unwrap());
 for mode in [PlaybackMode::Forward,PlaybackMode::Reverse]{for time in [1999999,2000000,2250000,3000000,4249999,4250000]{let before=source_tick(&original,mode,time).unwrap();let after=source_tick(&slipped,mode,time).unwrap();assert_eq!(after,before.map(|tick|tick+1000000));}}
}
