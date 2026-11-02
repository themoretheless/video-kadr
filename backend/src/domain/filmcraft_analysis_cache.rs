//! Request-scoped resumable analysis and decoded-frame working set.
//! Never shared between authoring documents, users or source revisions.
use super::{
    filmcraft_generator_source_provider::VideoFrameKey, motion_tracker::Gray,
    stabilizer_analysis::StabilizerAnalysis, warp_stabilizer::StabilizerPath,
};
use anyhow::{ensure, Result};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};
pub type SharedAnalysis = Arc<Mutex<AnalysisCache>>;
pub type AnalysisKey = (u64, u64, u64, i64, usize, u32);
pub type StableAnalysisKey = (u64, u64, u64, usize, u32);
pub type ReadyPaths = BTreeMap<StableAnalysisKey, Option<Arc<StabilizerPath>>>;
fn stable(key: AnalysisKey) -> StableAnalysisKey {
    (key.0, key.1, key.2, key.4, key.5)
}
pub fn ready_bytes(paths: &ReadyPaths) -> usize {
    let mut seen = BTreeSet::new();
    paths
        .values()
        .map(|p| {
            1024 + p
                .as_ref()
                .filter(|p| seen.insert(Arc::as_ptr(p) as usize))
                .map_or(0, |p| {
                    p.steps.capacity() * std::mem::size_of::<super::warp_stabilizer::Affine>()
                })
        })
        .sum()
}
enum Entry {
    Working(StabilizerAnalysis),
    Ready(Option<Arc<StabilizerPath>>),
}
#[derive(Default)]
pub struct AnalysisCache {
    entries: BTreeMap<AnalysisKey, Entry>,
    seeded: ReadyPaths,
    lru: VecDeque<VideoFrameKey>,
    reads: BTreeSet<VideoFrameKey>,
}
impl AnalysisCache {
    pub fn begin_attempt(&mut self) {
        self.reads.clear();
    }
    pub fn touch(&mut self, key: VideoFrameKey) {
        self.reads.insert(key);
        self.register(key);
    }
    pub fn register(&mut self, key: VideoFrameKey) {
        self.lru.retain(|k| *k != key);
        self.lru.push_back(key);
    }
    pub fn read_scope(&self) -> BTreeSet<VideoFrameKey> {
        self.reads.clone()
    }
    pub fn finish_read_scope(&mut self, before: BTreeSet<VideoFrameKey>) {
        self.reads.retain(|key| before.contains(key));
    }
    pub fn eviction_candidate(&mut self) -> Option<VideoFrameKey> {
        let i = self.lru.iter().position(|k| !self.reads.contains(k))?;
        self.lru.remove(i)
    }
    pub fn seed(&mut self, paths: ReadyPaths) -> Result<()> {
        ensure!(
            paths.len() <= 128 && ready_bytes(&paths) <= 32 * 1024 * 1024,
            "preview analysis seed budget exceeded"
        );
        self.seeded = paths;
        Ok(())
    }
    pub fn ready_paths(&self) -> ReadyPaths {
        let mut paths = self.seeded.clone();
        for (key, entry) in &self.entries {
            if let Entry::Ready(path) = entry {
                paths.insert(stable(*key), path.clone());
            }
        }
        paths
    }
    pub fn committed_frames(&self) -> usize {
        self.entries
            .values()
            .filter_map(|e| match e {
                Entry::Working(a) => Some(a.completed_frames()),
                _ => None,
            })
            .sum()
    }
    pub fn checkpoint(&self) -> Vec<(AnalysisKey, super::stabilizer_analysis::AnalysisCheckpoint)> {
        self.entries
            .iter()
            .filter_map(|(k, e)| match e {
                Entry::Working(a) => Some((*k, a.checkpoint())),
                _ => None,
            })
            .collect()
    }
    pub fn restore_checkpoint(
        &mut self,
        entries: Vec<(AnalysisKey, super::stabilizer_analysis::AnalysisCheckpoint)>,
    ) -> Result<()> {
        ensure!(entries.len() <= 128, "too many checkpoint states");
        let mut restored = Self::default();
        for (k, c) in entries {
            ensure!(
                !restored.entries.contains_key(&k),
                "duplicate checkpoint key"
            );
            restored.put(k, Entry::Working(StabilizerAnalysis::restore(c)?))?;
        }
        for (k, e) in restored.entries {
            if !self.seeded.contains_key(&stable(k)) {
                self.put(k, e)?;
            }
        }
        Ok(())
    }
    pub fn progress_percent(&self) -> f64 {
        let mut completed = 0usize;
        let mut total = 0usize;
        for entry in self.entries.values() {
            match entry {
                Entry::Working(a) => {
                    completed += a.completed_frames();
                    total += a.total_frames();
                }
                Entry::Ready(_) => {
                    completed += 1;
                    total += 1;
                }
            }
        }
        if total == 0 {
            0.
        } else {
            (completed as f64 / total as f64 * 95.).min(95.)
        }
    }
    pub fn storage_bytes(&self) -> usize {
        ready_bytes(&self.ready_paths())
            + self
                .entries
                .values()
                .filter_map(|e| {
                    if let Entry::Working(a) = e {
                        Some(a.storage_bytes() + 1024)
                    } else {
                        None
                    }
                })
                .sum::<usize>()
    }
    fn put(&mut self, key: AnalysisKey, entry: Entry) -> Result<()> {
        self.entries.insert(key, entry);
        while (self.storage_bytes() > 32 * 1024 * 1024 || self.ready_paths().len() > 128)
            && !self.seeded.is_empty()
        {
            self.seeded.pop_first();
        }
        if self.storage_bytes() > 32 * 1024 * 1024 || self.entries.len() > 128 {
            self.entries.remove(&key);
            anyhow::bail!("preview analysis state budget exceeded")
        }
        Ok(())
    }
}
/// Fetch errors (including the decoder's pending sentinel) retain committed frames.
/// Mutexes are released before callbacks, allowing nested clip analysis.
pub fn resume(
    cache: &SharedAnalysis,
    key: AnalysisKey,
    create: impl FnOnce() -> Option<StabilizerAnalysis>,
    now: f64,
    source: &mut dyn FnMut(f64, f32) -> Result<Option<Gray>>,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<Arc<StabilizerPath>>> {
    checkpoint()?;
    let entry = {
        let mut cache = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?;
        cache
            .entries
            .remove(&key)
            .or_else(|| cache.seeded.get(&stable(key)).cloned().map(Entry::Ready))
    };
    let mut state = match entry {
        Some(Entry::Ready(path)) => {
            cache
                .lock()
                .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?
                .put(key, Entry::Ready(path.clone()))?;
            return Ok(path);
        }
        Some(Entry::Working(a)) => a,
        None => {
            let Some(a) = create() else { return Ok(None) };
            a
        }
    };
    while let Some((_, dt, scale)) = state.request(now) {
        checkpoint()?;
        let frame = match source(dt, scale) {
            Ok(frame) => frame,
            Err(error) => {
                cache
                    .lock()
                    .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?
                    .put(key, Entry::Working(state))?;
                return Err(error);
            }
        };
        state.push(frame, checkpoint)?;
    }
    ensure!(
        state.completed_frames() == state.total_frames(),
        "unfinished source analysis"
    );
    checkpoint()?;
    let path = state.into_path().map(Arc::new);
    cache
        .lock()
        .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?
        .put(key, Entry::Ready(path.clone()))?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        motion_tracker::{Gray, TrackMethod},
        warp_stabilizer,
    };
    #[test]
    fn pending_reads_resume_once_and_ready_path_reuses_without_fetch() {
        let cache = SharedAnalysis::default();
        let key = (1, 2, 3, 4, 5, 0);
        let mut delivered = 0;
        let mut successful = 0;
        let mut attempts = 0;
        let path = loop {
            attempts += 1;
            assert!(attempts <= 8, "analysis restarted");
            let mut read = |_: f64, _: f32| {
                if successful == delivered {
                    return Err(anyhow::anyhow!("pending"));
                }
                successful += 1;
                Ok(Some(Gray::new(32, 24, vec![0.; 768])))
            };
            let r = resume(
                &cache,
                key,
                || StabilizerAnalysis::new(1., 6., 1920, TrackMethod::Position, false),
                0.5,
                &mut read,
                &mut || Ok(()),
            );
            if let Ok(p) = r {
                break p.unwrap();
            }
            delivered += 1;
        };
        assert_eq!(successful, 6);
        assert_eq!(attempts, 7);
        assert_eq!(path.steps, vec![warp_stabilizer::Affine::IDENTITY; 5]);
        let again = resume(
            &cache,
            key,
            || panic!("recreated"),
            0.5,
            &mut |_, _| panic!("reread"),
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert!(Arc::ptr_eq(&path, &again));
        assert!(cache.lock().unwrap().storage_bytes() < 65536);
    }
    #[test]
    fn eviction_protects_current_attempt_and_tracks_recency() {
        let mut c = AnalysisCache::default();
        let a = VideoFrameKey { item: 1, ticks: 1 };
        let b = VideoFrameKey { item: 1, ticks: 2 };
        c.register(a);
        c.register(b);
        c.begin_attempt();
        c.touch(a);
        assert_eq!(c.eviction_candidate(), Some(b));
        assert_eq!(c.eviction_candidate(), None);
        c.begin_attempt();
        assert_eq!(c.eviction_candidate(), Some(a));
    }
}
/// Bound the live decoded working set, rather than the cumulative number of reads.
/// Frames touched in the current render attempt are protected from eviction.
pub fn admit_frame(
    cache: &SharedAnalysis,
    frames: &mut super::filmcraft_generator_source_provider::VideoFrames,
    key: VideoFrameKey,
    value: Option<Arc<super::filmcraft_generator_source_provider::VideoSourceFrame>>,
    still_bytes: usize,
    max_bytes: usize,
    max_frames: usize,
) -> Result<()> {
    ensure!(
        !frames.contains_key(&key),
        "duplicate pending preview frame"
    );
    ensure!(max_frames > 0, "invalid preview frame budget");
    let bytes =
        |frames: &super::filmcraft_generator_source_provider::VideoFrames| -> Result<usize> {
            let mut seen = BTreeSet::new();
            let mut total = still_bytes;
            for frame in frames.values().flatten().chain(value.iter()) {
                if seen.insert(Arc::as_ptr(&frame.surface) as usize) {
                    total = total
                        .checked_add(frame.surface.rgba.capacity())
                        .ok_or_else(|| anyhow::anyhow!("preview decoded source budget overflow"))?;
                }
            }
            Ok(total)
        };
    while frames.len() >= max_frames || bytes(frames)? > max_bytes {
        let old = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?
            .eviction_candidate()
            .ok_or_else(|| {
                anyhow::anyhow!("preview decoded source budget exceeded (active working set)")
            })?;
        frames.remove(&old);
    }
    frames.insert(key, value);
    cache
        .lock()
        .map_err(|_| anyhow::anyhow!("analysis state lock poisoned"))?
        .register(key);
    Ok(())
}
#[cfg(test)]
mod working_set_tests {
    use super::*;
    use crate::domain::{
        filmcraft_generator_source_provider::{VideoFrames, VideoSourceFrame},
        group_surface::GroupSurface,
    };
    fn frame() -> Option<Arc<VideoSourceFrame>> {
        Some(Arc::new(VideoSourceFrame {
            surface: Arc::new(GroupSurface {
                width: 2,
                height: 2,
                rgba: vec![255; 16],
            }),
            color: filmcraft_color::ColorInfo::SRGB_FULL,
        }))
    }
    #[test]
    fn stream_240_frames_through_two_frame_cache_and_preserve_current_picture() {
        let cache = SharedAnalysis::default();
        let mut frames = VideoFrames::new();
        let root = VideoFrameKey {
            item: 1,
            ticks: 999,
        };
        admit_frame(&cache, &mut frames, root, frame(), 0, 32, 2).unwrap();
        for ticks in 0..240 {
            cache.lock().unwrap().begin_attempt();
            cache.lock().unwrap().touch(root);
            admit_frame(
                &cache,
                &mut frames,
                VideoFrameKey { item: 1, ticks },
                frame(),
                0,
                32,
                2,
            )
            .unwrap();
            assert!(frames.contains_key(&root));
            assert_eq!(frames.len(), 2);
            assert!(frames.contains_key(&VideoFrameKey { item: 1, ticks }));
        }
        assert!(cache.lock().unwrap().lru.len() <= 2);
    }
    #[test]
    fn aliases_share_bytes_and_protected_working_set_refuses() {
        let cache = SharedAnalysis::default();
        let mut frames = VideoFrames::new();
        let a = VideoFrameKey { item: 1, ticks: 1 };
        let b = VideoFrameKey { item: 1, ticks: 2 };
        let shared = frame();
        admit_frame(&cache, &mut frames, a, shared.clone(), 0, 16, 2).unwrap();
        admit_frame(&cache, &mut frames, b, shared, 0, 16, 2).unwrap();
        cache.lock().unwrap().begin_attempt();
        cache.lock().unwrap().touch(a);
        cache.lock().unwrap().touch(b);
        assert!(admit_frame(
            &cache,
            &mut frames,
            VideoFrameKey { item: 1, ticks: 3 },
            frame(),
            0,
            16,
            2
        )
        .is_err());
        assert_eq!(frames.len(), 2);
    }
}
#[cfg(test)]
mod read_scope_tests {
    use super::*;
    #[test]
    fn consumed_analysis_reads_release_without_releasing_outer_picture() {
        let mut c = AnalysisCache::default();
        let root = VideoFrameKey { item: 1, ticks: 1 };
        let child = VideoFrameKey { item: 1, ticks: 2 };
        let inner = VideoFrameKey { item: 1, ticks: 3 };
        c.touch(root);
        let outer = c.read_scope();
        c.touch(child);
        let nested = c.read_scope();
        c.touch(inner);
        c.finish_read_scope(nested);
        assert!(!c.reads.contains(&inner));
        assert!(c.reads.contains(&child));
        c.finish_read_scope(outer);
        assert!(c.reads.contains(&root));
        assert!(!c.reads.contains(&child));
        assert_eq!(c.eviction_candidate(), Some(child));
    }
}
#[cfg(test)]
mod seed_tests {
    use super::*;
    #[test]
    fn completed_seed_reuses_path_across_time_but_not_depth() {
        let path = Arc::new(StabilizerPath {
            w: 32,
            h: 24,
            fps: 6.,
            steps: vec![super::super::warp_stabilizer::Affine::IDENTITY; 5],
        });
        let cache = SharedAnalysis::default();
        cache
            .lock()
            .unwrap()
            .seed(
                [((1, 2, 3, 4, 0), Some(path.clone()))]
                    .into_iter()
                    .collect(),
            )
            .unwrap();
        let got = resume(
            &cache,
            (1, 2, 3, 999, 4, 0),
            || panic!("reanalysed"),
            0.75,
            &mut |_, _| panic!("read source"),
            &mut || Ok(()),
        )
        .unwrap()
        .unwrap();
        assert!(Arc::ptr_eq(&path, &got));
        let mut created = false;
        assert!(resume(
            &cache,
            (1, 2, 3, 999, 4, 1),
            || {
                created = true;
                None
            },
            0.75,
            &mut |_, _| panic!("read source"),
            &mut || Ok(())
        )
        .unwrap()
        .is_none());
        assert!(created);
        assert_eq!(cache.lock().unwrap().ready_paths().len(), 1);
    }
}
#[cfg(test)]
mod seed_budget_tests {
    use super::*;
    use crate::domain::motion_tracker::TrackMethod;
    #[test]
    fn optional_old_seeds_evict_instead_of_refusing_new_frame() {
        let cache = SharedAnalysis::default();
        let path = Arc::new(StabilizerPath {
            w: 1,
            h: 1,
            fps: 1.,
            steps: vec![],
        });
        cache
            .lock()
            .unwrap()
            .seed(
                (0..128)
                    .map(|i| ((1, i, 3, 4, 0), Some(path.clone())))
                    .collect(),
            )
            .unwrap();
        let result = resume(
            &cache,
            (1, 999, 3, 0, 4, 0),
            || StabilizerAnalysis::new(0., 1., 32, TrackMethod::Position, false),
            0.,
            &mut |_, _| Ok(None),
            &mut || Ok(()),
        )
        .unwrap();
        assert!(result.is_none());
        let cache = cache.lock().unwrap();
        assert!(cache.ready_paths().len() <= 128);
        assert!(cache.ready_paths().contains_key(&(1, 999, 3, 4, 0)));
    }
}
