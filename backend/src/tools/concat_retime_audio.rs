//! Source-owned curve audio stem preparation. Native host role/pan processing
//! belongs after this stem; source gain, fades and delay are already applied.
use crate::{domain::{composition::{AnimatableValue,AudioClip},concat_animation::PreparedTrack,concat_retime::expand_audio,media_probe::StreamMetadata},process_control::ProcessRuntime};
use anyhow::{ensure,Context,Result};
use std::{ops::Range,path::{Path,PathBuf},time::Duration};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
#[derive(Debug)]
pub struct PreparedCurvePcm{path:PathBuf,pub start_sample:u64,pub frames:u64}
impl PreparedCurvePcm{pub fn path(&self)->&Path{&self.path}
    pub(crate) fn into_owned_parts(mut self)->(PathBuf,u64,u64){(std::mem::take(&mut self.path),self.start_sample,self.frames)}
}
impl Drop for PreparedCurvePcm{fn drop(&mut self){if !self.path.as_os_str().is_empty(){let _=std::fs::remove_file(&self.path);}}}
/// Render the original source-owned part of an active audio clip, on its local
/// composition clock. Unsupported active effects refuse instead of disappearing.
#[allow(clippy::too_many_arguments)]
pub async fn prepare_source_curve_stem(runtime:&ProcessRuntime,ffmpeg:&Path,ffprobe:&Path,source:&Path,metadata:&StreamMetadata,clip:&AudioClip,range:Range<u64>,staging:&Path,staging_bytes:u64,limit:Duration,cancel:&CancellationToken)->Result<PreparedCurvePcm>{
    ensure!(clip.enabled&&!cancel.is_cancelled()&&!limit.is_zero(),"source curve stem disabled or cancelled");
    super::concat_curve_effects::validate_effects(&clip.dsp_effects)?;
    let frames=range.end.checked_sub(range.start).filter(|v|*v>0&&*v<=48000*60*60*6).context("invalid source curve output range")?;
    let bytes=frames.checked_mul(8).context("source curve PCM extent overflow")?;
    ensure!(bytes<staging_bytes&&staging_bytes<=8*1024*1024*1024,"source curve stem staging budget exceeded");
    let started=std::time::Instant::now();let remaining=||limit.checked_sub(started.elapsed()).filter(|v|!v.is_zero()).context("source curve stem timed out");
    let pieces=expand_audio(clip)?;
    let rest=match clip.gain{AnimatableValue::Constant{value}=>value,_=>anyhow::bail!("source curve native gain rewindowing is not compiled")};
    let mut command=Command::new(ffmpeg);command.args(["-nostdin","-v","error","-y","-protocol_whitelist",super::OFFLINE_PROTOCOLS]);
    let mut captures=Vec::new();let mut processed_captures=Vec::new();let mut inputs=Vec::new();let mut chains=Vec::new();let mut captured_bytes=0u64;
    for(index,piece)in pieces.iter().enumerate(){
        let available=staging_bytes.checked_sub(bytes).and_then(|v|v.checked_sub(captured_bytes)).context("source capture budget exhausted")?;
        let capture=super::concat_audio_frames::prepare_source_piece(runtime,ffmpeg,ffprobe,source,metadata,&piece.source,staging,available,remaining()?,cancel).await?;
        captured_bytes=captured_bytes.checked_add(tokio::fs::metadata(capture.path()).await?.len()).context("source capture byte count overflow")?;
        ensure!(captured_bytes<=staging_bytes-bytes,"source captures exceeded aggregate budget");
        let middle=PreparedTrack::new(&piece.clip.concat_gain_keys)?.audio_volume_filter(piece.source.duration_seconds,rest)?.into_iter().collect::<Vec<_>>();
        let feed=piece.source.frame_feed(capture.span.landed_source_seconds,metadata.sample_rate.context("source rate missing")?,&middle)?;
        if clip.dsp_effects.iter().any(|s|!s.bypass){
            let post=middle.len()+usize::from(piece.source.fade_in_seconds>0.)+usize::from(piece.source.fade_out_seconds>0.)+1;
            let prefix=feed.filters.len()-post;
            let available=staging_bytes.checked_sub(bytes).and_then(|v|v.checked_sub(captured_bytes)).context("source effect capture budget exhausted")?;
            let processed=super::concat_curve_effects::prepare(runtime,ffmpeg,ffprobe,&capture,metadata,&feed.filters[..prefix],!middle.is_empty(),pieces.len()>1,&clip.dsp_effects,staging,available,remaining()?,cancel).await?;
            captured_bytes=captured_bytes.checked_add(processed.bytes()).context("source effect captures overflow")?;
            command.arg("-i").arg(processed.path());inputs.push(processed.path().to_path_buf());
            chains.push(format!("[{index}:a]asettb=1/{},aformat=sample_fmts={},{}[a{index}]",metadata.sample_rate.context("source effect rate missing")?,processed.sample_format(),feed.filters[prefix..].join(",")));processed_captures.push(processed);
        }else{
            command.arg("-i").arg(capture.path());inputs.push(capture.path().to_path_buf());
            chains.push(format!("[{index}:a]{}[a{index}]",feed.filters.join(",")));
        }
        captures.push(capture);
    }
    ensure!(!captures.is_empty(),"source curve stem has no pieces");
    let labels=(0..captures.len()).map(|i|format!("[a{i}]")).collect::<String>();
    chains.push(if captures.len()==1{format!("{labels}anull[mix]")}else{format!("{labels}amix=inputs={}:normalize=0[mix]",captures.len())});
    chains.push(format!("[mix]aresample=48000,apad,aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo,atrim=start_sample={}:end_sample={},asetpts=PTS-STARTPTS[out]",range.start,range.end));
    let path=staging.join(format!(".concat-curve-{}.f32le",uuid::Uuid::new_v4()));
    let reservation=tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await?;
    let output=PreparedCurvePcm{path:path.clone(),start_sample:range.start,frames};drop(reservation);
    command.args(["-filter_complex",&chains.join(";"),"-map","[out]","-c:a","pcm_f32le","-f","f32le","-fs",&bytes.to_string()]).arg(&path);
    let result=crate::process_control::capture_output_cancellable(runtime,command,runtime.render_policy(inputs,path.clone()),remaining()?,cancel).await?;
    ensure!(result.status.success()&&!cancel.is_cancelled(),"source curve stem render failed: {}",String::from_utf8_lossy(&result.stderr));
    ensure!(tokio::fs::metadata(&path).await?.len()==bytes,"source curve stem produced incomplete PCM");
    drop(captures);Ok(output)
}
