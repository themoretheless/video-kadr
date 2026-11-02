//! Owned logarithmic sample quantizer with the original float rounding boundary.
pub(crate) struct LogCrusher {
    scale: f64,
    mix: f64,
}
impl LogCrusher {
    pub(crate) fn new(bits: f64, mix: f64) -> Self {
        assert!((4. ..=16.).contains(&bits) && (0. ..=1.).contains(&mix));
        Self {
            scale: ((bits.exp2() - 1.) / 2.).sqrt(),
            mix,
        }
    }
    pub(crate) fn process(&self, sample: f64) -> f64 {
        let input = self.mix.mul_add(sample, sample * (1. - self.mix));
        if input == 0. {
            return 0.;
        }
        let scale = self.scale;
        let position = scale.mul_add(input.abs().ln(), scale * scale);
        let rounded = (position as f32).round() as f64;
        let center = (rounded / scale - scale).exp();
        let distance = (position - rounded).abs();
        let magnitude = if distance <= 0.25 {
            center
        } else {
            let blend = 0.5
                * ((std::f64::consts::PI * (distance - 0.25) / 0.5 - std::f64::consts::FRAC_PI_2)
                    .sin()
                    + 1.);
            if position > rounded + 0.25 {
                let next = ((rounded + 1.) / scale - scale).exp();
                (next - center).mul_add(blend, center)
            } else {
                let previous = ((rounded - 1.) / scale - scale).exp();
                (-(center - previous)).mul_add(blend, center)
            }
        };
        let quantized = input.signum() * magnitude;
        (input - quantized).mul_add(self.mix, quantized)
    }
}
