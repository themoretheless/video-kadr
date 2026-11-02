// Included in video_clock for independent raw source clock qualification.
#[cfg(test)]
mod original_raw_source_clock_tests {
 use super::*;
 #[test]
 fn raw_transition_source_matches_original_reverse_speed_hold_clock_matrix(){
  let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-effect-frame-clock.json")).unwrap();
  let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),126);
  for case in cases {
   let input=&case["input"];
   let source_in=input["source_in"].as_i64().unwrap();let duration=input["duration"].as_i64().unwrap();let speed=input["speed"].as_f64().unwrap();
   let source_out=source_in+(duration as f64*speed.abs()).round()as i64;
   let placement=ClipPlacement{timeline_start_tick:input["timeline_start"].as_i64().unwrap()as u64/254016,source_in_tick:source_in as u64/254016,source_out_tick:source_out as u64/254016,speed,timeline_duration_tick:Some(duration as u64/254016),speed_ramp:None};
   let mode=if let Some(hold)=input["frame_hold"].as_i64(){PlaybackMode::Freeze{source_tick:hold as u64/254016}}else if input["reverse"].as_bool().unwrap(){PlaybackMode::Reverse}else{PlaybackMode::Forward};
   let actual=transition_source_sample_tick(&placement,mode,i128::from(case["at"].as_i64().unwrap())).unwrap();
   assert_eq!(actual,Some(case["frame"].as_i64().unwrap()),"original raw source clock {case}");
  }
 }
}
