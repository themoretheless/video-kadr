//! Filmcraft original Wipe soft-edge mask, MIT notice docs/licenses/filmcraft-MIT.txt.
use super::linear_color_frame::LinearColorFrame;
use anyhow::{Result,ensure};

/// Direction uses the original choice order: north, east, south, west.
pub fn render(a:&LinearColorFrame,b:&LinearColorFrame,progress:f32,reverse:bool,direction:u32,
    checkpoint:&mut dyn FnMut()->Result<()>) -> Result<LinearColorFrame> {
    ensure!(direction<4,"invalid wipe direction");
    let (a,b,p)=if reverse{(b,a,1.-progress)}else{(a,b,progress)};
    let p=if p.is_finite(){p.clamp(0.,1.)}else{0.};
    if a.dimensions()!=b.dimensions(){return Ok(if p<0.5{a.clone()}else{b.clone()});}
    if p<=0.{return Ok(a.clone());} if p>=1.{return Ok(b.clone());}
    let (width,height)=a.dimensions();let mut out=a.clone();let mut index=0;
    out.map_premultiplied(&mut|pixel|{
        let u=((index%width) as f32+0.5)/width as f32;
        let v=((index/width) as f32+0.5)/height as f32;
        let step=|edge:f32,x:f32|((x-edge)/0.004+0.5).clamp(0.,1.);
        let k=match direction{0=>1.-step(p,v),1=>step(1.-p,u),2=>step(1.-p,v),_=>1.-step(p,u)};
        let other=b.pixels()[index];index+=1;
        let k=k.clamp(0.,1.);
        std::array::from_fn(|c|pixel[c]*(1.-k)+other[c]*k)
    },checkpoint)?;
    Ok(out)
}

#[cfg(test)]mod tests{
    use super::*;
    use crate::domain::group_surface::GroupSurface;
    #[test]fn original_wipe_four_directions_672_pixel_cases(){
        let oracle:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-wipe-transition-pixels.json")).unwrap();
        assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");
        let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),672);
        for case in cases{
            let frame=|key:&str|LinearColorFrame::from_surface(&GroupSurface{
                width:case["width"].as_u64().unwrap() as usize,height:case["height"].as_u64().unwrap() as usize,
                rgba:serde_json::from_value(case[key].clone()).unwrap(),
            },&mut||Ok(())).unwrap();
            let actual=render(&frame("a"),&frame("b"),case["progress"].as_f64().unwrap() as f32,
                case["reverse"].as_bool().unwrap(),case["direction"].as_u64().unwrap() as u32,&mut||Ok(())).unwrap();
            let bits:Vec<u32>=actual.pixels().iter().flatten().map(|v|v.to_bits()).collect();
            assert_eq!(bits,serde_json::from_value::<Vec<u32>>(case["bits"].clone()).unwrap(),"{case}");
        }
    }
}
