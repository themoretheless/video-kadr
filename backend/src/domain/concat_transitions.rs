//! SPDX-License-Identifier: AGPL-3.0-or-later
//! Concat scene-linear transition family, pinned78e4be101c2d3c41a7dc73c112c6b1f11b9ce170.
use super::{
    concat_chroma::{display, linear},
    concat_working::texture_half,
    linear_color_frame::LinearColorFrame,
};
use anyhow::{ensure, Result};
#[derive(Clone, Copy, Debug)]
pub enum Style {
    Dissolve,
    GlitchWarp { amount: f32 },
    Glitch { amount: f32 },
    Cube,
    FilmBurn { heat: f32 },
    ZoomBlur { strength: f32 },
    LightLeak { warmth: f32 },
    LightLeakFlash { strength: f32 },
    Zoom { direction: u8 },
    WhipPan { speed: f32 },
    BlurDissolve { strength: f32 },
    Black,
    White,
    ClockWipe,
    Spin { turns: f32 },
    Swirl { twist: f32 },
    RgbSplitWipe { spread: f32 },
    Push { direction: u8 },
    Slide { direction: u8 },
    ShapeWipe { shape: u8, softness: f32 },
    Iris { softness: f32 },
    LinearWipe { direction: u8, softness: f32 },
}
fn rotated_uv(uv: [f32; 2], width: usize, height: usize, angle: f32, scale: f32) -> [f32; 2] {
    let c = [(uv[0] - 0.5) * width as f32, (uv[1] - 0.5) * height as f32];
    let s = angle.sin();
    let co = angle.cos();
    let scale = scale.max(0.0001);
    [
        (c[0] * co - c[1] * s) / scale / width as f32 + 0.5,
        (c[0] * s + c[1] * co) / scale / height as f32 + 0.5,
    ]
}
pub fn render(
    a: &LinearColorFrame,
    b: &LinearColorFrame,
    progress: f32,
    reverse: bool,
    style: Style,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<LinearColorFrame> {
    ensure!(
        progress.is_finite() && (0. ..=1.).contains(&progress),
        "invalid Concat transition progress"
    );
    if let Style::LinearWipe {
        direction,
        softness,
    } = style
    {
        ensure!(
            direction < 4 && softness.is_finite() && (0. ..=100.).contains(&softness),
            "invalid Concat linear wipe parameters"
        );
    }
    ensure!(
        a.dimensions() == b.dimensions(),
        "Concat transition canvas mismatch"
    );
    if let Style::Iris { softness } = style {
        ensure!(
            softness.is_finite() && (0. ..=100.).contains(&softness),
            "invalid Concat Iris softness"
        );
    }
    if let Style::ShapeWipe { shape, softness } = style {
        ensure!(
            shape < 3 && softness.is_finite() && (0. ..=100.).contains(&softness),
            "invalid Concat Shape Wipe parameters"
        );
    }
    if let Style::Push { direction } | Style::Slide { direction } = style {
        ensure!(direction < 4, "invalid Concat movement direction");
    }
    if let Style::RgbSplitWipe { spread } = style {
        ensure!(
            spread.is_finite() && (0. ..=100.).contains(&spread),
            "invalid Concat RGB Split spread"
        );
    }
    if let Style::Zoom { direction } = style { ensure!(direction < 2, "invalid Concat Zoom direction"); }
    if let Style::WhipPan { speed } = style { ensure!(speed.is_finite() && (20. ..=100.).contains(&speed), "invalid Concat Whip Pan speed"); }
    if let Style::GlitchWarp { amount } = style { ensure!(amount.is_finite() && (0. ..=100.).contains(&amount), "invalid Concat Glitch Warp amount"); }
    if let Style::Glitch { amount } = style { ensure!(amount.is_finite() && (0. ..=100.).contains(&amount), "invalid Concat Glitch amount"); }
    if let Style::FilmBurn { heat } = style { ensure!(heat.is_finite() && (0. ..=100.).contains(&heat), "invalid Concat Film Burn heat"); }
    if let Style::ZoomBlur { strength } = style { ensure!(strength.is_finite() && (0. ..=100.).contains(&strength), "invalid Concat Zoom Blur strength"); }
    if let Style::LightLeak { warmth } = style { ensure!(warmth.is_finite() && (0. ..=100.).contains(&warmth), "invalid Concat Light Leak warmth"); }
    if let Style::LightLeakFlash { strength } = style { ensure!(strength.is_finite() && (0. ..=100.).contains(&strength), "invalid Concat Light Leak Flash strength"); }
    if let Style::BlurDissolve { strength } = style {
        ensure!(strength.is_finite() && (0. ..=100.).contains(&strength), "invalid Concat Blur Dissolve strength");
    }
    if let Style::Spin { turns } = style {
        ensure!(
            turns.is_finite() && (0.5..=2.).contains(&turns),
            "invalid Concat Spin turns"
        );
    }
    if let Style::Swirl { twist } = style {
        ensure!(
            twist.is_finite() && (0. ..=100.).contains(&twist),
            "invalid Concat Swirl twist"
        );
    }
    let (a, b, p) = if reverse {
        (b, a, 1. - progress)
    } else {
        (a, b, progress)
    };
    let (w, h) = a.dimensions();
    let bytes = w
        .checked_mul(h)
        .and_then(|v| v.checked_mul(160))
        .ok_or_else(|| anyhow::anyhow!("Concat transition scratch overflow"))?;
    ensure!(
        bytes <= 1024 * 1024 * 1024,
        "Concat transition scratch exceeds1GiB budget"
    );
    let mut aa = a.clone().concat_straight_snapshot(checkpoint)?;
    let mut bb = b.clone().concat_straight_snapshot(checkpoint)?;
    for pixels in [&mut aa, &mut bb] {
        for row in pixels.chunks_mut(w) {
            checkpoint()?;
            for px in row {
                *px = px.map(|v| half::f16::from_f32(v).to_f32());
            }
        }
    }
    let mut out = a.clone();
    out.map_concat_straight_pixels(
        &mut |_, x, y| {
            let (a, b) = (aa[y * w + x], bb[y * w + x]);
            match style {
                Style::GlitchWarp { amount } => {
                    let uv=[(x as f32+0.5)/w as f32,(y as f32+0.5)/h as f32];
                    let k=(p*3.14159).sin();
                    let noise=super::concat_falling_dust::hash((uv[1]*20.).floor(),(p*15.).floor(),1.);
                    let disp=(noise-0.5)*k*(amount*0.001);
                    std::array::from_fn(|channel|{
                        let offset=match channel{0=>disp*1.5,1=>disp,2=>-disp,_=>0.};
                        let at=[uv[0]+offset,uv[1]];
                        let a=super::concat_zoom_blur::read(&aa,w,h,at);
                        let b=super::concat_zoom_blur::read(&bb,w,h,at);
                        texture_half(a[channel]*(1.-p)+b[channel]*p)
                    })
                }
                Style::Glitch { amount } => {
                    let uv=[(x as f32+0.5)/w as f32,(y as f32+0.5)/h as f32];
                    let k=(1.-(p*2.-1.).abs())*amount*0.01;
                    let jitter=(super::concat_falling_dust::hash((uv[1]*24.).floor(),0.,(p*40.).floor())-0.5)*k*0.15;
                    let ux=(uv[0]+jitter).clamp(0.,1.);
                    let green=super::concat_zoom_blur::read(&aa,w,h,[ux,uv[1]]);
                    let red=super::concat_zoom_blur::read(&aa,w,h,[(ux+k*0.01).clamp(0.,1.),uv[1]]);
                    let blue=super::concat_zoom_blur::read(&aa,w,h,[(ux-k*0.01).clamp(0.,1.),uv[1]]);
                    let a=[red[0],green[1],blue[2],green[3]];
                    let b=super::concat_zoom_blur::read(&bb,w,h,[(uv[0]-jitter).clamp(0.,1.),uv[1]]);
                    let p=p*p*(3.-2.*p);
                    std::array::from_fn(|k|texture_half(a[k]*(1.-p)+b[k]*p))
                }
                Style::Cube => {
                    let uv=[(x as f32+0.5)/w as f32,(y as f32+0.5)/h as f32];
                    let p=p*p*(3.-2.*p);
                    let (t,squeeze,source,shade)=if p<0.5 {
                        let t=p*2.;(t,(t*1.5707963).cos(),&aa,1.-t+0.35*t)
                    }else{let t=(p-0.5)*2.;(t,(t*1.5707963).sin(),&bb,0.35*(1.-t)+t)};
                    let _=t;
                    let cx=(uv[0]-0.5)/squeeze.max(0.001)+0.5;
                    if !(0. ..=1.).contains(&cx) {return [texture_half(linear(0.03)),texture_half(linear(0.03)),texture_half(linear(0.03)),1.];}
                    let c=super::concat_zoom_blur::read(source,w,h,[cx,uv[1]]);
                    let shade=shade.powf(2.4);
                    std::array::from_fn(|k|texture_half(if k==3{c[k]}else{c[k]*shade}))
                }
                Style::FilmBurn { heat } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let a = super::concat_zoom_blur::read(&aa,w,h,uv);
                    let b = super::concat_zoom_blur::read(&bb,w,h,uv);
                    let base: [f32;4] = std::array::from_fn(|k| a[k] * (1. - p) + b[k] * p);
                    let start = (p / 0.5).clamp(0.,1.);
                    let end = ((p - 0.5) / 0.5).clamp(0.,1.);
                    let burn = (start * start * (3. - 2. * start)) * (1. - end * end * (3. - 2. * end)) * 4.;
                    std::array::from_fn(|k| if k==3 {texture_half(base[k])} else {
                        let warm = [1.,0.85,0.6][k] * burn * (heat * 0.01);
                        let lit=display(base[k]);
                        texture_half(linear((lit + warm).min(lit.max(1.))))
                    })
                }
                Style::ZoomBlur { strength } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let direction = [uv[0] - 0.5, uv[1] - 0.5];
                    let k = (p * 3.14159).sin();
                    let zoom = 1. + k * 0.5;
                    let blur = k * (strength * 0.0006);
                    let mut sum = [0.;4];
                    for i in 0..5 {
                        let scale = 1. + i as f32 * blur;
                        let at = [0.5 + (direction[0] / zoom) * scale, 0.5 + (direction[1] / zoom) * scale];
                        let a = super::concat_zoom_blur::read(&aa,w,h,at);
                        let b = super::concat_zoom_blur::read(&bb,w,h,at);
                        for channel in 0..4 { sum[channel] += a[channel] * (1. - p) + b[channel] * p; }
                    }
                    sum.map(|v| texture_half(v * (1. / 5.)))
                }
                Style::LightLeak { warmth } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let p = p * p * (3. - 2. * p);
                    let a = super::concat_zoom_blur::read(&aa,w,h,uv);
                    let b = super::concat_zoom_blur::read(&bb,w,h,uv);
                    let base: [f32;4] = std::array::from_fn(|k| a[k] * (1. - p) + b[k] * p);
                    let sweep = uv[0] + uv[1] * 0.3 - (p * 2.2 - 0.6);
                    let core = (-sweep * sweep * 22.).exp() * 1.4;
                    let halo = (-sweep * sweep * 6.).exp() * 0.6;
                    let amount = warmth * 0.01;
                    std::array::from_fn(|k| if k==3 {texture_half(base[k])} else {
                        let leak = [1.,0.95,0.85][k] * core * amount * 1.3 + [1.,0.8,0.5][k] * halo * amount;
                        let lit=display(base[k]);
                        texture_half(linear((lit + leak).min(lit.max(1.))))
                    })
                }
                Style::LightLeakFlash { strength } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let a = super::concat_zoom_blur::read(&aa,w,h,uv);
                    let b = super::concat_zoom_blur::read(&bb,w,h,uv);
                    let base: [f32;4] = std::array::from_fn(|k| a[k] * (1. - p) + b[k] * p);
                    let flash = 1. - (p * 2. - 1.).abs();
                    let pos = uv[0] * 0.7 + uv[1] * 0.3;
                    let leak = (-((pos - p).powf(2.)) * 12.).exp();
                    let burn = (flash * 0.8 + leak * 0.5) * (strength * 0.01);
                    std::array::from_fn(|k| if k==3 {texture_half(base[k])} else {
                        let lit=display(base[k]);
                        texture_half(linear((lit + [1.,0.85,0.6][k] * burn).min(lit.max(1.))))
                    })
                }
                Style::Zoom { direction } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let p = p * p * (3. - 2. * p);
                    let (out_scale, in_scale) = if direction == 0 { (1. + p * 1.6, 1.6 - p * 1.6) } else { (1. - p * 0.6, 0.4 + p * 0.6) };
                    let zoom = |scale:f32| [(uv[0] - 0.5) / scale.max(0.0001) + 0.5, (uv[1] - 0.5) / scale.max(0.0001) + 0.5];
                    let a = super::concat_zoom_blur::read(&aa,w,h,zoom(out_scale));
                    let b = super::concat_zoom_blur::read(&bb,w,h,zoom(in_scale));
                    std::array::from_fn(|k| texture_half(a[k] * (1. - p) + b[k] * p))
                }
                Style::WhipPan { speed } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let eased = p * p * (3. - 2. * p);
                    let blur = (p * 3.14159).sin() * (speed * 0.0008);
                    let incoming = if uv[0] >= 1. - eased { 1. } else { 0. };
                    let mut sum = [0.; 4];
                    for i in -2..=2 {
                        let offset = i as f32 * blur;
                        let a = super::concat_zoom_blur::read(&aa,w,h,[uv[0] + eased + offset,uv[1]]);
                        let b = super::concat_zoom_blur::read(&bb,w,h,[uv[0] - (1. - eased) + offset,uv[1]]);
                        for k in 0..4 { sum[k] += a[k] * (1. - incoming) + b[k] * incoming; }
                    }
                    sum.map(|v| texture_half(v * (1. / 5.)))
                }
                Style::BlurDissolve { strength } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let radius = (1. - (p * 2. - 1.).abs()) * strength * 0.3;
                    let mut from = [0.; 4];
                    let mut to = [0.; 4];
                    for oy in -1..=1 {
                        for ox in -1..=1 {
                            let at = [uv[0] + ox as f32 * ((1. / w as f32) * radius), uv[1] + oy as f32 * ((1. / h as f32) * radius)];
                            let a = super::concat_zoom_blur::read(&aa, w, h, at);
                            let b = super::concat_zoom_blur::read(&bb, w, h, at);
                            for k in 0..4 { from[k] += a[k]; to[k] += b[k]; }
                        }
                    }
                    std::array::from_fn(|k| texture_half((from[k] * (1. / 9.)) * (1. - p) + (to[k] * (1. / 9.)) * p))
                }
                Style::Iris { softness } => {
                    let p = p * p * (3. - 2. * p);
                    let c = [
                        (((x as f32 + 0.5) / w as f32) - 0.5) * w as f32,
                        (((y as f32 + 0.5) / h as f32) - 0.5) * h as f32,
                    ];
                    let size = [w as f32, h as f32];
                    let distance = (c[0] * c[0] + c[1] * c[1]).sqrt()
                        / (0.5 * (size[0] * size[0] + size[1] * size[1]).sqrt());
                    let radius = p * 1.15;
                    let soft = (softness * 0.01).max(0.02);
                    let t = ((distance - (radius - soft)) / ((radius + soft) - (radius - soft)))
                        .clamp(0., 1.);
                    let m = t * t * (3. - 2. * t);
                    std::array::from_fn(|k| texture_half(b[k] * (1. - m) + a[k] * m))
                }
                Style::ShapeWipe { shape, softness } => {
                    let p = p * p * (3. - 2. * p);
                    let size = [w as f32, h as f32];
                    let unit = 0.5 * size[0].min(size[1]);
                    let c = [
                        (((x as f32 + 0.5) / size[0]) - 0.5) * size[0] / unit,
                        (((y as f32 + 0.5) / size[1]) - 0.5) * size[1] / unit,
                    ];
                    let reach = (size[0] * size[0] + size[1] * size[1]).sqrt() / (2. * unit);
                    let d = match shape {
                        0 => (c[0] * c[0] + c[1] * c[1]).sqrt(),
                        1 => {
                            let y = -c[1] - 0.1;
                            let r = (c[0] * c[0] + y * y).sqrt();
                            let theta = c[0].atan2(y);
                            let boundary = 0.55
                                * (theta.sin() * theta.cos().abs().sqrt() / (theta.sin() + 1.4)
                                    + 1.1);
                            r - boundary
                        }
                        _ => {
                            let r = (c[0] * c[0] + c[1] * c[1]).sqrt();
                            let angle = c[1].atan2(c[0]);
                            let segment = 6.28318530718f32 / 5.;
                            let m = ((angle / segment + 0.5).floor() * segment - angle).cos()
                                * 0.35
                                + 0.65;
                            r - 0.75 * m
                        }
                    };
                    let threshold = p * (reach + 1.6) - 1.;
                    let soft = (softness * 0.01).max(0.02) * 1.5;
                    let t = ((d - (threshold - soft)) / ((threshold + soft) - (threshold - soft)))
                        .clamp(0., 1.);
                    let m = t * t * (3. - 2. * t);
                    std::array::from_fn(|k| texture_half(b[k] * (1. - m) + a[k] * m))
                }
                Style::Push { direction } | Style::Slide { direction } => {
                    let p = p * p * (3. - 2. * p);
                    let axis = match direction {
                        0 => [1., 0.],
                        1 => [-1., 0.],
                        2 => [0., 1.],
                        _ => [0., -1.],
                    };
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let is_push = matches!(style, Style::Push { .. });
                    let offset = if is_push {
                        [axis[0] * (1. - p), axis[1] * (1. - p)]
                    } else {
                        [-axis[0] * (1. - p), -axis[1] * (1. - p)]
                    };
                    let incoming = [uv[0] - offset[0], uv[1] - offset[1]];
                    let inside = incoming.iter().all(|v| *v >= 0. && *v <= 1.);
                    let pixel = if inside {
                        super::concat_zoom_blur::read(&bb, w, h, incoming)
                    } else {
                        let outgoing = if is_push {
                            [uv[0] + axis[0] * p, uv[1] + axis[1] * p]
                        } else {
                            uv
                        };
                        super::concat_zoom_blur::read(&aa, w, h, outgoing)
                    };
                    pixel.map(texture_half)
                }
                Style::RgbSplitWipe { spread } => {
                    let k = (1. - (p * 2. - 1.).abs()) * spread * 0.0004;
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let from = super::concat_zoom_blur::read(&aa, w, h, uv);
                    let to = super::concat_zoom_blur::read(&bb, w, h, uv);
                    let sample = super::concat_chromatic_aberration::sample;
                    let a = [
                        sample(&aa, w, h, [uv[0] + k, uv[1]], 0),
                        from[1],
                        sample(&aa, w, h, [uv[0] - k, uv[1]], 2),
                        from[3],
                    ];
                    let b = [
                        sample(&bb, w, h, [uv[0] - k, uv[1]], 0),
                        to[1],
                        sample(&bb, w, h, [uv[0] + k, uv[1]], 2),
                        to[3],
                    ];
                    let t = p * p * (3. - 2. * p);
                    std::array::from_fn(|channel| {
                        texture_half(a[channel] * (1. - t) + b[channel] * t)
                    })
                }
                Style::Spin { turns } => {
                    let p = p * p * (3. - 2. * p);
                    let full = turns * 6.28318530718f32;
                    let angle = p * full;
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let a_uv = rotated_uv(uv, w, h, angle, 1. - p * 0.9);
                    let b_uv = rotated_uv(uv, w, h, angle - full, 0.1 + p * 0.9);
                    let inside = |uv: [f32; 2]| uv.iter().all(|v| *v >= 0. && *v <= 1.);
                    let a = if inside(a_uv) {
                        super::concat_zoom_blur::read(&aa, w, h, a_uv)
                    } else {
                        [0.; 4]
                    };
                    let b = if inside(b_uv) {
                        super::concat_zoom_blur::read(&bb, w, h, b_uv)
                    } else {
                        [0.; 4]
                    };
                    std::array::from_fn(|k| texture_half(a[k] * (1. - p) + b[k] * p))
                }
                Style::Swirl { twist } => {
                    let p = p * p * (3. - 2. * p);
                    let k = (p * 3.14159265f32).sin() * twist * 0.06;
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let c = [(uv[0] - 0.5) * w as f32, (uv[1] - 0.5) * h as f32];
                    let distance = (c[0] * c[0] + c[1] * c[1]).sqrt() / h as f32;
                    let t = (distance / 0.7).clamp(0., 1.);
                    let falloff = 1. - t * t * (3. - 2. * t);
                    let a = super::concat_zoom_blur::read(
                        &aa,
                        w,
                        h,
                        rotated_uv(uv, w, h, k * falloff, 1.),
                    );
                    let b = super::concat_zoom_blur::read(
                        &bb,
                        w,
                        h,
                        rotated_uv(uv, w, h, -k * falloff, 1.),
                    );
                    std::array::from_fn(|k| texture_half(a[k] * (1. - p) + b[k] * p))
                }
                Style::ClockWipe => {
                    let p = p * p * (3. - 2. * p);
                    if p >= 1. {
                        return b;
                    }
                    let c = [
                        (((x as f32 + 0.5) / w as f32) - 0.5) * w as f32,
                        (((y as f32 + 0.5) / h as f32) - 0.5) * h as f32,
                    ];
                    let mut angle = c[0].atan2(-c[1]);
                    let tau = 6.28318530718f32;
                    if angle < 0. {
                        angle += tau;
                    }
                    let radius = (c[0] * c[0] + c[1] * c[1]).sqrt();
                    let swept = ((p * tau - angle) * radius + 0.5).clamp(0., 1.);
                    let started = (angle * radius + 0.5).clamp(0., 1.);
                    let m = swept * started;
                    std::array::from_fn(|k| texture_half(a[k] * (1. - m) + b[k] * m))
                }
                Style::LinearWipe {
                    direction,
                    softness,
                } => {
                    let uv = [(x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32];
                    let coord = match direction {
                        0 => uv[0],
                        1 => 1. - uv[0],
                        2 => 1. - uv[1],
                        _ => uv[1],
                    };
                    let p = p * p * (3. - 2. * p);
                    let soft = (softness * 0.003).max(0.001);
                    // Metal factors the symmetric smoothstep interval before rounding.
                    // Original mask-bit probes and an independent parameter matrix
                    // qualify 2*soft; subtracting rounded edges shifts the midpoint.
                    let t = ((coord - (p - soft)) / (2. * soft)).clamp(0., 1.);
                    let m = t * t * (3. - 2. * t);
                    std::array::from_fn(|k| texture_half(b[k] * (1. - m) + a[k] * m))
                }
                Style::Dissolve => {
                    std::array::from_fn(|k| texture_half(a[k] * (1. - p) + b[k] * p))
                }
                Style::Black | Style::White => {
                    let base = if p >= 0.5 { b } else { a };
                    let level = 1. - (p * 2. - 1.).abs();
                    let rgb = std::array::from_fn::<_, 3, _>(|k| {
                        let encoded = display(base[k]);
                        let value = match style {
                            Style::Black => encoded * (1. - level),
                            _ => encoded * (1. - level) + level,
                        };
                        texture_half(linear(value).clamp(-65504., 65504.))
                    });
                    [rgb[0], rgb[1], rgb[2], base[3]]
                }
            }
        },
        checkpoint,
    )?;
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(rgb: [f32; 4]) -> LinearColorFrame {
        let mut f = LinearColorFrame::from_working(1, 1, &[0.; 4], &mut || Ok(())).unwrap();
        f.map_concat_straight_pixels(&mut |_, _, _| rgb, &mut || Ok(()))
            .unwrap();
        f
    }
    #[test]
    fn shape_wipe_circle_heart_star_and_parameter_bounds() {
        let make = |p: [f32; 4]| {
            let mut f = LinearColorFrame::from_working(9, 5, &[0.; 180], &mut || Ok(())).unwrap();
            f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                .unwrap();
            f
        };
        let (a, b) = (make([1., 0., 0., 0.25]), make([0., 0., 1., 0.75]));
        let mut images = Vec::new();
        for shape in 0..3 {
            let mid = render(
                &a,
                &b,
                0.5,
                false,
                Style::ShapeWipe {
                    shape,
                    softness: 15.,
                },
                &mut || Ok(()),
            )
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
            assert_eq!(mid[2 * 9 + 4], [0., 0., 1., 0.75]);
            assert!(mid[0][0] > 0.9, "outgoing corner shape{shape}");
            images.push(mid);
        }
        assert_ne!(images[0], images[1]);
        assert_ne!(images[0], images[2]);
        assert_ne!(images[1], images[2]);
        for style in [
            Style::ShapeWipe {
                shape: 3,
                softness: 15.,
            },
            Style::ShapeWipe {
                shape: 0,
                softness: -1.,
            },
            Style::ShapeWipe {
                shape: 0,
                softness: f32::NAN,
            },
        ] {
            assert!(render(&a, &b, 0.5, false, style, &mut || Ok(())).is_err());
        }
    }
    #[test]
    fn iris_opens_in_pixels_and_keeps_original_soft_endpoints() {
        let make = |p: [f32; 4]| {
            let mut f = LinearColorFrame::from_working(9, 5, &[0.; 180], &mut || Ok(())).unwrap();
            f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                .unwrap();
            f
        };
        let (a, b) = (make([1., 0., 0., 0.25]), make([0., 0., 1., 0.75]));
        let mid = render(
            &a,
            &b,
            0.5,
            false,
            Style::Iris { softness: 15. },
            &mut || Ok(()),
        )
        .unwrap()
        .concat_straight_snapshot(&mut || Ok(()))
        .unwrap();
        assert_eq!(mid[2 * 9 + 4], [0., 0., 1., 0.75]);
        assert_eq!(mid[2 * 9], [1., 0., 0., 0.25]);
        assert_eq!(mid[2 * 9 + 6], mid[4 * 9 + 4]);
        let start = render(
            &a,
            &b,
            0.,
            false,
            Style::Iris { softness: 100. },
            &mut || Ok(()),
        )
        .unwrap()
        .concat_straight_snapshot(&mut || Ok(()))
        .unwrap();
        assert_eq!(start[2 * 9 + 4], [0.5, 0., 0.5, 0.5]);
        let end = render(
            &a,
            &b,
            1.,
            false,
            Style::Iris { softness: 100. },
            &mut || Ok(()),
        )
        .unwrap()
        .concat_straight_snapshot(&mut || Ok(()))
        .unwrap();
        assert!(
            end[0][0] > 0.,
            "original broad softness retains outgoing corners at progress1"
        );
        assert!(render(
            &a,
            &b,
            0.5,
            false,
            Style::Iris { softness: 101. },
            &mut || Ok(())
        )
        .is_err());
    }
    #[test]
    fn clock_wipe_pixel_aspect_center_and_clockwise_sweep() {
        let make = |p: [f32; 4]| {
            let mut f = LinearColorFrame::from_working(9, 5, &[0.; 180], &mut || Ok(())).unwrap();
            f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                .unwrap();
            f
        };
        let (a, b) = (make([1., 0., 0., 0.25]), make([0., 0., 1., 0.75]));
        let mid = render(&a, &b, 0.5, false, Style::ClockWipe, &mut || Ok(()))
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
        assert_eq!(mid[2 * 9 + 8], [0., 0., 1., 0.75]);
        assert_eq!(mid[2 * 9], [1., 0., 0., 0.25]);
        assert_eq!(mid[2 * 9 + 4], [0.75, 0., 0.25, 0.375]);
        let end = render(&a, &b, 1., false, Style::ClockWipe, &mut || Ok(()))
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
        assert!(end.iter().all(|p| *p == [0., 0., 1., 0.75]));
    }
    #[test]
    fn original_linear_wipe_directions_softness_and_nonendpoint_clipping() {
        let make = |p: [f32; 4]| {
            let mut f = LinearColorFrame::from_working(5, 5, &[0.; 100], &mut || Ok(())).unwrap();
            f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                .unwrap();
            f
        };
        let (a, b) = (make([1., 0., 0., 0.25]), make([0., 0., 1., 0.75]));
        for direction in 0..4 {
            let pixels = render(
                &a,
                &b,
                0.5,
                false,
                Style::LinearWipe {
                    direction,
                    softness: 0.,
                },
                &mut || Ok(()),
            )
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
            assert_eq!(pixels[12], [0.499755859375, 0., 0.5, 0.5]);
            let (incoming, outgoing) = match direction {
                0 => (10, 14),
                1 => (14, 10),
                2 => (22, 2),
                _ => (2, 22),
            };
            assert_eq!(pixels[incoming], [0., 0., 1., 0.75]);
            assert_eq!(pixels[outgoing], [1., 0., 0., 0.25]);
        }
        let soft = render(
            &a,
            &b,
            0.,
            false,
            Style::LinearWipe {
                direction: 0,
                softness: 100.,
            },
            &mut || Ok(()),
        )
        .unwrap()
        .concat_straight_snapshot(&mut || Ok(()))
        .unwrap();
        assert!(
            soft[10][2] > 0.,
            "original soft edge still blends at progress0"
        );
        assert!(render(
            &a,
            &b,
            0.5,
            false,
            Style::LinearWipe {
                direction: 4,
                softness: 20.
            },
            &mut || Ok(())
        )
        .is_err());
        assert!(render(
            &a,
            &b,
            0.5,
            false,
            Style::LinearWipe {
                direction: 0,
                softness: f32::NAN
            },
            &mut || Ok(())
        )
        .is_err());
    }
    #[test]
    fn dips_retain_selected_alpha_and_dissolve_straight_rgb() {
        let (a, b) = (frame([1., 0., 0., 0.25]), frame([0., 0., 1., 0.75]));
        for (style, want) in [(Style::Black, 0.), (Style::White, 1.)] {
            let out = render(&a, &b, 0.5, false, style, &mut || Ok(()))
                .unwrap()
                .concat_straight_snapshot(&mut || Ok(()))
                .unwrap();
            assert_eq!(out[0], [want, want, want, 0.75]);
        }
        let out = render(&a, &b, 0.5, false, Style::Dissolve, &mut || Ok(()))
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
        assert_eq!(out[0], [0.5, 0., 0.5, 0.5]);
        let mut calls = 0;
        assert!(render(&a, &b, 0.25, false, Style::Black, &mut || {
            calls += 1;
            ensure!(calls < 3, "cancel");
            Ok(())
        })
        .is_err());
        assert_eq!(
            a.clone().concat_straight_snapshot(&mut || Ok(())).unwrap()[0],
            [1., 0., 0., 0.25]
        );
    }
    #[test]
    fn display_dip_reverse_and_invalid_progress() {
        let (a, b) = (frame([1., 1., 1., 1.]), frame([0.25, 0.25, 0.25, 0.5]));
        let x = render(&a, &b, 0.25, false, Style::Black, &mut || Ok(()))
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
        assert!((x[0][0] - 0.5f32.powf(2.4)).abs() < 0.001);
        let y = render(&b, &a, 0.75, true, Style::Black, &mut || Ok(()))
            .unwrap()
            .concat_straight_snapshot(&mut || Ok(()))
            .unwrap();
        assert_eq!(x, y);
        assert!(render(&a, &b, f32::NAN, false, Style::Black, &mut || Ok(())).is_err());
    }
}

