//! Shared source-coordinate mask correction; model runtime keeps its existing API.
#[cfg(test)]
use crate::object_matting::Mask;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
pub use video_kadr_mask_core::mask_edit::*;
#[cfg(test)]
#[allow(dead_code)]
#[path = "reference_mask_edit.rs"]
mod reference;
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn mask(w: usize, h: usize) -> Mask {
        Mask {
            width: w,
            height: h,
            bytes: (0..w * h).map(|i| ((i * 73 + i / 7) % 256) as u8).collect(),
        }
    }
    fn original(m: &Mask) -> reference::Mask {
        reference::Mask::from_bytes(m.width as u32, m.height as u32, m.bytes.clone()).unwrap()
    }
    #[test]
    fn corrections_and_feather_match_original_all_bytes() {
        let cancel = AtomicBool::new(false);
        for (w, h) in [(1, 1), (13, 7), (29, 41), (512, 512)] {
            let auto = mask(w, h);
            let region = mask(9, 17);
            let region_ref = Arc::new(original(&region));
            let tools = [
                Tool::Brush,
                Tool::SmartEraser,
                Tool::Eraser,
                Tool::SmartBrush,
            ];
            let strokes: Vec<_> = tools
                .iter()
                .enumerate()
                .map(|(i, tool)| Stroke {
                    tool: *tool,
                    size: [0.005, 0.1, 0.5, 0.25][i],
                    points: vec![[-0.4, 0.1], [0.2, 0.4], [1.4, 0.7]],
                    at: Some(1.2),
                    region: None,
                })
                .collect();
            for aspect in [0.5, 1., 2., f32::NAN] {
                for learned in [false, true] {
                    let regions = strokes
                        .iter()
                        .map(|_| if learned { Some(&region) } else { None })
                        .collect::<Vec<_>>();
                    let expected = reference::paint(&original(&auto), &strokes, aspect, &|_| {
                        learned.then(|| region_ref.clone())
                    });
                    let actual = paint(&auto, &strokes, aspect, &regions, &cancel).unwrap();
                    assert_eq!(
                        actual.bytes,
                        expected.bytes(),
                        "{w}x{h} aspect={aspect} learned={learned}"
                    );
                    for radius in [0., 0.49, 0.5, 1., 2.4, 8.] {
                        assert_eq!(
                            feather(&actual, radius, &cancel).unwrap().bytes,
                            expected.blurred(radius).bytes(),
                            "radius={radius}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn blank_custom_eraser_tidy_and_transactional_refusal() {
        let cancel = AtomicBool::new(false);
        let auto = Mask {
            width: 8,
            height: 8,
            bytes: vec![0; 64],
        };
        assert!(resolve(&auto, false, &[], &[], 1., 0.01, &cancel)
            .unwrap()
            .is_none());
        let stroke = Stroke {
            tool: Tool::Eraser,
            size: 0.5,
            points: vec![[0.5, 0.5]],
            at: None,
            region: None,
        };
        let blank = resolve(
            &auto,
            true,
            std::slice::from_ref(&stroke),
            &[],
            1.,
            0.01,
            &cancel,
        )
        .unwrap()
        .unwrap();
        assert!(blank.is_blank());
        let source = mask(19, 11);
        let before = source.clone();
        let stopped = AtomicBool::new(true);
        assert!(paint(&source, std::slice::from_ref(&stroke), 1., &[], &stopped).is_err());
        assert!(feather(&source, 2., &stopped).is_err());
        assert_eq!(source, before);
        let mut bad = stroke;
        bad.points[0][0] = f64::NAN;
        assert!(paint(&source, &[bad], 1., &[], &cancel).is_err());
        assert!(feather(&source, f32::INFINITY, &cancel).is_err());
        assert!(resolve(&source, true, &[], &[], 1., 0.11, &cancel).is_err());
        let tidy = Stroke {
            tool: Tool::Brush,
            size: f64::NAN,
            at: Some(-3.),
            region: None,
            points: vec![[f64::NAN, 0.], [2., -1.]],
        }
        .tidy()
        .unwrap();
        assert_eq!(tidy.size, 0.005);
        assert_eq!(tidy.at, Some(0.));
        assert_eq!(tidy.points, vec![[1.5, -0.5]]);
        assert!(paint(
            &Mask {
                width: 1,
                height: 1,
                bytes: vec![]
            },
            &[],
            1.,
            &[],
            &cancel
        )
        .is_err());
    }
    #[test]
    fn smart_fallback_uses_original_confidence_and_work_is_bounded() {
        let auto = Mask {
            width: 2,
            height: 1,
            bytes: vec![37, 218],
        };
        let cancel = AtomicBool::new(false);
        let strokes = vec![
            Stroke {
                tool: Tool::Brush,
                size: 0.5,
                points: vec![[0.25, 0.5], [0.75, 0.5]],
                at: None,
                region: None,
            },
            Stroke {
                tool: Tool::SmartEraser,
                size: 0.5,
                points: vec![[0.25, 0.5], [0.75, 0.5]],
                at: None,
                region: None,
            },
        ];
        assert_eq!(
            paint(&auto, &strokes, 1., &[], &cancel).unwrap().bytes,
            vec![0, 255]
        );
        let auto = Mask {
            width: 512,
            height: 512,
            bytes: vec![128; 512 * 512],
        };
        let stroke = Stroke {
            tool: Tool::Brush,
            size: 0.5,
            points: vec![[0.5, 0.5]; 4096],
            at: None,
            region: None,
        };
        let before = auto.clone();
        assert!(paint(&auto, &[stroke], 1., &[], &cancel)
            .unwrap_err()
            .to_string()
            .contains("budget"));
        assert_eq!(auto, before);
    }
}
