//! Policy-bound, early-stopping decoder-frame metadata for source audio curves.
use crate::{domain::{concat_retime::{AudioPiece,FrameFeedPlan},media_probe::{StreamKind,StreamMetadata}},process_control::{capture_lines_until,ProcessRuntime}};
use anyhow::{ensure,Context,Result};
use std::{path::Path,time::Duration};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[derive(Debug,Clone)]
pub struct DecodedFrameSpan {
    pub frame_count:u64,
    pub sample_count:u64,
    pub last_frame_samples:u64,
    pub frame_samples:Vec<u32>,
    pub landed_source_seconds:Option<f64>,
    pub seek_stream_microseconds:i64,
    pub source_exhausted:bool,
    pub feed:FrameFeedPlan,
}
/// Stop once the last complete frame needed by original MixInput is observed.
/// A short source is returned with source_exhausted=true; an empty one refuses.
pub async fn trace_source_piece(runtime:&ProcessRuntime,ffprobe:&Path,source:&Path,stream:&StreamMetadata,piece:&AudioPiece,limit:Duration,cancel:&CancellationToken)->Result<DecodedFrameSpan>{
    ensure!(stream.kind==StreamKind::Audio,"source frame probe needs an audio stream");
    let rate=stream.sample_rate.filter(|v|*v>0).context("source audio sample rate missing")?;
    let base=stream.time_base.filter(|v|v.numerator>0&&v.denominator>0).context("source audio time base missing")?;
    let origin=stream.start_time_seconds.unwrap_or(0.);ensure!(origin.is_finite(),"invalid stream origin");
    let sought=(piece.source_start_seconds+origin)*1e6;
    ensure!(sought.is_finite()&&sought>=i64::MIN as f64&&sought<i64::MAX as f64,"source seek overflow");
    let seek=sought as i64;
    let interval=format!("{:.6}%",seek as f64/1e6);
    let mut command=Command::new(ffprobe);
    command.args(["-v","error","-protocol_whitelist",super::OFFLINE_PROTOCOLS]);
    if piece.source_start_seconds>0.{command.args(["-read_intervals",&interval]);}
    command.args(["-select_streams",&stream.index.to_string(),"-show_frames","-show_entries","frame=best_effort_timestamp,nb_samples","-of","compact=p=0:nk=0"]).arg(source);
    let mut span:Option<DecodedFrameSpan>=None;
    let(completed,diagnostic)=capture_lines_until(runtime,command,runtime.probe_policy(source),limit,cancel,|line|{
        if line.is_empty(){return Ok(None);}
        let(mut stamp,mut samples)=(None,None);
        for field in line.split('|'){
            let Some((name,value))=field.split_once('=')else{continue;};
            match name{"best_effort_timestamp"=>{if value!="N/A"{stamp=Some(value.parse::<i64>().context("invalid decoded timestamp")?);}},"nb_samples"=>samples=Some(value.parse::<u64>().context("invalid decoded frame size")?),_=>{}}
        }
        let samples=samples.context("decoded audio frame sample count missing")?;
        ensure!(samples>0&&samples<=i32::MAX as u64,"invalid decoded audio frame extent");
        if span.is_none(){
            let landed=stamp.map(|stamp|reduced_seconds(stamp,base.numerator,base.denominator)-origin);
            span=Some(DecodedFrameSpan{frame_count:0,sample_count:0,last_frame_samples:0,frame_samples:Vec::new(),landed_source_seconds:landed,seek_stream_microseconds:seek,source_exhausted:false,feed:piece.frame_feed(landed,rate,&[])?});
        }
        let span=span.as_mut().unwrap();
        span.last_frame_samples=samples;
        span.frame_samples.push(samples as u32);
        span.frame_count=span.frame_count.checked_add(1).context("decoded frame count overflow")?;
        span.sample_count=span.sample_count.checked_add(samples).context("decoded sample count overflow")?;
        if span.sample_count>=span.feed.minimum_input_samples as u64{return Ok(Some(span.clone()));}
        Ok(None)
    }).await?;
    ensure!(diagnostic.trim().is_empty(),"source frame decoder: {diagnostic}");
    match completed{Some(span)=>Ok(span),None=>{let mut span=span.context("source has no decoded audio frames after seek")?;span.source_exhausted=true;Ok(span)}}
}
fn reduced_seconds(stamp:i64,num:i64,den:i64)->f64{
    let(mut numerator,mut denominator)=(stamp as i128*num as i128,den as i128);
    let(mut a,mut b)=(numerator.abs(),denominator.abs());while b!=0{let r=a%b;a=b;b=r;}
    numerator/=a;denominator/=a;numerator as f64/denominator as f64
}

