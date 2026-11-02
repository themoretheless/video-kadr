//! Sweet's fixed, zero-delay compand profile; independent double-precision envelopes.
//! SPDX-License-Identifier: LGPL-2.1-or-later
//! License text: backend/licenses/FFmpeg-LGPL-2.1.txt.
//! Algorithm port from FFmpeg libavfilter/af_compand.c (LGPL-2.1-or-later).
//! Copyright (c) 1999 Chris Bagwell, Nick Bailey; 2007 Rob Sykes;
//! 2013 Paul B Mahol; 2014 Andrew Kelley.
//! This implementation is distributed under LGPL-2.1-or-later, without warranty.
#[derive(Clone, Copy, Default)]
struct Segment {
    x: f64,
    y: f64,
    a: f64,
    b: f64,
}
pub(crate) struct Compand {
    segments: [Segment; 18],
    minimum: f64,
    minimum_gain: f64,
    attack: f64,
    decay: f64,
    volume: Vec<f64>,
}
impl Compand {
    pub(crate) fn sweet(rate: u32, channels: usize) -> Self {
        assert!((8000..=192000).contains(&rate) && (1..=16).contains(&channels));
        let mut s = [Segment::default(); 18];
        for (i, (x, y)) in [
            (-80., -80.),
            (-30., -24.),
            (-18., -13.),
            (-8., -5.),
            (0., -1.5),
        ]
        .into_iter()
        .enumerate()
        {
            s[2 * (i + 1)].x = x;
            s[2 * (i + 1)].y = y - x;
        }
        // These fixed points contain no adjacent collinear segments.
        s[0].x = s[2].x - 10.;
        s[0].y = s[2].y;
        for i in (0..18).step_by(2) {
            s[i].y += 1.;
            s[i].x *= std::f64::consts::LN_10 / 20.;
            s[i].y *= std::f64::consts::LN_10 / 20.;
        }
        let radius = 5. * std::f64::consts::LN_10 / 20.;
        for i in (4..18).step_by(2) {
            s[i - 4].a = 0.;
            s[i - 4].b = (s[i - 2].y - s[i - 4].y) / (s[i - 2].x - s[i - 4].x);
            s[i - 2].a = 0.;
            s[i - 2].b = (s[i].y - s[i - 2].y) / (s[i].x - s[i - 2].x);
            let theta = (s[i - 2].y - s[i - 4].y).atan2(s[i - 2].x - s[i - 4].x);
            let len = (s[i - 2].x - s[i - 4].x).hypot(s[i - 2].y - s[i - 4].y);
            let r = radius.min(len);
            s[i - 3].x = s[i - 2].x - r * theta.cos();
            s[i - 3].y = s[i - 2].y - r * theta.sin();
            let theta = (s[i].y - s[i - 2].y).atan2(s[i].x - s[i - 2].x);
            let len = (s[i].x - s[i - 2].x).hypot(s[i].y - s[i - 2].y);
            let r = radius.min(len / 2.);
            let x = s[i - 2].x + r * theta.cos();
            let y = s[i - 2].y + r * theta.sin();
            let cx = (s[i - 3].x + s[i - 2].x + x) / 3.;
            let cy = (s[i - 3].y + s[i - 2].y + y) / 3.;
            s[i - 2].x = x;
            s[i - 2].y = y;
            let in1 = cx - s[i - 3].x;
            let out1 = cy - s[i - 3].y;
            let in2 = s[i - 2].x - s[i - 3].x;
            let out2 = s[i - 2].y - s[i - 3].y;
            s[i - 3].a = (out2 / in2 - out1 / in1) / (in2 - in1);
            s[i - 3].b = (-s[i - 3].a).mul_add(in1, out1 / in1);
        }
        s[15].x = 0.;
        s[15].y = s[16].y;
        Self {
            minimum: s[1].x.exp(),
            minimum_gain: s[1].y.exp(),
            segments: s,
            attack: 1. - (-1. / (f64::from(rate) * 0.010)).exp(),
            decay: 1. - (-1. / (f64::from(rate) * 0.180)).exp(),
            volume: vec![1.; channels],
        }
    }
    pub(crate) fn reset(&mut self) {
        self.volume.fill(1.);
    }
    fn gain(&self, volume: f64) -> f64 {
        if volume < self.minimum {
            return self.minimum_gain;
        }
        let x = volume.ln();
        let index = (1..self.segments.len())
            .find(|&i| x <= self.segments[i].x)
            .map(|i| i - 1)
            .unwrap_or(self.segments.len() - 1);
        let s = self.segments[index];
        let x = x - s.x;
        x.mul_add(s.a.mul_add(x, s.b), s.y).exp()
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        assert_eq!(samples.len(), self.volume.len());
        for ch in 0..samples.len() {
            let delta = samples[ch].abs() - self.volume[ch];
            self.volume[ch] = delta.mul_add(
                if delta > 0. { self.attack } else { self.decay },
                self.volume[ch],
            );
            samples[ch] *= self.gain(self.volume[ch]);
        }
    }
}
