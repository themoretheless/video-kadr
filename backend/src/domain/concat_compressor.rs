//! Owned linked RMS downward compressor for Concat Enhance Voice.
//! Its logarithmic soft knee joins the unity and ratio slopes continuously.
pub(crate) struct Compressor {
    envelope: f64,
    attack: f64,
    release: f64,
    threshold: f64,
    ratio: f64,
    knee_start: f64,
    knee_width: f64,
    detector_start: f64,
}
impl Compressor {
    pub(crate) fn new(rate: u32, threshold: f64, ratio: f64) -> Self {
        let knee = 2.82843f64;
        let low = threshold / knee.sqrt();
        let high = threshold * knee.sqrt();
        Self {
            envelope: 0.,
            attack: (1. / (15. * f64::from(rate) / 4000.)).min(1.),
            release: (1. / (120. * f64::from(rate) / 4000.)).min(1.),
            threshold: threshold.ln(),
            ratio,
            knee_start: low.ln(),
            knee_width: high.ln() - low.ln(),
            detector_start: low * low,
        }
    }
    pub(crate) fn reset(&mut self) {
        self.envelope = 0.;
    }
    pub(crate) fn process_frame(&mut self, samples: &mut [f64]) {
        let mean = samples.iter().map(|v| v.abs()).sum::<f64>() / samples.len() as f64;
        let detector = mean * mean;
        let speed = if detector > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = (detector - self.envelope).mul_add(speed, self.envelope);
        if self.envelope <= self.detector_start {
            return;
        }
        let level = self.envelope.ln() * 0.5;
        let compressed = if level < self.knee_start + self.knee_width {
            let distance = level - self.knee_start;
            level + (1. / self.ratio - 1.) * distance * distance / (2. * self.knee_width)
        } else {
            (level - self.threshold) / self.ratio + self.threshold
        };
        let gain = (compressed - level).exp();
        for sample in samples {
            *sample *= gain;
        }
    }
}
