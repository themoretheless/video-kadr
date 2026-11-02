//! Owned time-varying delay stages for the Concat Robot contract.
pub(crate) struct Phaser {
    ring: Vec<Vec<f64>>,
    triangle: Vec<usize>,
    phase: usize,
    write: usize,
}
impl Phaser {
    pub(crate) fn new(rate: u32, channels: usize, speed: f64) -> Self {
        let rate = f64::from(rate);
        let delay = (0.003 * rate + 0.5) as usize;
        let period = (rate / speed + 0.5) as usize;
        let triangle = (0..period)
            .map(|i| {
                let point = (i + (period as f64 * 0.25 + 0.5) as usize) % period;
                let x = point as f64 * 2. / period as f64;
                let unit = match 4 * point / period {
                    0 => x + 0.5,
                    1 | 2 => 1.5 - x,
                    _ => x - 1.5,
                };
                (unit.mul_add((delay - 1) as f64, 1.) + 0.5) as usize
            })
            .collect();
        Self {
            ring: vec![vec![0.; delay]; channels],
            triangle,
            phase: 0,
            write: 0,
        }
    }
    pub(crate) fn reset(&mut self) {
        for ring in &mut self.ring {
            ring.fill(0.);
        }
        self.phase = 0;
        self.write = 0;
    }
    /// Scalar units are retained here; packet output owns integer clipping or float rounding.
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        let size = self.ring[0].len();
        let read = (self.write + self.triangle[self.phase]) % size;
        self.write = (self.write + 1) % size;
        for (ch, sample) in samples.iter_mut().enumerate() {
            let value = sample.mul_add(0.4, self.ring[ch][read] * 0.6);
            self.ring[ch][self.write] = value;
            *sample = value * 0.74;
        }
        self.phase = (self.phase + 1) % self.triangle.len();
    }
}
pub(crate) struct Flanger {
    ring: Vec<Vec<f64>>,
    sine: Vec<f32>,
    sweep: usize,
    position: usize,
}
impl Flanger {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        let rate = f64::from(rate);
        let length = (0.006 * rate + 2.5) as usize;
        let period = (rate / 0.8) as usize;
        let minimum = (0.002 * rate).round_ties_even();
        let sine = (0..period)
            .map(|i| {
                let point = (i + (period as f64 * 0.75 + 0.5) as usize) % period;
                let unit =
                    ((point as f64 / period as f64 * 2. * std::f64::consts::PI).sin() + 1.) / 2.;
                unit.mul_add((length - 2) as f64 - minimum, minimum) as f32
            })
            .collect();
        Self {
            ring: vec![vec![0.; length]; channels],
            sine,
            sweep: 0,
            position: 0,
        }
    }
    pub(crate) fn reset(&mut self) {
        for ring in &mut self.ring {
            ring.fill(0.);
        }
        self.sweep = 0;
        self.position = 0;
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        let length = self.ring[0].len();
        self.position = (self.position + length - 1) % length;
        for (ch, sample) in samples.iter_mut().enumerate() {
            let input = *sample;
            let offset = (ch as f64 * self.sine.len() as f64 * 0.25 + 0.5) as usize;
            let delay = f64::from(self.sine[(self.sweep + offset) % self.sine.len()]);
            let n = delay as usize;
            let fraction = delay - n as f64;
            let ring = &mut self.ring[ch];
            ring[self.position] = input;
            let a = ring[(self.position + n) % length];
            let b = ring[(self.position + n + 1) % length];
            let delayed = (b - a).mul_add(fraction, a);
            *sample = input.mul_add(1. / 1.71, delayed * (0.71 / 1.71));
        }
        self.sweep = (self.sweep + 1) % self.sine.len();
    }
}
pub(crate) struct Robot {
    phaser: Phaser,
    flanger: Flanger,
}
impl Robot {
    pub(crate) fn new(rate: u32, channels: usize, speed: f64) -> Self {
        Self {
            phaser: Phaser::new(rate, channels, speed),
            flanger: Flanger::new(rate, channels),
        }
    }
    pub(crate) fn reset(&mut self) {
        self.phaser.reset();
        self.flanger.reset();
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64], double: bool) {
        if !double {
            for sample in &mut *samples {
                *sample = f64::from(*sample as f32);
            }
        }
        self.phaser.process_frame(samples);
        if !double {
            for sample in &mut *samples {
                *sample = f64::from(*sample as f32);
            }
        }
        self.flanger.process_frame(samples);
    }
}
