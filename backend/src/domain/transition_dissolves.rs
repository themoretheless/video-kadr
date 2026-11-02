//! Original Filmcraft legacy dissolve family, MIT (docs/licenses/filmcraft-MIT.txt).
use super::linear_color_frame::LinearColorFrame;
use anyhow::Result;

#[derive(Clone, Copy)]
pub enum DissolveStyle { Cross, Film, Additive, Black, White }

pub fn render(a: &LinearColorFrame, b: &LinearColorFrame, progress: f32, reverse: bool,
    style: DissolveStyle, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<LinearColorFrame> {
    let (a,b,p) = if reverse { (b,a,1.-progress) } else { (a,b,progress) };
    let p = if p.is_finite() { p.clamp(0.,1.) } else { 0. };
    if a.dimensions() != b.dimensions() { return Ok(if p < 0.5 { a.clone() } else { b.clone() }); }
    if p <= 0. { return Ok(a.clone()); }
    if p >= 1. { return Ok(b.clone()); }
    let mut out = a.clone();
    let mut index = 0;
    out.map_premultiplied(&mut |pixel| {
        let other = b.pixels()[index]; index += 1;
        match style {
            DissolveStyle::Cross => std::array::from_fn(|c| pixel[c]*(1.-p)+other[c]*p),
            DissolveStyle::Film => std::array::from_fn(|c| {
                let g = 1. / 2.2;
                let v = pixel[c].max(0.).powf(g)*(1.-p)+other[c].max(0.).powf(g)*p;
                v.powf(2.2)
            }),
            DissolveStyle::Additive => {
                let (ka,kb) = ((2.*(1.-p)).min(1.),(2.*p).min(1.));
                std::array::from_fn(|c| (pixel[c]*ka+other[c]*kb).min(1.))
            },
            DissolveStyle::Black | DissolveStyle::White => {
                let color = if matches!(style,DissolveStyle::Black) { 0. } else { 1. };
                let (source,k) = if p < 0.5 { (pixel,1.-p*2.) } else { (other,(p-0.5)*2.) };
                let scaled = if (k-1.).abs() < 1e-6 { source } else { source.map(|v|v*k) };
                if scaled[3] <= 0. { return [color,color,color,1.]; }
                [scaled[0]+color*(1.-scaled[3]),scaled[1]+color*(1.-scaled[3]),
                 scaled[2]+color*(1.-scaled[3]),scaled[3]+(1.-scaled[3])]
            },
        }
    }, checkpoint)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]
    fn original_dissolve_family_all_pixel_bits() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-all-transition-pixels.json")).unwrap();
        assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let mut count = 0;
        for case in oracle["cases"].as_array().unwrap() {
            let style = match case["effect"].as_str().unwrap() {
                "cross_dissolve" | "cross_dissolve_legacy" | "morph_cut" => DissolveStyle::Cross,
                "film_dissolve" | "film_dissolve_legacy" => DissolveStyle::Film,
                "additive_dissolve" | "additive_dissolve_legacy" => DissolveStyle::Additive,
                "dip_to_black" | "dip_to_black_legacy" => DissolveStyle::Black,
                "dip_to_white" | "dip_to_white_legacy" => DissolveStyle::White,
                _ => continue,
            };
            let frame = |key: &str| LinearColorFrame::from_surface(&GroupSurface {
                width:case["width"].as_u64().unwrap() as usize,height:case["height"].as_u64().unwrap() as usize,
                rgba:serde_json::from_value(case[key].clone()).unwrap(),
            },&mut || Ok(())).unwrap();
            let actual = render(&frame("a"),&frame("b"),case["progress"].as_f64().unwrap() as f32,
                case["reverse"].as_bool().unwrap(),style,&mut || Ok(())).unwrap();
            let bits: Vec<u32> = actual.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,serde_json::from_value::<Vec<u32>>(case["bits"].clone()).unwrap(),"{case}");
            count += 1;
        }
        assert_eq!(count,11*168);
    }
}
