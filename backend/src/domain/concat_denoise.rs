//! Owned fixed white-noise spectral reduction used by Concat Denoise/Enhance Voice.
//! FFT precision follows the input; the perceptual masking model uses double precision.
use std::f64::consts::{LN_10, PI};
// Conjugate-pair split-radix real transform, specialized at the source precision.
macro_rules! real_transform {
    ($name:ident,$scalar:ty) => {
        struct $name {
            packed: Vec<($scalar, $scalar)>,
            work: Vec<($scalar, $scalar)>,
        }
        impl $name {
            fn new(n: usize) -> Self {
                Self {
                    packed: vec![(0., 0.); n / 2],
                    work: vec![(0., 0.); n / 2],
                }
            }
            fn complex(
                input: &[($scalar, $scalar)],
                output: &mut [($scalar, $scalar)],
                start: isize,
                step: isize,
                inverse: bool,
            ) {
                let n = output.len();
                let period = input.len() as isize;
                if n == 1 {
                    let index = if inverse { -start } else { start };
                    output[0] = input[index.rem_euclid(period) as usize];
                    return;
                }
                if std::mem::size_of::<$scalar>() == 4 && n == 8 {
                    let x = |k: isize| {
                        let index = start + k * step;
                        let index = if inverse { -index } else { index };
                        input[index.rem_euclid(period) as usize]
                    };
                    let add = |a: ($scalar, $scalar), b: ($scalar, $scalar)| (a.0 + b.0, a.1 + b.1);
                    let sub = |a: ($scalar, $scalar), b: ($scalar, $scalar)| (a.0 - b.0, a.1 - b.1);
                    let s0 = add(x(0), x(4));
                    let s2 = add(x(2), x(6));
                    let s1 = add(x(1), x(5));
                    let s3 = add(x(7), x(3));
                    let a = add(s0, s2);
                    let b = add(s1, s3);
                    let e = sub(s0, s2);
                    let f = sub(s1, s3);
                    output[0] = add(a, b);
                    output[4] = sub(a, b);
                    output[2] = (e.0 + f.1, e.1 - f.0);
                    output[6] = (e.0 - f.1, e.1 + f.0);
                    let r0 = sub(x(0), x(4));
                    let r2 = sub(x(2), x(6));
                    let j1 = sub(x(1), x(5));
                    let j7 = sub(x(7), x(3));
                    let c = std::f64::consts::FRAC_1_SQRT_2 as $scalar;
                    let u = [j1.0 * c, j1.1 * (-c), j7.0 * (-c), j7.1 * c];
                    let l = [u[0] - u[1], u[1] + u[0], u[2] - u[3], u[3] + u[2]];
                    let t = [l[3] - l[0], l[2] + l[1], l[1] - l[2], l[0] + l[3]];
                    let a = (r0.0 + r2.1, r0.1 - r2.0);
                    let b = (r0.0 - r2.1, r0.1 + r2.0);
                    output[1] = (a.0 - t[0], a.1 - t[1]);
                    output[5] = (a.0 + t[0], a.1 + t[1]);
                    output[3] = (b.0 - t[2], b.1 - t[3]);
                    output[7] = (b.0 + t[2], b.1 + t[3]);
                    return;
                }
                let (even, odd) = output.split_at_mut(n / 2);
                Self::complex(input, even, start, step * 2, inverse);
                if n == 2 {
                    let index = if inverse {
                        -(start + step)
                    } else {
                        start + step
                    };
                    let b = input[index.rem_euclid(period) as usize];
                    let a = output[0];
                    output[0] = (a.0 + b.0, a.1 + b.1);
                    output[1] = (a.0 - b.0, a.1 - b.1);
                    return;
                }
                let (one, minus) = odd.split_at_mut(n / 4);
                Self::complex(input, one, start + step, step * 4, inverse);
                Self::complex(input, minus, start - step, step * 4, inverse);
                for k in 0..n / 4 {
                    let a = output[n / 2 + k];
                    let b = output[3 * n / 4 + k];
                    let (a, b) = if k == 0 {
                        (a, b)
                    } else if std::mem::size_of::<$scalar>() == 4 && n == 16 && k == 2 {
                        let c = std::f64::consts::FRAC_1_SQRT_2 as $scalar;
                        (
                            ((a.0 + a.1) * c, (a.1 - a.0) * c),
                            ((b.0 - b.1) * c, (b.1 + b.0) * c),
                        )
                    } else {
                        let angle = 2. * PI / n as f64;
                        let c = (k as f64 * angle).cos() as $scalar;
                        let si = ((n / 4 - k) as f64 * angle).cos() as $scalar;
                        if std::mem::size_of::<$scalar>() == 4 && n == 64 && k < 8 {
                            let a = (a.0.mul_add(c, a.1 * si), a.1.mul_add(c, -a.0 * si));
                            let b = if k % 2 == 0 {
                                (b.0 * c - b.1 * si, b.1 * c + b.0 * si)
                            } else {
                                ((-b.1).mul_add(si, b.0 * c), b.0.mul_add(si, b.1 * c))
                            };
                            (a, b)
                        } else if std::mem::size_of::<$scalar>() == 4 {
                            // The float split-radix kernel rounds the cosine product
                            // before accumulating the sine product.
                            (
                                (a.1.mul_add(si, a.0 * c), a.0.mul_add(-si, a.1 * c)),
                                ((-b.1).mul_add(si, b.0 * c), b.0.mul_add(si, b.1 * c)),
                            )
                        } else {
                            (
                                (a.0.mul_add(c, a.1 * si), a.0.mul_add(-si, a.1 * c)),
                                (b.0.mul_add(c, -b.1 * si), b.0.mul_add(si, b.1 * c)),
                            )
                        }
                    };
                    let e = output[k];
                    let f = output[n / 4 + k];
                    let re_sum = b.0 + a.0;
                    let re_diff = b.0 - a.0;
                    let im_sum = a.1 + b.1;
                    let im_diff = a.1 - b.1;
                    output[k] = (e.0 + re_sum, e.1 + im_sum);
                    output[n / 2 + k] = (e.0 - re_sum, e.1 - im_sum);
                    output[n / 4 + k] = (f.0 + im_diff, f.1 + re_diff);
                    output[3 * n / 4 + k] = (f.0 - im_diff, f.1 - re_diff);
                }
            }
            fn run(&mut self, data: &mut [(f64, f64)], inverse: bool) {
                let n = data.len();
                let m = n / 2;
                let quarter = n / 4;
                if !inverse {
                    for k in 0..m {
                        self.packed[k] = (data[2 * k].0 as $scalar, data[2 * k + 1].0 as $scalar);
                    }
                    Self::complex(&self.packed, &mut self.work, 0, 1, false);
                } else {
                    for k in 0..m {
                        self.work[k] = (data[k].0 as $scalar, data[k].1 as $scalar);
                    }
                    self.work[0].1 = data[m].0 as $scalar;
                }
                let first = self.work[0];
                self.work[0] = (first.0 + first.1, first.0 - first.1);
                let factor = if inverse { 2. } else { 1. };
                self.work[quarter].0 *= factor;
                self.work[quarter].1 *= -factor;
                for k in 1..quarter {
                    let a = self.work[k];
                    let b = self.work[m - k];
                    let scale = if inverse { 1. } else { 0.5 };
                    let er = scale * (a.0 + b.0);
                    let ei = -scale * (a.1 - b.1);
                    let direction = if inverse { -1. } else { 1. };
                    let or = direction * scale * (a.1 + b.1);
                    let oi = -direction * scale * (a.0 - b.0);
                    let angle = 2. * PI / n as f64;
                    let c = (k as f64 * angle).cos() as $scalar;
                    let si = ((n as f64 - k as f64 * 4.) / 4. * angle).cos() as $scalar
                        * if inverse { 1. } else { -1. };
                    let wr = or.mul_add(c, -oi * si);
                    let wi = or.mul_add(si, oi * c);
                    self.work[k] = (er + wr, wi - ei);
                    self.work[m - k] = (er - wr, wi + ei);
                }
                if inverse {
                    Self::complex(&self.work, &mut self.packed, 0, 1, true);
                    for k in 0..m {
                        data[2 * k] = (f64::from(self.packed[k].0), 0.);
                        data[2 * k + 1] = (f64::from(self.packed[k].1), 0.);
                    }
                } else {
                    for k in 0..m {
                        data[k] = (f64::from(self.work[k].0), f64::from(self.work[k].1));
                    }
                    data[m] = (data[0].1, 0.);
                    data[0].1 = 0.;
                }
            }
        }
    };
}
real_transform!(RealTransform, f64);
real_transform!(FloatRealTransform, f32);
struct Channel {
    window: Vec<f64>,
    overlap: Vec<f64>,
    prior: Vec<f64>,
    excitation: Vec<f64>,
}
pub(crate) struct Denoiser {
    float: bool,
    hop: usize,
    window: Vec<f64>,
    real: RealTransform,
    float_real: FloatRealTransform,
    spectrum: Vec<(f64, f64)>,
    bins: Vec<usize>,
    alpha: Vec<f64>,
    beta: Vec<f64>,
    spread: Vec<f64>,
    band_energy: Vec<f64>,
    masking: Vec<f64>,
    gain: Vec<f64>,
    variance: f64,
    minimum_variance: f64,
    maximum_gain: f64,
    first: bool,
    initial_excitation: Vec<f64>,
    channels: Vec<Channel>,
}
impl Denoiser {
    pub(crate) fn new(rate: u32, channels: usize, noise_floor: f64, reduction: f64) -> Self {
        Self::with_precision(rate, channels, noise_floor, reduction, false)
    }
    pub(crate) fn with_precision(
        rate: u32,
        channels: usize,
        noise_floor: f64,
        reduction: f64,
        float: bool,
    ) -> Self {
        let hop = rate as usize / 80;
        let length = hop * 3;
        // Strictly greater than the analysis window, including power-of-two boundaries.
        let n = 1usize << (usize::BITS - length.leading_zeros());
        let scale = (8. / (9. * n as f64)).sqrt();
        let window: Vec<f64> = (0..length)
            .map(|i| {
                let s = (i as f64 * PI / length as f64).sin();
                s * (scale * s)
            })
            .collect();
        let weight = 0.5 * window.iter().map(|w| w * w).sum::<f64>();
        let floor = (1u64 << 48) as f64 * (-23.025558369790467f64).exp() * weight;
        let variance =
            (floor * ((100. + f64::from(noise_floor as f32)) * (LN_10 * 0.1)).exp()).max(1.);
        let maximum_gain = (f64::from(reduction as f32) * (0.5 * LN_10 * 0.1)).exp();
        let bins: Vec<usize> = (0..=n / 2)
            .map(|i| {
                // The leading 0.5 promotes source frequency arithmetic to double.
                let hz = 0.5 * i as f64 * f64::from(rate) / n as f64;
                let bark = 13. * (7.6e-4 * hz).atan() + 3.5 * (hz / 7500.).powi(2).atan();
                (1.25 * bark).round_ties_even() as usize
            })
            .collect();
        let bands = bins[bins.len() - 1] + 1;
        let mut counts = vec![0.; bands];
        for &band in &bins {
            counts[band] += 1.;
        }
        let mut spread = vec![0.; bands * bands];
        let mut initial_excitation = vec![0.; bands];
        let down = 0.1f64.powf(1. / 1.25);
        let up = 0.1f64.powf(2.5 / 1.25);
        for row in 0..bands {
            let mut normal = 0.;
            for col in 0..bands {
                let value = if col < row {
                    down.powi((row - col) as i32)
                } else {
                    up.powi((col - row) as i32)
                };
                spread[row * bands + col] = value;
                normal = value.mul_add(counts[col], normal);
            }
            initial_excitation[row] = normal;
            let power = if row < 15 {
                1.45 + 0.1 * row as f64 / 1.25
            } else {
                2.5 - 0.2 * (row as f64 / 1.25 - 14.)
            };
            let strength = 0.1f64.powf(power).clamp(0.1f64.powf(2.5), 0.1);
            for col in 0..bands {
                spread[row * bands + col] *= strength / normal;
            }
        }
        let mut alpha = vec![0.; bands];
        let mut beta = vec![0.; bands];
        let mut previous = 0;
        for (i, &band) in bins.iter().enumerate() {
            if band > previous {
                let frequency = f64::from(((i - 1) as f32 * rate as f32) / n as f32);
                let time = (0.008 + 2.2 / frequency).min(0.03);
                let a = (-f64::from(hop as f32 / rate as f32) / time).exp();
                alpha[previous] = a;
                beta[previous] = 1. - a;
                previous = band;
            }
        }
        Self {
            float,
            hop,
            window,
            real: RealTransform::new(n),
            float_real: FloatRealTransform::new(n),
            spectrum: vec![(0., 0.); n],
            bins,
            alpha,
            beta,
            spread,
            band_energy: vec![0.; bands],
            masking: vec![0.; bands],
            gain: vec![0.; n / 2 + 1],
            variance,
            minimum_variance: variance / (maximum_gain * maximum_gain),
            maximum_gain,
            first: true,
            initial_excitation: initial_excitation.clone(),
            channels: (0..channels)
                .map(|_| Channel {
                    window: vec![0.; length],
                    overlap: vec![0.; length],
                    prior: vec![0.; n / 2 + 1],
                    excitation: initial_excitation.clone(),
                })
                .collect(),
        }
    }
    pub(crate) fn hop(&self) -> usize {
        self.hop
    }
    pub(crate) fn reset(&mut self) {
        self.first = true;
        for ch in &mut self.channels {
            ch.window.fill(0.);
            ch.overlap.fill(0.);
            ch.prior.fill(0.);
            ch.excitation.copy_from_slice(&self.initial_excitation);
        }
    }
    /// One source analysis hop, including a final short hop. No synthetic EOF tail.
    pub(crate) fn process_hop(&mut self, input: &[Vec<f64>]) -> Vec<Vec<f64>> {
        let count = input[0].len();
        assert!(count > 0 && count <= self.hop);
        assert_eq!(input.len(), self.channels.len());
        let bands = self.alpha.len();
        let n = self.spectrum.len();
        let length = self.window.len();
        let mut result = Vec::with_capacity(input.len());
        for (source, ch) in input.iter().zip(&mut self.channels) {
            assert_eq!(source.len(), count);
            ch.window.copy_within(self.hop.., 0);
            ch.window[length - self.hop..].fill(0.);
            ch.window[length - self.hop..length - self.hop + count].copy_from_slice(source);
            if self.float {
                for sample in &mut ch.window[length - self.hop..] {
                    *sample = f64::from(*sample as f32);
                }
            }
            self.spectrum.fill((0., 0.));
            for i in 0..length {
                let value = self.window[i] * ch.window[i] * 8388608.;
                self.spectrum[i].0 = if self.float {
                    f64::from(value as f32)
                } else {
                    value
                };
            }
            if self.float {
                self.float_real.run(&mut self.spectrum, false);
            } else {
                self.real.run(&mut self.spectrum, false);
            }
            self.band_energy.fill(0.);
            for i in 0..self.bins.len() {
                let mag = self.spectrum[i].0.hypot(self.spectrum[i].1);
                let power = mag * mag;
                let snr = power / self.variance;
                let estimate = if self.first {
                    ch.prior[i]
                } else {
                    0.5f64.mul_add(ch.prior[i], 0.5 * (snr - 1.).max(0.))
                };
                let gain = estimate / (1. + estimate);
                let squared = gain * gain;
                ch.prior[i] = snr * squared;
                self.gain[i] = gain;
                self.band_energy[self.bins[i]] += power * squared;
            }
            for i in 0..bands {
                self.band_energy[i] = self.band_energy[i].max(
                    self.alpha[i].mul_add(self.band_energy[i], self.beta[i] * ch.excitation[i]),
                );
                ch.excitation[i] = self.band_energy[i];
            }
            for row in 0..bands {
                let mut value = 0.;
                for col in 0..bands {
                    value = self.spread[row * bands + col].mul_add(self.band_energy[col], value);
                }
                self.masking[row] = value;
            }
            for i in 0..self.bins.len() {
                let mask = self.masking[self.bins[i]];
                let gain = if mask > self.variance {
                    1.
                } else {
                    let limit = if mask > self.minimum_variance {
                        (self.variance / mask).sqrt()
                    } else {
                        self.maximum_gain
                    };
                    let g = self.gain[i];
                    if g > 1. {
                        (limit * g - 1.) / (limit + g - 2.)
                    } else if g < 1. {
                        ((limit - 2.) * g + 1.) / (limit - g)
                    } else {
                        1.
                    }
                };
                if self.float {
                    let gain = gain as f32;
                    self.spectrum[i].0 = f64::from(self.spectrum[i].0 as f32 * gain);
                    self.spectrum[i].1 = f64::from(self.spectrum[i].1 as f32 * gain);
                } else {
                    self.spectrum[i].0 *= gain;
                    self.spectrum[i].1 *= gain;
                }
                if i > 0 && i < n / 2 {
                    self.spectrum[n - i] = (self.spectrum[i].0, -self.spectrum[i].1);
                }
            }
            if self.float {
                self.float_real.run(&mut self.spectrum, true);
            } else {
                self.real.run(&mut self.spectrum, true);
            }
            for i in 0..length {
                ch.overlap[i] += self.window[i] * self.spectrum[i].0 / 8388608.;
            }
            result.push(
                ch.overlap[..count]
                    .iter()
                    .map(|&v| if self.float { f64::from(v as f32) } else { v })
                    .collect(),
            );
            ch.overlap.copy_within(self.hop.., 0);
            ch.overlap[length - self.hop..].fill(0.);
        }
        self.first = false;
        result
    }
}

