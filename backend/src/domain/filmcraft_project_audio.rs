//! Recursive source-project audio: original sequence clocks, nested sample-rate
//! conversion and multi-camera audio selection, without Composition time rounding.
use super::{
    filmcraft_clip_audio::{SourceAudioReader, SourceSequenceAudioRuntime},
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_sequence_source::SourceSequence,
};
use anyhow::{ensure, Context, Result};
use filmcraft_audio_dsp::channels::{convert_owned,Layout,Mixdown};
use serde_json::Value;
use std::collections::BTreeMap;
const MAX_DEPTH: u32 = 8;
type Key = (u64, Option<Option<usize>>, u32);
pub struct SourceProjectAudioRuntime {
    document: SourceProjectDocument,
    runtimes: BTreeMap<Key, SourceSequenceAudioRuntime>,
}
impl SourceProjectAudioRuntime {
    pub fn new(document: SourceProjectDocument) -> Result<Self> {
        ProjectIndex::new(&document)?;
        Ok(Self {
            document,
            runtimes: BTreeMap::new(),
        })
    }
    pub fn document(&self) -> &SourceProjectDocument {
        &self.document
    }
    pub fn render(
        &mut self,
        sequence: u64,
        start: i64,
        frames: usize,
        read: impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_channels(sequence,start,frames,2,read)
    }
    pub fn render_channels(&mut self,sequence:u64,start:i64,frames:usize,channels:usize,mut read:impl FnMut(u64,i64,usize,u32)->Result<Option<Vec<Vec<f32>>>>)->Result<Vec<Vec<f32>>> {
        ensure!(matches!(channels,1|2|6),"invalid output channel layout");
        let bus=self.sequence(sequence,None,0,start,frames,&mut read)?;
        let bus=convert_owned(bus,if channels==6{Layout::Surround51}else{Layout::Stereo},Mixdown::FrontRear);
        if channels==1 {Ok(vec![bus[0].iter().zip(&bus[1]).map(|(l,r)|0.5*(l+r)).collect()])}else{Ok(bus)}
    }
    /// Resolve subclips and synthetic sources internally. File readers receive root media IDs;
    /// source-authored paths are never treated as trusted local file bindings.
    pub fn render_media(
        &mut self, sequence: u64, start: i64, frames: usize,
        files: impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_media_channels(sequence,start,frames,2,files)
    }
    pub fn render_media_channels(&mut self,sequence:u64,start:i64,frames:usize,channels:usize,mut files:impl FnMut(u64,i64,usize,u32)->Result<Option<Vec<Vec<f32>>>>)->Result<Vec<Vec<f32>>> {
        let document = self.document.clone();
        let index = ProjectIndex::new(&document)?;
        self.render_channels(sequence, start, frames,channels, |id, start, frames, rate| {
            read_media(&index, id, start, frames, rate, &mut files)
        })
    }

    pub fn render_wav(
        &mut self, sequence:u64, start:i64, frames:usize,
        sources:&super::filmcraft_wav_audio::SourceWavPool,
    ) -> Result<Vec<Vec<f32>>> {
        self.render_media(sequence,start,frames,|id,start,frames,rate| sources.read(id,start,frames,rate))
    }

