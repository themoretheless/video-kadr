//! Filmcraft Wave Warp CPU reference at source-resolution pixel scale.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WaveWarp {
    pub height: f32,
    pub width: f32,
    pub direction: f32,
    pub speed: f32,
}
impl Default for WaveWarp {
    fn default() -> Self {
        Self { height: 10.0, width: 40.0, direction: 90.0, speed: 1.0 }
    }
}
impl WaveWarp {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.height.is_finite() && (-4000.0..=4000.0).contains(&self.height), "Wave Warp height out of range");
        ensure!(self.width.is_finite() && (1.0..=4000.0).contains(&self.width), "Wave Warp width out of range");
        ensure!(self.direction.is_finite(), "Wave Warp direction out of range");
        ensure!(self.speed.is_finite() && (-100.0..=100.0).contains(&self.speed), "Wave Warp speed out of range");
        Ok(())
    }
    pub fn apply_linear(&self, frame: &mut super::linear_color_frame::LinearColorFrame, local_tick: u64, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {
 self.apply_linear_at_seconds(frame,local_tick as f64/1_000_000.0,checkpoint)
 }
 pub fn apply_linear_at_seconds(&self, frame: &mut super::linear_color_frame::LinearColorFrame, seconds:f64, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<()> {self.apply_linear_at_seconds_scaled(frame,seconds,1.,checkpoint)
 }
 pub(crate) fn apply_linear_at_seconds_scaled(&self,frame:&mut super::linear_color_frame::LinearColorFrame,seconds:f64,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 ensure!(px_scale.is_finite()&&px_scale>0.,"invalid Wave Warp pixel scale");
 ensure!(seconds.is_finite() && seconds.abs() <= i64::MAX as f64 / 254016000000.0,"invalid signed generator time");
        self.validate()?;
        checkpoint()?;
        let (w,h) = frame.dimensions();
        let source = frame.pixels().to_vec();
        let height = self.height as f64*px_scale as f64;
        let width = (self.width as f64*px_scale as f64).max(1.0);
        let direction = (self.direction as f64).to_radians();
        let phase = seconds * self.speed as f64 * std::f64::consts::TAU;
        let (ux,uy) = (direction.sin(), -direction.cos());
        let mut index = 0;
        frame.map_premultiplied(&mut |_| {
            let x = (index % w) as f64 + 0.5;
            let y = (index / w) as f64 + 0.5;
            index += 1;
            let along = x * ux + y * uy;
            let off = height * (along / width * std::f64::consts::TAU - phase).sin();
            super::twirl::sample(&source, w, h, (x - uy * off) as f32, (y + ux * off) as f32)
        }, checkpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{group_surface::GroupSurface, linear_color_frame::LinearColorFrame};
    #[test]
    fn original_working_frame_rgba_bits() {
        #[derive(Deserialize)]
        #[serde(rename_all="camelCase")]
        struct Case { local_tick:u64, parameters: WaveWarp, width: usize, height: usize, rgba: Vec<u8>, bits: Vec<u32> }
        #[derive(Deserialize)]
        struct Oracle { commit: String, cases: Vec<Case> }
        let oracle: Oracle = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wave-warp.json")).unwrap();
        assert_eq!(oracle.commit, "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(oracle.cases.len(),144);
        for (index, case) in oracle.cases.into_iter().enumerate() {
            let source = GroupSurface {width:case.width,height:case.height,rgba:case.rgba};
            let mut frame = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
            case.parameters.apply_linear(&mut frame, case.local_tick, &mut || Ok(())).unwrap();
            let bits: Vec<u32> = frame.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,case.bits,"original WaveWarp frame {index}");
        }
    }

    #[test]
    fn persisted_four_curves_local_sampling_and_admission() {
        let curve=|a:f64,b:f64|serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
        let effect:crate::domain::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"wave_warp","parameters":{},"animation":{"height":curve(-10.0,10.0),"width":curve(10.0,40.0),"direction":curve(-45.0,45.0),"speed":curve(-1.0,1.0)}})).unwrap();
        let restored=serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap();assert_eq!(effect,restored);
        let crate::domain::composition::VideoEffect::WaveWarp{parameters,animation:Some(curves)}=effect else {panic!("Wave Warp")};
        assert_eq!(sample_animation(parameters,&curves,250000).unwrap(),WaveWarp{height:0.0,width:25.0,direction:0.0,speed:0.0});
        let invalid:WaveWarpAnimation=serde_json::from_value(serde_json::json!({"width":curve(0.0,10.0)})).unwrap();assert!(validate_animation(&invalid).is_err());
    }
    #[test]
    fn zero_height_identity_local_phase_and_cancellation() {
        let source = GroupSurface { width: 6, height: 4, rgba: (0..24).flat_map(|i| [i*10, 255-i*8, i*7, 128]).collect() };
        let original = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        let mut identity = original.clone();
        WaveWarp {height:0.0,..Default::default()}.apply_linear(&mut identity,250000,&mut || Ok(())).unwrap();
        assert_eq!(identity.pixels(),original.pixels());
        let parameters = WaveWarp {height:1.0,width:4.0,direction:90.0,speed:1.0};
        let mut start = original.clone();let mut later = original.clone();
        parameters.apply_linear(&mut start,0,&mut || Ok(())).unwrap();
        parameters.apply_linear(&mut later,250000,&mut || Ok(())).unwrap();
        assert_ne!(start.pixels(),later.pixels());
        assert!(parameters.apply_linear(&mut later,0,&mut || anyhow::bail!("cancelled")).is_err());
        assert!(WaveWarp {width:0.0,..parameters}.validate().is_err());
    }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum WaveWarpChannel {Height,Width,Direction,Speed}
pub type WaveWarpAnimation=std::collections::BTreeMap<WaveWarpChannel,super::composition::AnimatableValue>;
pub fn validate_animation(animation:&WaveWarpAnimation)->Result<()> {
 for(channel,curve)in animation {curve.validate()?;let(min,max)=match channel {WaveWarpChannel::Height=>(-4000.0,4000.0),WaveWarpChannel::Width=>(1.0,4000.0),WaveWarpChannel::Direction=>(-(f32::MAX as f64),f32::MAX as f64),WaveWarpChannel::Speed=>(-100.0,100.0)};ensure!(super::composition::animatable_values_in(curve,min,max,true),"Wave Warp animation out of range");}Ok(())
}
pub fn sample_animation(mut parameters:WaveWarp,animation:&WaveWarpAnimation,time: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<WaveWarp> {
    let time: super::animation_sample_time::AnimationSampleTime = time.into();

 parameters.validate()?;validate_animation(animation)?;
 for(channel,curve)in animation {let value=super::nested_composition::sample_value(curve,time)? as f32;match channel {WaveWarpChannel::Height=>parameters.height=value,WaveWarpChannel::Width=>parameters.width=value,WaveWarpChannel::Direction=>parameters.direction=value,WaveWarpChannel::Speed=>parameters.speed=value}}
 parameters.validate()?;Ok(parameters)
}
