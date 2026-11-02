//! Native stereo/5.1 routing of source audio tracks, submixes and master.
//! Input provider owns clip decoding/effects and nested-sequence layout.
use super::{
    filmcraft_audio_inserts::{InsertHost, SourceAudioInserts, SourceInsertRuntime},
    filmcraft_master_volume::SourceMasterVolume,
    filmcraft_sequence_source::SourceSequence,
    filmcraft_stereo_strip::SourceStereoStrip,
    filmcraft_surround_route::{SourceSurroundSend,SourceSurroundRoute},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
fn stereo_width()->usize{2}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Strip {
    #[serde(default="stereo_width")]
    width:usize,
    #[serde(default)]
    output:Option<SourceSurroundRoute>,
    id: u64,
    processor: SourceStereoStrip,
    target: usize,
    audio: bool,
    audible: bool,
    sends: Vec<Send>,
    inserts: SourceAudioInserts,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Send {
    target: usize,
    pre: bool,
    route: SourceSurroundSend,
}
impl Send {
    fn render(&self, source: &[Vec<f32>], width:usize,start: i64, rate: u32) -> Result<Vec<Vec<f32>>> {
        self.route.render(source, width, start, rate)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceStereoGraph {
    #[serde(default="stereo_width")]
    master_width:usize,
    strips: Vec<Strip>,
    master: SourceMasterVolume,
    master_inserts: SourceAudioInserts,
}
impl SourceStereoGraph {
    pub fn compile(sequence: &SourceSequence<'_>) -> Result<Self> {
        ensure!(
            matches!(
                sequence
                    .raw
                    .pointer("/settings/audio_master")
                    .and_then(Value::as_str),
                Some("Stereo" | "Mono" | "Adaptive" | "Surround51")
            ),
            "master requires surround/adaptive graph"
        );
        let empty_effects = json!([]);
        let empty_mixer = json!({});
        let master_inserts = SourceAudioInserts::compile(
            sequence.raw.get("master_effects").unwrap_or(&empty_effects),
            sequence.raw.get("master_mixer").unwrap_or(&empty_mixer),
            InsertHost::Master,
        )?;
        let tracks: Vec<_> = sequence
            .audio_tracks
            .iter()
            .chain(&sequence.submix_tracks)
            .collect();
        let count = tracks.len();
        let n_audio = sequence.audio_tracks.len();
        let mut ids = std::collections::BTreeSet::new();
        let mut strips = Vec::new();
        let mut solos = Vec::new();
        let mut safe = Vec::new();
        for (i, track) in tracks.iter().enumerate() {
            ensure!(
                track.id != u64::MAX && ids.insert(track.id),
                "duplicate/reserved mixer strip ID"
            );
            let inserts = SourceAudioInserts::compile(
                track.raw.get("effects").unwrap_or(&empty_effects),
                track.raw.get("mixer").unwrap_or(&empty_mixer),
                InsertHost::Track,
            )?;
            let mut sends = Vec::new();
            if let Some(raw_sends) = track.raw.pointer("/mixer/sends") {
                for (slot, send) in raw_sends
                    .as_array()
                    .context("invalid sends")?
                    .iter()
                    .take(5)
                    .enumerate()
                {
                    let id = send
                        .get("target")
                        .and_then(Value::as_u64)
                        .context("invalid send target")?;
                    let target = tracks
                        .iter()
                        .enumerate()
                        .skip(n_audio)
                        .find(|(_, track)| track.id == id)
                        .map(|(index, _)| index)
                        .filter(|target| i < n_audio || *target > i);
                    let Some(target) = target else {
                        continue;
                    };
                    let route=SourceSurroundSend::read(track.raw,slot)?.context("missing send route")?;
                    sends.push(Send {target,pre:route.pre_fader,route});
                }
            }
            let output = match track.raw.pointer("/mixer/output") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().context("invalid mixer route ID")?),
            };
            let target = output
                .and_then(|id| {
                    tracks
                        .iter()
                        .enumerate()
                        .skip(n_audio)
                        .find(|(_, t)| t.id == id)
                        .map(|(i, _)| i)
                })
                .filter(|target| i < n_audio || *target > i)
                .unwrap_or(count);
            let flag = |key: &str, default: bool| -> Result<bool> {
                Ok(track
                    .raw
                    .pointer(key)
                    .map(|v| v.as_bool().context("invalid strip flag"))
                    .transpose()?
                    .unwrap_or(default))
            };
            solos.push(flag("/solo", false)?);
            safe.push(flag("/mixer/solo_safe", false)?);
            strips.push(Strip {
                width:if track.raw["channels"].as_str()==Some("Surround51"){6}else{2},
                output:Some(SourceSurroundRoute::read(track.raw)?),
                id: track.id,
                processor: SourceStereoStrip::read(track.raw)?,
                target,
                audio: i < n_audio,
                audible: true,
                sends,
                inserts,
            });
        }
        if solos.iter().any(|v| *v) {
            let seeds: Vec<_> = (0..count).filter(|i| solos[*i] || safe[*i]).collect();
            let mut keep = vec![false; count + 1];
            for seed in seeds {
                keep[seed] = true;
                let mut stack = vec![seed];
                while let Some(at) = stack.pop() {
                    if at == count {
                        continue;
                    }
                    for target in std::iter::once(strips[at].target)
                        .chain(strips[at].sends.iter().map(|send| send.target))
                    {
                        if !keep[target] {
                            keep[target] = true;
                            stack.push(target);
                        }
                    }
                }
                let mut stack = vec![seed];
                while let Some(target) = stack.pop() {
                    for (i, strip) in strips.iter().enumerate() {
                        if (strip.target == target
                            || strip.sends.iter().any(|send| send.target == target))
                            && !keep[i]
                        {
                            keep[i] = true;
                            stack.push(i);
                        }
                    }
                }
            }
            for (strip, audible) in strips.iter_mut().zip(keep) {
                strip.audible = audible;
            }
        }
        for strip in &mut strips {
            if strip.processor.statically_muted() && !strip.sends.iter().any(|send| send.pre) {
                strip.audible = false;
            }
        }
        Ok(Self {
            master_width:if sequence.raw.pointer("/settings/audio_master").and_then(Value::as_str)==Some("Surround51"){6}else{2},
            strips,
            master: SourceMasterVolume::from_sequence(sequence)?,
            master_inserts,
        })
    }
    pub fn channels(&self)->usize{self.master_width}
    pub fn uses_surround(&self)->bool{self.master_width==6||self.strips.iter().any(|s|s.width==6)}
    fn width(&self,index:usize)->usize{self.strips.get(index).map_or(self.master_width,|s|s.width)}
    fn buses(&self,frames:usize)->Vec<Vec<Vec<f32>>>{(0..=self.strips.len()).map(|i|vec![vec![0.;frames];self.width(i)]).collect()}
    fn output(&self,strip:&Strip,mut bus:Vec<Vec<f32>>,start:i64,rate:u32)->Result<Vec<Vec<f32>>>{
        if let Some(route)=&strip.output{route.output(&bus,self.width(strip.target),start,rate)}else{
            strip.processor.apply_pan_only(&mut bus,start,rate)?;Ok(bus)
        }
    }
    /// Provider supplies the pre-strip clip bus for each audio track at this range.
    pub fn render(
        &self,
        start: i64,
        frames: usize,
        rate: u32,
        mut input: impl FnMut(u64) -> Result<Vec<Vec<f32>>>,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(rate > 0, "invalid audio rate");
        ensure!(
            self.master_inserts.is_empty()
                && self.strips.iter().all(|strip| strip.inserts.is_empty()),
            "insert graph requires SourceStereoGraphRuntime"
        );
        let mut buses = self.buses(frames);
        for (i, strip) in self.strips.iter().enumerate() {
            if !strip.audible {
                continue;
            }
            let mut bus = if strip.audio {
                // Original audio graph ignores the track's video visibility flag.
                input(strip.id)?
            } else {
                std::mem::take(&mut buses[i])
            };
            ensure!(
                bus.len() == strip.width && bus.iter().all(|c| c.len() == frames),
                "invalid graph input bus"
            );
            strip
                .processor
                .apply_before_pan(&mut bus, start, rate, |pre, source| {
                    for send in strip.sends.iter().filter(|send| send.pre == pre) {
                        let signal = send.render(source, self.width(send.target),start, rate)?;
                        for (out, channel) in buses[send.target].iter_mut().zip(signal) {
                            for (out, value) in out.iter_mut().zip(channel) {
                                *out += value;
                            }
                        }
                    }
                    Ok(())
                })?;
            let bus=self.output(strip,bus,start,rate)?;
            for (out, channel) in buses[strip.target].iter_mut().zip(bus) {
                for (out, value) in out.iter_mut().zip(channel) {
                    *out += value;
                }
            }
        }
        let mut output = buses.pop().expect("master bus");
        self.master.apply_bus(&mut output, start, rate)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        filmcraft_project_file::SourceProjectDocument, filmcraft_project_index::ProjectIndex,
    };
    use super::*;
    #[test]
    fn original_compensated_insert_graph_pcm_matches() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-stereo-insert-graph.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 330);
        let mut failures = vec![];
        for case in cases {
            let doc = SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                .unwrap();
            let index = ProjectIndex::new(&doc).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            let graph = SourceStereoGraph::compile(&sequence).unwrap();
            let graph = serde_json::from_value(serde_json::to_value(graph).unwrap()).unwrap();
            let mut runtime = SourceStereoGraphRuntime::new(graph, 48000).unwrap();
            let output = runtime
                .render(case["start"].as_i64().unwrap(), 257, |id, start, frames| {
                    let side = match id {
                        10 => 0,
                        11 => 1,
                        _ => panic!("unexpected source {id}"),
                    };
                    Ok((0..2)
                        .map(|channel| {
                            (0..frames)
                                .map(|i| {
                                    let n = start + i as i64;
                                    if !(0..480000).contains(&n) {
                                        0.
                                    } else if channel == 0 {
                                        (((n + side * 7) % 31) - 15) as f32 / 32.
                                    } else {
                                        (((n + side * 11) % 23) - 7) as f32 / 64.
                                    }
                                })
                                .collect()
                        })
                        .collect())
                })
                .unwrap();
            let bits: Vec<Vec<u32>> = output
                .iter()
                .map(|c| c.iter().map(|v| v.to_bits()).collect())
                .collect();
            if serde_json::to_value(bits).unwrap() != case["bits"] {
                failures.push(format!(
                    "{} host={} post={}",
                    case["effect"], case["host"], case["post"]
                ));
            }
        }
        assert!(failures.is_empty(), "insert graph PCM: {failures:?}");
    }
    #[test]
    fn original_graph_continuation_and_seek_pcm_matches() {
        let oracle: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-stereo-graph-seek.json"
        ))
        .unwrap();
        assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 330);
        let mut failures = vec![];
        for case in cases {
            let doc = SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                .unwrap();
            let index = ProjectIndex::new(&doc).unwrap();
            let sequence = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
            let graph = SourceStereoGraph::compile(&sequence).unwrap();
            let graph = serde_json::from_value(serde_json::to_value(graph).unwrap()).unwrap();
            let mut runtime = SourceStereoGraphRuntime::new(graph, 48000).unwrap();
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                assert_eq!(
                    request["reused"].as_bool(),
                    Some(request_index == 1),
                    "original oracle cache isolation"
                );
                let output = runtime
                    .render(
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        |id, start, frames| {
                            let side = match id {
                                10 => 0,
                                11 => 1,
                                _ => panic!("unexpected source {id}"),
                            };
                            Ok((0..2)
                                .map(|channel| {
                                    (0..frames)
                                        .map(|i| {
                                            let n = start + i as i64;
                                            if !(0..480000).contains(&n) {
                                                0.
                                            } else if channel == 0 {
                                                (((n + side * 7) % 31) - 15) as f32 / 32.
                                            } else {
                                                (((n + side * 11) % 23) - 7) as f32 / 64.
                                            }
                                        })
                                        .collect()
                                })
                                .collect())
                        },
                    )
                    .unwrap();
                let bits: Vec<Vec<u32>> = output
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != request["bits"] {
                    failures.push(format!(
                        "{} host={} post={} start={}",
                        case["effect"], case["host"], case["post"], request["start"]
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "insert graph PCM: {failures:?}");
    }
    #[test]
    fn original_multitrack_submix_solo_graph_pcm_matches() {
        for fixture in [
            include_str!("../../tests/fixtures/filmcraft-stereo-graph.json"),
            include_str!("../../tests/fixtures/filmcraft-stereo-send-graph.json"),
        ] {
            let oracle: Value = serde_json::from_str(fixture).unwrap();
            assert_eq!(oracle["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
            let cases = oracle["cases"].as_array().unwrap();
            assert_eq!(cases.len(), 432);
            let mut failures = vec![];
            for (i, case) in cases.iter().enumerate() {
                let doc =
                    SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                        .unwrap();
                let index = ProjectIndex::new(&doc).unwrap();
                let seq = SourceSequence::read(&index, case["id"].as_u64().unwrap()).unwrap();
                let graph = SourceStereoGraph::compile(&seq).unwrap();
                let graph: SourceStereoGraph =
                    serde_json::from_value(serde_json::to_value(graph).unwrap()).unwrap();
                let input = |id| -> Result<Vec<Vec<f32>>> {
                    let input = case["inputs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| v["id"].as_u64() == Some(id))
                        .unwrap();
                    let bits: Vec<Vec<u32>> =
                        serde_json::from_value(input["bits"].clone()).unwrap();
                    Ok(bits
                        .into_iter()
                        .map(|c| c.into_iter().map(f32::from_bits).collect())
                        .collect())
                };
                let start = case["start"].as_i64().unwrap();
                let rate = case["rate"].as_u64().unwrap() as u32;
                let out = graph.render(start, 31, rate, input).unwrap();
                let bits: Vec<Vec<u32>> = out
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != case["bits"] {
                    failures.push(i);
                }
                assert_eq!(
                    serde_json::from_slice::<Value>(&doc.encode_preserved().unwrap()).unwrap(),
                    case["document"]
                );
            }
            assert!(failures.is_empty(), "graph PCM cases: {failures:?}");
        }
    }
}

struct Delay {
    ring: Vec<Vec<f32>>,
    position: usize,
}
impl Delay {
    fn new(samples: usize,channels:usize) -> Self {
        Self {
            ring: vec![vec![0.; samples]; channels],
            position: 0,
        }
    }
    fn clear(&mut self) {
        for channel in &mut self.ring {
            channel.fill(0.);
        }
    }
    fn apply(&mut self, bus: &mut [Vec<f32>]) {
        let length = self.ring[0].len();
        if length == 0 {
            return;
        }
        let mut end = self.position;
        for (ring, channel) in self.ring.iter_mut().zip(bus) {
            let mut at = self.position;
            for value in channel {
                std::mem::swap(&mut ring[at], value);
                at = (at + 1) % length;
            }
            end = at;
        }
        self.position = end;
    }
}
struct RuntimeNode {
    input_latency: usize,
    pre_latency: usize,
    post_latency: usize,
    inserts: SourceInsertRuntime,
}
struct RuntimeRoute {
    source: usize,
    target: usize,
    send: Option<usize>,
    delay: Delay,
}
/// Stateful stereo/5.1 graph. Provider reads pre-strip clip audio by absolute range.
/// Owns graph seek reconstruction, source preroll, insert look-ahead and per-route
/// compensation. Project nesting and live edit/recording are separate integrations.
pub struct SourceStereoGraphRuntime {
    graph: SourceStereoGraph,
    rate: u32,
    nodes: Vec<RuntimeNode>,
    routes: Vec<RuntimeRoute>,
    latency: usize,
    next: Option<i64>,
    dirty: bool,
}
impl SourceStereoGraphRuntime {
    pub fn new(graph: SourceStereoGraph, rate: u32) -> Result<Self> {
        let mut runtime = Self {
            graph,
            rate,
            nodes: vec![],
            routes: vec![],
            latency: 0,
            next: None,
            dirty: false,
        };
        runtime.rebuild()?;
        Ok(runtime)
    }
    fn rebuild(&mut self) -> Result<()> {
        ensure!(matches!(self.graph.master_width,2|6)&&self.graph.strips.iter().all(|s|matches!(s.width,2|6)),"invalid graph channel widths");
        self.nodes.clear();
        self.routes.clear();
        for (index,plan) in self
            .graph
            .strips
            .iter()
            .map(|strip| &strip.inserts)
            .chain(std::iter::once(&self.graph.master_inserts))
            .enumerate()
        {
            let inserts = SourceInsertRuntime::new(plan.clone(), self.rate, self.graph.width(index))?;
            self.nodes.push(RuntimeNode {
                input_latency: 0,
                pre_latency: inserts.latency_samples(false),
                post_latency: inserts.latency_samples(true),
                inserts,
            });
        }
        for (i, strip) in self.graph.strips.iter().enumerate() {
            for (slot, send) in strip.sends.iter().enumerate() {
                self.routes.push(RuntimeRoute {
                    source: i,
                    target: send.target,
                    send: Some(slot),
                    delay: Delay::new(0,self.graph.width(send.target)),
                });
            }
            self.routes.push(RuntimeRoute {
                source: i,
                target: strip.target,
                send: None,
                delay: Delay::new(0,self.graph.width(strip.target)),
            });
        }
        for i in 0..self.nodes.len() {
            self.nodes[i].input_latency = self
                .routes
                .iter()
                .filter(|r| r.target == i)
                .map(|route| {
                    let node = &self.nodes[route.source];
                    let pre = route
                        .send
                        .is_some_and(|slot| self.graph.strips[route.source].sends[slot].pre);
                    node.input_latency + node.pre_latency + if pre { 0 } else { node.post_latency }
                })
                .max()
                .unwrap_or(0);
        }
        for route in &mut self.routes {
            let node = &self.nodes[route.source];
            let pre = route
                .send
                .is_some_and(|slot| self.graph.strips[route.source].sends[slot].pre);
            let source_latency =
                node.input_latency + node.pre_latency + if pre { 0 } else { node.post_latency };
            route.delay = Delay::new(self.nodes[route.target].input_latency - source_latency,self.graph.width(route.target));
        }
        let master = self.nodes.last().expect("master node");
        self.latency = master.input_latency + master.pre_latency + master.post_latency;
        self.next = None;
        self.dirty = false;
        Ok(())
    }
    pub fn latency_samples(&self) -> usize {
        self.latency
    }
    pub fn render(
        &mut self,
        start: i64,
        frames: usize,
        mut read: impl FnMut(u64, i64, usize) -> Result<Vec<Vec<f32>>>,
    ) -> Result<Vec<Vec<f32>>> {
        ensure!(frames <= 4_194_304, "graph output block exceeds budget");
        if self.dirty || self.next.is_some_and(|next| next != start) {
            self.rebuild()?;
        }
        let contiguous = self.next == Some(start);
        let tick = i64::try_from(start as i128 * 254016000000i128 / self.rate as i128)?;
        let mut preroll = 0;
        if !contiguous {
            for plan in self
                .graph
                .strips
                .iter()
                .map(|strip| &strip.inserts)
                .chain(std::iter::once(&self.graph.master_inserts))
            {
                preroll = preroll.max(plan.preroll_samples(tick, self.rate)?);
            }
            preroll = preroll.min(self.rate as usize * 3).max(self.latency);
        }
        let total = preroll
            .checked_add(frames)
            .context("graph buffer overflow")?;
        let input_start = i64::try_from(start as i128 + self.latency as i128 - preroll as i128)?;
        let end = start
            .checked_add(frames as i64)
            .context("graph output clock overflow")?;
        i64::try_from(input_start as i128 + total as i128)?;
        ensure!(total <= 4_194_304, "graph expanded input exceeds budget");
        let mut inputs = std::collections::BTreeMap::new();
        for strip in self.graph.strips.iter().filter(|s| s.audio && s.audible) {
            let bus = match read(strip.id, input_start, total) {
                Ok(bus) => bus,
                Err(error) => {
                    self.dirty = true;
                    return Err(error);
                }
            };
            if bus.len() != strip.width || bus.iter().any(|c| c.len() != total) {
                self.dirty = true;
                anyhow::bail!("invalid graph provider bus");
            }
            inputs.insert(strip.id, bus);
        }
        let mut slice_input = |id: u64, start: i64, count: usize| -> Result<Vec<Vec<f32>>> {
            let offset = usize::try_from(start - input_start)?;
            Ok(inputs
                .get(&id)
                .context("missing graph track input")?
                .iter()
                .map(|c| c[offset..offset + count].to_vec())
                .collect())
        };
        let mut output = vec![Vec::with_capacity(frames);self.graph.master_width];
        for offset in (0..total).step_by(16384) {
            let count = (total - offset).min(16384);
            let chunk =
                match self.process_chunk(input_start + offset as i64, count, &mut slice_input) {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        self.dirty = true;
                        return Err(error);
                    }
                };
            let skip = preroll.saturating_sub(offset).min(count);
            for (out, channel) in output.iter_mut().zip(chunk) {
                out.extend_from_slice(&channel[skip..]);
            }
        }
        self.next = Some(end);
        Ok(output)
    }
    fn process_chunk(
        &mut self,
        start: i64,
        frames: usize,
        read: &mut impl FnMut(u64, i64, usize) -> Result<Vec<Vec<f32>>>,
    ) -> Result<Vec<Vec<f32>>> {
        let mut buses = self.graph.buses(frames);
        let graph = &self.graph;
        let routes = &mut self.routes;
        for (i, strip) in graph.strips.iter().enumerate() {
            if !strip.audible {
                for route in routes.iter_mut().filter(|route| route.source == i) {
                    route.delay.clear();
                }
                continue;
            }
            let mut bus = if strip.audio {
                read(strip.id, start, frames)?
            } else {
                std::mem::take(&mut buses[i])
            };
            ensure!(
                bus.len() == strip.width && bus.iter().all(|c| c.len() == frames),
                "invalid graph provider bus"
            );
            let node = &mut self.nodes[i];
            let before = node.input_latency;
            let after_pre = before + node.pre_latency;
            let after_post = after_pre + node.post_latency;
            let clock = |latency: usize| -> Result<i64> {
                Ok(i64::try_from(start as i128 - latency as i128)?)
            };
            strip.processor.apply_before_pan(
                &mut bus,
                clock(after_pre)?,
                self.rate,
                |pre, signal| {
                    node.inserts.process(
                        !pre,
                        signal,
                        start,
                        if pre { before } else { after_pre },
                    )?;
                    for route in routes.iter_mut().filter(|route| {
                        route.source == i
                            && route.send.is_some_and(|slot| strip.sends[slot].pre == pre)
                    }) {
                        let send = &strip.sends[route.send.unwrap()];
                        let mut signal = send.render(
                            signal,
                            graph.width(route.target),
                            clock(if pre { after_pre } else { after_post })?,
                            self.rate,
                        )?;
                        route.delay.apply(&mut signal);
                        for (out, channel) in buses[route.target].iter_mut().zip(signal) {
                            for (out, value) in out.iter_mut().zip(channel) {
                                *out += value;
                            }
                        }
                    }
                    Ok(())
                },
            )?;
            let route = routes
                .iter_mut()
                .find(|route| route.source == i && route.send.is_none())
                .expect("main output route");
            let mut bus=graph.output(strip,bus,clock(after_post)?,self.rate)?;
            route.delay.apply(&mut bus);
            for (out, channel) in buses[route.target].iter_mut().zip(bus) {
                for (out, value) in out.iter_mut().zip(channel) {
                    *out += value;
                }
            }
        }
        let mut output = buses.pop().expect("master bus");
        let master = self.nodes.last_mut().expect("master state");
        master
            .inserts
            .process(false, &mut output, start, master.input_latency)?;
        graph.master.apply_bus(
            &mut output,
            i64::try_from(
                start as i128 - master.input_latency as i128 - master.pre_latency as i128,
            )?,
            self.rate,
        )?;
        master.inserts.process(
            true,
            &mut output,
            start,
            master.input_latency + master.pre_latency,
        )?;
        Ok(output)
    }
}
