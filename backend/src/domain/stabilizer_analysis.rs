//! Resumable original Warp Stabilizer analysis; only the preceding pyramid is retained.
use super::{
    motion_tracker::{track_step, Gray, Prepared, TrackMethod},
    warp_stabilizer::{Affine, StabilizerPath, Vec2},
};
/// State is committed only after a frame has been fetched and all checkpoints pass.
/// A failed fetch leaves `request` unchanged, so an asynchronous decoder can retry it.
pub struct StabilizerAnalysis {
    fps: f64,
    count: usize,
    scale: f32,
    method: TrackMethod,
    next: usize,
    previous: Option<Prepared>,
    restore_previous: bool,
    dimensions: Option<(usize, usize)>,
    region: [Vec2; 4],
    steps: Vec<Affine>,
}
impl StabilizerAnalysis {
    pub fn new(
        duration: f64,
        frame_rate: f64,
        source_width: u32,
        method: TrackMethod,
        detailed: bool,
    ) -> Option<Self> {
        if !duration.is_finite() || duration < 0. || !frame_rate.is_finite() {
            return None;
        }
        let fps = frame_rate.max(1.);
        let count = ((duration * fps).round() as usize).clamp(1, 20000);
        Some(Self {
            fps,
            count,
            scale: ((if detailed { 960. } else { 480. }) / source_width.max(1) as f32).min(1.),
            method,
            next: 0,
            previous: None,
            restore_previous: false,
            dimensions: None,
            region: [Vec2::ZERO; 4],
            steps: Vec::with_capacity(count.saturating_sub(1)),
        })
    }
    pub fn request(&self, now: f64) -> Option<(usize, f64, f32)> {
        (self.next < self.count && now.is_finite()).then(|| {
            let ordinal = if self.restore_previous {
                self.next - 1
            } else {
                self.next
            };
            (ordinal, ordinal as f64 / self.fps - now, self.scale)
        })
    }
    pub fn completed_frames(&self) -> usize {
        self.next
    }
    pub fn total_frames(&self) -> usize {
        self.count
    }
    pub fn storage_bytes(&self) -> usize {
        self.steps.capacity() * std::mem::size_of::<Affine>()
            + self.previous.as_ref().map_or(0, Prepared::storage_bytes)
    }
    pub fn push<E>(
        &mut self,
        frame: Option<Gray>,
        checkpoint: &mut dyn FnMut() -> Result<(), E>,
    ) -> Result<bool, E> {
        if self.next == self.count {
            return Ok(false);
        }
        checkpoint()?;
        let current = frame.map(Prepared::new);
        if self.restore_previous {
            checkpoint()?;
            self.previous = current;
            self.restore_previous = false;
            return Ok(true);
        }
        let mut dimensions = self.dimensions;
        let mut region = self.region;
        if dimensions.is_none() {
            if let Some(frame) = current.as_ref() {
                let (w, h) = (frame.base().w, frame.base().h);
                dimensions = Some((w, h));
                region = [
                    Vec2::new(8., 8.),
                    Vec2::new(w as f64 - 8., 8.),
                    Vec2::new(w as f64 - 8., h as f64 - 8.),
                    Vec2::new(8., h as f64 - 8.),
                ];
            }
        }
        let step = if self.next > 0 {
            checkpoint()?;
            Some(match (&self.previous, &current) {
                (Some(a), Some(b)) if a.base().w == b.base().w => {
                    track_step(a, b, &region, self.method)
                        .map(|s| s.transform)
                        .unwrap_or(Affine::IDENTITY)
                }
                _ => Affine::IDENTITY,
            })
        } else {
            None
        };
        checkpoint()?;
        if let Some(step) = step {
            self.steps.push(step)
        }
        self.previous = current;
        self.dimensions = dimensions;
        self.region = region;
        self.next += 1;
        Ok(true)
    }
    pub fn into_path(self) -> Option<StabilizerPath> {
        if self.next != self.count {
            return None;
        }
        let (w, h) = self.dimensions?;
        Some(StabilizerPath {
            w,
            h,
            steps: self.steps,
            fps: self.fps,
        })
    }
}
/// Checkpoints retain motion results and source ordinals, never source pixels.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisCheckpoint {
    version: u32,
    fps: u64,
    count: usize,
    scale: u32,
    method: u8,
    next: usize,
    previous: bool,
    dimensions: Option<(usize, usize)>,
    region: [[u64; 2]; 4],
    steps: Vec<[u64; 6]>,
}
impl StabilizerAnalysis {
    pub fn checkpoint(&self) -> AnalysisCheckpoint {
        AnalysisCheckpoint {
            version: 1,
            fps: self.fps.to_bits(),
            count: self.count,
            scale: self.scale.to_bits(),
            method: match self.method {
                TrackMethod::Position => 0,
                TrackMethod::PositionRotation => 1,
                TrackMethod::PositionScaleRotation => 2,
            },
            next: self.next,
            previous: self.restore_previous || self.previous.is_some(),
            dimensions: self.dimensions,
            region: self.region.map(|p| [p.x.to_bits(), p.y.to_bits()]),
            steps: self
                .steps
                .iter()
                .map(|a| {
                    [
                        a.a.to_bits(),
                        a.b.to_bits(),
                        a.c.to_bits(),
                        a.d.to_bits(),
                        a.e.to_bits(),
                        a.f.to_bits(),
                    ]
                })
                .collect(),
        }
    }
    pub fn restore(c: AnalysisCheckpoint) -> anyhow::Result<Self> {
        use anyhow::ensure;
        ensure!(
            c.version == 1
                && c.count >= 1
                && c.count <= 20000
                && c.next <= c.count
                && c.steps.len() == c.next.saturating_sub(1),
            "invalid analysis checkpoint clock/count"
        );
        let fps = f64::from_bits(c.fps);
        let scale = f32::from_bits(c.scale);
        ensure!(
            fps.is_finite() && fps >= 1. && scale.is_finite() && scale > 0. && scale <= 1.,
            "invalid analysis checkpoint rate/scale"
        );
        ensure!(
            !c.previous || (c.next > 0 && c.dimensions.is_some()),
            "invalid preceding frame checkpoint"
        );
        if let Some((w, h)) = c.dimensions {
            ensure!(
                w > 0 && h > 0 && w <= 65536 && h <= 65536,
                "invalid checkpoint dimensions"
            )
        }
        let method = match c.method {
            0 => TrackMethod::Position,
            1 => TrackMethod::PositionRotation,
            2 => TrackMethod::PositionScaleRotation,
            _ => anyhow::bail!("invalid checkpoint tracking method"),
        };
        let mut region = [Vec2::ZERO; 4];
        for (p, b) in region.iter_mut().zip(c.region) {
            let x = f64::from_bits(b[0]);
            let y = f64::from_bits(b[1]);
            ensure!(x.is_finite() && y.is_finite(), "invalid checkpoint region");
            *p = Vec2::new(x, y);
        }
        let mut steps = Vec::with_capacity(c.count.saturating_sub(1));
        for b in c.steps {
            let v = b.map(f64::from_bits);
            ensure!(
                v.iter().all(|v| v.is_finite()),
                "invalid checkpoint transform"
            );
            steps.push(Affine {
                a: v[0],
                b: v[1],
                c: v[2],
                d: v[3],
                e: v[4],
                f: v[5],
            });
        }
        Ok(Self {
            fps,
            count: c.count,
            scale,
            method,
            next: c.next,
            previous: None,
            restore_previous: c.previous && c.next < c.count,
            dimensions: c.dimensions,
            region,
            steps,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(k: usize) -> Gray {
        let (w, h) = (48, 40);
        Gray::new(
            w,
            h,
            (0..w * h)
                .map(|i| {
                    let x = (i % w + k) % w;
                    let y = i / w;
                    ((x * 17 + y * 31 + (x / 7) * (y / 5) * 53) % 256) as f32 / 255.
                })
                .collect(),
        )
    }
    #[test]
    fn pause_missing_frames_and_transactional_cancel() {
        let mut a = StabilizerAnalysis::new(1., 6., 1920, TrackMethod::Position, false).unwrap();
        assert_eq!(a.request(0.25), Some((0, -0.25, 0.25)));
        a.push::<&str>(Some(frame(0)), &mut || Ok(())).unwrap();
        let bytes = a.storage_bytes();
        let request = a.request(0.25);
        assert_eq!(
            a.push(Some(frame(1)), &mut || Err("cancelled"))
                .unwrap_err(),
            "cancelled"
        );
        assert_eq!(a.request(0.25), request);
        assert_eq!(a.storage_bytes(), bytes);
        a.push::<&str>(None, &mut || Ok(())).unwrap();
        for k in 2..6 {
            assert!(a.push::<&str>(Some(frame(k)), &mut || Ok(())).unwrap());
        }
        assert!(a.request(0.).is_none());
        assert!(!a.push::<&str>(None, &mut || Ok(())).unwrap());
        let path = a.into_path().unwrap();
        assert_eq!(path.steps.len(), 5);
        assert_eq!(path.steps[0], Affine::IDENTITY);
        assert_eq!(path.steps[1], Affine::IDENTITY);
    }
    #[test]
    fn resumed_path_equals_continuous_original_analysis() {
        let mut incremental =
            StabilizerAnalysis::new(1., 6., 1920, TrackMethod::PositionScaleRotation, false)
                .unwrap();
        let mut peak = 0;
        while let Some((k, _, _)) = incremental.request(0.5) {
            let before = incremental.completed_frames();
            assert_eq!(incremental.request(0.5).unwrap().0, before);
            incremental
                .push::<&str>(Some(frame(k)), &mut || Ok(()))
                .unwrap();
            peak = peak.max(incremental.storage_bytes());
        }
        let result = incremental.into_path().unwrap();
        let continuous = super::super::warp_stabilizer::analyse_clip(
            0.5,
            1.,
            6.,
            1920,
            TrackMethod::PositionScaleRotation,
            false,
            &mut |dt, _| Ok::<_, &str>(Some(frame(((dt + 0.5) * 6.).round() as usize))),
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(result.steps, continuous.steps);
        assert_eq!(
            (result.w, result.h, result.fps),
            (continuous.w, continuous.h, continuous.fps)
        );
        assert!(peak < 48 * 40 * 8 + 6 * std::mem::size_of::<Affine>());
    }
}
#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    fn frame(k: usize) -> Option<Gray> {
        if k == 2 {
            return None;
        }
        Some(Gray::new(
            48,
            40,
            (0..48 * 40)
                .map(|i| {
                    let x = (i % 48 + k) % 48;
                    let y = i / 48;
                    ((x * 17 + y * 31 + (x / 7) * (y / 5) * 53) % 256) as f32 / 255.
                })
                .collect(),
        ))
    }
    fn run(a: &mut StabilizerAnalysis) -> Vec<usize> {
        let mut read = vec![];
        while let Some((k, _, _)) = a.request(0.5) {
            read.push(k);
            a.push::<()>(frame(k), &mut || Ok(())).unwrap();
        }
        read
    }
    fn bits(p: StabilizerPath) -> Vec<u64> {
        p.steps
            .into_iter()
            .flat_map(|a| [a.a, a.b, a.c, a.d, a.e, a.f].map(f64::to_bits))
            .collect()
    }
    #[test]
    fn serialized_checkpoint_resumes_all_methods_and_missing_boundaries_exactly() {
        for method in [
            TrackMethod::Position,
            TrackMethod::PositionRotation,
            TrackMethod::PositionScaleRotation,
        ] {
            for cut in 0..=6 {
                let mut full = StabilizerAnalysis::new(1., 6., 1920, method, false).unwrap();
                run(&mut full);
                let expected = bits(full.into_path().unwrap());
                let mut part = StabilizerAnalysis::new(1., 6., 1920, method, false).unwrap();
                for k in 0..cut {
                    part.push::<()>(frame(k), &mut || Ok(())).unwrap();
                }
                let bytes = serde_json::to_vec(&part.checkpoint()).unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains("px"));
                let mut restored =
                    StabilizerAnalysis::restore(serde_json::from_slice(&bytes).unwrap()).unwrap();
                assert_eq!(restored.completed_frames(), cut);
                let read = run(&mut restored);
                if cut < 6 {
                    let first = if cut > 0 && cut != 3 { cut - 1 } else { cut };
                    assert_eq!(read[0], first);
                    assert!(read.iter().all(|k| *k >= first));
                } else {
                    assert!(read.is_empty())
                }
                assert_eq!(
                    bits(restored.into_path().unwrap()),
                    expected,
                    "cut={cut}, method={method:?}"
                );
            }
        }
    }
    #[test]
    fn malformed_checkpoint_rejected() {
        let a = StabilizerAnalysis::new(1., 6., 1920, TrackMethod::Position, false).unwrap();
        let mut value = serde_json::to_value(a.checkpoint()).unwrap();
        value["next"] = serde_json::json!(999);
        assert!(StabilizerAnalysis::restore(serde_json::from_value(value).unwrap()).is_err());
        let mut c = a.checkpoint();
        c.previous = true;
        assert!(StabilizerAnalysis::restore(c).is_err());
    }
}