#[cfg(test)]
mod original_qualification {
    use super::*;
    #[test]
    #[ignore = "requires independently generated CONCAT_TRANSITION_GPU_ORACLE"]
    fn original_metal_two_input_bits() {
        let oracle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("CONCAT_TRANSITION_GPU_ORACLE").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
        let style = match oracle["effect"].as_str().unwrap() {
            "concat.dissolve" => Style::Dissolve,
            "concat.fade-black" => Style::Black,
            "concat.fade-white" => Style::White,
            "concat.clock-wipe" => Style::ClockWipe,
            _ => panic!("unsupported oracle"),
        };
        let frame = |side: usize, p: [f32; 4]| {
            let mut f =
                LinearColorFrame::from_working(side, side, &vec![0.; side * side * 4], &mut || {
                    Ok(())
                })
                .unwrap();
            f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                .unwrap();
            f
        };
        let mut mismatches = [0usize; 4];
        let mut channels = 0;
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            if oracle["effect"] == "concat.clock-wipe" {
                1728
            } else {
                576
            }
        );
        for case in cases {
            let side = case["side"].as_u64().unwrap_or(1) as usize;
            let a = frame(side, serde_json::from_value(case["from"].clone()).unwrap());
            let b = frame(side, serde_json::from_value(case["to"].clone()).unwrap());
            let mut got = render(
                &a,
                &b,
                case["progress"].as_f64().unwrap() as f32,
                false,
                style,
                &mut || Ok(()),
            )
            .unwrap();
            let got =
                got.concat_straight_snapshot(&mut || Ok(())).unwrap()[side / 2 * side + side / 2];
            let expected: [f32; 4] = serde_json::from_value(case["expected"].clone()).unwrap();
            for k in 0..4 {
                channels += 1;
                if got[k].to_bits() != expected[k].to_bits() {
                    mismatches[k] += 1;
                }
            }
        }
        println!(
            "{} original: {channels} channels, mismatch RGBA {mismatches:?}",
            oracle["effect"]
        );
        assert_eq!(mismatches, [0; 4]);
    }
}

