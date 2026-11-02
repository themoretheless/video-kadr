//! Filmcraft Black & White: direct linear Rec709 luminance (MIT, pinned gpufx.rs).
use anyhow::Result;
pub fn gray(input:[f32;3])->f32 {0.2126*input[0]+0.7152*input[1]+0.0722*input[2]}
pub fn apply_linear(frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
    frame.map_premultiplied(&mut |mut pixel|{
        let alpha=pixel[3];
        if alpha>1e-6 {let value=gray([pixel[0]/alpha,pixel[1]/alpha,pixel[2]/alpha])*alpha;pixel[..3].fill(value);}
        pixel
    },checkpoint)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_cpu_rgba_bits() {
        #[derive(serde::Deserialize)]
        struct Case {input:[f32;4],bits:[u32;4]}
        #[derive(serde::Deserialize)]
        struct Oracle {commit:String,cases:Vec<Case>}
        let oracle:Oracle=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-black-white.json")).unwrap();
        assert_eq!(oracle.commit,"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(oracle.cases.len(),9);
        for (index,case) in oracle.cases.iter().enumerate() {
            let mut actual=case.input;let alpha=actual[3];
            if alpha>1e-6 {let value=gray([actual[0]/alpha,actual[1]/alpha,actual[2]/alpha])*alpha;actual[..3].fill(value);}
            assert_eq!(actual.map(f32::to_bits),case.bits,"original Black & White case {index}");
        }
    }
    #[test]
    fn source_primaries_alpha_and_hidden_pixels() {
        assert_eq!(gray([1.0,0.0,0.0]),0.2126);assert_eq!(gray([0.0,1.0,0.0]),0.7152);assert_eq!(gray([0.0,0.0,1.0]),0.0722);
        let image=super::super::group_surface::GroupSurface{width:2,height:1,rgba:vec![255,0,0,128,12,34,56,0]};
        let mut frame=super::super::linear_color_frame::LinearColorFrame::from_surface(&image,&mut||Ok(())).unwrap();
        apply_linear(&mut frame,&mut||Ok(())).unwrap();let output=frame.to_surface(&mut||Ok(())).unwrap();
        assert_eq!(output.rgba,vec![127,127,127,128,12,34,56,0]);
    }
}
