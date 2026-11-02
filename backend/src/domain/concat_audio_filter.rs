//! Independently authored float32 direct-form-I filter with explicit input-frame boundaries.
#[derive(Clone, Copy)]
struct Coefficients {
    b: [f32; 3],
    feedback: [f32; 2],
}
impl Coefficients {
    fn lowpass(rate: u32, frequency: f64) -> Option<Self> {
        if frequency <= 0. || frequency > f64::from(rate) * 0.5 {
            return None;
        }
        let angle = std::f64::consts::TAU * frequency / f64::from(rate);
        let cosine = angle.cos();
        let alpha = angle.sin() / (2. * 0.707);
        let normal = 1. + alpha;
        Some(Self {
            b: [
                ((1. - cosine) * 0.5 / normal) as f32,
                ((1. - cosine) / normal) as f32,
                ((1. - cosine) * 0.5 / normal) as f32,
            ],
            feedback: [
                (2. * cosine / normal) as f32,
                (-(1. - alpha) / normal) as f32,
            ],
        })
    }
}
pub(crate) struct Lowpass {
    coefficients: Coefficients,
    state: Vec<[f32; 4]>,
}
impl Lowpass {
    pub(crate) fn normalized(b: [f32; 3], feedback: [f32; 2], channels: usize) -> Self {
        Self {
            coefficients: Coefficients { b, feedback },
            state: vec![[0.; 4]; channels],
        }
    }
    pub(crate) fn new(rate: u32, channels: usize, frequency: f64) -> Option<Self> {
        Some(Self {
            coefficients: Coefficients::lowpass(rate, frequency)?,
            state: vec![[0.; 4]; channels],
        })
    }
    pub(crate) fn reset(&mut self) {
        self.state.fill([0.; 4]);
    }
    pub(crate) fn process(&mut self, channel: usize, x: f32, odd_frame_end: bool) -> f32 {
        let s = &mut self.state[channel];
        let c = self.coefficients;
        // Fixed operation order and fused arithmetic reproduce the pinned source
        // transport. An odd frame's last sample has a distinct accumulation order.
        let (values, weights) = if odd_frame_end {
            (
                [x, s[0], s[1], s[2], s[3]],
                [c.b[0], c.b[1], c.b[2], c.feedback[0], c.feedback[1]],
            )
        } else {
            (
                [s[1], s[0], x, s[3], s[2]],
                [c.b[2], c.b[1], c.b[0], c.feedback[1], c.feedback[0]],
            )
        };
        let mut y = weights[0].mul_add(values[0], weights[1] * values[1]);
        for i in 2..5 {
            y = weights[i].mul_add(values[i], y);
        }
        *s = [x, s[0], y, s[2]];
        y
    }
}
