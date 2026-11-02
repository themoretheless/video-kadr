//! Filmcraft export/pipeline.rs SDR video limiter (MIT, commit
//! 5231852443363f001c3f6b396dd9b1e6461ae2be).
use super::group_surface::GroupSurface;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default, rename_all="camelCase", deny_unknown_fields)]
pub struct VideoLimiter {
    pub enabled: bool,
    pub min_percent: f32,
    pub max_percent: f32,
}
impl Default for VideoLimiter {
    fn default()->Self{Self{enabled:false,min_percent:0.,max_percent:100.}}
}
impl VideoLimiter {
    pub fn validate(self)->Result<()>{
        ensure!(self.min_percent.is_finite()&&self.max_percent.is_finite(),"nonfinite video limiter threshold");
        Ok(())
    }
    /// Display-encoded BT.709 luma; alpha stays untouched. The source independently
    /// clamps both thresholds and clips channels after lowering/lifting luma.
    pub fn apply(self,surface:&mut GroupSurface,mut checkpoint:impl FnMut()->Result<()>)->Result<()> {
        self.validate()?;surface.validate()?;
        if !self.enabled{return Ok(())}
        let lo=self.min_percent.clamp(0.,100.)/100.;
        let hi=self.max_percent.clamp(0.,100.)/100.;
        for chunk in surface.rgba.chunks_mut(4*1024){
            checkpoint()?;
            for p in chunk.chunks_exact_mut(4){
                let mut c=[p[0]as f32/255.,p[1]as f32/255.,p[2]as f32/255.];
                let y=0.2126*c[0]+0.7152*c[1]+0.0722*c[2];
                if y>hi&&y>0.{let k=hi/y;for v in &mut c{*v*=k;}}
                else if y<lo{let d=lo-y;for v in &mut c{*v+=d;}}
                for i in 0..3{p[i]=(c[i].clamp(0.,hi.max(lo))*255.).floor().clamp(0.,255.)as u8;}
            }
        }
        Ok(())
    }
}
