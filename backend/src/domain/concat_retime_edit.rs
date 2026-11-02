//! Document-second edit clocks for Concat curves, before render quantization.
//! Callers still own keys, clip IDs, linked participants, neighbors and history.
use super::concat_retime::TimelineCurve;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
const MIN_DURATION: f64 = 1. / 60.;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub at: f64,
    pub speed: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clock {
    pub start: f64,
    pub duration: f64,
    pub source_start: f64,
    pub speed: f64,
    #[serde(default)]
    pub speed_curve: Option<Vec<Point>>,
}
#[derive(Clone, Debug)]
pub struct Trim {
    pub applied: bool,
    pub source_from: f64,
    pub source_to: f64,
    pub clock: Clock,
    pub ripple_by: f64,
}
#[derive(Clone, Debug)]
pub struct Freeze {
    pub head: Clock,
    pub still: Clock,
    pub tail: Clock,
    pub ripple_by: f64,
}
/// Raw document placements, before native microsecond quantization. Kind is
/// intentionally absent: the source command ripples every kind on this lane.
#[derive(Clone, Debug, PartialEq)]
pub struct LanePlacement {
    pub id: String,
    pub track_id: String,
    pub clock: Clock,
}
#[derive(Clone, Debug)]
pub struct FreezeLanePlan {
    pub freeze: Freeze,
    /// Existing neighbors only; target pieces are in `freeze`.
    pub shifted: Vec<LanePlacement>,
}
/// Original FreezeFrame lane ripple. IDs, media/looks, typed-track mapping and
/// history remain caller-owned. Never round `time` or `ripple_by` to wire ticks.
pub fn freeze_lane(items: &[LanePlacement], target: &str, time: f64, hold: Option<f64>) -> Result<Option<FreezeLanePlan>> {
    ensure!(items.len() <= 100_000, "freeze lane placement budget exceeded");
    let mut ids = std::collections::BTreeSet::new();
    for item in items {
        ensure!(!item.id.is_empty() && !item.track_id.is_empty() && ids.insert(&item.id), "invalid freeze lane identity");
        item.clock.validate()?;
    }
    let selected = items.iter().find(|item| item.id == target).ok_or_else(|| anyhow::anyhow!("freeze target missing"))?;
    let Some(freeze) = selected.clock.freeze(time, hold)? else { return Ok(None) };
    let mut shifted = Vec::new();
    for item in items {
        if item.id == target || item.track_id != selected.track_id || item.clock.start < time { continue }
        let mut next = item.clone();
        next.clock.start += freeze.ripple_by;
        next.clock = next.clock.tidy();
        next.clock.validate()?;
        shifted.push(next);
    }
    Ok(Some(FreezeLanePlan { freeze, shifted }))
}
impl Clock {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.start.is_finite()
                && self.start >= 0.
                && self.duration.is_finite()
                && self.duration > 0.
                && self.source_start.is_finite()
                && self.source_start >= 0.
                && self.speed.is_finite()
                && (0.0625..=16.).contains(&self.speed),
            "invalid source edit clock"
        );
        ensure!(
            self.speed_curve.as_ref().is_none_or(|points| points
                .iter()
                .all(|p| p.at.is_finite() && p.speed.is_finite())),
            "nonfinite source curve point"
        );
        ensure!(
            (self.start + self.duration).is_finite(),
            "source edit end overflow"
        );
        Ok(())
    }
    // Editor::apply normalizes touched clips after executing each command.
    // Curve mean is computed before this filtering; never recompute it here.
    fn tidy(mut self) -> Self {
        self.start = self.start.clamp(0., 360_000.);
        self.duration = self.duration.clamp(MIN_DURATION, 360_000.);
        self.source_start = self.source_start.clamp(0., 360_000.);
        self.speed_curve = self.speed_curve.take().and_then(|points| {
            let points: Vec<_> = points
                .into_iter()
                .filter(|p| (0. ..=1.).contains(&p.at))
                .map(|p| Point {
                    at: p.at,
                    speed: p.speed.clamp(0.0625, 16.),
                })
                .collect();
            if points.is_empty() {
                None
            } else {
                Some(points)
            }
        });
        self
    }
    /// Build native placement while retaining original f64 seconds for source rendering.
    pub fn placement(
        &self,
        rate_num: u32,
        rate_den: u32,
    ) -> Result<super::composition::ClipPlacement> {
        self.validate()?;
        ensure!(
            rate_num > 0 && rate_den > 0,
            "invalid source edit frame rate"
        );
        let raw = self.clone().tidy();
        let micro = |seconds: f64| -> Result<u64> {
            let v = (seconds * 1e6).round();
            ensure!(
                v.is_finite() && v >= 0. && v <= 9_007_199_254_740_991.,
                "source edit placement exceeds wire range"
            );
            Ok(v as u64)
        };
        let curve = raw.speed_curve.as_ref().and_then(|points| {
            TimelineCurve::new(&points.iter().map(|p| (p.at, p.speed)).collect::<Vec<_>>())
        });
        let span = micro(raw.duration * curve.as_ref().map_or(raw.speed, TimelineCurve::mean))?;
        let source_in = micro(raw.source_start)?;
        let duration = micro(raw.duration)?;
        let placement = super::composition::ClipPlacement {
            concat_source_eof: false,
timeline_start_tick: micro(raw.start)?,
            source_in_tick: source_in,
            source_out_tick: source_in
                .checked_add(span)
                .ok_or_else(|| anyhow::anyhow!("source edit range overflow"))?,
            speed: raw.speed,
            timeline_duration_tick: Some(duration),
            source_timeline_offset_tick: None,
            source_head_timeline_tick: None,
            speed_ramp: None,
            concat_retime: Some(super::concat_retime::SourceClock {
                points: raw
                    .speed_curve
                    .as_ref()
                    .map(|points| points.iter().map(|p| (p.at, p.speed)).collect())
                    .unwrap_or_default(),
                duration_ticks: duration,
                rate_num,
                rate_den,
                document_clock: Some(raw.clone()),
            }),
        };
        placement.timeline_duration_ticks()?;
        Ok(placement)
    }
    /// Relocate a wire placement by a common microsecond delta without dropping
    /// the original document's sub-microsecond phase. Source Editor tidies at 0/100h.
    pub fn relocate(&self, start_tick: u64) -> Result<Self> {
        self.validate()?;
        ensure!(
            start_tick <= 360_000_000_000,
            "source move exceeds document range"
        );
        let old_tick = (self.start * 1e6).round();
        let delta = (start_tick as i128 - old_tick as i128) as f64 / 1e6;
        let mut next = self.clone();
        next.start = (self.start + delta).clamp(0., 360_000.);
        ensure!(
            (next.start * 1e6).round() == start_tick as f64,
            "source move cannot retain document phase at requested wire position"
        );
        Ok(next)
    }
    pub fn set_curve(&self, points: Option<Vec<Point>>) -> Result<Self> {
        Ok(self.set_curve_outcome(points)?.0)
    }
    fn set_curve_outcome(&self, points: Option<Vec<Point>>) -> Result<(Self, bool)> {
        self.validate()?;
        ensure!(
            points.as_ref().is_none_or(|points| points
                .iter()
                .all(|p| p.at.is_finite() && p.speed.is_finite())),
            "nonfinite source curve command"
        );
        let curve = points.as_ref().and_then(|points| {
            TimelineCurve::new(&points.iter().map(|p| (p.at, p.speed)).collect::<Vec<_>>())
        });
        let speed = curve
            .as_ref()
            .map_or(self.speed, TimelineCurve::mean)
            .clamp(0.0625, 16.);
        let mut next = self.clone();
        next.duration = (self.duration * self.speed / speed).max(MIN_DURATION);
        next.speed = speed;
        let candidate = if curve.is_some() { points } else { None };
        let applied =
            next.duration != self.duration || speed != self.speed || candidate != self.speed_curve;
        // Source assign uses PartialEq: equal signed zeros retain the old points,
        // even though computing the candidate mean can distinguish their ordering.
        if next.speed_curve != candidate {
            next.speed_curve = candidate;
        }
        let next = next.tidy();
        next.validate()?;
        Ok((next, applied))
    }
    /// A cut intentionally flattens both halves at stored speed, as source commands do.
    pub fn split(&self, time: f64) -> Result<Option<(Self, Self)>> {
        self.validate()?;
        ensure!(time.is_finite(), "nonfinite source cut");
        let offset = time - self.start;
        if offset <= MIN_DURATION || offset >= self.duration - MIN_DURATION {
            return Ok(None);
        }
        let mut head = self.clone();
        let mut tail = self.clone();
        head.speed_curve = None;
        tail.speed_curve = None;
        head.duration = offset;
        tail.start = self.start + offset;
        tail.duration = self.duration - offset;
        tail.source_start = self.source_start + offset * self.speed;
        let head = head.tidy();
        let tail = tail.tidy();
        head.validate()?;
        tail.validate()?;
        Ok(Some((head, tail)))
    }
    /// `floor` is the preceding neighbor end; ripple ignores that timeline bound.
    pub fn trim_start(&self, delta: f64, floor: f64, ripple: bool) -> Result<Trim> {
        self.validate()?;
        ensure!(
            delta.is_finite() && floor.is_finite() && floor >= 0. && floor <= self.start,
            "invalid source head trim"
        );
        let shift = delta
            .min(self.duration - MIN_DURATION)
            .max(-self.source_start / self.speed);
        let proposed = self.start + shift;
        let start = if ripple {
            proposed
        } else {
            proposed.max(floor)
        };
        let moved = start - self.start;
        let mut next = self.clone();
        next.duration = self.duration - moved;
        next.source_start = (self.source_start + moved * self.speed).max(0.);
        next.start = if ripple { self.start } else { start };
        let applied = next.duration != self.duration
            || next.source_start != self.source_start
            || start != self.start;
        let next = next.tidy();
        next.validate()?;
        Ok(Trim {
            clock: next,
            applied,
            source_from: moved,
            source_to: self.duration,
            ripple_by: if ripple { -moved } else { 0. },
        })
    }
    /// `ceiling` is the following neighbor start; None means a ripple/unbounded tail.
    pub fn trim_end(&self, delta: f64, ceiling: Option<f64>, ripple: bool) -> Result<Trim> {
        self.validate()?;
        ensure!(
            delta.is_finite()
                && ceiling.is_none_or(|v| v.is_finite() && v >= self.start + self.duration),
            "invalid source tail trim"
        );
        let ceiling = if ripple {
            f64::INFINITY
        } else {
            ceiling.unwrap_or(f64::INFINITY)
        };
        let duration = (self.duration + delta)
            .min(ceiling - self.start)
            .max(MIN_DURATION);
        let mut next = self.clone();
        next.duration = duration;
        let next = next.tidy();
        next.validate()?;
        Ok(Trim {
            clock: next,
            applied: duration != self.duration,
            source_from: 0.,
            source_to: duration,
            ripple_by: if ripple { duration - self.duration } else { 0. },
        })
    }
    /// Video freeze clock after a still has been extracted; media and look ownership stay outside.
    pub fn freeze(&self, time: f64, hold: Option<f64>) -> Result<Option<Freeze>> {
        ensure!(
            hold.is_none_or(f64::is_finite),
            "nonfinite source freeze duration"
        );
        let Some((head, mut tail)) = self.split(time)? else {
            return Ok(None);
        };
        let hold = hold.filter(|v| *v > 0.).unwrap_or(1.).max(MIN_DURATION);
        tail.start += hold;
        let tail = tail.tidy();
        tail.validate()?;
        let still = Self {
            start: time,
            duration: hold,
            source_start: 0.,
            speed: 1.,
            speed_curve: None,
        };
        let still = still.tidy();
        still.validate()?;
        Ok(Some(Freeze {
            head,
            still,
            tail,
            ripple_by: hold,
        }))
    }
}