    fn raw_sequence(&self, id: u64) -> Result<&Value> {
        self.document
            .project()
            .get("items")
            .and_then(|items| items.get(id.to_string()))
            .and_then(|item| item.pointer("/kind/Sequence"))
            .context("missing source audio sequence")
    }
    fn build(
        &self,
        id: u64,
        selection: Option<Option<usize>>,
    ) -> Result<SourceSequenceAudioRuntime> {
        let mut document = self.document.clone();
        if let Some(angle) = selection {
            let raw =
                &mut document.envelope["project"]["items"][id.to_string()]["kind"]["Sequence"];
            if let Some(multicam) = raw.get("multicam").filter(|v| !v.is_null()) {
                let cameras = multicam["cameras"]
                    .as_array()
                    .context("invalid source cameras")?;
                let first = cameras
                    .iter()
                    .position(|c| c.get("video_track").is_some_and(|v| !v.is_null()));
                let mut audible = vec![];
                for (i, camera) in cameras.iter().enumerate() {
                    let audio_only = camera.get("video_track").is_none_or(Value::is_null);
                    let on = audio_only
                        || match multicam["audio"].as_str().unwrap_or("Camera1") {
                            "Camera1" => i == first.unwrap_or(0),
                            "AllCameras" => true,
                            "SwitchAudio" => Some(i) == angle.or(first),
                            _ => anyhow::bail!("unknown multi-camera audio mode"),
                        };
                    if on {
                        for track in camera["audio_tracks"]
                            .as_array()
                            .context("invalid camera audio tracks")?
                        {
                            audible.push(track.as_u64().context("invalid camera track ID")?);
                        }
                    }
                }
                for track in raw["audio_tracks"]
                    .as_array_mut()
                    .context("invalid camera sequence tracks")?
                {
                    if !audible.contains(&track["id"].as_u64().context("invalid source track ID")?)
                    {
                        track["muted"] = Value::Bool(true);
                    }
                }
            }
        }
        let index = ProjectIndex::new(&document)?;
        SourceSequenceAudioRuntime::new(&SourceSequence::read(&index, id)?)
    }
    fn sequence(
        &mut self,
        id: u64,
        selection: Option<Option<usize>>,
        depth: u32,
        start: i64,
        frames: usize,
        read: &mut impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Vec<Vec<f32>>> {
        let selection = if self
            .raw_sequence(id)?
            .get("multicam")
            .is_none_or(Value::is_null)
        {
            None
        } else {
            selection
        };
        let key = (id, selection, depth);
        let mut runtime = match self.runtimes.remove(&key) {
            Some(runtime) => runtime,
            None => self.build(id, selection)?,
        };
        let result = runtime.render_native_with_reader(
            start,
            frames,
            ProjectReader {
                project: self,
                depth,
                read,
            },
        );
        self.runtimes.insert(key, runtime);
        result
    }
    fn nested(
        &mut self,
        id: u64,
        clip: &Value,
        depth: u32,
        start: i64,
        frames: usize,
        rate: u32,
        width:usize,
        read: &mut impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
    ) -> Result<Option<Vec<Vec<f32>>>> {
        ensure!(matches!(width,2|6)&&rate>0,"invalid nested channel layout/rate");
        if depth >= MAX_DEPTH {
            return Ok(None);
        }
        if frames == 0 {
            return Ok(Some(vec![Vec::new();width]));
        }
        let raw = self.raw_sequence(id)?;
        let own = u32::try_from(
            raw.pointer("/settings/sample_rate")
                .and_then(Value::as_u64)
                .context("missing nested sample rate")?,
        )?
        .clamp(1000, 768000);
        let angle = if clip.pointer("/multicam/enabled").and_then(Value::as_bool) == Some(true) {
            Some(usize::try_from(
                clip.pointer("/multicam/angle")
                    .and_then(Value::as_u64)
                    .context("invalid clip camera angle")?,
            )?)
        } else {
            None
        };
        let at = |i: i64| i as f64 * own as f64 / rate as f64;
        let first = if own == rate {
            start
        } else {
            at(start).floor() as i64
        };
        let count = if own == rate {
            frames
        } else {
            (at(start.saturating_add(frames as i64)).ceil() as i64)
                .saturating_sub(first)
                .max(0) as usize
                + 2
        };
        ensure!(
            count <= 4_194_304,
            "nested audio expanded request exceeds budget"
        );
        let lead = first.min(0).unsigned_abs().min(count as u64) as usize;
        let mut mix = vec![vec![0.; count]; width];
        if count > lead {
            let bus = self.sequence(
                id,
                Some(angle),
                depth + 1,
                first + lead as i64,
                count - lead,
                read,
            )?;
            let bus=convert_owned(bus,Layout::from_channels(width),Mixdown::FrontRear);
            for (out, channel) in mix.iter_mut().zip(bus) {
                let n = channel.len().min(count - lead);
                out[lead..lead + n].copy_from_slice(&channel[..n]);
            }
        }
        if own == rate {
            return Ok(Some(mix));
        }
        let mut output = vec![vec![0.; frames]; width];
        for (out, input) in output.iter_mut().zip(mix) {
            for (i, out) in out.iter_mut().enumerate() {
                let position = (at(start + i as i64) - first as f64).max(0.);
                let index = position.floor() as usize;
                let fraction = (position - index as f64) as f32;
                let a = input.get(index).copied().unwrap_or(0.);
                let b = input.get(index + 1).copied().unwrap_or(a);
                *out = a + (b - a) * fraction;
            }
        }
        Ok(Some(output))
    }
}
fn read_media(
    index: &ProjectIndex<'_>, id: u64, start: i64, frames: usize, rate: u32,
    files: &mut impl FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>,
) -> Result<Option<Vec<Vec<f32>>>> {
    ensure!(rate > 0 && frames <= 4_194_304, "invalid source media request");
    let Some(resolved) = index.resolve_media(id) else { return Ok(None) };
    let media = resolved.media;
    if let Some(generator) = media.pointer("/media/Generator") {
        let generator: super::filmcraft_generator_audio::Generator = serde_json::from_value(generator.clone())?;
        let duration = media.pointer("/info/duration").and_then(Value::as_i64).context("missing generator duration")?;
        return generator.audio(duration, start, frames, rate);
    }
    if media.get("offline").and_then(Value::as_bool).unwrap_or(false) {
        let channels = media.pointer("/info/audio/channels").and_then(Value::as_u64).unwrap_or(2);
        ensure!((1..=16).contains(&channels), "invalid offline audio channel count");
        ensure!(frames.checked_mul(channels as usize).is_some_and(|n| n <= 8_388_608), "offline audio exceeds sample budget");
        return Ok(Some(vec![vec![0.; frames]; channels as usize]));
    }
    files(resolved.item_id, start, frames, rate)
}
struct ProjectReader<'a, F> {
    project: &'a mut SourceProjectAudioRuntime,
    depth: u32,
    read: &'a mut F,
}
impl<F: FnMut(u64, i64, usize, u32) -> Result<Option<Vec<Vec<f32>>>>> SourceAudioReader
    for ProjectReader<'_, F>
{
    fn read(
        &mut self,
        id: u64,
        clip: &Value,
        start: i64,
        frames: usize,
        rate: u32,
    ) -> Result<Option<Vec<Vec<f32>>>> {
        self.read_width(id,clip,start,frames,rate,2)
    }
    fn read_width(&mut self,id:u64,clip:&Value,start:i64,frames:usize,rate:u32,width:usize)->Result<Option<Vec<Vec<f32>>>> {
        if self
            .project
            .document
            .project()
            .get("items")
            .and_then(|items| items.get(id.to_string()))
            .and_then(|item| item.pointer("/kind/Sequence"))
            .is_some()
        {
            self.project
                .nested(id, clip, self.depth, start, frames, rate,width, self.read)
        } else {
            (self.read)(id, start, frames, rate)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_wav_nested_project_pcm_match() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wav-project.json")).unwrap();
        assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let bytes:Vec<u8>=fixture["wav"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap() as u8).collect();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("pattern.wav");std::fs::write(&path,&bytes).unwrap();
        assert_eq!(fixture["cases"].as_array().unwrap().len(),24);
        let mut compared=0;let mut nonzero=0;
        for (case_index,case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let document=SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap()).unwrap();
            let mut runtime=SourceProjectAudioRuntime::new(document).unwrap();
            let mut sources=super::super::filmcraft_wav_audio::SourceWavPool::default();
            sources.bind_file(case["source"].as_u64().unwrap(),&path,bytes.len() as u64).unwrap();
            for (request_index,request) in case["requests"].as_array().unwrap().iter().enumerate() {
                let actual=runtime.render_wav(case["root"].as_u64().unwrap(),request["start"].as_i64().unwrap(),request["frames"].as_u64().unwrap() as usize,&sources).unwrap();
                let bits:Vec<Vec<u32>>=actual.iter().map(|ch|ch.iter().map(|v|v.to_bits()).collect()).collect();
                compared+=bits.iter().map(Vec::len).sum::<usize>();
                nonzero+=actual.iter().flatten().filter(|v|**v!=0.).count();
                assert_eq!(serde_json::json!(bits),request["bits"],"WAV project {case_index}/{request_index}");
            }
            assert_eq!(serde_json::from_slice::<Value>(&runtime.document().encode_preserved().unwrap()).unwrap(),case["document"]);
        }
        assert_eq!(compared,26_160);assert!(nonzero>10_000,"silent oracle: {nonzero}");
    }
    #[test]
    fn source_media_subclips_generators_and_offline() {
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-project-index.json")).unwrap();
        let mut document = SourceProjectDocument::read(&serde_json::to_vec(&fixture["document"]).unwrap()).unwrap();
        let media = &mut document.envelope["project"]["items"]["1"]["kind"]["Media"];
        media["media"] = serde_json::json!({"Generator":"BarsAndTone"});
        let index = ProjectIndex::new(&document).unwrap();
        let mut forbidden = |_,_,_,_| -> Result<Option<Vec<Vec<f32>>>> { panic!("generator called file reader") };
        let generated = read_media(&index,9,0,97,48000,&mut forbidden).unwrap().unwrap();
        assert_eq!(generated, super::super::filmcraft_generator_audio::Generator::BarsAndTone.audio(254_016_000_000,0,97,48000).unwrap().unwrap());
        let media = &mut document.envelope["project"]["items"]["1"]["kind"]["Media"];
        media["media"] = serde_json::json!({"File":{"path":"untrusted.wav"}});
        media["offline"] = Value::Bool(true);
        media["info"]["audio"] = serde_json::json!({"channels":4});
        assert_eq!(read_media(&ProjectIndex::new(&document).unwrap(),9,-3,17,44100,&mut forbidden).unwrap().unwrap(),vec![vec![0.;17];4]);
        document.envelope["project"]["items"]["1"]["kind"]["Media"]["offline"] = Value::Bool(false);
        let mut calls = vec![];
        let mut files = |id,start,frames,rate| { calls.push((id,start,frames,rate)); Ok(None) };
        assert!(read_media(&ProjectIndex::new(&document).unwrap(),9,-7,13,96000,&mut files).unwrap().is_none());
        assert_eq!(calls,vec![(1,-7,13,96000)]);
        assert_eq!(document.envelope["project"]["items"]["1"]["kind"]["Media"]["media"]["File"]["path"],"untrusted.wav");
    }
    #[test]
    fn original_nested_rates_multicam_and_depth_pcm_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-project-audio.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            "5231852443363f001c3f6b396dd9b1e6461ae2be"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 652);
        let mut failures = vec![];
        for (case_index, case) in cases.iter().enumerate() {
            let document =
                SourceProjectDocument::read(&serde_json::to_vec(&case["document"]).unwrap())
                    .unwrap();
            let mut runtime = SourceProjectAudioRuntime::new(document).unwrap();
            for (request_index, request) in case["requests"].as_array().unwrap().iter().enumerate()
            {
                let output = runtime
                    .render(
                        case["root"].as_u64().unwrap(),
                        request["start"].as_i64().unwrap(),
                        request["frames"].as_u64().unwrap() as usize,
                        |source, start, frames, _rate| {
                            assert_eq!(Some(source), case["source"].as_u64());
                            Ok(Some(
                                (0..2)
                                    .map(|channel| {
                                        (0..frames)
                                            .map(|i| {
                                                let n = start + i as i64;
                                                if n < 0 {
                                                    0.
                                                } else if channel == 0 {
                                                    ((n % 31) - 15) as f32 / 32.
                                                } else {
                                                    ((n % 23) - 7) as f32 / 64.
                                                }
                                            })
                                            .collect()
                                    })
                                    .collect(),
                            ))
                        },
                    )
                    .unwrap();
                let bits: Vec<Vec<u32>> = output
                    .iter()
                    .map(|c| c.iter().map(|v| v.to_bits()).collect())
                    .collect();
                if serde_json::to_value(bits).unwrap() != request["bits"] {
                    failures.push((case_index, request_index));
                }
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&runtime.document().encode_preserved().unwrap())
                    .unwrap(),
                case["document"]
            );
        }
        assert!(failures.is_empty(), "nested source audio PCM: {failures:?}");
    }
}
