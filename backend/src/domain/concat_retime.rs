// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Source-owned timeline-linear retiming used by Concat.
//!
//! Native source-progress ramps have different semantics and cannot represent
//! these curves by copying their knots. Keep source normalization, integration
//! and audio piece boundaries together for subsequent placement integration.
use anyhow::{ensure, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct TimelineCurve {
    points: Vec<(f64, f64)>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioPiece {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub source_start_seconds: f64,
    pub speed: f64,
    pub from: f64,
    pub to: f64,
    pub fade_in_seconds: f64,
    pub fade_out_seconds: f64,
}
impl TimelineCurve {
    /// Original source normalization preserves equal-position point order.
    pub fn new(points: &[(f64, f64)]) -> Option<Self> {
        let mut points: Vec<_> = points.iter().copied()
            .filter(|(x, speed)| x.is_finite() && speed.is_finite())
            .map(|(x, speed)| (x.clamp(0., 1.), speed.clamp(0.0625, 16.)))
            .collect();
        if points.is_empty() { return None; }
        points.sort_by(|a,b| a.0.total_cmp(&b.0));
        if points[0].0 > 0. { points.insert(0, (0., points[0].1)); }
        if points.last().unwrap().0 < 1. { points.push((1., points.last().unwrap().1)); }
        Some(Self { points })
    }
    pub fn points(&self) -> &[(f64, f64)] { &self.points }
    pub fn speed_at(&self, fraction: f64) -> f64 {
        let fraction=fraction.clamp(0.,1.);
        for pair in self.points.windows(2) {
            let ((x0,v0),(x1,v1))=(pair[0],pair[1]);
            if fraction<=x1 {
                if x1<=x0 {return v1;}
                return v0+(v1-v0)*(fraction-x0)/(x1-x0);
            }
        }
        self.points.last().unwrap().1
    }
    pub fn consumed(&self, fraction: f64) -> f64 {
        let fraction=fraction.clamp(0.,1.);
        let mut area=0.;
        for pair in self.points.windows(2) {
            let ((x0,v0),(x1,_))=(pair[0],pair[1]);
            if fraction<=x0 {break;}
            let end=fraction.min(x1);
            area+=(end-x0)*(v0+self.speed_at(end))/2.;
            if fraction<=x1 {break;}
        }
        area
    }
    pub fn mean(&self) -> f64 { self.consumed(1.).max(0.0625) }

    /// Matches Rational::approximate(consumed * quantized_duration), including
    /// its six-decimal source-second precision. Call only inside the clip.
    pub fn source_offset_ticks(&self, local: i128, duration: i128) -> Result<i128> {
        ensure!(local>=0 && duration>0 && local<duration && duration<=i64::MAX as i128,"invalid Concat retime sample clock");
        let fraction=rational_f64(local,duration);
        let seconds=self.consumed(fraction)*rational_f64(duration,254_016_000_000);
        let micros=(seconds*1_000_000.).round();
        ensure!(micros.is_finite() && micros.abs()<(i64::MAX as f64),"Concat retime source clock overflow");
        Ok(micros as i128*254_016)
    }

    /// Source audio uses equal timeline pieces, not source-progress pieces.
    /// Gain curves must subsequently be sliced at each piece's from/to bounds.
    pub fn audio_pieces(&self, start:f64, duration:f64, source_start:f64, fade_in:f64, fade_out:f64) -> Result<Vec<AudioPiece>> {
        ensure!(start.is_finite()&&start>=0.&&duration.is_finite()&&duration>0.&&source_start.is_finite()&&source_start>=0.&&fade_in.is_finite()&&fade_in>=0.&&fade_out.is_finite()&&fade_out>=0.,"invalid Concat retime audio clock");
        ensure!((start+duration).is_finite()&&(source_start+duration*self.mean()).is_finite(),"Concat retime audio clock overflow");
        let count=((duration/0.1).ceil()as usize).clamp(8,400);
        Ok((0..count).map(|i|{
            let from=i as f64/count as f64;
            let to=(i+1)as f64/count as f64;
            let consumed=self.consumed(from);
            let speed=((self.consumed(to)-consumed)/(to-from)).max(0.0625);
            let piece_duration=(to-from)*duration;
            let piece_start=start+from*duration;
            let piece_end=piece_start+piece_duration;
            AudioPiece{start_seconds:piece_start,duration_seconds:piece_duration,
                source_start_seconds:source_start+consumed*duration,speed:speed.clamp(0.0625,16.),
                from,to,fade_in_seconds:if fade_in>0.{(fade_in-from*duration).clamp(0.,piece_duration)}else{0.},
                fade_out_seconds:if fade_out>0.{(piece_end-(start+duration-fade_out).max(piece_start)).clamp(0.,piece_duration)}else{0.}}
        }).collect())
    }
}
fn rational_f64(mut numerator:i128,mut denominator:i128)->f64 {
    let(mut a,mut b)=(numerator.abs(),denominator.abs());
    while b!=0 {let rem=a%b;a=b;b=rem;}
    numerator/=a;denominator/=a;
    numerator as f64/denominator as f64
}

/// Retained source coordinates. Authoring speed is independent of this curve;
/// Concat ignores the constant speed while a nonempty curve is active.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SourceClock {
    pub points: Vec<(f64,f64)>,
    pub duration_ticks: u64,
    pub rate_num: u32,
    pub rate_den: u32,
    /// Original document seconds survive native microsecond placement serialization.
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub document_clock:Option<super::concat_retime_edit::Clock>,
}
impl SourceClock {
    /// None denotes original constant-speed document timing, not a synthetic curve.
    pub fn retime_curve(&self)->Result<Option<TimelineCurve>>{
        ensure!(self.duration_ticks>0&&self.duration_ticks<=86_400_000_000&&self.rate_num>0&&self.rate_den>0,"invalid Concat retained clock");
        ensure!(self.points.iter().all(|(x,v)|x.is_finite()&&v.is_finite()),"nonfinite retained curve point");
        if let Some(raw)=&self.document_clock {
            ensure!(raw.start.is_finite()&&raw.start>=0.&&raw.duration.is_finite()&&raw.duration>0.&&raw.source_start.is_finite()&&raw.source_start>=0.&&raw.speed.is_finite()&&(0.0625..=16.).contains(&raw.speed),"invalid raw source clock");
            ensure!(Self::micro(raw.duration)?==self.duration_ticks,"raw source duration differs from retained duration");
            match &raw.speed_curve {
                None=>{ensure!(self.points.is_empty(),"constant document clock has retained curve points");return Ok(None)},
                Some(points)=>ensure!(points.len()==self.points.len()&&points.iter().zip(&self.points).all(|(p,(at,speed))|p.at.to_bits()==at.to_bits()&&p.speed.to_bits()==speed.to_bits()),"raw source curve differs from retained points"),
            }
        }
        Ok(Some(TimelineCurve::new(&self.points).ok_or_else(||anyhow::anyhow!("empty retained curve"))?))
    }
    pub fn curve(&self)->Result<TimelineCurve>{self.retime_curve()?.ok_or_else(||anyhow::anyhow!("document clock has no speed curve"))}
    fn micro(seconds:f64)->Result<u64>{let ticks=(seconds*1e6).round();ensure!(ticks.is_finite()&&ticks>=0.&&ticks<=9_007_199_254_740_991.,"raw source clock exceeds wire range");Ok(ticks as u64)}
    pub fn document_times(&self,start:u64,source_in:u64)->Result<(f64,f64,f64)>{
        if let Some(raw)=&self.document_clock {
            self.retime_curve()?;
            ensure!(Self::micro(raw.start)?==start&&Self::micro(raw.source_start)?==source_in,"raw source placement clock was not updated");
            Ok((raw.start,raw.duration,raw.source_start))
        }else{Ok((start as f64/1e6,self.duration_ticks as f64/1e6,source_in as f64/1e6))}
    }
    pub fn duration_source_ticks(&self)->Result<i128>{
        let duration=self.document_clock.as_ref().map_or(self.duration_ticks as f64/1e6,|raw|raw.duration);
        Ok(super::concat_project_transitions::frame_tick(duration,self.rate_num as u64,self.rate_den as u64)? as i128)
    }
    pub fn timeline_window_ticks(&self,start:u64,source_in:u64)->Result<(i128,i128)>{
        let(start,_,_)=self.document_times(start,source_in)?;
        Ok((super::concat_project_transitions::frame_tick(start,self.rate_num as u64,self.rate_den as u64)? as i128,self.duration_source_ticks()?))
    }
    /// Reconstruct the original separately quantized source/start clocks.
    pub fn sample(&self,start:u64,source_in:u64,exact:i128)->Result<Option<i64>>{
        let curve=self.retime_curve()?;
        let (start,_,source_in)=self.document_times(start,source_in)?;
        let start=super::concat_project_transitions::frame_tick(start,self.rate_num as u64,self.rate_den as u64)? as i128;
        let duration=self.duration_source_ticks()?;
        if exact<start||exact>=start+duration{return Ok(None);}
        let source=super::concat_project_transitions::frame_tick(source_in,self.rate_num as u64,self.rate_den as u64)? as i128;
        let local=exact-start;
        let offset=match curve {
            Some(curve)=>curve.source_offset_ticks(local,duration)?,
            None=>{let speed=self.document_clock.as_ref().expect("validated constant document clock").speed;let numerator=(speed*1e6).round()as i128;local.checked_mul(numerator).ok_or_else(||anyhow::anyhow!("constant source clock overflow"))?/1_000_000},
        };
        Ok(Some(i64::try_from(source+offset)?))
    }
    pub fn validate_placement(&self,placement:&super::composition::ClipPlacement)->Result<()> {
        let curve=self.retime_curve()?;
        ensure!(placement.speed_ramp.is_none()&&placement.source_timeline_offset_tick.is_none()&&placement.source_head_timeline_tick.is_none(),"mixed source retime ownership");
        ensure!(placement.timeline_duration_tick.is_none_or(|duration|duration==self.duration_ticks),"modified retained curve duration");
        let (_,duration,_)=self.document_times(placement.timeline_start_tick,placement.source_in_tick)?;
        ensure!(self.document_clock.as_ref().is_none_or(|raw|raw.speed.to_bits()==placement.speed.to_bits()),"raw source stored speed differs from placement");
        let speed=curve.as_ref().map_or(placement.speed,TimelineCurve::mean);
        let consumed=(duration*speed*1e6).round();
        ensure!(consumed>=1.&&consumed<=u64::MAX as f64&&placement.source_out_tick.checked_sub(placement.source_in_tick)==Some(consumed as u64),"source curve selected range differs from its integral");
        self.duration_source_ticks()?;
        Ok(())
    }
}

/// A prepared native piece plus its complete unrounded source clock. Keeping
/// the latter lets the worker qualify sample-boundary conversion separately.
#[derive(Debug,Clone)]
pub struct ExpandedAudioPiece {
    pub clip: super::composition::AudioClip,
    pub source: AudioPiece,
}
/// Derive constant-tempo pieces without mutating the authored source clip.
/// Source filters restart per piece, as they do in original audio_pieces.
pub fn expand_audio(clip:&super::composition::AudioClip)->Result<Vec<ExpandedAudioPiece>>{
    use super::{composition::{AnimatableValue,AudioVoiceEffect,ClipPlacement,CompositionClipId},concat_animation::PreparedTrack};
    use sha2::{Digest,Sha256};
    let clock=clip.placement.concat_retime.as_ref().ok_or_else(||anyhow::anyhow!("missing source retime audio clock"))?;
    clock.validate_placement(&clip.placement)?;
    ensure!(!clip.reversed&&matches!(clip.gain,AnimatableValue::Constant{..})&&matches!(clip.pan,AnimatableValue::Constant{..})&&clip.crossfade_in_ticks==0&&clip.ducking.is_none()&&clip.voice_effect==AudioVoiceEffect::None&&clip.pitch_semitones==0.&&clip.tone_db==0.,"native processing needs separate source curve rewindowing");
    let track=PreparedTrack::new(&clip.concat_gain_keys)?;
    let (start,duration,source_start)=clock.document_times(clip.placement.timeline_start_tick,clip.placement.source_in_tick)?;
    let curve=clock.retime_curve()?;
    let pieces=match &curve {
        Some(curve)=>curve.audio_pieces(start,duration,source_start,clip.fade_in_ticks as f64/1e6,clip.fade_out_ticks as f64/1e6)?,
        None=>vec![AudioPiece{start_seconds:start,duration_seconds:duration,source_start_seconds:source_start,speed:clip.placement.speed,from:0.,to:1.,fade_in_seconds:clip.fade_in_ticks as f64/1e6,fade_out_seconds:clip.fade_out_ticks as f64/1e6}],
    };
    let identity=format!("{:x}",Sha256::digest(clip.id.as_str().as_bytes()));
    pieces.into_iter().enumerate().map(|(index,source)|{
        let micro=|value:f64|->Result<u64>{let value=(value*1e6).round();ensure!(value.is_finite()&&value>=0.&&value<=9_007_199_254_740_991.,"source audio piece clock overflow");Ok(value as u64)};
        let duration=micro(source.duration_seconds)?;
        let source_in=micro(source.source_start_seconds)?;
        let span=micro(source.duration_seconds*source.speed)?;
        ensure!(duration>0&&span>0,"sub-microsecond source curve audio piece needs exact sample conversion");
        let mut derived=clip.clone();
        derived.id=CompositionClipId::parse(format!("concat-retime-{identity}-{index}"))?;
        derived.placement=ClipPlacement{
            timeline_start_tick:((source.start_seconds*1000.).round().max(0.)as u64).checked_mul(1000).ok_or_else(||anyhow::anyhow!("source delay overflow"))?,
            source_in_tick:source_in,source_out_tick:source_in.checked_add(span).ok_or_else(||anyhow::anyhow!("source audio range overflow"))?,
            speed:source.speed,timeline_duration_tick:Some(duration),source_timeline_offset_tick:None,source_head_timeline_tick:None,speed_ramp:None,concat_retime:None,
        };
        derived.concat_gain_keys=if curve.is_some(){track.slice(source.from,source.to)?}else{clip.concat_gain_keys.clone()};
        derived.fade_in_ticks=micro(source.fade_in_seconds)?;
        derived.fade_out_ticks=micro(source.fade_out_seconds)?;
        Ok(ExpandedAudioPiece{clip:derived,source})
    }).collect()
}

/// Original MixInput feeds complete decoder frames until this sample count is
/// reached. It does not trim the last input frame to the authored source end.
#[derive(Debug,Clone,PartialEq)]
pub struct FrameFeedPlan {
    pub source_seek_microseconds: u64,
    pub source_lead_seconds: f64,
    pub minimum_input_samples: i64,
    pub filters: Vec<String>,
}
impl AudioPiece {
    /// `landed_source_seconds` comes from the first decoded frame's timestamp,
    /// relative to the stream start, not from the requested seek position.
    /// Extra source effects/gain filters belong after tempo and before fades.
    /// This compiler does not authorize paths or bound the decoder process.
    pub fn frame_feed(&self,landed_source_seconds:Option<f64>,sample_rate:u32,middle_filters:&[String])->Result<FrameFeedPlan>{
        ensure!(self.start_seconds.is_finite()&&self.start_seconds>=0.&&self.duration_seconds.is_finite()&&self.duration_seconds>0.&&self.source_start_seconds.is_finite()&&self.source_start_seconds>=0.&&self.speed.is_finite()&&(0.0625..=16.).contains(&self.speed)&&self.fade_in_seconds.is_finite()&&self.fade_in_seconds>=0.&&self.fade_out_seconds.is_finite()&&self.fade_out_seconds>=0.&&sample_rate>0,"invalid source frame feed clock");
        ensure!(landed_source_seconds.is_none_or(f64::is_finite),"nonfinite landed source timestamp");
        ensure!(middle_filters.iter().all(|filter|!filter.chars().any(|c|matches!(c,';'|'['|']'|'\n'|'\r'))),"source audio filter escaped its slot");
        ensure!((self.start_seconds*1000.).is_finite()&&self.start_seconds*1000.<i64::MAX as f64,"source delay overflow");
        let seek=self.source_start_seconds*1e6;
        ensure!(seek<i64::MAX as f64,"source seek overflow");
        let lead=(self.source_start_seconds-landed_source_seconds.unwrap_or(self.source_start_seconds)).max(0.);
        let samples=((self.duration_seconds*self.speed+lead)*sample_rate as f64).round();
        ensure!(samples.is_finite()&&samples>0.&&samples<i64::MAX as f64,"source input sample count overflow");
        let mut filters=vec![format!("aselect=lt(st(0\\,ld(0)+samples_n)-samples_n\\,{})",samples as i64),"asetpts=N/SR/TB".into()];
        if lead>0.{filters.extend([format!("atrim=start={lead:.6}"),"asetpts=PTS-STARTPTS".into()]);}
        if (self.speed-1.).abs()>=f64::EPSILON{
            let mut speed=self.speed;
            while speed>2.{filters.push("atempo=2.000000".into());speed/=2.;}
            while speed<0.5{filters.push("atempo=0.500000".into());speed/=0.5;}
            filters.push(format!("atempo={speed:.6}"));
        }
        filters.extend(middle_filters.iter().filter(|filter|!filter.is_empty()).cloned());
        if self.fade_in_seconds>0.{filters.push(format!("afade=t=in:st=0:d={:.4}",self.fade_in_seconds));}
        if self.fade_out_seconds>0.{filters.push(format!("afade=t=out:st={:.4}:d={:.4}",(self.duration_seconds-self.fade_out_seconds).max(0.),self.fade_out_seconds));}
        filters.push(format!("adelay={}:all=1",(self.start_seconds*1000.).round().max(0.)as u64));
        Ok(FrameFeedPlan{source_seek_microseconds:seek as u64,source_lead_seconds:lead,minimum_input_samples:samples as i64,filters})
    }
}
