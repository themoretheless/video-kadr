//! Filmcraft Horizontal/Vertical Flip; MIT attribution docs/licenses/filmcraft-MIT.txt.
use anyhow::Result;
pub fn apply_linear(
    frame: &mut super::linear_color_frame::LinearColorFrame,
    horizontal: bool,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    let (w, h) = frame.dimensions();
    let source = frame.pixels().to_vec();
    let mut index = 0;
    frame.map_premultiplied(
        &mut |_| {
            let (x, y) = (index % w, index / w);
            index += 1;
            source[if horizontal {
                y * w + w - 1 - x
            } else {
                (h - 1 - y) * w + x
            }]
        },
        checkpoint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flip_original_working_bits_and_cancellation() {
        #[derive(serde::Deserialize)]
        struct Case {
            effects: Vec<String>,
            width: usize,
            height: usize,
            rgba: Vec<u8>,
            bits: Vec<u32>,
        }
        #[derive(serde::Deserialize)]
        struct Fixture {
            cases: Vec<Case>,
        }
        let f: Fixture =
            serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-flip.json")).unwrap();
        assert_eq!(f.cases.len(), 60);
        for (i, c) in f.cases.into_iter().enumerate() {
            let mut frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
                &super::super::group_surface::GroupSurface {
                    width: c.width,
                    height: c.height,
                    rgba: c.rgba,
                },
                &mut || Ok(()),
            )
            .unwrap();
            for effect in &c.effects {
                apply_linear(&mut frame, effect == "horizontal_flip", &mut || Ok(())).unwrap();
            }
            assert_eq!(
                frame
                    .pixels()
                    .iter()
                    .flatten()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                c.bits,
                "original flip {i}"
            );
            assert!(apply_linear(&mut frame, true, &mut || anyhow::bail!("cancelled")).is_err());
        }
    }
}
