//! Adapted from Filmcraft captions/burn.rs and project/caption.rs at 5231852443363f001c3f6b396dd9b1e6461ae2be.
//! MIT notice: docs/licenses/filmcraft-MIT.txt.
//! Caption layout and burn-in.
//!
//! [`overlay`] lays out the caption showing on a track at a time with the track's
//! [`CaptionStyle`] (size relative to a 1080-line frame, colour, background box, outline,
//! alignment, anchor and margin; WebVTT `line:N%` and `align:` cue settings override the style)
//! and rasterises it into a small premultiplied linear-light RGBA [`Overlay`] positioned in the
//! frame. Renderers composite overlays over the finished picture.

fn srgb_to_linear(v:f32)->f32 {if v<=0.04045 {v/12.92}else{((v+0.055)/1.055).powf(2.4)}}
use super::{caption_timeline::CaptionCue as Caption,caption_graphic::{CaptionAlign,CaptionAnchor,CaptionStyle}};


use filmcraft_text::{ParagraphStyle, TextStyle, layout, render};

/// A rendered caption: premultiplied linear-light RGBA f32 pixels at `(x, y)` in the frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Overlay {
    /// Composite over a premultiplied RGBA f32 canvas of `cw`×`ch` pixels.
    pub fn composite_onto(&self, canvas: &mut [f32], cw: usize, ch: usize) {
        for y in 0..self.h {
            let cy = self.y + y as i32;
            if cy < 0 || cy >= ch as i32 {
                continue;
            }
            for x in 0..self.w {
                let cx = self.x + x as i32;
                if cx < 0 || cx >= cw as i32 {
                    continue;
                }
                let s = &self.px[(y * self.w + x) * 4..(y * self.w + x) * 4 + 4];
                if s[3] <= 0.0 {
                    continue;
                }
                let i = (cy as usize * cw + cx as usize) * 4;
                let k = 1.0 - s[3];
                for c in 0..4 {
                    canvas[i + c] = s[c] + canvas[i + c] * k;
                }
            }
        }
    }
}

/// The caption face: Inter SemiBold at `px`.
fn caption_style(px: f32) -> TextStyle {
    TextStyle { style: "SemiBold".into(), size: px, ..Default::default() }
}

fn measure(text: &str, px: f32) -> f32 {
    filmcraft_text::measure(text, &caption_style(px))
}

fn lin(c: [u8; 4]) -> [f32; 4] {
    let a = c[3] as f32 / 255.0;
    [srgb_to_linear(c[0] as f32 / 255.0) * a, srgb_to_linear(c[1] as f32 / 255.0) * a, srgb_to_linear(c[2] as f32 / 255.0) * a, a]
}

/// Word-wrap one line to `max_w` pixels.
fn wrap(line: &str, px: f32, max_w: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in line.split(' ') {
        let cand = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && measure(&cand, px) > max_w {
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = cand;
        }
    }
    out.push(cur);
    out
}

