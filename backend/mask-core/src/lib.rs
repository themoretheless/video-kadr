//! Bounded mask math shared by HTTP backend and native model runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}
impl Mask {
    pub fn is_blank(&self) -> bool {
        self.bytes.iter().all(|value| *value < 10)
    }
}

pub fn sample(mask: &Mask, u: f32, v: f32) -> f32 {
    let x = u.clamp(0., 1.) * mask.width as f32 - 0.5;
    let y = v.clamp(0., 1.) * mask.height as f32 - 0.5;
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let at = |x: i64, y: i64| {
        f32::from(
            mask.bytes[y.clamp(0, mask.height as i64 - 1) as usize * mask.width
                + x.clamp(0, mask.width as i64 - 1) as usize],
        )
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let upper = at(x0, y0) * (1. - fx) + at(x0 + 1, y0) * fx;
    let lower = at(x0, y0 + 1) * (1. - fx) + at(x0 + 1, y0 + 1) * fx;
    (upper * (1. - fy) + lower * fy) / 255.
}

pub mod mask_edit;
