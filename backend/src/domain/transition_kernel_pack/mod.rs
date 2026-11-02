//! Procedural transition kernels, isolated from editor/project/runtime ownership.
//! Adapted from Filmcraft MIT, pinned 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! Experimental until oracle qualification and bounded native integration complete.
pub mod image;
pub(crate) mod geom;
mod effects;
mod blend;
pub mod project;
mod transitions;
pub mod context;
pub mod budget;
mod time {pub struct Tick; impl Tick {pub const ZERO:Self=Self;}}
mod color {pub fn srgb_to_linear(v:f32)->f32 {if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}}}
pub fn apply(effect:&project::EffectInstance,a:&image::Image,b:&image::Image,p:f32,scale:f32)->image::Image {
 transitions::apply_scaled(effect,a,b,p,scale)
}

#[cfg(test)]mod tests;
