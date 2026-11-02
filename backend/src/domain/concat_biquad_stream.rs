//! Packet-aware typed direct-form-I filters using the existing owned coefficient/state core.
use super::concat_tone_filter::Shelf;
use anyhow::{ensure, Result};
pub(crate) trait Sample: Copy {
    const DOUBLE: bool;
    fn value(self) -> f64;
    fn output(value: f64) -> Self;
}
macro_rules! integer {
    ($ty:ty,$double:literal) => {
        impl Sample for $ty {
            const DOUBLE: bool = $double;
            fn value(self) -> f64 {
                f64::from(self)
            }
            fn output(value: f64) -> Self {
                value.clamp(Self::MIN as f64, Self::MAX as f64) as Self
            }
        }
    };
}
integer!(i16, false);
integer!(i32, true);
impl Sample for f32 {
    const DOUBLE: bool = false;
    fn value(self) -> f64 {
        f64::from(self)
    }
    fn output(v: f64) -> Self {
        v as Self
    }
}
impl Sample for f64 {
    const DOUBLE: bool = true;
    fn value(self) -> f64 {
        self
    }
    fn output(v: f64) -> Self {
        v
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Design {
    Lowpass,
    Highpass,
    Peak,
    LowShelf,
    HighShelf,
}
pub(crate) struct Filter<T: Sample> {
    core: Option<Shelf>,
    channels: usize,
    sample: std::marker::PhantomData<T>,
}
impl<T: Sample> Filter<T> {
    pub(crate) fn new(
        design: Design,
        rate: u32,
        channels: usize,
        frequency: f64,
        width: f64,
        gain: f64,
    ) -> Result<Self> {
        ensure!(
            (8000..=192000).contains(&rate) && (1..=16).contains(&channels),
            "invalid biquad audio shape"
        );
        ensure!(
            frequency.is_finite()
                && (0. ..=999999.).contains(&frequency)
                && width.is_finite()
                && (0. ..=99999.).contains(&width)
                && gain.is_finite()
                && (-900. ..=900.).contains(&gain),
            "invalid biquad coefficients"
        );
        // The original static initialization bypasses zero/out-of-Nyquist frequency or zero width.
        let core = if frequency == 0. || frequency > f64::from(rate) / 2. || width == 0. {
            None
        } else {
            match design {
                Design::Lowpass => Shelf::pass_with_width(rate, channels, frequency, width, false),
                Design::Highpass => Shelf::pass_with_width(rate, channels, frequency, width, true),
                Design::Peak => Shelf::peaking(rate, channels, frequency, width, gain),
                Design::LowShelf => Shelf::new(rate, channels, frequency, width, gain, false),
                Design::HighShelf => Shelf::new(rate, channels, frequency, width, gain, true),
            }
        };
        Ok(Self {
            core,
            channels,
            sample: std::marker::PhantomData,
        })
    }
    pub(crate) fn reset(&mut self) {
        if let Some(core) = &mut self.core {
            core.reset();
        }
    }
    pub(crate) fn push_packet(&mut self, input: &[&[T]]) -> Result<Vec<Vec<T>>> {
        ensure!(input.len() == self.channels, "biquad channel mismatch");
        let frames = input[0].len();
        ensure!(
            frames <= 1048576
                && input
                    .iter()
                    .all(|ch| ch.len() == frames && ch.iter().all(|v| v.value().is_finite())),
            "invalid biquad packet"
        );
        let mut output: Vec<Vec<T>> = input.iter().map(|ch| ch.to_vec()).collect();
        if let Some(core) = &mut self.core {
            for i in 0..frames {
                for ch in 0..self.channels {
                    let value = core.process(
                        ch,
                        input[ch][i].value(),
                        T::DOUBLE,
                        frames % 2 == 1 && i + 1 == frames,
                    );
                    output[ch][i] = T::output(value);
                }
            }
        }
        Ok(output)
    }
}