/// Pure plan; the caller applies keys/fades, source bindings, lane ripple and history.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequest {
    #[serde(default)]
    pub tracks: TrackGroups,
    #[serde(default)]
    pub fade_in: f64,
    #[serde(default)]
    pub fade_out: f64,
    pub clock: Clock,
    pub rate_num: u32,
    pub rate_den: u32,
    pub command: Edit,
}
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Edit {
    Move {
        start_tick: u64,
    },
    SetCurve {
        points: Option<Vec<Point>>,
    },
    Split {
        time: f64,
    },
    TrimStart {
        delta: f64,
        floor: f64,
        ripple: bool,
    },
    TrimEnd {
        delta: f64,
        ceiling: Option<f64>,
        ripple: bool,
    },
    Freeze {
        time: f64,
        duration: Option<f64>,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedPart {
    pub tracks: TrackGroups,
    pub fade_in: f64,
    pub fade_out: f64,
    pub role: &'static str,
    pub clock: Clock,
    pub placement: super::composition::ClipPlacement,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StillCaptureSelection {
    AtOrAfterThenFirst,
}
/// Original Studio freeze extraction contract; extraction remains caller-owned.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StillCapture {
    pub selection: StillCaptureSelection,
    pub source_seconds: f64,
    pub seek_time_us: u64,
    pub cache_time_ms: u64,
    pub width: u32,
    pub jpeg_quality: u8,
}
impl Clock {
    pub fn still_capture(&self, time: f64) -> Result<StillCapture> {
        self.validate()?;
        ensure!(
            time.is_finite() && time >= self.start && time <= self.start + self.duration,
            "freeze capture outside clip"
        );
        // Studio intentionally uses the stored scalar speed, including for curves.
        let source_seconds = self.source_start + (time - self.start) * self.speed;
        let micros = (source_seconds * 1_000_000.).round();
        let millis = (source_seconds * 1_000.).round();
        ensure!(
            micros.is_finite() && micros >= 0. && micros <= 9_007_199_254_740_991.,
            "freeze capture clock exceeds wire range"
        );
        Ok(StillCapture {
            selection: StillCaptureSelection::AtOrAfterThenFirst,
            source_seconds,
            seek_time_us: micros as u64,
            cache_time_ms: millis as u64,
            width: 1280,
            jpeg_quality: 4,
        })
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<StillCapture>,
    pub changed: bool,
    pub parts: Vec<PlannedPart>,
    pub ripple_by: f64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackGroups {
    #[serde(default)]
    pub clip: std::collections::BTreeMap<String, Vec<super::concat_effect_upgrade::ParamKey>>,
    #[serde(default)]
    pub audio: std::collections::BTreeMap<String, Vec<super::concat_effect_upgrade::ParamKey>>,
    #[serde(default)]
    pub video: std::collections::BTreeMap<String, Vec<super::concat_effect_upgrade::ParamKey>>,
}
impl TrackGroups {
    fn clip_value(name: &str, value: f64) -> f64 {
        match name {
            "scale" => value.clamp(0.05, 8.),
            "offsetX" | "offsetY" => value.clamp(-3., 3.),
            "opacity" => value.clamp(0., 1.),
            "volume" => value.max(0.),
            _ => value,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.clip.keys().all(|name| matches!(
                name.as_str(),
                "scale" | "offsetX" | "offsetY" | "rotation" | "opacity" | "volume"
            )),
            "unknown source clip key property"
        );
        ensure!(
            self.clip.iter().all(|(name, run)| run
                .iter()
                .all(|key| Self::clip_value(name, key.value) == key.value)),
            "source clip keys are not normalized"
        );
        let runs = [&self.clip, &self.audio, &self.video];
        ensure!(
            runs.iter().map(|r| r.len()).sum::<usize>() <= 512
                && runs
                    .iter()
                    .flat_map(|r| r.values())
                    .map(Vec::len)
                    .sum::<usize>()
                    <= 8192,
            "source edit track budget exceeded"
        );
        for run in runs.iter().flat_map(|r| r.values()) {
            ensure!(
                run.len() <= 4096
                    && run.iter().all(|k| k.at.is_finite()
                        && (0. ..=1.).contains(&k.at)
                        && k.value.is_finite()
                        && k.ease.iter().all(|v| v.is_finite())),
                "invalid source edit key run"
            );
        }
        Ok(())
    }
    fn rewindow(&self, from: f64, to: f64) -> Result<Self> {
        let map = |runs: &std::collections::BTreeMap<
            String,
            Vec<super::concat_effect_upgrade::ParamKey>,
        >|
         -> Result<
            std::collections::BTreeMap<String, Vec<super::concat_effect_upgrade::ParamKey>>,
        > {
            runs.iter()
                .map(|(name, keys)| {
                    Ok((
                        name.clone(),
                        super::concat_animation::PreparedTrack::new(keys)?.rewindow(from, to)?,
                    ))
                })
                .collect()
        };
        let mut next = Self {
            clip: map(&self.clip)?,
            audio: map(&self.audio)?,
            video: map(&self.video)?,
        };
        // The source Editor tidies clip-property bounds after the rewindow.
        for (name, run) in &mut next.clip {
            for key in run {
                key.value = Self::clip_value(name, key.value);
            }
        }
        next.validate()?;
        Ok(next)
    }
}
pub fn plan(request: PlanRequest) -> Result<EditPlan> {
    ensure!(
        request
            .clock
            .speed_curve
            .as_ref()
            .is_none_or(|p| p.len() <= 4096),
        "source edit curve exceeds point budget"
    );
    ensure!(
        [request.fade_in, request.fade_out]
            .iter()
            .all(|v| v.is_finite() && (0. ..=360_000.).contains(v)),
        "invalid source fades"
    );
    request.tracks.validate()?;
    let clock = request.clock;
    clock.validate()?;
    ensure!(
        clock.clone().tidy() == clock,
        "source edit requires a normalized document clock"
    );
    let mut ripple_by = 0.;
    let mut capture = None;
    let changed;
    // Window values are original seconds before post-command duration normalization.
    let clocks = match request.command {
        Edit::Move { start_tick } => {
            let next = clock.relocate(start_tick)?;
            changed = next != clock;
            vec![("clip", next, None)]
        }
        Edit::SetCurve { points } => {
            ensure!(
                points.as_ref().is_none_or(|p| p.len() <= 4096),
                "source edit curve exceeds point budget"
            );
            let (next, applied) = clock.set_curve_outcome(points)?;
            changed = applied;
            vec![("clip", next, None)]
        }
        Edit::Split { time } => match clock.split(time)? {
            Some((head, tail)) => {
                changed = true;
                let offset = time - clock.start;
                vec![
                    ("head", head, Some((0., offset))),
                    ("tail", tail, Some((offset, clock.duration))),
                ]
            }
            None => {
                changed = false;
                vec![("clip", clock.clone(), None)]
            }
        },
        Edit::TrimStart {
            delta,
            floor,
            ripple,
        } => {
            let t = clock.trim_start(delta, floor, ripple)?;
            changed = t.applied;
            ripple_by = t.ripple_by;
            vec![(
                "clip",
                t.clock,
                t.applied.then_some((t.source_from, t.source_to)),
            )]
        }
        Edit::TrimEnd {
            delta,
            ceiling,
            ripple,
        } => {
            let t = clock.trim_end(delta, ceiling, ripple)?;
            changed = t.applied;
            ripple_by = t.ripple_by;
            vec![(
                "clip",
                t.clock,
                t.applied.then_some((t.source_from, t.source_to)),
            )]
        }
        Edit::Freeze { time, duration } => match clock.freeze(time, duration)? {
            Some(f) => {
                capture = Some(clock.still_capture(time)?);
                changed = true;
                ripple_by = f.ripple_by;
                let offset = time - clock.start;
                vec![
                    ("head", f.head, Some((0., offset))),
                    ("still", f.still, None),
                    ("tail", f.tail, Some((offset, clock.duration))),
                ]
            }
            None => {
                changed = false;
                vec![("clip", clock.clone(), None)]
            }
        },
    };
    let parts = clocks
        .into_iter()
        .map(|(role, next, window)| {
            let mut tracks = if let Some((from, to)) = window {
                request
                    .tracks
                    .rewindow(from / clock.duration, to / clock.duration)?
            } else {
                request.tracks.clone()
            };
            if role == "still" {
                tracks.audio.clear();
            }
            Ok(PlannedPart {
                role,
                tracks,
                fade_in: if role == "tail" || role == "still" {
                    0.
                } else {
                    request.fade_in
                },
                fade_out: if role == "head" || role == "still" {
                    0.
                } else {
                    request.fade_out
                },
                placement: next.placement(request.rate_num, request.rate_den)?,
                clock: next,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(EditPlan {
        capture,
        changed,
        parts,
        ripple_by,
    })
}
