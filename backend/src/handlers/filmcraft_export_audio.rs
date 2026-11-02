//! Bounded source-project DSP -> stereo 48 kHz float PCM for the video encoder.
//! File decoding is asynchronous; native graph state stays on one CPU worker.
use super::filmcraft_video_export::VideoRequest;
use crate::{
    domain::{
        filmcraft_project_audio::SourceProjectAudioRuntime, filmcraft_project_index::ProjectIndex,
        filmcraft_wav_audio::SourceWavPool,
    },
    state::AppState,
};
use anyhow::{ensure, Context, Result};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
const T: i128 = 254_016_000_000;
struct ReadRequest {
    id: u64,
    start: i64,
    frames: usize,
    rate: u32,
    reply: std::sync::mpsc::SyncSender<Result<Vec<Vec<f32>>>>,
}
#[derive(Clone)]
struct FileSource {
    path: PathBuf,
}
pub(super) struct PreparedAudio {
    pub path: PathBuf,
    pub frames: u64,
    pub loudness: Option<crate::tools::native_loudness_export::LoudnessReport>,
}
impl Drop for PreparedAudio {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
struct Window {
    id: u64,
    rate: u32,
    start: i64,
    end: i64,
    channels: usize,
    bytes: Arc<Vec<u8>>,
}
fn remember(cache: &mut std::collections::VecDeque<Window>, window: Window) {
    while cache.iter().map(|v| v.bytes.len()).sum::<usize>() + window.bytes.len() > 32 * 1024 * 1024
    {
        if cache.pop_front().is_none() {
            break;
        }
    }
    cache.push_back(window);
}
async fn channels(
    state: &AppState,
    path: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<(usize, u32)> {
    let mut command = tokio::process::Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            crate::tools::OFFLINE_PROTOCOLS,
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=channels,sample_rate",
            "-of",
            "json",
        ])
        .arg(path);
    let output = crate::process_control::capture_output_cancellable(
        &state.process_runtime,
        command,
        state.process_runtime.probe_policy(path),
        timeout,
        cancel,
    )
    .await?;
    ensure!(output.status.success(), "audio source probe failed");
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let channels = usize::try_from(
        value["streams"][0]["channels"]
            .as_u64()
            .context("source audio channels missing")?,
    )?;
    ensure!(
        (1..=16).contains(&channels),
        "unsupported source audio channels"
    );
    let rate = value["streams"][0]["sample_rate"]
        .as_str()
        .context("source audio rate missing")?
        .parse::<u32>()?;
    ensure!(rate > 0 && rate <= 768000, "unsupported source audio rate");
    Ok((channels, rate))
}
async fn decode(
    state: &AppState,
    path: &Path,
    folder: &Path,
    start: i64,
    frames: usize,
    rate: u32,
    channels: usize,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Vec<u8>> {
    let target = folder.join(format!(
        ".filmcraft-audio-decode-{}.f32le",
        uuid::Uuid::new_v4()
    ));
    let _cleanup = Temporary(target.clone());
    let file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&target)
        .await?;
    drop(file);
    let end = start
        .checked_add(i64::try_from(frames)?)
        .context("audio decode clock overflow")?;
    let maximum = frames
        .checked_mul(channels)
        .and_then(|v| v.checked_mul(4))
        .context("audio decode byte overflow")?;
    let mut command = tokio::process::Command::new("ffmpeg");
    command.args(["-nostdin","-v","error","-y","-protocol_whitelist",crate::tools::OFFLINE_PROTOCOLS,"-i"]).arg(path).args(["-map","0:a:0","-vn","-sn","-dn","-af"]).arg(format!("asetpts=PTS-STARTPTS,aresample={rate},atrim=start_sample={start}:end_sample={end},asetpts=PTS-STARTPTS")).args(["-ar"]).arg(rate.to_string()).arg("-ac").arg(channels.to_string()).args(["-c:a","pcm_f32le","-f","f32le","-fs"]).arg(maximum.to_string()).args(["-progress","pipe:1"]).arg(&target);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let (status, diagnostic) = crate::process_control::stream_with_progress(
        &state.process_runtime,
        command,
        state
            .process_runtime
            .render_policy(vec![path.to_path_buf()], &target),
        |_| None,
        &tx,
        cancel,
        timeout,
    )
    .await?;
    ensure!(
        matches!(status, crate::process_control::ProcessStatus::Ok) && !cancel.is_cancelled(),
        "audio source decode failed: {status:?} {diagnostic}"
    );
    let size = tokio::fs::metadata(&target).await?.len();
    ensure!(
        size <= maximum as u64 && size % (channels as u64 * 4) == 0,
        "audio decode byte budget exceeded"
    );
    Ok(tokio::fs::read(&target).await?)
}
/// Caller has already authorized/snapshotted the root bindings, and will recheck them.
pub(super) async fn prepare(
    state: &AppState,
    request: &VideoRequest,
    start_ticks: i64,
    frames: u64,
    folder: &Path,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &tokio::sync::mpsc::UnboundedSender<f64>,
) -> Result<PreparedAudio> {
    let output_channels=request.audio_channels()? as usize;
    let frame_bytes=output_channels as u64*4;
    ensure!(
        frames > 0
            && frames
                .checked_mul(frame_bytes)
                .is_some_and(|n| n <= 8 * 1024 * 1024 * 1024),
        "audio staging budget exceeded"
    );
    let sequence = request.sequence_id.parse::<u64>()?;
    let mut document = request.document()?;
    // Source AudioOut::new clones the root sequence at the resolved output
    // rate before DSP; other presets retain their 48 kHz contract.
    let rate = request.audio_sample_rate()?;
    *document.envelope.pointer_mut(&format!("/project/items/{sequence}/kind/Sequence/settings/sample_rate"))
        .context("export sample rate missing")? = serde_json::json!(rate);
    let index = ProjectIndex::new(&document)?;
    let mut files = BTreeMap::new();
    let mut silent = std::collections::BTreeSet::new();
    for (item, id) in &request.sources {
        let media = index
            .resolve_media(item.parse()?)
            .context("audio source missing")?;
        if media.media["info"]["audio"].is_null() {
            silent.insert(item.parse::<u64>()?);
            continue;
        }
        let entry = state
            .library
            .get(id)
            .await
            .context("audio library source missing")?;
        let path = state.library.resolve_media_path(&entry).await?;
        files.insert(item.parse::<u64>()?, FileSource { path });
    }
    let path = folder.join(format!(
        ".filmcraft-audio-mix-{}.f32le",
        uuid::Uuid::new_v4()
    ));
    let prepared = PreparedAudio {
        path: path.clone(),
        frames,
        loudness: None,
    };
    let child = cancel.child_token();
    let _cancel = child.clone().drop_guard();
    let worker_cancel = child.clone();
    let worker_files = files.clone();
    let signal = progress.clone();
    let mix_scale=if request.loudness.enabled{0.4}else{1.0};
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ReadRequest>();
    let work = state.cpu_pool.execute(child.clone(), move |_| {
        let mut native = SourceWavPool::default();
        let mut memory = 128 * 1024 * 1024u64;
        for (id, source) in &worker_files {
            ensure!(!worker_cancel.is_cancelled(), "audio mix cancelled");
            let mut file = std::fs::File::open(&source.path)?;
            let mut head = [0; 12];
            if file.read_exact(&mut head).is_ok()
                && matches!(&head[..4], b"RIFF" | b"RF64")
                && &head[8..] == b"WAVE"
            {
                let bytes = file.metadata()?.len();
                if bytes <= memory && native.bind_file(*id, &source.path, bytes.max(1)).is_ok() {
                    memory -= bytes;
                }
            }
        }
        let mut runtime = SourceProjectAudioRuntime::new(document)?;
        let mut reader = |id, start, count, source_rate| -> Result<Option<Vec<Vec<f32>>>> {
            ensure!(!worker_cancel.is_cancelled(), "audio mix cancelled");
            if silent.contains(&id) {
                return Ok(None);
            }
            if let Some(audio) = native.read(id, start, count, source_rate)? {
                return Ok(Some(audio));
            }
            ensure!(worker_files.contains_key(&id), "unbound audio source {id}");
            let (reply, receive) = std::sync::mpsc::sync_channel(1);
            tx.send(ReadRequest {
                id,
                start,
                frames: count,
                rate: source_rate,
                reply,
            })
            .context("audio decoder unavailable")?;
            loop {
                ensure!(!worker_cancel.is_cancelled(), "audio mix cancelled");
                match receive.recv_timeout(Duration::from_millis(100)) {
                    Ok(result) => return result.map(Some),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => anyhow::bail!("audio decoder disconnected"),
                }
            }
        };
        let origin = i128::from(start_ticks) * i128::from(rate);
        let first = i64::try_from(origin.div_euclid(T))?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut output = std::io::BufWriter::new(file);
        let mix_width=if output_channels==6 {6}else{2};
        let mut done=0u64;
        while done<frames {
            ensure!(!worker_cancel.is_cancelled(),"audio mix cancelled");
            let _=signal.send(100.*mix_scale*done as f64/frames as f64);
            let start=first.checked_add(i64::try_from(done)?).context("audio sample clock overflow")?;
            let page=runtime.render_media_channels(sequence,start,4096,mix_width,&mut reader)?;
            ensure!(page.len()==mix_width&&page.iter().all(|c|c.len()==4096),"invalid export audio bus");
            let count=(frames-done).min(4096) as usize;
            for i in 0..count {for c in 0..output_channels {
                let value=if output_channels==1 {0.5*(page[0][i]+page[1][i])}else{page[c][i]};
                ensure!(value.is_finite(),"nonfinite rendered audio");output.write_all(&value.to_le_bytes())?;
            }}
            done+=count as u64;
        }
        output.flush()?;
        output.get_ref().sync_all()?;
        let _ = signal.send(100. * mix_scale);
        Ok(())
    });
    let mut channel_cache = BTreeMap::new();
    let mut cache: std::collections::VecDeque<Window> = std::collections::VecDeque::new();
    let deadline = tokio::time::Instant::now()
        .checked_add(timeout)
        .context("audio deadline overflow")?;
    let remaining = || {
        deadline
            .checked_duration_since(tokio::time::Instant::now())
            .context("audio export timeout")
    };
    tokio::pin!(work);
    loop {
        tokio::select! {
            result=&mut work=>{result.map_err(|e|anyhow::anyhow!("native audio mix: {e}"))?;break},
            _=child.cancelled()=>anyhow::bail!("audio mix cancelled"),
            _=tokio::time::sleep_until(deadline)=>anyhow::bail!("audio export timeout"),
            Some(read)=rx.recv()=>{
                let result=async{
                    ensure!(read.rate>0&&read.frames<=4_194_304,"invalid source audio request");let file=files.get(&read.id).context("unbound audio source")?;
                    let (channels,source_rate)=if let Some(value)=channel_cache.get(&read.id){*value}else{let value=channels(state,&file.path,remaining()?,&child).await?;channel_cache.insert(read.id,value);value};
                    ensure!(read.frames.checked_mul(channels).is_some_and(|n|n<=8_388_608),"source audio request exceeds budget");
                    read.start.checked_add(i64::try_from(read.frames)?).context("audio read clock overflow")?;
                    let ratio=source_rate as f64/read.rate as f64;
                    let mut out=vec![vec![0.;read.frames];channels];let mut window:Option<Window>=None;
                    for i in 0..read.frames {
                        if i%4096==0{ensure!(!child.is_cancelled(),"audio decode cancelled");}
                        let position=(read.start+i as i64)as f64*ratio;if position<0.{continue}
                        ensure!(position.is_finite()&&position<(i64::MAX-2)as f64,"source audio clock overflow");let at=position.floor()as i64;let fraction=(position-at as f64)as f32;
                        if !window.as_ref().is_some_and(|v|at>=v.start&&at+1<v.end){
                            if let Some(old)=window.take(){remember(&mut cache,old);}
                            let hit=cache.iter().position(|v|v.id==read.id&&v.rate==source_rate&&at>=v.start&&at+1<v.end);
                            window=Some(if let Some(hit)=hit{cache.remove(hit).unwrap()}else{
                                let count=(source_rate as usize*2).max(2).min(8_388_608/channels);let bytes=decode(state,&file.path,folder,at,count,source_rate,channels,remaining()?,&child).await?;
                                Window{id:read.id,rate:source_rate,start:at,end:at.checked_add(count as i64).context("audio cache clock overflow")?,channels,bytes:Arc::new(bytes)}
                            });
                        }
                        let window=window.as_ref().unwrap();let frame=usize::try_from(at-window.start)?;
                        for(c,channel)in out.iter_mut().enumerate(){let at=(frame*window.channels+c)*4;if let Some(bytes)=window.bytes.get(at..at+4){let a=f32::from_le_bytes(bytes.try_into()?);let b=window.bytes.get(at+window.channels*4..at+window.channels*4+4).map(|v|f32::from_le_bytes(v.try_into().unwrap())).unwrap_or(a);let value=a+(b-a)*fraction;ensure!(value.is_finite(),"nonfinite decoded audio");channel[i]=value;}}
                    }
                    if let Some(window)=window{remember(&mut cache,window);}Ok(out)
                }.await;
                let _=read.reply.send(result);
            }
        }
    }
    ensure!(
        tokio::fs::metadata(&prepared.path).await?.len() == frames * frame_bytes,
        "audio mix byte count mismatch"
    );
    if request.loudness.enabled {
        let path=folder.join(format!(".filmcraft-normalized-{}.f32le",uuid::Uuid::new_v4()));
        let report=crate::tools::native_loudness_export::normalize_layout(&prepared.path,frames,rate,output_channels,request.loudness,&path,deadline.checked_duration_since(tokio::time::Instant::now()).context("loudness timeout")?,&child,progress,state.cpu_pool.clone()).await?;
        return Ok(PreparedAudio{path,frames,loudness:Some(report)});
    }
    Ok(prepared)
}
