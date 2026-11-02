//! Independently authored Planckian/Bradford white adaptation for Concat parameters.
use super::{composition::AnimatableValue, linear_color_frame::LinearColorFrame};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
type Matrix = [[f32; 3]; 3];
fn transform(m: Matrix, v: [f32; 3]) -> [f32; 3] {
    m.map(|r| r[0] * v[0] + r[1] * v[1] + r[2] * v[2])
}
const CONES: Matrix = [
    [0.4227252927, 0.4913453244, 0.0273579445],
    [0.0556997770, 0.9615340509, 0.0231838105],
    [0.0213826437, 0.0876418678, 0.9805081326],
];
const INVERSE: Matrix = [
    [2.5380445168, -1.2932769979, -0.0402368843],
    [-0.1460040576, 1.1166483055, -0.0223290262],
    [-0.0422985104, -0.0716072203, 1.0227526883],
];
// Keep each polynomial contraction explicit: finite-difference tint direction amplifies
// intermediate rounding, unlike a direct scalar colour gain.
fn polynomial([a, b, c, d]: [f32; 4], x: f32) -> f32 {
    a.mul_add(x, b).mul_add(x, c).mul_add(x, d)
}
fn locus(t: f32) -> [f32; 2] {
    let t = t.clamp(1667., 25000.);
    let k = 1000. / t;
    let x = if t <= 4000. {
        polynomial([-0.2661239, -0.2343589, 0.8776956, 0.179910], k)
    } else {
        polynomial([-3.0258469, 2.1070379, 0.2226347, 0.240390], k)
    };
    let y = if t <= 2222. {
        polynomial([-1.1063814, -1.34811020, 2.18555832, -0.20219683], x)
    } else if t <= 4000. {
        polynomial([-0.9549476, -1.37418593, 2.09137015, -0.16748867], x)
    } else {
        polynomial([3.0817580, -5.87338670, 3.75112997, -0.37001483], x)
    };
    let d = -2. * x + 12. * y + 3.;
    [4. * x / d, 6. * y / d]
}
fn white(t: f32, tint: f32) -> [f32; 3] {
    let uv = locus(t);
    let next = locus(t * 1.01);
    let dx = next[0] - uv[0];
    let dy = next[1] - uv[1];
    let length = (dx * dx + dy * dy).sqrt();
    let direction = if -dx > 0. { -1. } else { 1. };
    let u = uv[0] + direction * dy / length * tint * 0.0007;
    let v = uv[1] - direction * dx / length * tint * 0.0007;
    let d = 2. * u - 8. * v + 4.;
    let x = 3. * u / d;
    let y = 2. * v / d;
    transform(
        [
            [0.8951, 0.2664, -0.1614],
            [-0.7502, 1.7135, 0.0367],
            [0.0389, -0.0685, 1.0296],
        ],
        [x / y, 1., (1. - x - y) / y],
    )
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WhiteBalance {
    pub temperature: f32,
    pub tint: f32,
}
impl Default for WhiteBalance {
    fn default() -> Self {
        Self {
            temperature: 6500.,
            tint: 0.,
        }
    }
}
pub(crate) struct PreparedWhiteBalance {
    matrix: Matrix,
    luma: f32,
}
impl PreparedWhiteBalance {
    pub(crate) fn signal(&self, rgb: [f32; 3]) -> [f32; 3] {
        transform(self.matrix, rgb).map(|v| v / self.luma)
    }
}
impl WhiteBalance {
    pub(crate) fn prepared(&self) -> Result<PreparedWhiteBalance> {
        self.validate()?;
        let matrix = self.adaptation();
        let neutral = transform(matrix, [1.; 3]);
        let luma = neutral[0] * 0.2126 + neutral[1] * 0.7152 + neutral[2] * 0.0722;
        Ok(PreparedWhiteBalance { matrix, luma })
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.temperature.is_finite()
                && (2000. ..=11000.).contains(&self.temperature)
                && self.tint.is_finite()
                && (-100. ..=100.).contains(&self.tint),
            "Concat white balance out of range"
        );
        Ok(())
    }
    fn adaptation(&self) -> Matrix {
        let target = white(self.temperature, self.tint);
        let reference = white(6500., 0.);
        let gains = std::array::from_fn::<_, 3, _>(|i| target[i] / reference[i]);
        let columns: [[f32; 3]; 3] = std::array::from_fn(|i| {
            transform(INVERSE, std::array::from_fn(|j| CONES[j][i] * gains[j]))
        });
        std::array::from_fn(|r| std::array::from_fn(|c| columns[c][r]))
    }
    pub fn signal(&self, rgb: [f32; 3]) -> Result<[f32; 3]> {
        self.validate()?;
        let matrix = self.adaptation();
        let neutral = transform(matrix, [1.; 3]);
        let y = neutral[0] * 0.2126 + neutral[1] * 0.7152 + neutral[2] * 0.0722;
        Ok(transform(matrix, rgb).map(|v| v / y))
    }
    pub fn apply_sampled_linear(&self,frame:&mut LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        ensure!(self.temperature.is_finite()&&self.tint.is_finite(),"nonfinite Concat white balance source result");
        let matrix=self.adaptation();let neutral=transform(matrix,[1.;3]);let y=neutral[0]*0.2126+neutral[1]*0.7152+neutral[2]*0.0722;
        ensure!(matrix.iter().flatten().all(|v|v.is_finite())&&y.is_finite()&&y!=0.,"nonfinite Concat white adaptation");
        super::concat_working::apply_rgb(frame,|rgb|transform(matrix,rgb).map(|v|v/y),checkpoint)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        let matrix = self.adaptation();
        let neutral = transform(matrix, [1.; 3]);
        let y = neutral[0] * 0.2126 + neutral[1] * 0.7152 + neutral[2] * 0.0722;
        super::concat_working::apply_rgb(
            frame,
            |rgb| transform(matrix, rgb).map(|v| v / y),
            checkpoint,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Channel {
    Temperature,
    Tint,
}
pub type Animation = std::collections::BTreeMap<Channel, AnimatableValue>;
pub fn validate_animation(values: &Animation) -> Result<()> {
    for (channel, value) in values {
        value.validate()?;
        let (lo, hi) = match channel {
            Channel::Temperature => (2000., 11000.),
            Channel::Tint => (-100., 100.),
        };
        ensure!(
            super::composition::animatable_values_in(value, lo, hi, true),
            "Concat white balance animation out of range"
        );
    }
    Ok(())
}
pub fn sample(
    mut p: WhiteBalance,
    values: &Animation,
    time: impl Into<super::animation_sample_time::AnimationSampleTime>,
) -> Result<WhiteBalance> {
    p.validate()?;
    validate_animation(values)?;
    let time = time.into();
    for (channel, value) in values {
        let v = super::nested_composition::sample_value(value, time)? as f32;
        match channel {
            Channel::Temperature => p.temperature = v,
            Channel::Tint => p.tint = v,
        }
    }
    p.validate()?;
    Ok(p)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_ranges_and_luminance() {
        for temperature in [2000., 2222., 4000., 6500., 11000.] {
            for tint in [-100., 0., 100.] {
                let p = WhiteBalance { temperature, tint };
                p.validate().unwrap();
                let m = p.adaptation();
                let v = transform(m, [1.; 3]);
                let y = v[0] * 0.2126 + v[1] * 0.7152 + v[2] * 0.0722;
                assert!(y.is_finite() && y > 0.);
            }
        }
        let d = WhiteBalance::default();
        let v = transform(d.adaptation(), [0.2, 0.5, 2.]);
        for (a, b) in v.into_iter().zip([0.2, 0.5, 2.]) {
            assert!((a - b).abs() < 1e-5);
        }
        for p in [
            WhiteBalance {
                temperature: 1999.,
                tint: 0.,
            },
            WhiteBalance {
                temperature: 6500.,
                tint: 101.,
            },
            WhiteBalance {
                temperature: f32::NAN,
                tint: 0.,
            },
        ] {
            assert!(p.validate().is_err());
        }
    }
    #[test]
    fn independent_animation_and_wire() {
        let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"concat_white_balance","parameters":{},"animation":{"temperature":{"mode":"constant","value":4500},"tint":{"mode":"constant","value":25}}})).unwrap();
        effect.validate().unwrap();
        let a:Animation=serde_json::from_value(serde_json::json!({"temperature":{"mode":"constant","value":4500},"tint":{"mode":"constant","value":25}})).unwrap();
        let p = sample(WhiteBalance::default(), &a, 0).unwrap();
        assert_eq!(
            p,
            WhiteBalance {
                temperature: 4500.,
                tint: 25.
            }
        );
        let mut f =
            LinearColorFrame::from_working(1, 1, &[0.2, 0.3, 0.4, 0.5], &mut || Ok(())).unwrap();
        super::super::flat_surface::apply_effect_stack_working(
            &mut f,
            &[effect],
            0,
            &[],
            None,
            &mut || Ok(()),
        )
        .unwrap();
        assert_eq!(f.pixels()[0][3], 0.5);
        assert_ne!(f.pixels()[0][0], 0.2);
    }
}

pub type OriginalKeys=std::collections::BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>;
pub fn validate_keys(keys:&OriginalKeys)->Result<()>{
    let mut total=0usize;
    for(channel,run)in keys{let(lo,hi)=match channel.as_str(){"temperature"=>(2000.,11000.),"tint"=>(-100.,100.),_=>anyhow::bail!("unknown White Balance source channel")};total+=run.len();ensure!(total<=4096,"too many White Balance source keys");ensure!(run.iter().all(|k|k.at.is_finite()&&k.value.is_finite()&&(k.derived||(lo..=hi).contains(&k.value))&&k.ease.iter().all(|v|v.is_finite())),"invalid White Balance source key");}Ok(())
}
pub fn sample_keys(mut p:WhiteBalance,keys:&OriginalKeys,progress:f64)->Result<WhiteBalance>{
    p.validate()?;validate_keys(keys)?;
    for(channel,run)in keys{let target=match channel.as_str(){"temperature"=>&mut p.temperature,"tint"=>&mut p.tint,_=>unreachable!()};*target=super::concat_animation::PreparedTrack::new(run)?.value_at(progress,*target as f64)? as f32;ensure!(target.is_finite(),"nonfinite White Balance source value");}Ok(p)
}
