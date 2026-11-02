//! Capture responsive distances from native evaluated (including animated/pinned) layer bounds.
use super::{
    graphic_clip::GraphicClip,
    graphic_pin::{Pin, PinTarget},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicPinCapture {
    pub graphic: GraphicClip,
    pub width: u32,
    pub height: u32,
    pub local_us: i64,
    pub uid: u64,
    pub to: PinTarget,
    pub edges: [bool; 4],
}
impl GraphicPinCapture {
    pub fn execute(&self) -> Result<Pin> {
        ensure!(
            self.width > 0 && self.height > 0 && self.width <= 32768 && self.height <= 32768,
            "invalid graphic canvas"
        );
        ensure!(self.graphic.layers.len() <= 512, "too many graphic layers");
        ensure!(self.uid != 0, "pin capture requires a stable layer uid");
        let layers = self.graphic.evaluated_layers_signed(
            (self.width, self.height),
            i128::from(self.local_us),
            &mut || Ok(()),
        )?;
        let own = layers
            .iter()
            .find(|layer| layer.uid == self.uid)
            .ok_or_else(|| anyhow::anyhow!("graphic layer not found"))?
            .bounds()?;
        let target = match self.to {
            PinTarget::Frame => [0., 0., self.width as f64, self.height as f64],
            PinTarget::Layer(uid) => {
                ensure!(uid != 0 && uid != self.uid, "invalid pin target");
                layers
                    .iter()
                    .find(|layer| layer.uid == uid)
                    .ok_or_else(|| anyhow::anyhow!("pin target not found"))?
                    .bounds()?
            }
        };
        let pin = Pin::new(self.to, self.edges, own, target);
        pin.validate()?;
        Ok(pin)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graphic_clip::{GraphicContent, GraphicLayer};
    use crate::domain::graphic_shape::GraphicShape;
    #[test]
    fn captures_native_bounds_of_hidden_target_and_rejects_invalid_identity() {
        let graphic = GraphicClip {
            layers: vec![
                GraphicLayer {
                    original_parameters: None,
                    uid: 1,
                    enabled: true,
                    pin: None,
                    content: GraphicContent::Shape {
                        parameters: GraphicShape {
                            position: Some([20., 30.]),
                            size: [10., 8.],
                            ..Default::default()
                        },
                        animation: None,
                    },
                },
                GraphicLayer {
                    original_parameters: None,
                    uid: 2,
                    enabled: false,
                    pin: None,
                    content: GraphicContent::Shape {
                        parameters: GraphicShape {
                            position: Some([40., 50.]),
                            size: [20., 12.],
                            ..Default::default()
                        },
                        animation: None,
                    },
                },
            ],
        };
        let mut request = GraphicPinCapture {
            graphic,
            width: 100,
            height: 80,
            local_us: 0,
            uid: 1,
            to: PinTarget::Layer(2),
            edges: [true; 4],
        };
        let evaluated = request
            .graphic
            .evaluated_layers((100, 80), 0, &mut || Ok(()))
            .unwrap();
        let own = evaluated[0].bounds().unwrap();
        let target = evaluated[1].bounds().unwrap();
        assert_eq!(
            request.execute().unwrap(),
            Pin::new(PinTarget::Layer(2), [true; 4], own, target)
        );
        request.to = PinTarget::Layer(1);
        assert!(request.execute().is_err());
        request.to = PinTarget::Layer(99);
        assert!(request.execute().is_err());
        request.to = PinTarget::Frame;
        assert_eq!(
            request.execute().unwrap(),
            Pin::new(PinTarget::Frame, [true; 4], own, [0., 0., 100., 80.])
        );
        request.width = 0;
        assert!(request.execute().is_err());
    }
}
