//! Linked-channel linear lookahead envelope for Concat's fixed limiter stages.
//! Candidate records preserve the pinned finite-ring sentinel behavior.
#[derive(Clone, Copy, Default)]
struct Candidate {
    slot: Option<usize>,
    following_slope: f64,
}
pub(crate) struct LookaheadLimiter {
    limit: f64,
    level: f64,
    release_frames: f64,
    lookahead: usize,
    channels: usize,
    ring: Vec<f64>,
    peaks: Vec<f64>,
    candidates: Vec<Candidate>,
    head: usize,
    length: i64,
    frame: u64,
    gain: f64,
    slope: f64,
    uniform_ceiling: Option<f64>,
}
impl LookaheadLimiter {
    pub(crate) fn new(rate: u32, channels: usize, limit: f64, auto_level: bool) -> Self {
        Self::with_release(rate, channels, limit, auto_level, 50.)
    }
    /// Original Concat limiter stages use the default 5 ms attack.
    pub(crate) fn with_release(
        rate: u32,
        channels: usize,
        limit: f64,
        auto_level: bool,
        release_ms: f64,
    ) -> Self {
        assert!(release_ms.is_finite() && (1. ..=8000.).contains(&release_ms));
        assert!((8000..=192000).contains(&rate) && (1..=16).contains(&channels));
        assert!((0.0625..=1.).contains(&limit));
        let lookahead = (f64::from(rate) * 0.005) as usize;
        Self {
            limit,
            level: if auto_level { 1. / limit } else { 1. },
            release_frames: f64::from(rate) * (release_ms / 1000.),
            lookahead,
            channels,
            ring: vec![0.; lookahead * channels],
            peaks: vec![0.; lookahead],
            candidates: vec![Candidate::default(); lookahead * channels],
            head: 0,
            length: 0,
            frame: 0,
            gain: 1.,
            slope: 0.,
            uniform_ceiling: None,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.ring.fill(0.);
        self.peaks.fill(0.);
        self.candidates.fill(Candidate::default());
        self.head = 0;
        self.length = 0;
        self.frame = 0;
        self.gain = 1.;
        self.slope = 0.;
        self.uniform_ceiling = None;
    }
    #[cfg(test)]
    pub(crate) fn audible_delay(&self) -> usize {
        self.lookahead - 1
    }
    fn enqueue_after(
        &mut self,
        offset: usize,
        slope: f64,
        slot: usize,
        release: f64,
        ceiling: f64,
    ) {
        let cap = self.candidates.len();
        self.candidates[(self.head + offset) % cap].following_slope = slope;
        self.length = offset as i64 + 1;
        let next = (self.head + self.length as usize) % cap;
        self.candidates[next] = Candidate {
            slot: Some(slot),
            following_slope: release,
        };
        self.candidates[(next + 1) % cap].slot = None;
        self.length += 1;
        if self.length as usize >= cap || self.uniform_ceiling != Some(ceiling) {
            self.uniform_ceiling = None;
        }
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        assert_eq!(samples.len(), self.channels);
        let write_frame = self.frame as usize % self.lookahead;
        let write_slot = write_frame * self.channels;
        let mut peak = 0f64;
        for ch in 0..self.channels {
            self.ring[write_slot + ch] = samples[ch];
            peak = peak.max(samples[ch].abs());
        }
        self.peaks[write_frame] = peak;
        let cap = self.candidates.len();
        if peak > self.limit {
            let ceiling = (self.limit / peak).min(1.);
            let release = (1. - ceiling) / self.release_frames;
            let attack = (ceiling - self.gain) / self.lookahead as f64;
            if attack < self.slope {
                self.slope = attack;
                self.head = 0;
                self.length = 1;
                self.candidates[0] = Candidate {
                    slot: Some(write_slot),
                    following_slope: release,
                };
                self.candidates[1].slot = None;
                self.uniform_ceiling = Some(ceiling);
            } else if self.uniform_ceiling == Some(ceiling) && self.length > 0 {
                // A flat candidate run cannot constrain another equal ceiling
                // before its final release. No intermediate scan is needed.
                let last = self.length as usize - 1;
                if self.candidates[(self.head + last) % cap].following_slope > 0. {
                    self.enqueue_after(last, 0., write_slot, release, ceiling);
                }
            } else {
                // Repeated physical slots have identical constraints, so a
                // logical queue longer than its storage needs only one lap.
                let count = (self.length.max(0) as usize).min(cap);
                let mut offset = 0;
                while offset < count {
                    let candidate = self.candidates[(self.head + offset) % cap];
                    let old_peak = candidate
                        .slot
                        .map_or(0., |slot| self.peaks[slot / self.channels]);
                    let distance = match candidate.slot {
                        Some(slot) => (cap + write_slot - slot) % cap / self.channels,
                        None => (cap + write_slot + 1) % cap / self.channels,
                    };
                    let slope = (ceiling - self.limit / old_peak) / distance as f64;
                    if slope < candidate.following_slope {
                        self.enqueue_after(offset, slope, write_slot, release, ceiling);
                        break;
                    }
                    offset += 1;
                }
            }
        }
        self.gain += self.slope;
        let read_frame = (write_frame + 1) % self.lookahead;
        let read_slot = read_frame * self.channels;
        for ch in 0..self.channels {
            samples[ch] =
                (self.ring[read_slot + ch] * self.gain).clamp(-self.limit, self.limit) * self.level;
        }
        if self.candidates[self.head].slot == Some(read_slot) {
            self.gain = self.limit / self.peaks[read_frame];
            self.slope = self.candidates[self.head].following_slope;
            self.candidates[self.head].slot = None;
            self.head = (self.head + 1) % cap;
            self.length -= 1;
            if self.length <= 0 {
                self.uniform_ceiling = None;
            }
        }
        if self.gain > 1. {
            self.gain = 1.;
            self.slope = 0.;
            self.head = 0;
            self.length = 0;
            self.candidates[0].slot = None;
            self.uniform_ceiling = None;
        }
        if self.gain <= 0. {
            self.gain = 1e-13;
            self.slope = (1. - self.gain) / self.release_frames;
        }
        if self.gain != 1. && 1. - self.gain < 1e-13 {
            self.gain = 1.;
        }
        if self.slope != 0. && self.slope.abs() < 1e-14 {
            self.slope = 0.;
        }
        self.frame += 1;
    }
}
