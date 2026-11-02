// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
//! Owned original Concat incoming-key CSS easing and normalized track semantics.
//! Source contract: pinned concat-core/src/animate.rs; independent oracle below.
use super::concat_effect_upgrade::ParamKey;
use anyhow::{ensure, Result};

fn axis(p1: f64, p2: f64, t: f64) -> f64 {
    let u = 1. - t;
    3. * u * u * t * p1 + 3. * u * t * t * p2 + t * t * t
}
fn slope(p1: f64, p2: f64, t: f64) -> f64 {
    let u = 1. - t;
    3. * u * u * p1 + 6. * u * t * (p2 - p1) + 3. * t * t * (1. - p2)
}
fn ease(points: [f64; 4], x: f64) -> f64 {
    let [x1, y1, x2, y2] = points;
    if (x1 - y1).abs() < 1e-6 && (x2 - y2).abs() < 1e-6 {
        return x.clamp(0., 1.);
    }
    let x = x.clamp(0., 1.);
    let mut t = x;
    for _ in 0..8 {
        let error = axis(x1, x2, t) - x;
        if error.abs() < 1e-7 {
            return axis(y1, y2, t);
        }
        let derivative = slope(x1, x2, t);
        if derivative.abs() < 1e-9 {
            break;
        }
        t -= error / derivative;
    }
    let (mut lo, mut hi) = (0., 1.);
    t = x;
    for _ in 0..32 {
        let at = axis(x1, x2, t);
        if (at - x).abs() < 1e-7 {
            break;
        }
        if at > x {
            hi = t
        } else {
            lo = t
        }
        t = (lo + hi) / 2.;
    }
    axis(y1, y2, t)
}
#[derive(Debug,Clone)]
pub struct PreparedTrack {
    keys: Vec<ParamKey>,
}
impl PreparedTrack {
    pub fn new(keys: &[ParamKey]) -> Result<Self> {
        ensure!(keys.len() <= 4096, "too many original Concat keys");
        let mut keys: Vec<_> = keys
            .iter()
            .filter(|k| k.at.is_finite() && k.value.is_finite())
            .cloned()
            .collect();
        for k in &mut keys {
            k.at = k.at.clamp(0., 1.);
            k.ease = if k.ease.iter().all(|v| v.is_finite()) {
                [
                    k.ease[0].clamp(0., 1.),
                    k.ease[1],
                    k.ease[2].clamp(0., 1.),
                    k.ease[3],
                ]
            } else {
                [0., 0., 1., 1.]
            };
        }
        keys.sort_by(|a, b| a.at.total_cmp(&b.at));
        Ok(Self { keys })
    }
    /// Original frame-evaluated source volume expression, including its
    /// twelve-chord CSS easing approximation and source decimal precision.
    pub fn audio_volume_filter(&self,duration:f64,rest:f64)->Result<Option<String>>{
        ensure!(duration.is_finite()&&rest.is_finite(),"nonfinite source volume clock/value");
        let linear=|e:[f64;4]|(e[0]-e[1]).abs()<1e-6&&(e[2]-e[3]).abs()<1e-6;
        let mut keys=self.keys.clone();
        if keys.len()>=2&&!keys.iter().all(|key|linear(key.ease)){
            let mut out=vec![keys[0].clone()];
            for pair in keys.windows(2){let(a,b)=(&pair[0],&pair[1]);
                if linear(b.ease)||b.at<=a.at{let mut key=b.clone();key.ease=[0.,0.,1.,1.];out.push(key);continue;}
                for step in 1..=12{let t=step as f64/12.;out.push(ParamKey{derived:true,at:a.at+(b.at-a.at)*t,value:a.value+(b.value-a.value)*ease(b.ease,t),ease:[0.,0.,1.,1.]});}
            }
            keys=out;
        }
        let Some(first)=keys.first()else{return Ok(if(rest-1.).abs()>f64::EPSILON{Some(format!("volume={:.4}",rest.max(0.)))}else{None});};
        let gain=|value:f64|format!("{:.6}",value.max(0.));
        let expr=if duration<=0.{gain(first.value)}else{
            let x=format!("(t/{duration:.6})");let mut expr=gain(keys.last().unwrap().value);
            for pair in keys.windows(2).rev(){let(a,b)=(&pair[0],&pair[1]);let span=b.at-a.at;
                let segment=if span<=0.{gain(b.value)}else{format!("({}+({:.6})*clip(({x}-{:.6})/{span:.6},0,1))",gain(a.value),b.value.max(0.)-a.value.max(0.),a.at)};
                expr=format!("if(lte({x},{:.6}),{segment},{expr})",b.at);
            }
            format!("if(lte({x},{:.6}),{},{expr})",first.at,gain(first.value))
        };
        Ok(Some(format!("volume=volume='{expr}':eval=frame")))
    }
    /// Document edit windows use KEY_EPSILON, unlike export audio subdivisions.
    pub fn rewindow(&self,from:f64,to:f64)->Result<Vec<ParamKey>>{
        let span=to-from;
        ensure!(from.is_finite()&&to.is_finite()&&span.is_finite()&&span>0.,"invalid source edit key window");
        let mut out=Vec::with_capacity(self.keys.len()+2);
        if self.keys.iter().any(|key|key.at<from-0.002){out.push(ParamKey{derived:true,at:0.,value:self.value_at(from,0.)?,ease:[0.,0.,1.,1.]});}
        let mut after=None;
        for key in &self.keys {
            if key.at<from-0.002{continue;}
            if key.at>to+0.002{if after.is_none(){after=Some(key.ease);}continue;}
            let mut key=key.clone();key.at=((key.at-from)/span).clamp(0.,1.);out.push(key);
        }
        if let Some(ease)=after{out.push(ParamKey{derived:true,at:1.,value:self.value_at(to,0.)?,ease});}
        out.sort_by(|a,b|a.at.total_cmp(&b.at));
        ensure!(out.len()<=4096&&out.iter().all(|key|key.value.is_finite()),"source edit key output exceeds budget or contains nonfinite values");
        Ok(out)
    }
    /// Original export track_slice semantics, including the closing key's ease.
    pub fn slice(&self, from:f64, to:f64)->Result<Vec<ParamKey>>{
        ensure!(from.is_finite()&&to.is_finite(),"nonfinite original Concat gain slice");
        if self.keys.is_empty(){return Ok(Vec::new());}
        let boundary=|at,value,ease|ParamKey{derived:true,at,value,ease};
        let mut keys=vec![boundary(0.,self.value_at(from,1.)?,[0.,0.,1.,1.])];
        let span=to-from;
        if span<=0.{return Ok(keys);}
        keys.extend(self.keys.iter().filter(|key|key.at>from&&key.at<to).cloned().map(|mut key|{key.at=(key.at-from)/span;key}));
        let ease=self.keys.iter().find(|key|key.at>=to).map_or([0.,0.,1.,1.],|key|key.ease);
        keys.push(boundary(1.,self.value_at(to,1.)?,ease));
        Ok(keys)
    }
    pub fn value_at(&self, x: f64, rest: f64) -> Result<f64> {
        ensure!(
            x.is_finite() && rest.is_finite(),
            "nonfinite original Concat animation time/value"
        );
        let Some(first) = self.keys.first() else {
            return Ok(rest);
        };
        if x <= first.at {
            return Ok(first.value);
        }
        for pair in self.keys.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if x <= b.at {
                if b.at <= a.at {
                    return Ok(b.value);
                }
                let t = ease(b.ease, (x - a.at) / (b.at - a.at));
                return Ok(a.value + (b.value - a.value) * t);
            }
        }
        Ok(self.keys.last().unwrap().value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires independent CONCAT_EFFECT_UPGRADE_ORACLE animationCases"]
    fn original_normalized_tracks_match_bits() {
        let path = std::env::var("CONCAT_EFFECT_UPGRADE_ORACLE")
            .expect("set CONCAT_EFFECT_UPGRADE_ORACLE");
        let oracle: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        let cases = oracle["animationCases"].as_array().unwrap();
        for (index, case) in cases.iter().enumerate() {
            let keys: Vec<ParamKey> = serde_json::from_value(case["keys"].clone()).unwrap();
            let track = PreparedTrack::new(&keys).unwrap();
            let actual = track
                .value_at(
                    case["progress"].as_f64().unwrap(),
                    case["rest"].as_f64().unwrap(),
                )
                .unwrap();
            assert_eq!(
                actual.to_bits(),
                case["expectedBits"].as_u64().unwrap(),
                "case{index}"
            );
        }
        eprintln!(
            "Original Concat normalized animation: {} exact f64 samples",
            cases.len()
        );
    }
}