/// Placement overrides from WebVTT cue settings.
fn vtt_overrides(settings: &str) -> (Option<f32>, Option<CaptionAlign>) {
    let mut line = None;
    let mut align = None;
    for kv in settings.split_whitespace() {
        let Some((k, v)) = kv.split_once(':') else { continue };
        match k {
            "line" => {
                let v = v.split(',').next().unwrap_or("");
                if let Some(p) = v.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()) {
                    line = Some((p / 100.0).clamp(0.0, 1.0));
                }
            }
            "align" => {
                align = match v {
                    "start" | "left" => Some(CaptionAlign::Left),
                    "end" | "right" => Some(CaptionAlign::Right),
                    "center" | "middle" => Some(CaptionAlign::Center),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    (line, align)
}

/// Lay out and rasterise one caption for a `w`×`h` frame.
pub fn render_caption(c: &Caption, style: &CaptionStyle, w: usize, h: usize) -> Option<Overlay> {
    render_caption_with_admission(c,style,w,h,|_|Ok(()),&mut||Ok(())).expect("unbounded caption rendering")
}
/// Charge raster scratch before allocation and keep admission alive through rasterization.
pub(crate) fn render_caption_with_admission<R>(c:&Caption,style:&CaptionStyle,w:usize,h:usize,admit:impl FnOnce(usize)->anyhow::Result<R>,checkpoint:&mut dyn FnMut()->anyhow::Result<()>)->anyhow::Result<Option<Overlay>> {
    checkpoint()?;
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let scale = h as f32 / 1080.0;
    let px = (style.size * scale).max(4.0);
    let max_w = w as f32 * 0.9;
    let mut lines: Vec<String> = Vec::new();
    // (speaker names are metadata; like Premiere, they are not burned in)
    for l in plain_text(&c.text).lines() {
        checkpoint()?;
        lines.extend(wrap(l.trim(), px, max_w));
    }
    lines.retain(|l| !l.is_empty());
    if lines.is_empty() {
        return Ok(None);
    }
    let vm = filmcraft_text::fonts::face(filmcraft_text::resolve("Inter", "SemiBold").face).metrics(px);
    let (asc, desc) = (vm.ascent, vm.descent);
    let lh = px * style.line_spacing.max(0.8);
    let pad_x = (px * 0.3).round();
    let block_h = lh * lines.len() as f32;
    let (line_override, align_override) = vtt_overrides(&c.settings);
    let align = align_override.unwrap_or(style.align);
    let margin = style.margin.clamp(0.0, 0.45) * h as f32;
    let top = match (line_override, style.anchor) {
        (Some(f), _) => (f * h as f32).min(h as f32 - block_h),
        (None, CaptionAnchor::Top) => margin,
        (None, CaptionAnchor::Middle) => (h as f32 - block_h) / 2.0,
        (None, CaptionAnchor::Bottom) => h as f32 - margin - block_h,
    }
    .max(0.0)
    .round();
    let side = w as f32 * 0.05;
    let widths: Vec<f32> = lines.iter().map(|l| measure(l, px)).collect();
    let xs: Vec<f32> = widths
        .iter()
        .map(|&lw| match align {
            CaptionAlign::Left => side + pad_x,
            CaptionAlign::Right => w as f32 - side - pad_x - lw,
            CaptionAlign::Center => (w as f32 - lw) / 2.0,
        })
        .collect();
    let outline = (style.outline * scale).max(0.0);
    let grow = outline.ceil() + 1.0;
    let bx0 = xs.iter().zip(&widths).map(|(x, _)| x - pad_x - grow).fold(f32::MAX, f32::min).floor();
    let bx1 = xs.iter().zip(&widths).map(|(x, lw)| x + lw + pad_x + grow).fold(f32::MIN, f32::max).ceil();
    let by0 = (top - grow).floor();
    let by1 = (top + block_h + grow).ceil();
    let (ox, oy) = (bx0 as i32, by0 as i32);
    let ow = (bx1 - bx0).max(1.0) as usize;
    let oh = (by1 - by0).max(1.0) as usize;
    let count=ow.checked_mul(oh).ok_or_else(||anyhow::anyhow!("caption overlay overflow"))?;
    let side=(outline.ceil()as usize).checked_mul(2).and_then(|n|n.checked_add(1)).ok_or_else(||anyhow::anyhow!("caption outline overflow"))?;
    let offsets=if outline>0.0{side.checked_mul(side).and_then(|n|n.checked_mul(32)).ok_or_else(||anyhow::anyhow!("caption outline storage overflow"))?}else{0};
    let bytes=count.checked_mul(7*4).and_then(|n|n.checked_add(offsets)).ok_or_else(||anyhow::anyhow!("caption scratch overflow"))?;
    let _storage=admit(bytes)?;
    let mut cover = vec![0.0f32; count];
    let mut bg = vec![0.0f32; ow * oh];
    for (i, line) in lines.iter().enumerate() {
        checkpoint()?;
        let lt = top + lh * i as f32;
        if style.background && style.background_color[3] > 0 {
            let (x0, x1) = (xs[i] - pad_x, xs[i] + widths[i] + pad_x);
            let (y0, y1) = (lt, lt + lh);
            for y in (y0.floor() as i32).max(oy)..(y1.ceil() as i32).min(oy + oh as i32) {
                let fy = ((y as f32 + 1.0).min(y1) - (y as f32).max(y0)).clamp(0.0, 1.0);
                for x in (x0.floor() as i32).max(ox)..(x1.ceil() as i32).min(ox + ow as i32) {
                    let fx = ((x as f32 + 1.0).min(x1) - (x as f32).max(x0)).clamp(0.0, 1.0);
                    let j = (y - oy) as usize * ow + (x - ox) as usize;
                    bg[j] = (bg[j] + fx * fy).min(1.0);
                }
            }
        }
        let baseline = lt + (lh - (asc + desc)) / 2.0 + asc;
        let l = layout(line, &caption_style(px), &ParagraphStyle::default());
        let mut m = filmcraft_text::Mask { w: ow, h: oh, a: std::mem::take(&mut cover) };
        render::draw(&l, &render::at(xs[i], baseline.round()), &mut m, (ox, oy));
        cover = m.a;
    }
    let stroke = if outline > 0.0 { dilate(&cover, ow, oh, outline,checkpoint)? } else { Vec::new() };
    let (tc, bc, oc) = (lin(style.color), lin(style.background_color), lin(style.outline_color));
    let mut out = vec![0.0f32; ow * oh * 4];
    for j in 0..ow * oh {
        if j%4096==0{checkpoint()?;}
        let mut p = [0.0f32; 4];
        let over = |p: &mut [f32; 4], c: [f32; 4], a: f32| {
            if a > 0.0 {
                for k in 0..4 {
                    p[k] = c[k] * a + p[k] * (1.0 - c[3] * a);
                }
            }
        };
        over(&mut p, bc, bg[j]);
        if !stroke.is_empty() {
            over(&mut p, oc, stroke[j]);
        }
        over(&mut p, tc, cover[j]);
        out[j * 4..j * 4 + 4].copy_from_slice(&p);
    }
    Ok(Some(Overlay { x: ox, y: oy, w: ow, h: oh, px: out }))
}

/// Max-filter a coverage mask with a disc of radius `r`.
fn dilate(m: &[f32], w: usize, h: usize, r: f32,checkpoint:&mut dyn FnMut()->anyhow::Result<()>) -> anyhow::Result<Vec<f32>> {
    let ri = r.ceil() as i32;
    let mut offs = Vec::new();
    for dy in -ri..=ri {
        for dx in -ri..=ri {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d <= r + 0.5 {
                offs.push((dx, dy, (r + 0.5 - d).clamp(0.0, 1.0)));
            }
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h as i32 {
        checkpoint()?;
        for x in 0..w as i32 {
            let mut v = 0.0f32;
            for &(dx, dy, wgt) in &offs {
                let (sx, sy) = (x + dx, y + dy);
                if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                    v = v.max(m[sy as usize * w + sx as usize] * wgt);
                }
            }
            out[y as usize * w + x as usize] = v;
        }
    }
    Ok(out)
}


pub fn plain_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => {
                // a tag runs to the next '>' (if none, keep the '<' literally)
                let rest: String = chars.clone().collect();
                if let Some(end) = rest.find('>') {
                    for _ in 0..rest[..=end].chars().count() {
                        chars.next();
                    }
                } else {
                    out.push('<');
                }
            }
            '{' if chars.peek() == Some(&'\\') => {
                // SSA-style override blocks some SRT files carry: {\an8}
                let rest: String = chars.clone().collect();
                if let Some(end) = rest.find('}') {
                    for _ in 0..rest[..=end].chars().count() {
                        chars.next();
                    }
                } else {
                    out.push('{');
                }
            }
            '&' => {
                let rest: String = chars.clone().take(8).collect();
                let ents = [
                    ("amp;", '&'),
                    ("lt;", '<'),
                    ("gt;", '>'),
                    ("nbsp;", '\u{a0}'),
                    ("quot;", '"'),
                    ("apos;", '\''),
                    ("lrm;", '\u{200e}'),
                    ("rlm;", '\u{200f}'),
                ];
                if let Some((name, ch)) = ents.iter().find(|(n, _)| rest.starts_with(n)) {
                    for _ in 0..name.len() {
                        chars.next();
                    }
                    out.push(*ch);
                } else {
                    out.push('&');
                }
            }
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}


pub fn render_tracks(composition:&super::composition::Composition,time_us:u64,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->anyhow::Result<()>)->anyhow::Result<()> {
    render_tracks_at(composition,super::caption_time::CaptionTime::Microseconds(time_us).exact_ticks(),frame,checkpoint)
}
pub fn render_tracks_at(composition:&super::composition::Composition,time:i128,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->anyhow::Result<()>)->anyhow::Result<()> {
    let (width,height)=frame.dimensions();
    for track in composition.caption_tracks.iter().rev() {
        if track.hidden {continue}
        let Some(cue)=track.clips.iter().rev().find(|cue|cue.timeline_start_tick.exact_ticks()<=time&&time<cue.timeline_end_tick.exact_ticks()) else {continue};
        checkpoint()?;
        if let Some(overlay)=render_caption(cue,&track.style,width,height) {overlay.composite_working(frame,checkpoint)?;}
    }
    Ok(())
}
impl Overlay {
    pub fn composite_working(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->anyhow::Result<()>)->anyhow::Result<()> {
        let (width,_)=frame.dimensions();let mut index=0usize;
        frame.map_premultiplied(&mut |mut pixel|{
            let (x,y)=((index%width)as i64-self.x as i64,(index/width)as i64-self.y as i64);index+=1;
            if x>=0&&y>=0&&(x as usize)<self.w&&(y as usize)<self.h {
                let source=&self.px[(y as usize*self.w+x as usize)*4..][..4];
                if source[3]>0. {for k in 0..4 {pixel[k]=source[k]+pixel[k]*(1.-source[3]);}}
            }
            pixel
        },checkpoint)
    }
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn original_caption_renderer_overlay_bits_and_bounds() {
  let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-render.json")).unwrap();
  let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),32);
  for (index,case) in cases.iter().enumerate() {
   let style:CaptionStyle=serde_json::from_value(case["style"].clone()).unwrap();
   let cue=Caption{cue_id:None,id:super::super::composition::CompositionClipId::parse("cue").unwrap(),timeline_start_tick:(0_u64).into(),timeline_end_tick:(1000000_u64).into(),text:case["text"].as_str().unwrap().into(),speaker:Some("Narrator".into()),settings:case["settings"].as_str().unwrap().into()};
   let actual=render_caption(&cue,&style,case["width"].as_u64().unwrap() as usize,case["height"].as_u64().unwrap() as usize);
   if case["overlay"].is_null(){assert!(actual.is_none(),"case{index}");continue}
   let overlay=actual.unwrap();let expected=&case["overlay"];
   assert_eq!((overlay.x,overlay.y,overlay.w,overlay.h),(expected["x"].as_i64().unwrap()as i32,expected["y"].as_i64().unwrap()as i32,expected["w"].as_u64().unwrap()as usize,expected["h"].as_u64().unwrap()as usize),"bounds{index}");
   let bits:Vec<u32>=serde_json::from_value(expected["bits"].clone()).unwrap();assert_eq!(overlay.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),bits,"working pixels{index}");
   let width=case["width"].as_u64().unwrap() as usize;let height=case["height"].as_u64().unwrap() as usize;
   let surface=super::super::group_surface::GroupSurface{width,height,rgba:[32,64,128,128].repeat(width*height)};
   let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&surface,&mut||Ok(())).unwrap();
   let mut pixels=frame.pixels().iter().flatten().copied().collect::<Vec<_>>();overlay.composite_onto(&mut pixels,width,height);
   overlay.composite_working(&mut frame,&mut||Ok(())).unwrap();
   assert_eq!(frame.pixels().iter().flatten().copied().collect::<Vec<_>>(),pixels,"composition{index}");
  }
 }
}