/// An owned, bounded lossless capture of exactly the selected decoder frames.
/// Drop removes only the temporary file created by this preparation call.
#[derive(Debug)]
pub struct PreparedAudioFrames {
    path:std::path::PathBuf,
    pub span:DecodedFrameSpan,
}
impl PreparedAudioFrames{pub fn path(&self)->&Path{&self.path}}
impl Drop for PreparedAudioFrames{fn drop(&mut self){let _=std::fs::remove_file(&self.path);}}
#[allow(clippy::too_many_arguments)]
pub async fn prepare_source_piece(runtime:&ProcessRuntime,ffmpeg:&Path,ffprobe:&Path,source:&Path,stream:&StreamMetadata,piece:&AudioPiece,staging:&Path,staging_bytes:u64,limit:Duration,cancel:&CancellationToken)->Result<PreparedAudioFrames>{
    let started=std::time::Instant::now();
    let remaining=||limit.checked_sub(started.elapsed()).filter(|v|!v.is_zero()).context("source frame preparation timed out");
    let span=trace_source_piece(runtime,ffprobe,source,stream,piece,remaining()?,cancel).await?;
    let channels=stream.channels.filter(|v|*v>0&&*v<=64).context("source audio channels missing")?;
    let(codec,bytes)=match stream.sample_format.as_deref(){
        Some("u8"|"u8p")=>("pcm_u8",1u64),Some("s16"|"s16p")=>("pcm_s16le",2),Some("s32"|"s32p")=>("pcm_s32le",4),Some("s64"|"s64p")=>("pcm_s64le",8),Some("flt"|"fltp")=>("pcm_f32le",4),Some("dbl"|"dblp")=>("pcm_f64le",8),_=>anyhow::bail!("source sample format needs lossless frame capture support"),
    };
    let payload=span.sample_count.checked_mul(channels as u64).and_then(|v|v.checked_mul(bytes)).context("source frame payload overflow")?;
    ensure!(payload<=staging_bytes&&staging_bytes<=i64::MAX as u64,"source frame staging budget exceeded");
    let path=staging.join(format!(".concat-frames-{}.nut",uuid::Uuid::new_v4()));
    let reservation=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await?;
    let owned=PreparedAudioFrames{path:path.clone(),span};drop(reservation);
    let mut command=Command::new(ffmpeg);
    command.args(["-nostdin","-v","error","-y","-protocol_whitelist",super::OFFLINE_PROTOCOLS,"-noaccurate_seek"]);
    if piece.source_start_seconds>0.{command.args(["-seek_timestamp","1","-ss",&format!("{:.6}",owned.span.seek_stream_microseconds as f64/1e6)]);}
    command.arg("-i").arg(source).args(["-map",&format!("0:{}",stream.index),"-vn","-sn","-dn","-frames:a",&owned.span.frame_count.to_string(),"-af","asetpts=N/SR/TB","-c:a",codec,"-f","nut","-fs",&staging_bytes.to_string()]).arg(&path);
    let output=crate::process_control::capture_output_cancellable(runtime,command,runtime.render_policy(vec![source.to_path_buf()],path.clone()),remaining()?,cancel).await?;
    ensure!(output.status.success()&&!cancel.is_cancelled(),"source frame capture failed: {}",String::from_utf8_lossy(&output.stderr));
    ensure!(tokio::fs::metadata(&path).await?.len()<=staging_bytes,"source frame file exceeded staging budget");
    let mut probe_command=Command::new(ffprobe);
    probe_command.args(["-v","error","-protocol_whitelist",super::OFFLINE_PROTOCOLS,"-select_streams","a:0","-show_streams","-show_format","-of","json"]).arg(&path);
    let header=crate::process_control::capture_output_cancellable(runtime,probe_command,runtime.probe_policy(&path),remaining()?,cancel).await?;
    ensure!(header.status.success(),"captured frame metadata probe failed: {}",String::from_utf8_lossy(&header.stderr));
    let header:serde_json::Value=serde_json::from_slice(&header.stdout)?;
    let probe=crate::domain::media_probe::ProbeResult::from_ffprobe_json(&header)?;
    let actual=probe.streams.iter().find(|s|s.kind==StreamKind::Audio).context("captured source frames have no audio")?;
    ensure!(actual.sample_rate==stream.sample_rate&&actual.channels==stream.channels,"frame capture changed source audio shape");
    ensure!(actual.sample_format.as_deref().map(|v|v.trim_end_matches('p'))==stream.sample_format.as_deref().map(|v|v.trim_end_matches('p')),"frame capture changed source sample precision");
    let mut verify=*piece;verify.source_start_seconds=0.;verify.speed=1.;verify.duration_seconds=owned.span.sample_count as f64/stream.sample_rate.unwrap()as f64;verify.fade_in_seconds=0.;verify.fade_out_seconds=0.;
    let captured=trace_source_piece(runtime,ffprobe,&path,actual,&verify,remaining()?,cancel).await?;
    ensure!(captured.frame_samples==owned.span.frame_samples,"lossless capture changed decoder frame boundaries");
    Ok(owned)
}
