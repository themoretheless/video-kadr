//! Filmcraft Lens Distortion CPU reference at source resolution.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all="camelCase", deny_unknown_fields)]
pub struct LensDistortion {
    pub curvature: f32,
    pub h_decentering: f32,
    pub v_decentering: f32,
}
impl LensDistortion {
    pub fn validate(&self) -> Result<()> {
        for value in [self.curvature,self.h_decentering,self.v_decentering] {
            ensure!(value.is_finite() && (-100.0..=100.0).contains(&value), "Lens Distortion parameter out of range");
        }
        Ok(())
    }
    pub fn apply_linear(&self, frame:&mut super::linear_color_frame::LinearColorFrame, checkpoint:&mut dyn FnMut()->Result<()>) -> Result<()> {
        self.validate()?;checkpoint()?;
        let (w,h)=frame.dimensions();let source=frame.pixels().to_vec();
        let k=self.curvature as f64/100.0;
        let cx=w as f64/2.0+self.h_decentering as f64/100.0*w as f64/2.0;
        let cy=h as f64/2.0+self.v_decentering as f64/100.0*h as f64/2.0;
        let norm=(w as f64/2.0).hypot(h as f64/2.0);
        let mut index=0;
        frame.map_premultiplied(&mut |_| {
            let x=(index%w)as f64+0.5;let y=(index/w)as f64+0.5;index+=1;
            let(dx,dy)=((x-cx)/norm,(y-cy)/norm);
            let r2=dx*dx+dy*dy;let scale=1.0-k*r2;
            super::twirl::sample(&source,w,h,(cx+dx*scale*norm)as f32,(cy+dy*scale*norm)as f32)
        },checkpoint)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        struct Case { parameters: LensDistortion, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-lens-distortion.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),64);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original LensDistortion frame {index}");
        }
    }

    #[test]
    fn persisted_three_curves_local_sampling_and_admission() {
        let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"lens_distortion","parameters":{},"animation":{"curvature":curve(-100.0,100.0),"hDecentering":curve(-50.0,50.0),"vDecentering":curve(0.0,100.0)}})).unwrap();
        let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
        let crate::domain::composition::VideoEffect::LensDistortion{parameters,animation:Some(curves)}=effect else {panic!("Lens Distortion")};
        assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),LensDistortion{curvature:0.0,h_decentering:0.0,v_decentering:50.0});
        let invalid:LensDistortionAnimation=serde_json::from_value(serde_json::json!({"curvature":curve(0.0,100.1)})).unwrap();assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn zero_curvature_identity_transparent_edges_and_cancellation() {
        let source=GroupSurface{width:6,height:4,rgba:(0..24).flat_map(|i|[i*10,255-i*8,i*7,128]).collect()};
        let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
        let mut identity=original.clone();
        LensDistortion{h_decentering:100.0,v_decentering:-100.0,..Default::default()}.apply_linear(&mut identity,&mut||Ok(())).unwrap();
        assert_eq!(identity.pixels(),original.pixels());
        let mut distorted=original.clone();
        LensDistortion{curvature:-100.0,..Default::default()}.apply_linear(&mut distorted,&mut||Ok(())).unwrap();
        assert!(distorted.pixels().iter().any(|p|p[3]<original.pixels()[0][3]));
        assert!(LensDistortion::default().apply_linear(&mut distorted,&mut||anyhow::bail!("cancelled")).is_err());
        assert!(LensDistortion{curvature:101.0,..Default::default()}.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum LensDistortionChannel {Curvature,HDecentering,VDecentering}
pub type LensDistortionAnimation=std::collections::BTreeMap<LensDistortionChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&LensDistortionAnimation)->Result<()> {
 for curve in animation.values() {curve.validate()?;ensure!(super::composition::animatable_values_in(curve,-100.0,100.0,true),"Lens Distortion animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:LensDistortion,animation:&LensDistortionAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<LensDistortion> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {LensDistortionChannel::Curvature=>parameters.curvature=value,LensDistortionChannel::HDecentering=>parameters.h_decentering=value,LensDistortionChannel::VDecentering=>parameters.v_decentering=value}}
 parameters.validate()?;Ok(parameters)
}
