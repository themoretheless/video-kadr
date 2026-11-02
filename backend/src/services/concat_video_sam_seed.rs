//! Resolve a source instant to the same rational ordinal used by native SAM movies.
use anyhow::{ensure,Result};
use std::{path::Path,time::Duration};
use tokio_util::sync::CancellationToken;
#[derive(Debug,Clone,serde::Serialize)]
#[serde(rename_all="camelCase")]
pub struct Seed{#[serde(skip_serializing_if="Option::is_none")]pub frame_times_us:Option<Vec<u64>>,pub source_us:u64,pub seed_frame:u64,pub frames:u64,pub frame_rate:crate::domain::composition::FrameRateRatio}
pub async fn resolve(runtime:&crate::process_control::ProcessRuntime,source:&Path,source_us:u64,cancel:&CancellationToken)->Result<Seed>{
 let tools=super::local_enhance_video::resolve_tools().await?;
 let(info,json)=super::local_enhance_video::probe(runtime,&tools.ffprobe,source,tokio::time::Instant::now()+Duration::from_secs(30),cancel).await?;
 let frames=super::local_enhance_video::frames(&json)?;
 let rate=info.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Video).and_then(|s|s.frame_rate).ok_or_else(||anyhow::anyhow!("SAM source frame rate missing"))?;
 let frame_rate=crate::domain::composition::FrameRateRatio{numerator:rate.numerator.try_into()?,denominator:rate.denominator.try_into()?};
 ensure!(frame_rate.numerator>0&&frame_rate.denominator>0&&frames>0&&frames<=360000,"SAM source clock exceeds budget");
 let video=info.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Video).ok_or_else(||anyhow::anyhow!("SAM video stream missing"))?;
 let tick=video.time_base.filter(|t|t.numerator>0&&t.denominator>0).map(|t|t.numerator as f64/t.denominator as f64).unwrap_or(0.);
 let frame_times_us=super::sam_video_clock::probe(runtime,&tools.ffprobe,source,frames,frame_rate,tick+0.000002,tokio::time::Instant::now()+Duration::from_secs(30),cancel).await?;
 let seed=crate::domain::video_region::frame_ordinal(source_us,frames,frame_rate,frame_times_us.as_deref())?;
 Ok(Seed{source_us,seed_frame:seed,frames,frame_rate,frame_times_us})
}
