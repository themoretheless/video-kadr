//! Owned two-pole shelves, pass filters and peaking EQ with transport-selected precision.
use super::concat_audio_filter::Lowpass;
pub(crate) struct Shelf {
    float: Lowpass,
    b: [f64; 3],
    feedback: [f64; 2],
    state: Vec<[f64; 4]>,
}
impl Shelf {
    pub(crate) fn new(
        rate: u32,
        channels: usize,
        frequency: f64,
        width: f64,
        gain: f64,
        high: bool,
    ) -> Option<Self> {
        if frequency <= 0. || frequency > f64::from(rate) * 0.5 {
            return None;
        }
        let a = (std::f64::consts::LOG2_10 * (gain / 40.)).exp2();
        let angle = std::f64::consts::TAU * frequency / f64::from(rate);
        let cosine = angle.cos();
        let alpha = angle.sin() / (2. * width);
        let sum = a + 1.;
        let difference = a - 1.;
        let beta = (-difference).mul_add(difference, a.mul_add(a, 1.)).sqrt();
        let positive = difference.mul_add(cosine, sum);
        let negative = (-difference).mul_add(cosine, sum);
        let (normal, b, denominator) = if high {
            (
                beta.mul_add(alpha, negative),
                [
                    a * beta.mul_add(alpha, positive),
                    -2. * a * sum.mul_add(cosine, difference),
                    a * (-beta).mul_add(alpha, positive),
                ],
                [
                    2. * (-sum).mul_add(cosine, difference),
                    (-beta).mul_add(alpha, negative),
                ],
            )
        } else {
            (
                beta.mul_add(alpha, positive),
                [
                    a * beta.mul_add(alpha, negative),
                    2. * a * (-sum).mul_add(cosine, difference),
                    a * (-beta).mul_add(alpha, negative),
                ],
                [
                    -2. * sum.mul_add(cosine, difference),
                    (-beta).mul_add(alpha, positive),
                ],
            )
        };
        let b = b.map(|v| v / normal);
        let feedback = denominator.map(|v| -v / normal);
        Some(Self {
            float: Lowpass::normalized(b.map(|v| v as f32), feedback.map(|v| v as f32), channels),
            b,
            feedback,
            state: vec![[0.; 4]; channels],
        })
    }
    pub(crate) fn lowpass(rate: u32, channels: usize, frequency: f64) -> Option<Self> {
        Self::pass_with_width(rate, channels, frequency, 0.707, false)
    }
    pub(crate) fn highpass(rate: u32, channels: usize, frequency: f64) -> Option<Self> {
        Self::pass_with_width(rate, channels, frequency, 0.707, true)
    }
    pub(crate) fn pass_with_width(
        rate: u32,
        channels: usize,
        frequency: f64,
        width: f64,
        high: bool,
    ) -> Option<Self> {
        if frequency <= 0. || frequency > f64::from(rate) * 0.5 || width <= 0. {
            return None;
        }
        let angle = std::f64::consts::TAU * frequency / f64::from(rate);
        let cosine = angle.cos();
        let alpha = angle.sin() / (2. * width);
        let normal = 1. + alpha;
        let b = if high {
            [
                (1. + cosine) * 0.5 / normal,
                -(1. + cosine) / normal,
                (1. + cosine) * 0.5 / normal,
            ]
        } else {
            [
                (1. - cosine) * 0.5 / normal,
                (1. - cosine) / normal,
                (1. - cosine) * 0.5 / normal,
            ]
        };
        let feedback = [2. * cosine / normal, -(1. - alpha) / normal];
        Some(Self {
            float: Lowpass::normalized(b.map(|v| v as f32), feedback.map(|v| v as f32), channels),
            b,
            feedback,
            state: vec![[0.; 4]; channels],
        })
    }
    pub(crate) fn peaking(
        rate: u32,
        channels: usize,
        frequency: f64,
        width: f64,
        gain: f64,
    ) -> Option<Self> {
        if frequency <= 0. || frequency > f64::from(rate) * 0.5 {
            return None;
        }
        let a = (std::f64::consts::LOG2_10 * (gain / 40.)).exp2();
        let angle = std::f64::consts::TAU * frequency / f64::from(rate);
        let cosine = angle.cos();
        let alpha = angle.sin() / (2. * width);
        let normal = 1. + alpha / a;
        let b = [
            alpha.mul_add(a, 1.) / normal,
            -2. * cosine / normal,
            (-alpha).mul_add(a, 1.) / normal,
        ];
        let feedback = [2. * cosine / normal, -(1. - alpha / a) / normal];
        Some(Self {
            float: Lowpass::normalized(b.map(|v| v as f32), feedback.map(|v| v as f32), channels),
            b,
            feedback,
            state: vec![[0.; 4]; channels],
        })
    }
    pub(crate) fn reset(&mut self) {
        self.float.reset();
        self.state.fill([0.; 4]);
    }
    pub(crate) fn process(&mut self, ch: usize, input: f64, double: bool, odd: bool) -> f64 {
        if !double {
            return f64::from(self.float.process(ch, input as f32, odd));
        }
        let s = &mut self.state[ch];
        let (values, weights) = if odd {
            (
                [input, s[0], s[1], s[2], s[3]],
                [
                    self.b[0],
                    self.b[1],
                    self.b[2],
                    self.feedback[0],
                    self.feedback[1],
                ],
            )
        } else {
            (
                [s[1], s[0], input, s[3], s[2]],
                [
                    self.b[2],
                    self.b[1],
                    self.b[0],
                    self.feedback[1],
                    self.feedback[0],
                ],
            )
        };
        let mut output = weights[0].mul_add(values[0], weights[1] * values[1]);
        for i in 2..5 {
            output = weights[i].mul_add(values[i], output);
        }
        *s = [input, s[0], output, s[2]];
        output
    }
}
