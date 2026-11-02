//! Source-frame interpolation clock. OpticalFlow follows original frame blending;
//! no motion-estimation implementation is implied.
use super::filmcraft_sequence_source::SourceTrackItem;use anyhow::{ensure,Context,Result};use filmcraft_time::{FrameRate,TICKS_PER_SECOND};
#[derive(Debug)]pub struct BlendFrame{pub next:i64,pub weight:f32}
pub fn plan(item:&SourceTrackItem<'_>,at:i64,rate:FrameRate)->Result<Option<BlendFrame>>{
 let mode=item.raw.get("time_interpolation").and_then(serde_json::Value::as_str).unwrap_or("frameSampling");
 ensure!(matches!(mode,"frameSampling"|"frameBlending"|"opticalFlow"),"invalid source time interpolation");
 if mode=="frameSampling"||item.timing.frame_hold.is_some()||(item.timing.speed.abs()-1.).abs()<1e-9{return Ok(None)}
 let rate=rate.sane();let unit=TICKS_PER_SECOND as i128*rate.den as i128;
 if unit/(rate.num as i128)<=0{return Ok(None)}
 let media=item.clocks(at)?.frame;
 let frame=(media as i128*rate.num as i128).div_euclid(unit);
 let tick=|frame:i128|->Result<i64>{i64::try_from(frame.checked_mul(unit).context("frame interpolation clock overflow")?.div_euclid(rate.num as i128)).context("frame interpolation tick overflow")};
 let start=tick(frame)?;let next=tick(frame.checked_add(1).context("frame interpolation frame overflow")?)?;
 let span=next.checked_sub(start).context("frame interpolation span overflow")?.max(1);
 let offset=media.checked_sub(start).context("frame interpolation offset overflow")?;
 let fraction=offset as f64/span as f64;
 if fraction<=1e-6{return Ok(None)}
 Ok(Some(BlendFrame{next,weight:fraction.clamp(0.,1.)as f32}))
}
#[cfg(test)]mod tests{use super::*;use serde_json::Value;
 #[test]fn original_frame_blending_clocks_match(){let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-frame-blend-clock.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),2592);for(index,case)in cases.iter().enumerate(){let item=SourceTrackItem::read(&case["item"]).unwrap();let rate=serde_json::from_value(case["rate"].clone()).unwrap();let result=plan(&item,case["at"].as_i64().unwrap(),rate).unwrap().map(|r|serde_json::json!({"next":r.next,"weightBits":r.weight.to_bits()}));assert_eq!(serde_json::json!(result),case["result"],"clock {index}");}}
}
