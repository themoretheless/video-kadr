// Adapted from Filmcraft MIT 5231852443363f001c3f6b396dd9b1e6461ae2be; see LICENSE-MIT.
//! Blend modes (Premiere's 27), composited on premultiplied linear-light images.
//!
//! Normal is computed in linear light. The other modes are defined on display-encoded values (as
//! artists expect from every editing app), so their math runs on sRGB-encoded straight colour and
//! the result is converted back to linear. Formulas follow the W3C Compositing and Blending spec.

use super::transition_kernel_pack::image::Image;
fn srgb_to_linear(v:f32)->f32{if v<=0.04045{v/12.92}else{((v+0.055)/1.055).powf(2.4)}}
fn linear_to_srgb(v:f32)->f32{if v<=0.003_130_8{v*12.92}else{1.055*v.powf(1./2.4)-0.055}}




#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Normal,
    Dissolve,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Blend {
    pub const ALL: [Blend; 27] = [
        Blend::Normal,
        Blend::Dissolve,
        Blend::Darken,
        Blend::Multiply,
        Blend::ColorBurn,
        Blend::LinearBurn,
        Blend::DarkerColor,
        Blend::Lighten,
        Blend::Screen,
        Blend::ColorDodge,
        Blend::LinearDodge,
        Blend::LighterColor,
        Blend::Overlay,
        Blend::SoftLight,
        Blend::HardLight,
        Blend::VividLight,
        Blend::LinearLight,
        Blend::PinLight,
        Blend::HardMix,
        Blend::Difference,
        Blend::Exclusion,
        Blend::Subtract,
        Blend::Divide,
        Blend::Hue,
        Blend::Saturation,
        Blend::Color,
        Blend::Luminosity,
    ];
    /// Position in [`Blend::ALL`] (= `filmcraft_project::effect::BLEND_MODES`).
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0) as u32
    }
    /// Whether compositing needs the destination colour (every mode but Normal and Dissolve,
    /// which are plain "over" per pixel).
    pub fn reads_destination(self) -> bool {
        !matches!(self, Blend::Normal | Blend::Dissolve)
    }
    /// Index into `filmcraft_project::effect::BLEND_MODES`.
    pub fn from_index(i: u32) -> Blend {
        Self::ALL.get(i as usize).copied().unwrap_or(Blend::Normal)
    }
}

#[inline]
fn sep(mode: Blend, b: f32, s: f32) -> f32 {
    match mode {
        Blend::Darken => b.min(s),
        Blend::Multiply => b * s,
        Blend::ColorBurn => {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }
        Blend::LinearBurn => (b + s - 1.0).max(0.0),
        Blend::Lighten => b.max(s),
        Blend::Screen => b + s - b * s,
        Blend::ColorDodge => {
            if b <= 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }
        Blend::LinearDodge => (b + s).min(1.0),
        Blend::Overlay => sep(Blend::HardLight, s, b),
        Blend::HardLight => {
            if s <= 0.5 {
                b * 2.0 * s
            } else {
                sep(Blend::Screen, b, 2.0 * s - 1.0)
            }
        }
        Blend::SoftLight => {
            if s <= 0.5 {
                b - (1.0 - 2.0 * s) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * s - 1.0) * (d - b)
            }
        }
        Blend::VividLight => {
            if s <= 0.5 {
                sep(Blend::ColorBurn, b, 2.0 * s)
            } else {
                sep(Blend::ColorDodge, b, 2.0 * s - 1.0)
            }
        }
        Blend::LinearLight => (b + 2.0 * s - 1.0).clamp(0.0, 1.0),
        Blend::PinLight => {
            if s <= 0.5 {
                b.min(2.0 * s)
            } else {
                b.max(2.0 * s - 1.0)
            }
        }
        Blend::HardMix => {
            if sep(Blend::VividLight, b, s) >= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        Blend::Difference => (b - s).abs(),
        Blend::Exclusion => b + s - 2.0 * b * s,
        Blend::Subtract => (b - s).max(0.0),
        Blend::Divide => {
            if s <= 0.0 {
                1.0
            } else {
                (b / s).min(1.0)
            }
        }
        _ => s,
    }
}

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut o = c;
    if n < 0.0 {
        for v in &mut o {
            *v = l + (*v - l) * l / (l - n).max(1e-6);
        }
    }
    if x > 1.0 {
        for v in &mut o {
            *v = l + (*v - l) * (1.0 - l) / (x - l).max(1e-6);
        }
    }
    o
}
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    if mx - mn <= 1e-6 {
        return [0.0; 3];
    }
    let mut o = [0.0; 3];
    for k in 0..3 {
        o[k] = (c[k] - mn) * s / (mx - mn);
    }
    o
}

/// Blend function B(Cb, Cs) on straight, display-encoded colour.
pub fn blend_rgb(mode: Blend, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
    match mode {
        Blend::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        Blend::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        Blend::Color => set_lum(s, lum(b)),
        Blend::Luminosity => set_lum(b, lum(s)),
        Blend::DarkerColor => {
            if lum(s) < lum(b) {
                s
            } else {
                b
            }
        }
        Blend::LighterColor => {
            if lum(s) > lum(b) {
                s
            } else {
                b
            }
        }
        m => [sep(m, b[0], s[0]), sep(m, b[1], s[1]), sep(m, b[2], s[2])],
    }
}