/// Shared real float transform for time-domain fragment correlation.
pub(crate) struct FloatFft {
    plan: FloatRealTransform,
    data: Vec<(f64, f64)>,
}
impl FloatFft {
    pub(crate) fn new(size: usize) -> Self {
        assert!(size >= 8 && size.is_power_of_two());
        Self {
            plan: FloatRealTransform::new(size),
            data: vec![(0., 0.); size],
        }
    }
    pub(crate) fn forward(&mut self, input: &[f32], output: &mut [(f32, f32)]) {
        assert!(input.len() <= self.data.len());
        assert_eq!(output.len(), self.data.len() / 2 + 1);
        self.data.fill((0., 0.));
        for (dest, &value) in self.data.iter_mut().zip(input) {
            dest.0 = f64::from(value);
        }
        self.plan.run(&mut self.data, false);
        for (dest, &(re, im)) in output.iter_mut().zip(&self.data) {
            *dest = (re as f32, im as f32);
        }
    }
    pub(crate) fn inverse(&mut self, input: &[(f32, f32)], output: &mut [f32]) {
        assert_eq!(input.len(), self.data.len() / 2 + 1);
        assert_eq!(output.len(), self.data.len());
        self.data.fill((0., 0.));
        for (dest, &(re, im)) in self.data.iter_mut().zip(input) {
            *dest = (f64::from(re), f64::from(im));
        }
        self.plan.run(&mut self.data, true);
        for (dest, &(re, _)) in output.iter_mut().zip(&self.data) {
            *dest = re as f32;
        }
    }
}