#[cfg(test)]
mod linear_wipe_original {
    use super::*;
    #[test]
    #[ignore = "requires independently generated CONCAT_LINEAR_WIPE_GPU_ORACLE"]
    fn original_metal_direction_softness_bits() {
        let oracle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("CONCAT_LINEAR_WIPE_GPU_ORACLE").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(oracle["effect"], "concat.linear-wipe");
        assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            if oracle["independentValidation"].as_bool().unwrap_or(false) {
                2304
            } else {
                3456
            }
        );
        let mut mismatches = [0usize; 4];
        let mut channels = 0;
        for case in cases {
            let side = case["side"].as_u64().unwrap() as usize;
            let frame = |key: &str| {
                let p: [f32; 4] = serde_json::from_value(case[key].clone()).unwrap();
                let mut f = LinearColorFrame::from_working(
                    side,
                    side,
                    &vec![0.; side * side * 4],
                    &mut || Ok(()),
                )
                .unwrap();
                f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                    .unwrap();
                f
            };
            let style = Style::LinearWipe {
                direction: case["direction"].as_u64().unwrap() as u8,
                softness: case["softness"].as_f64().unwrap() as f32,
            };
            let mut got = render(
                &frame("from"),
                &frame("to"),
                case["progress"].as_f64().unwrap() as f32,
                false,
                style,
                &mut || Ok(()),
            )
            .unwrap();
            let got =
                got.concat_straight_snapshot(&mut || Ok(())).unwrap()[side / 2 * side + side / 2];
            let expected: [f32; 4] = serde_json::from_value(case["expected"].clone()).unwrap();
            for k in 0..4 {
                channels += 1;
                if got[k].to_bits() != expected[k].to_bits() {
                    mismatches[k] += 1;
                }
            }
        }
        println!("Linear Wipe original: {channels} channels, mismatch RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}

#[cfg(test)]
mod iris_original {
    use super::*;
    #[test]
    #[ignore = "requires independently generated CONCAT_IRIS_GPU_ORACLE"]
    fn original_metal_iris_softness_bits() {
        let oracle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("CONCAT_IRIS_GPU_ORACLE").unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            oracle["reference"]["commit"],
            "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
        );
        assert_eq!(oracle["effect"], "concat.iris");
        assert!(!oracle["instrumented"].as_bool().unwrap_or(false));
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            if oracle["independentValidation"].as_bool().unwrap_or(false) {
                864
            } else {
                864
            }
        );
        let mut mismatches = [0usize; 4];
        let mut channels = 0;
        for case in cases {
            let side = case["side"].as_u64().unwrap() as usize;
            let frame = |key: &str| {
                let p: [f32; 4] = serde_json::from_value(case[key].clone()).unwrap();
                let mut f = LinearColorFrame::from_working(
                    side,
                    side,
                    &vec![0.; side * side * 4],
                    &mut || Ok(()),
                )
                .unwrap();
                f.map_concat_straight_pixels(&mut |_, _, _| p, &mut || Ok(()))
                    .unwrap();
                f
            };
            let style = Style::Iris {
                softness: case["softness"].as_f64().unwrap() as f32,
            };
            let mut got = render(
                &frame("from"),
                &frame("to"),
                case["progress"].as_f64().unwrap() as f32,
                false,
                style,
                &mut || Ok(()),
            )
            .unwrap();
            let got =
                got.concat_straight_snapshot(&mut || Ok(())).unwrap()[side / 2 * side + side / 2];
            let expected: [f32; 4] = serde_json::from_value(case["expected"].clone()).unwrap();
            for k in 0..4 {
                channels += 1;
                if got[k].to_bits() != expected[k].to_bits() {
                    mismatches[k] += 1;
                }
            }
        }
        println!("Iris original: {channels} channels, mismatch RGBA {mismatches:?}");
        assert_eq!(mismatches, [0; 4]);
    }
}
