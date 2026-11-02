//! Owned stereo PCM staging for the focused native nested audio-track plan.
use super::pcm_conform::{ConformedPcm, PcmConformSpec, conform_pcm_with_tempo_and_stream};
use crate::{
    domain::{
        composition::SourceId,
        nested_audio::{plan_audio_tracks, plan_audio_tracks_with_context},
        nested_composition::NestedCompositionProject,
        nested_dsp::{Mixer, validate_export_processing},
        nested_pcm::mix_block_with_video_transitions as mix_block,
    },
    process_control::ProcessRuntime,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;
const RATE: u64 = 48000;
/// Use the default stream when any active layer needs it; otherwise probe an explicit selection.
pub(crate) fn preferred_audio_stream(layers:&[crate::domain::nested_audio::NestedAudioLayer],id:&SourceId)->Option<u32>{
 let mut found=false;let mut selected=None;
 for layer in layers.iter().filter(|layer|layer.source_id()==id){if layer.clip.audio_stream.is_none(){return None}found=true;selected=Some(selected.map_or(layer.clip.audio_stream.unwrap(),|old:u32|old.min(layer.clip.audio_stream.unwrap())));}
 if found{selected}else{None}
}
fn bound_stream(source:&PcmSource)->Option<u32>{source.probe.as_ref()?.streams.iter().find(|stream|stream.kind==crate::domain::media_probe::StreamKind::Audio).map(|stream|stream.index)}
/// Resolved trusted local path and probed channel count; never client path input.
#[derive(Debug, Clone)]
pub struct PcmSource {
    pub path: PathBuf,
    pub channels: u32,
    /// Trusted fresh ffprobe result; legacy test bindings can omit this metadata.
    pub probe: Option<crate::domain::media_probe::ProbeResult>,
}
impl PcmSource {
    pub fn from_probe(path: PathBuf, probe: crate::domain::media_probe::ProbeResult) -> Result<Self> {
        let channels=probe.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Audio)
            .and_then(|s|s.channels).context("native audio source has no probed channels")?;
        ensure!(matches!(channels,1|2),"native audio source requires mono/stereo");
        Ok(Self {path,channels,probe:Some(probe)})
    }
}
#[derive(Debug)]
pub struct MixedPcm {
    path: PathBuf,
    pub frames: u64,
    file: Option<std::fs::File>,
}
impl MixedPcm {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for MixedPcm {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}
struct Bridge {
    flag: Arc<AtomicBool>,
    watch: tokio::task::JoinHandle<()>,
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.flag.store(true, Ordering::Relaxed);
        self.watch.abort();
    }
}
#[allow(clippy::too_many_arguments)]
pub async fn mix_project_audio_tracks(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    staging: &Path,
    project: &NestedCompositionProject,
    focus: &str,
    range: Range<u64>,
    output_frames: u64,
    sources: &BTreeMap<SourceId, PcmSource>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
) -> Result<MixedPcm> {
    mix_project_audio_tracks_with_fade_clock(runtime,ffmpeg,staging,project,focus,range,output_frames,sources,staging_bytes,timeout,cancel,progress,true).await
}
#[allow(clippy::too_many_arguments)]
pub(crate) async fn mix_project_audio_tracks_with_fade_clock(
    runtime: &ProcessRuntime,
    ffmpeg: &Path,
    staging: &Path,
    project: &NestedCompositionProject,
    focus: &str,
    range: Range<u64>,
    output_frames: u64,
    sources: &BTreeMap<SourceId, PcmSource>,
    staging_bytes: u64,
    timeout: Duration,
    cancel: &CancellationToken,
    progress: &UnboundedSender<f64>,
    export_fade_clock:bool,
) -> Result<MixedPcm> {
    let selected = plan_audio_tracks(project, focus, range.clone())?;
    validate_export_processing(&selected)?;
    let has_dsp=selected.iter().any(|layer|layer.clip.dsp_effects.iter().any(|e|!e.bypass));
    let layers=if has_dsp {plan_audio_tracks_with_context(project,focus,range.clone())?}else{selected};
    mix_prepared_audio_layers(runtime,ffmpeg,staging,layers,range,output_frames,sources,staging_bytes,timeout,cancel,progress,export_fade_clock).await
}
/// Trusted native layer entry point. Source IDs must already be authorized by
/// the caller; retained source owners are compiled before ordinary DSP admission.
#[allow(clippy::too_many_arguments)]
pub async fn mix_prepared_audio_layers(
    runtime:&ProcessRuntime,ffmpeg:&Path,staging:&Path,
    mut layers:Vec<crate::domain::nested_audio::NestedAudioLayer>,range:Range<u64>,
    output_frames:u64,sources:&BTreeMap<SourceId,PcmSource>,staging_bytes:u64,
    timeout:Duration,cancel:&CancellationToken,progress:&UnboundedSender<f64>,export_fade_clock:bool,
)->Result<MixedPcm>{
    ensure!(!cancel.is_cancelled()&&!timeout.is_zero(),"native audio cancelled or zero timeout");
    ensure!(range.start<range.end&&layers.len()<=4096,"invalid native audio layer window");
    let deadline=std::time::Instant::now().checked_add(timeout).context("native audio deadline overflow")?;

    ensure!(
        output_frames > 0
            && output_frames <= 48000 * 60 * 60 * 6
            && staging_bytes > 0
            && staging_bytes <= 8 * 1024 * 1024 * 1024,
        "invalid native audio output budget"
    );
    let start_sample =
        u64::try_from((u128::from(range.start) * u128::from(RATE)).div_ceil(1_000_000))?;
    let requested_frames =
        (u128::from(range.end) * u128::from(RATE)).div_ceil(1_000_000)
            - u128::from(start_sample);
    ensure!(
        u128::from(output_frames) >= requested_frames,
        "native audio output cannot truncate selected interval"
    );
    // Authorize original IDs in callers; aliases are private decoder bindings only.
    let mut sources=sources.clone();let mut selections=BTreeMap::<(SourceId,u32),SourceId>::new();
    for layer in &mut layers {
        let Some(index)=layer.clip.audio_stream else{continue};
        // Transition providers resolve side clip IDs; only their private source
        // binding changes here, so every side keeps its own selected stream.
        let original=layer.source_id().clone();let key=(original.clone(),index);
        let alias=if let Some(alias)=selections.get(&key){alias.clone()}else{
            let source=sources.get(&original).context("selected audio source binding missing")?;
            let probe=if bound_stream(source)==Some(index){source.probe.clone().context("selected audio metadata missing")?}else{
                let remaining=deadline.checked_duration_since(std::time::Instant::now()).context("native audio stream probe timeout")?;
                super::pcm_conform::probe_pcm_metadata_for_stream(runtime,Path::new("ffprobe"),&source.path,remaining,cancel,Some(index)).await?
            };
            let binding=PcmSource::from_probe(source.path.clone(),probe)?;
            let hash=crate::domain::artifact_graph::Fingerprint::digest(&serde_json::to_vec(&(original.as_str(),index))?);
            let mut serial=0;let mut alias=SourceId::parse(format!("stream-{}",hash.as_str()))?;
            while sources.contains_key(&alias){serial+=1;alias=SourceId::parse(format!("stream-{}-{serial}",hash.as_str()))?;}
            sources.insert(alias.clone(),binding);selections.insert(key,alias.clone());alias
        };
        layer.clip.source_id=alias;
    }
    let mut curve_readers=BTreeMap::<SourceId,ConformedPcm>::new();
    let mut held_curve_bytes=output_frames.checked_mul(8).context("source owner output budget overflow")?;
    for layer in &mut layers {
        let Some(owner)=layer.concat_retime_owner else{continue};
        let local_start=u64::try_from(i128::from(layer.root_range_us.start)-owner.root_offset_us)?;
        let local_end=u64::try_from(i128::from(layer.root_range_us.end)-owner.root_offset_us)?;
        let start=u64::try_from(u128::from(local_start)*u128::from(RATE)/1_000_000)?;
        let end=u64::try_from((u128::from(local_end)*u128::from(RATE)).div_ceil(1_000_000))?.checked_add(1).context("source owner guard overflow")?;
        let source=sources.get(layer.source_id()).context("source owner binding missing")?;
        let probe=if let Some(probe)=&source.probe{probe.clone()}else{
            let remaining=deadline.checked_duration_since(std::time::Instant::now()).context("source owner probe timeout")?;
            super::pcm_conform::probe_pcm_metadata_for_stream(runtime,Path::new("ffprobe"),&source.path,remaining,cancel,layer.clip.audio_stream).await?
        };
        let metadata=probe.streams.iter().find(|s|s.kind==crate::domain::media_probe::StreamKind::Audio).context("source owner audio metadata missing")?;
        let budget=staging_bytes.checked_sub(held_curve_bytes).context("source owner aggregate staging budget exceeded")?;
        let remaining=deadline.checked_duration_since(std::time::Instant::now()).context("source owner preparation timeout")?;
        let stem=super::concat_retime_audio::prepare_source_curve_stem(runtime,ffmpeg,Path::new("ffprobe"),&source.path,metadata,&layer.clip,start..end,staging,budget,remaining,cancel).await?;
        held_curve_bytes=held_curve_bytes.checked_add(stem.frames.checked_mul(8).context("source owner bytes overflow")?).context("source owner aggregate overflow")?;
        let reader=ConformedPcm::adopt_source_stem(stem,budget)?;
        let mut alias;
        loop{alias=SourceId::parse(format!("curve-{}",uuid::Uuid::new_v4()))?;if !sources.contains_key(&alias){break}}
        sources.insert(alias.clone(),PcmSource{path:reader.staged_path().into(),channels:2,probe:None});
        curve_readers.insert(alias.clone(),reader);
        layer.clip.source_id=alias;
        let duration=u64::try_from((u128::from(end)*1_000_000).div_ceil(u128::from(RATE)))?;
        layer.clip.placement=crate::domain::composition::ClipPlacement{concat_source_eof: false,
timeline_start_tick:0,source_in_tick:0,source_out_tick:duration,speed:1.,timeline_duration_tick:Some(duration),source_timeline_offset_tick:None,source_head_timeline_tick:None,speed_ramp:None,concat_retime:None};
        layer.clip_local_start_us=local_start;
        layer.clip.gain=crate::domain::composition::AnimatableValue::Constant{value:1.};
        layer.clip.concat_gain_keys.clear();layer.clip.fade_in_ticks=0;layer.clip.fade_out_ticks=0;layer.clip.dsp_effects.clear();
        layer.gain_track=None;layer.concat_retime_owner=None;layer.concat_retime_piece=None;
    }
    validate_export_processing(&layers)?;
    let has_dsp=layers.iter().any(|layer|layer.clip.dsp_effects.iter().any(|e|!e.bypass));
    // Keep packet state per clip instance; two clips sharing media must not share
    // their recursive effects, trim origin or source packet cursor.
    let mut packet_layers=Vec::new();
    let mut ordinary_layers=Vec::new();
    for mut layer in layers {
        layer.export_fade_clock=export_fade_clock;
        if super::packet_clip::resolve_chain(&layer.clip.dsp_effects)?.is_some() {
            packet_layers.push(layer);
        } else {
            layer.clip.dsp_effects.retain(|spec|!super::packet_clip::is_effect(&spec.id));
            ordinary_layers.push(layer);
        }
    }
    let mut layers=ordinary_layers;
    ensure!(layers.len()+packet_layers.len()<=4096,"native audio layer budget exceeded");
    let active=layers.iter().chain(&packet_layers).map(|l|l.clip.dsp_effects.iter().filter(|e|!e.bypass).count()).sum::<usize>();
    ensure!(active<=128,"native audio active effect budget exceeded");
    let mut tempo_sources: BTreeMap<SourceId, (SourceId, String)> = BTreeMap::new();
    let mut tempo_keys: BTreeMap<String, SourceId> = BTreeMap::new();
    for layer in &mut layers {
        let p = &layer.clip.placement;
        if p.speed == 1.0 && p.speed_ramp.is_none() { continue; }
        let head_time = p.source_head_timeline_ticks()?;
        let (graph, duration) = super::composition_args::native_tempo_source_filter(p, layer.clip.reversed)?;
        let key = serde_json::to_string(&(layer.source_id(), &graph))?;
        let alias = if let Some(alias) = tempo_keys.get(&key) { alias.clone() } else {
            let mut alias;
            loop { alias = SourceId::parse(format!("tempo-{}", uuid::Uuid::new_v4()))?; if !sources.contains_key(&alias) && !tempo_sources.contains_key(&alias) { break; } }
            tempo_sources.insert(alias.clone(), (layer.source_id().clone(), graph));
            tempo_keys.insert(key, alias.clone());
            alias
        };
        layer.clip.source_id = alias;
        layer.clip.placement.source_timeline_offset_tick = Some(i64::try_from(i128::from(layer.clip.placement.source_timeline_offset_tick.unwrap_or(0)) + i128::from(head_time)).context("prepared tempo head phase overflow")?);
        layer.clip.placement.source_head_timeline_tick = None;
        layer.clip.placement.source_in_tick = 0;
        layer.clip.placement.source_out_tick = duration;
        layer.clip.placement.speed = 1.0;
        layer.clip.placement.speed_ramp = None;
        layer.clip.reversed = false;
    }
    let mut specs: BTreeMap<SourceId, PcmConformSpec> = BTreeMap::new();
    for layer in &layers {
        let first = layer
            .sample(layer.root_range_us.start)?
            .context("native audio first sample missing")?
            .source_time_us;
        let last = layer
            .sample(layer.root_range_us.end - 1)?
            .context("native audio last sample missing")?
            .source_time_us;
        let start = u64::try_from(u128::from(first.min(last)) * u128::from(RATE) / 1_000_000)?;
        let end = u64::try_from(
            (u128::from(first.max(last) + 1) * u128::from(RATE)).div_ceil(1_000_000),
        )?;
        let prepared = tempo_sources.get(layer.source_id());
        let source = sources
            .get(prepared.map(|(id, _)| id).unwrap_or(layer.source_id()))
            .context("native audio source binding missing")?;
        let channels = if prepared.is_some() { 2 } else { source.channels };
        specs
            .entry(layer.source_id().clone())
            .and_modify(|spec| {
                spec.start_sample = spec.start_sample.min(start);
                spec.end_sample = spec.end_sample.max(end)
            })
            .or_insert(PcmConformSpec {
                start_sample: start,
                end_sample: end,
                channels,
                staging_bytes,
            });
    }
    let mut bytes = output_frames
        .checked_mul(8)
        .context("native audio output size overflow")?;
    for (id,spec) in &specs {
        spec.validate()?;
        bytes = bytes
            .checked_add(if let Some(reader)=curve_readers.get(id){reader.decoded_frames()*8}else{(spec.end_sample - spec.start_sample) * u64::from(spec.channels) * 4})
            .context("native audio staging sum overflow")?;
    }
    let mut packet_budgets=Vec::with_capacity(packet_layers.len());
    for layer in &packet_layers {
        let source=sources.get(layer.source_id()).context("packet audio source binding missing")?;
        let probe=source.probe.as_ref().context("packet audio requires trusted source metadata")?;
        let (format,rate,channels)=super::packet_pcm::Format::from_probe(probe)?;
        ensure!(channels as u32==source.channels,"packet audio metadata channel mismatch");
        let mut admitted=layer.clone();admitted.clip.dsp_effects.clear();
        let bounds=super::packet_clip::source_bounds(&admitted,rate)?;
        let scalar_bytes=match format {super::packet_pcm::Format::U8=>1,super::packet_pcm::Format::S16=>2,super::packet_pcm::Format::S32|super::packet_pcm::Format::F32=>4,super::packet_pcm::Format::F64=>8};
        let budget=(bounds.end-bounds.start).checked_mul(channels as u64 * scalar_bytes)
            .and_then(|n|n.checked_add(64*1024*1024)).context("packet audio staging budget overflow")?;
        bytes=bytes.checked_add(budget).context("native audio aggregate staging overflow")?;
        let effect=super::packet_clip::resolve_chain(&layer.clip.dsp_effects)?.context("packet effect missing")?;
        bytes=bytes.checked_add(super::packet_clip::live_memory_bytes(effect)).context("native audio DSP reservation overflow")?;
        packet_budgets.push(budget);
    }
    ensure!(
        bytes <= staging_bytes,
        "native audio aggregate staging budget exceeded"
    );
    let stream_relative_sources:std::collections::BTreeSet<_>=selections.values().cloned().collect();
    let mut readers: BTreeMap<SourceId, ConformedPcm> = BTreeMap::new();
    for (index, (id, spec)) in specs.iter().enumerate() {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .context("native audio timeout")?;
        let allow_empty_source=layers.iter().any(|layer|layer.source_id()==id&&layer.clip.placement.concat_source_eof);
        let reader = if let Some(reader)=curve_readers.remove(id){reader}else if let Some((source, graph)) = tempo_sources.get(id) {
            conform_pcm_with_tempo_and_stream(runtime, ffmpeg, &sources[source].path, staging, *spec, remaining, cancel, Some(graph),bound_stream(&sources[source]),stream_relative_sources.contains(source),allow_empty_source).await?
        } else {
            conform_pcm_with_tempo_and_stream(runtime, ffmpeg, &sources[id].path, staging, *spec, remaining, cancel,None,bound_stream(&sources[id]),stream_relative_sources.contains(id),allow_empty_source).await?
        };
        readers.insert(id.clone(), reader);
        let _ = progress.send(30.0 * (index + 1) as f64 / specs.len() as f64);
    }
    let mut packet_processors=Vec::with_capacity(packet_layers.len());
    for (layer,budget) in packet_layers.into_iter().zip(packet_budgets) {
        let source=sources.get(layer.source_id()).context("packet audio source binding missing")?;
        let remaining=deadline.checked_duration_since(std::time::Instant::now()).context("native audio timeout")?;
        packet_processors.push(super::packet_clip::prepare_authored(runtime,ffmpeg,staging,layer,source,budget,remaining,cancel).await?);
    }
    let flag = Arc::new(AtomicBool::new(cancel.is_cancelled()));
    let watch_flag = flag.clone();
    let watch_cancel = cancel.clone();
    let remaining = deadline
        .checked_duration_since(std::time::Instant::now())
        .context("native audio timeout")?;
    let _bridge = Bridge {
        flag: flag.clone(),
        watch: tokio::spawn(async move {
            tokio::select! {_=watch_cancel.cancelled()=>{},_=tokio::time::sleep(remaining)=>{}}
            watch_flag.store(true, Ordering::Relaxed);
        }),
    };
    let path = staging.join(format!(".mix-{}.f32le", uuid::Uuid::new_v4()));
    let progress = progress.clone();
    tokio::task::spawn_blocking(move || -> Result<MixedPcm> {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut output = MixedPcm {
            path,
            frames: output_frames,
            file: Some(file),
        };
        let mut dsp_mixer = if has_dsp {
            let inputs = layers
                .iter()
                .map(|layer| {
                    let channels = readers
                        .get(layer.source_id())
                        .context("DSP reader missing")?
                        .channels() as usize;
                    Ok((
                        layer.clone(),
                        channels,
                        crate::domain::filmcraft_audio_mapping::native_export_preroll_at(
                            &layer.clip.dsp_effects,
                            RATE as u32,
                            (range.start as f64 + layer.clip_local_start_us as f64 - layer.crossfade.head_ticks as f64
                                - layer.root_range_us.start as f64)
                                / 1_000_000.0,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            Some(Mixer::new(RATE as u32, inputs)?)
        } else {
            None
        };
        // An absent packet bus contributes no PCM; a synthetic +0 would
        // destroy signed zero from an otherwise unchanged prepared stem.
        let has_packet_output=packet_processors.iter().any(|processor|processor.layer().video_transitions.is_none());
        let mut written = 0u64;
        while written < output_frames {
            ensure!(
                !flag.load(Ordering::Relaxed) && std::time::Instant::now() < deadline,
                "native audio cancelled or timed out"
            );
            let count = (output_frames - written).min(2048) as usize;
            let position = start_sample
                .checked_add(written)
                .context("native audio mix clock overflow")?;
            let mut transition_sides=Vec::new();
            let mut packet_output=if has_packet_output{vec![[0f64;2];count]}else{Vec::new()};
            for processor in &mut packet_processors {
                let layer=processor.layer().clone();
                let processed=processor.mix(position,count,&mut || {
                    ensure!(!flag.load(Ordering::Relaxed) && std::time::Instant::now()<deadline,"native packet audio cancelled or timed out");
                    Ok(())
                })?;
                if layer.video_transitions.is_some(){transition_sides.push((layer,processed));}else{
                    for(out,frame)in packet_output.iter_mut().zip(processed){out[0]+=frame[0];out[1]+=frame[1];ensure!(out.iter().all(|v|v.is_finite()),"native packet audio mix overflow");}
                }
            }
            let mut block = if let Some(mixer) = &mut dsp_mixer {
                mixer.mix_with_transition_sides(
                    position,
                    count,
                    &mut |id, time| {
                        readers
                            .get_mut(id)
                            .context("native audio reader missing")?
                            .read(time)
                    },
                    &mut || {
                        ensure!(
                            !flag.load(Ordering::Relaxed) && std::time::Instant::now() < deadline,
                            "native audio cancelled or timed out"
                        );
                        Ok(())
                    },
                    transition_sides,
                )?
            } else {
                ensure!(transition_sides.is_empty(),"packet transition sides require processed mixer");
                mix_block(
                    &layers,
                    start_sample
                        .checked_add(written)
                        .context("native audio mix clock overflow")?,
                    count,
                    RATE as u32,
                    &flag,
                    |id, time| {
                        readers
                            .get_mut(id)
                            .context("native audio reader missing")?
                            .read(time)
                    },
                )?
            };
            for(out,frame)in block.iter_mut().zip(packet_output){out[0]+=frame[0];out[1]+=frame[1];ensure!(out.iter().all(|v|v.is_finite()),"native packet audio mix overflow");}
            // Context is input-only: container frame padding must not leak sound
            // beyond the user's selected interval, even for nonzero DSP tails.
            for (index, frame) in block.iter_mut().enumerate() {
                let time = u128::from(position + index as u64) * 1_000_000 / u128::from(RATE);
                if time >= u128::from(range.end) {
                    *frame = [0.0; 2];
                }
            }
            let mut data = Vec::with_capacity(count * 8);
            for frame in block {
                for value in frame {
                    let sample = value as f32;
                    ensure!(sample.is_finite(), "native audio float32 mix overflow");
                    data.extend(sample.to_le_bytes());
                }
            }
            output
                .file
                .as_mut()
                .context("native audio writer closed")?
                .write_all(&data)?;
            written += count as u64;
            let _ = progress.send(30.0 + 70.0 * written as f64 / output_frames as f64);
        }
        output
            .file
            .as_mut()
            .context("native audio writer closed")?
            .flush()?;
        drop(output.file.take());
        ensure!(
            !flag.load(Ordering::Relaxed) && std::time::Instant::now() < deadline,
            "native audio cancelled or timed out before handoff"
        );
        Ok(output)
    })
    .await
    .context("native audio mixer worker stopped")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::composition::{CanvasSpec, Composition, TransformSpec};
    pub(super) fn fixture(source: PathBuf) -> (NestedCompositionProject, BTreeMap<SourceId, PcmSource>) {
        let mut child = Composition::new(CanvasSpec::default());
        child.sources=serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"audio","durationTicks":1000000,"width":0,"height":0,"hasAudio":true,"audioChannels":2}})).unwrap();
        child.tracks=serde_json::from_value(serde_json::json!([{"kind":"audio","id":"audio","name":"Audio","clips":[{"id":"clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":600000,"speed":1.0},"gain":{"mode":"constant","value":0.5},"roleGain":2.0,"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0}]}])).unwrap();
        let project=serde_json::from_value(serde_json::json!({"schemaVersion":1,"root":"root","compositions":{"root":{"durationUs":1000000,"content":Composition::new(CanvasSpec::default()),"instances":[{"id":"group","timing":{"composition":"child","timelineStartUs":250000,"sourceInUs":100000,"sourceOutUs":600000},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0.0},"blendMode":"normal"}]},"child":{"durationUs":1000000,"content":child,"instances":[]}}})).unwrap();
        (
            project,
            BTreeMap::from([(
                SourceId::parse("tone").unwrap(),
                PcmSource {
                    path: source,
                    channels: 2,
                    probe: None,
                },
            )]),
        )
    }
    fn wav(path: &Path) {
        let bytes = 48000u32 * 4;
        let mut data = Vec::new();
        data.extend(b"RIFF");
        data.extend((36 + bytes).to_le_bytes());
        data.extend(b"WAVEfmt ");
        data.extend(16u32.to_le_bytes());
        data.extend(1u16.to_le_bytes());
        data.extend(2u16.to_le_bytes());
        data.extend(48000u32.to_le_bytes());
        data.extend(192000u32.to_le_bytes());
        data.extend(4u16.to_le_bytes());
        data.extend(16u16.to_le_bytes());
        data.extend(b"data");
        data.extend(bytes.to_le_bytes());
        for i in 0..48000 {
            let value = match i / 16000 {
                0 => 8192i16,
                1 => 16384,
                _ => 24576,
            };
            data.extend(value.to_le_bytes());
            data.extend((-value).to_le_bytes());
        }
        std::fs::write(path, data).unwrap();
    }
    async fn mix(
        project: &NestedCompositionProject,
        sources: &BTreeMap<SourceId, PcmSource>,
        dir: &Path,
    ) -> MixedPcm {
        let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            dir,
            project,
            "root",
            200000..800000,
            33600,
            sources,
            1024 * 1024,
            Duration::from_secs(5),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap()
    }
    fn sample(bytes: &[u8], frame: usize) -> [f32; 2] {
        std::array::from_fn(|channel| {
            f32::from_le_bytes(
                bytes[frame * 8 + channel * 4..frame * 8 + channel * 4 + 4]
                    .try_into()
                    .unwrap(),
            )
        })
    }
    #[tokio::test]
    async fn real_nested_audio_mix_preserves_group_window_markers_split_and_padded_silence() {
        if !std::process::Command::new("ffmpeg")
            .arg("-version")
            .output()
            .is_ok_and(|v| v.status.success())
        {
            eprintln!("FFmpeg unavailable: native audio mix fixture skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("tone.wav");
        wav(&source);
        let (project, sources) = fixture(source.clone());
        let output = mix(&project, &sources, dir.path()).await;
        let owned = output.path().to_path_buf();
        assert_eq!(output.frames, 33600);
        let before = std::fs::read(output.path()).unwrap();
        assert_eq!(before.len(), 33600 * 8);
        for frame in [0, 2399, 21600, 33599] {
            assert_eq!(sample(&before, frame), [0.0, 0.0]);
        }
        assert_eq!(sample(&before, 2400), [0.25, -0.25]);
        assert_eq!(sample(&before, 21599), [0.5, -0.5]);
        let embedded_source = dir.path().join("embedded.mkv");
        let encoded = std::process::Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(&source)
            .args([
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=2x2:r=2:d=1",
                "-map",
                "1:v:0",
                "-map",
                "0:a:0",
                "-c:v",
                "ffv1",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&embedded_source)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let mut embedded = project.clone();
        let child = embedded.compositions.get_mut("child").unwrap();
        child.content.sources = serde_json::from_value(serde_json::json!({"tone":{"id":"tone","kind":"video","durationTicks":1000000,"width":2,"height":2,"hasAudio":true,"audioChannels":2}})).unwrap();
        child.content.tracks = serde_json::from_value(serde_json::json!([{"kind":"video","id":"video","name":"Video","clips":[{"id":"video-clip","sourceId":"tone","placement":{"timelineStartTick":0,"sourceInTick":100000,"sourceOutTick":600000,"speed":1.0},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":0.0},"blendMode":"normal","sourceAudioEnabled":true,"audioGain":{"mode":"constant","value":1.0},"audioPan":{"mode":"constant","value":0.0}}]}])).unwrap();
        let embedded_bindings = BTreeMap::from([(
            SourceId::parse("tone").unwrap(),
            PcmSource {
                path: embedded_source,
                channels: 2,
                probe: None,
            },
        )]);
        let embedded_mix = mix(&embedded, &embedded_bindings, dir.path()).await;
        assert_eq!(std::fs::read(embedded_mix.path()).unwrap(), before);
        drop(embedded_mix);
        let crate::domain::composition::CompositionTrack::Video { clips, .. } = &mut embedded
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!()
        };
        clips[0].playback_mode = crate::domain::composition::PlaybackMode::Freeze {
            source_tick: 100000,
        };
        let silent = mix(&embedded, &BTreeMap::new(), dir.path()).await;
        assert!(
            std::fs::read(silent.path())
                .unwrap()
                .iter()
                .all(|value| *value == 0)
        );
        drop(silent);
        let mut visual = project.clone();
        let root = visual.compositions.get_mut("root").unwrap();
        root.content.canvas.width = 2;
        root.content.canvas.height = 2;
        root.content.canvas.fps_milli = 1429;
        root.content.canvas.fps_rate = Some(crate::domain::composition::FrameRateRatio {
            numerator: 10,
            denominator: 7,
        });
        let combined = dir.path().join("combined.mkv");
        let (mux_progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        crate::tools::native_rgba_export::encode_project_with_audio(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            &combined,
            visual,
            "root".into(),
            BTreeMap::new(),
            BTreeMap::new(),
            200000..800000,
            1024 * 1024,
            Duration::from_secs(5),
            &CancellationToken::new(),
            &mux_progress,
            Some(crate::tools::native_rgba_export::RasterAudio {
                path: output.path(),
                frames: output.frames,
            }),
        )
        .await
        .unwrap();
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&combined)
            .args([
                "-map",
                "0:a:0",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout, before);
        let pixels = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&combined)
            .args([
                "-map", "0:v:0", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(pixels.status.success());
        assert_eq!(pixels.stdout.len(), 16);
        let split = project
            .split_instance("root", "group", "right", 400000)
            .unwrap();
        let output2 = mix(&split, &sources, dir.path()).await;
        assert_eq!(std::fs::read(output2.path()).unwrap(), before);
        drop(output2);
        let mut reverse = project.clone();
        let crate::domain::composition::CompositionTrack::Audio { clips, .. } = &mut reverse
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!()
        };
        clips[0].reversed = true;
        let reversed = mix(&reverse, &sources, dir.path()).await;
        let bytes = std::fs::read(reversed.path()).unwrap();
        assert_eq!(sample(&bytes, 2400), [0.5, -0.5]);
        assert_eq!(sample(&bytes, 21599), [0.25, -0.25]);
        drop(reversed);
        let mut overflowing = project.clone();
        let crate::domain::composition::CompositionTrack::Audio { clips, .. } = &mut overflowing
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!()
        };
        clips[0].role_gain = 1e40;
        let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        let failed = mix_project_audio_tracks(
            &ProcessRuntime::local_default(),
            Path::new("ffmpeg"),
            dir.path(),
            &overflowing,
            "root",
            200000..800000,
            33600,
            &sources,
            1024 * 1024,
            Duration::from_secs(5),
            &CancellationToken::new(),
            &progress,
        )
        .await;
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("float32 mix overflow")
        );
        drop(output);
        assert!(!owned.exists());
        assert!(source.exists());
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|v| {
            let n = v.unwrap().file_name();
            n.to_string_lossy().starts_with(".mix-") || n.to_string_lossy().starts_with(".pcm-")
        }));
    }
    #[tokio::test]
    async fn admission_refuses_aggregate_budget_missing_bindings_and_dsp_before_decode() {
        let dir = tempfile::tempdir().unwrap();
        let (mut project, sources) = fixture(dir.path().join("missing.wav"));
        let (progress, _rx) = tokio::sync::mpsc::unbounded_channel();
        let runtime = ProcessRuntime::local_default();
        let cancel = CancellationToken::new();
        let result = mix_project_audio_tracks(
            &runtime,
            Path::new("missing-ffmpeg"),
            dir.path(),
            &project,
            "root",
            200000..800000,
            33600,
            &sources,
            1,
            Duration::from_secs(5),
            &cancel,
            &progress,
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("budget"));
        let empty = BTreeMap::new();
        let result = mix_project_audio_tracks(
            &runtime,
            Path::new("missing-ffmpeg"),
            dir.path(),
            &project,
            "root",
            200000..800000,
            33600,
            &empty,
            1024 * 1024,
            Duration::from_secs(5),
            &cancel,
            &progress,
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("binding"));
        let crate::domain::composition::CompositionTrack::Audio { clips, .. } = &mut project
            .compositions
            .get_mut("child")
            .unwrap()
            .content
            .tracks[0]
        else {
            panic!()
        };
        clips[0].voice_effect = crate::domain::composition::AudioVoiceEffect::Echo;
        let result = mix_project_audio_tracks(
            &runtime,
            Path::new("missing-ffmpeg"),
            dir.path(),
            &project,
            "root",
            200000..800000,
            33600,
            &sources,
            1024 * 1024,
            Duration::from_secs(5),
            &cancel,
            &progress,
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("DSP"));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod packet_delivery_tests {
    use super::*;
    #[tokio::test]
    async fn authored_packet_effects_export_real_source_and_selected_ranges() {
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("source.wav");
        let generated=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-f","lavfi","-i","aevalsrc=1.4*sin(2*PI*317*t)|0.7*cos(2*PI*997*t):s=44100:d=1","-c:a","pcm_f64le"]).arg(&source).output().unwrap();
        assert!(generated.status.success(),"{}",String::from_utf8_lossy(&generated.stderr));
        let runtime=ProcessRuntime::local_default();let cancel=CancellationToken::new();
        let probe=super::super::pcm_conform::probe_pcm_metadata(&runtime,Path::new("ffprobe"),&source,Duration::from_secs(10),&cancel).await.unwrap();
        for (id,pitch,suffix) in [("concat.pitch",7f64,""),("concat.chipmunk",7.,",highpass=f=120,alimiter=limit=0.94"),("concat.deep",-3.,",highpass=f=55,equalizer=f=140:t=q:w=1.0:g=3.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60")] {
            let (mut project,mut sources)=super::tests::fixture(source.clone());
            sources.get_mut(&SourceId::parse("tone").unwrap()).unwrap().probe=Some(probe.clone());
            let crate::domain::composition::CompositionTrack::Audio{clips,..}=&mut project.compositions.get_mut("child").unwrap().content.tracks[0] else {panic!()};
            clips[0].dsp_effects=vec![crate::domain::filmcraft_dsp::EffectSpec{ concat_source_keys: Default::default(),id:id.into(),params:if id=="concat.pitch" {BTreeMap::from([("semitones".into(),pitch as f32)])} else {BTreeMap::new()},..Default::default()}];
            crate::domain::filmcraft_dsp::validate_specs(&clips[0].dsp_effects).unwrap();
            let ratio=2f64.powf(pitch/12.);
            let chain=format!("atrim=start_sample=4410:end_sample=26460,asetpts=PTS-STARTPTS,aresample=48000,asetrate={:.6},aresample=48000,atempo={:.8}{suffix}",48000.*ratio,1./ratio);
            let expected=std::process::Command::new("ffmpeg").args(["-nostdin","-v","error","-i"]).arg(&source).args(["-af",&chain,"-c:a","pcm_f32le","-f","f32le","pipe:1"]).output().unwrap();
            assert!(expected.status.success(),"{}",String::from_utf8_lossy(&expected.stderr));
            let reference:Vec<[f32;2]>=expected.stdout.chunks_exact(8).map(|b|std::array::from_fn(|ch|f32::from_le_bytes(b[ch*4..ch*4+4].try_into().unwrap()))).collect();
            let (progress,_rx)=tokio::sync::mpsc::unbounded_channel();
            let full=mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&project,"root",200000..800000,33600,&sources,256*1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap();
            let bytes=std::fs::read(full.path()).unwrap();
            for (i,frame) in bytes.chunks_exact(8).enumerate() {
                let clock=9600+i;
                let expected=if (12000..31200).contains(&clock) {reference.get(clock-7200).copied().unwrap_or([0.;2])} else {[0.;2]};
                for ch in 0..2 {
                    let actual=f32::from_le_bytes(frame[ch*4..ch*4+4].try_into().unwrap());
                    assert!((actual-expected[ch]).abs()<=2e-7,"{id} frame{i} ch{ch}: {actual} vs {}",expected[ch]);
                }
            }
            let selected=mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&project,"root",300000..400000,4800,&sources,256*1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap();
            assert_eq!(std::fs::read(selected.path()).unwrap(),bytes[4800*8..9600*8],"{id} selected range must replay original DSP context");
            let mut dry_project=project.clone();
            let crate::domain::composition::CompositionTrack::Audio{clips,..}=&mut dry_project.compositions.get_mut("child").unwrap().content.tracks[0] else {panic!()};
            clips[0].dsp_effects[0].bypass=true;
            let mut dry_clip=clips[0].clone();dry_clip.id=serde_json::from_value(serde_json::json!("dry-clip")).unwrap();
            let dry=mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&dry_project,"root",200000..800000,33600,&sources,256*1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap();
            let dry_bytes=std::fs::read(dry.path()).unwrap();
            let mut combined_project=project.clone();
            combined_project.compositions.get_mut("child").unwrap().content.tracks.push(serde_json::from_value(serde_json::json!({"kind":"audio","id":"dry-track","name":"Dry","clips":[dry_clip]})).unwrap());
            let combined=mix_project_audio_tracks(&runtime,Path::new("ffmpeg"),dir.path(),&combined_project,"root",200000..800000,33600,&sources,256*1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap();
            let combined_bytes=std::fs::read(combined.path()).unwrap();
            for ((a,b),mixed) in bytes.chunks_exact(4).zip(dry_bytes.chunks_exact(4)).zip(combined_bytes.chunks_exact(4)) {
                let a=f32::from_le_bytes(a.try_into().unwrap());let b=f32::from_le_bytes(b.try_into().unwrap());
                let mixed=f32::from_le_bytes(mixed.try_into().unwrap());
                assert!((mixed-(a+b)).abs()<=5e-7,"{id}: mixed packet/ordinary source {mixed} vs {}",a+b);
            }
            drop(combined);drop(dry);drop(selected);drop(full);
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1,"packet staging must be removed after export");
            let budget_error=mix_project_audio_tracks(&runtime,Path::new("missing-ffmpeg"),dir.path(),&project,"root",200000..800000,33600,&sources,1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap_err();
            assert!(budget_error.to_string().contains("aggregate staging budget"),"{budget_error}");
            sources.get_mut(&SourceId::parse("tone").unwrap()).unwrap().probe=None;
            let err=mix_project_audio_tracks(&runtime,Path::new("missing-ffmpeg"),dir.path(),&project,"root",200000..800000,33600,&sources,256*1024*1024,Duration::from_secs(20),&cancel,&progress).await.unwrap_err();
            assert!(err.to_string().contains("trusted source metadata"),"{err}");
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(),1);
        }
    }
}

#[cfg(test)]
#[path = "packet_clip_context_tests.rs"]
mod packet_context_tests;