fn hash2(x: usize, y: usize) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// [`composite_checked`] for a `src` that is only a rectangle of its layer, with its top-left corner at
/// (`x0`, `y0`) of `dst`. The rest of the layer is transparent, which leaves `dst` as it is, so only
/// the rectangle is visited; its pixels are mixed exactly as [`composite_checked`] mixes them (the dissolve
/// pattern follows the pixel's place in `dst`). The part of the rectangle outside `dst` is ignored.
fn composite_at(dst: &mut Image, src: &Image, x0: usize, y0: usize, opacity: f32, mode: Blend, cancelled:&dyn Fn()->bool) {
    let (cw, ch) = (src.w.min(dst.w.saturating_sub(x0)), src.h.min(dst.h.saturating_sub(y0)));
    if cw == 0 || ch == 0 || src.px.len() != src.w * src.h * 4 {
        return;
    }
    let dw = dst.w;
    dst.px.chunks_mut(dw * 4).skip(y0).take(ch).zip(src.px.chunks(src.w * 4)).enumerate().for_each(|(ry, (d, s))| {
        if cancelled(){return}
        let y = y0 + ry;
        for rx in 0..cw {
            let x = x0 + rx;
            let (i, j) = (x * 4, rx * 4);
            let mut sp = [s[j] * opacity, s[j + 1] * opacity, s[j + 2] * opacity, s[j + 3] * opacity];
            let sa = sp[3];
            if sa <= 0.0 {
                continue;
            }
            if mode == Blend::Dissolve {
                if hash2(x, y) >= sa {
                    continue;
                }
                sp = [sp[0] / sa, sp[1] / sa, sp[2] / sa, 1.0];
            }
            let da = d[i + 3];
            if mode == Blend::Normal || mode == Blend::Dissolve || da <= 0.0 {
                let k = 1.0 - sp[3];
                for c in 0..4 {
                    d[i + c] = sp[c] + d[i + c] * k;
                }
                continue;
            }
            let sa = sp[3];
            let cs = [linear_to_srgb(sp[0] / sa), linear_to_srgb(sp[1] / sa), linear_to_srgb(sp[2] / sa)];
            let cb = [linear_to_srgb(d[i] / da), linear_to_srgb(d[i + 1] / da), linear_to_srgb(d[i + 2] / da)];
            let bl = blend_rgb(mode, cb, cs);
            for c in 0..3 {
                let mixed = srgb_to_linear(bl[c].clamp(0.0, 1.0));
                d[i + c] = sp[c] * (1.0 - da) + d[i + c] * (1.0 - sa) + sa * da * mixed;
            }
            d[i + 3] = sa + da - sa * da;
        }
    });
}


/// A cancelled operation leaves a partial working destination: discard it.
/// No worker pool or allocation is created by this pure pixel operation.
pub fn composite_checked(dst:&mut Image,src:&Image,x:usize,y:usize,opacity:f32,mode:Blend,cancelled:impl Fn()->bool)->anyhow::Result<()> {
 anyhow::ensure!(opacity.is_finite()&&(0.0..=1.0).contains(&opacity),"invalid blend opacity");
 for image in [&*dst,src]{
  anyhow::ensure!(image.w>0&&image.h>0&&image.w<=3840&&image.h<=2160&&image.w.checked_mul(image.h).and_then(|n|n.checked_mul(4))==Some(image.px.len()),"invalid blend image shape");
  for chunk in image.px.chunks(4096){anyhow::ensure!(!cancelled(),"blend cancelled");anyhow::ensure!(chunk.iter().all(|v|v.is_finite()),"nonfinite blend input");}
 }
 composite_at(dst,src,x,y,opacity,mode,&cancelled);
 anyhow::ensure!(!cancelled(),"blend cancelled");Ok(())
}
#[cfg(test)]mod tests{
 use super::*;use serde_json::Value;
 #[test]fn original_all_modes_and_rectangles_match_every_f32_bit(){
  let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-blend.json")).unwrap();assert_eq!(fixture["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),972);
  let image=|value:&Value|Image{w:value["w"].as_u64().unwrap() as usize,h:value["h"].as_u64().unwrap() as usize,px:value["bits"].as_array().unwrap().iter().map(|v|f32::from_bits(v.as_u64().unwrap() as u32)).collect()};
  for case in cases{let mut dst=image(&case["dst"]);let src=image(&case["src"]);let mode=Blend::from_index(case["mode"].as_u64().unwrap()as u32);composite_checked(&mut dst,&src,case["x"].as_u64().unwrap()as usize,case["y"].as_u64().unwrap()as usize,case["opacity"].as_f64().unwrap()as f32,mode,||false).unwrap();let bits:Vec<_>=dst.px.iter().map(|v|v.to_bits()).collect();assert_eq!(serde_json::to_value(bits).unwrap(),case["bits"],"mode {} case {}",case["mode"],case["variant"]);}
 }
 #[test]fn invalid_or_cancelled_work_is_refused(){let mut d=Image::filled(2,2,[0.,0.,0.,1.]);let mut s=d.clone();assert!(composite_checked(&mut d,&s,0,0,1.,Blend::Normal,||true).is_err());assert!(composite_checked(&mut d,&s,0,0,f32::NAN,Blend::Normal,||false).is_err());s.px[0]=f32::NAN;assert!(composite_checked(&mut d,&s,0,0,1.,Blend::Normal,||false).is_err());}
}
