// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors
// Original pinned segment.rs person method and helpers; imports/visibility adapted.
use crate::object_matting::reference::Mask;
use crate::Frame;
use std::{path::Path, sync::Mutex};
const BYTES_PER_PIXEL: usize = 4;
struct Data(Vec<f32>);
impl From<Vec<f32>> for Data {
    fn from(v: Vec<f32>) -> Self {
        Self(v)
    }
}
struct Input {
    name: &'static str,
    dims: Vec<usize>,
    data: Data,
}
struct Output {
    dims: Vec<usize>,
    data: Vec<f32>,
}
struct Model(ort::session::Session, Vec<f32>);
impl Model {
    fn from_file(path: &Path) -> Result<Self, String> {
        (move || -> anyhow::Result<Self> {
            Ok(Self(
                ort::session::Session::builder()?
                    .with_intra_threads(2)
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .commit_from_file(path)?,
                Vec::new(),
            ))
        })()
        .map_err(|e| e.to_string())
    }
    fn run(&mut self, mut inputs: Vec<Input>, names: &[&str]) -> Result<Vec<Output>, String> {
        (move||->anyhow::Result<Vec<Output>>{let mut take=|name:&str|->anyhow::Result<ort::value::Tensor<f32>>{let i=inputs.iter().position(|input|input.name==name).ok_or_else(||anyhow::anyhow!("missing reference input"))?;let input=inputs.remove(i);Ok(ort::value::Tensor::from_array((input.dims,input.data.0.into_boxed_slice()))?)};let src=take("src")?;let r1=take("r1i")?;let r2=take("r2i")?;let r3=take("r3i")?;let r4=take("r4i")?;let ratio=take("downsample_ratio")?;let outputs=self.0.run(ort::inputs!["src"=>src,"r1i"=>r1,"r2i"=>r2,"r3i"=>r3,"r4i"=>r4,"downsample_ratio"=>ratio])?;self.1=outputs["pha"].try_extract_tensor::<f32>()?.1.to_vec();names.iter().map(|name|{let(shape,data)=outputs[*name].try_extract_tensor::<f32>()?;Ok(Output{dims:shape.iter().map(|v|*v as usize).collect(),data:data.to_vec()})}).collect()})().map_err(|e|e.to_string())
    }
}
pub struct Person {
    model: Mutex<Model>,
    state: Mutex<Option<Vec<Output>>>,
}
impl Person {
    pub fn alpha(&self) -> Vec<f32> {
        self.model.lock().unwrap().1.clone()
    }
    pub fn state_bits(&self) -> Vec<(Vec<usize>, Vec<u32>)> {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| (s.dims.clone(), s.data.iter().map(|v| v.to_bits()).collect()))
            .collect()
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        Ok(Self {
            model: Mutex::new(Model::from_file(path)?),
            state: Mutex::new(None),
        })
    }
    pub fn begin(&self) {
        *self.state.lock().unwrap() = None;
    }
    pub fn mask(&self, frame: &Frame, w: u32, h: u32) -> Result<Mask, String> {
        self.person_mask(planes(frame, w, h), w, h)
    }
    fn run(&self, inputs: Vec<Input>, names: &[&str]) -> Result<Vec<Output>, String> {
        self.model
            .lock()
            .map_err(|_| "reference poisoned".to_owned())?
            .run(inputs, names)
    }
    pub fn person_mask(&self, planes: Vec<f32>, width: u32, height: u32) -> Result<Mask, String> {
        let size = (width * height) as usize;
        let mut inputs = vec![Input {
            name: "src",
            dims: vec![1, 3, height as usize, width as usize],
            data: planes.into(),
        }];
        let carried = self
            .state
            .lock()
            .map_err(|_| "cutout state poisoned".to_owned())?
            .take();
        const STATES: [&str; 4] = ["r1i", "r2i", "r3i", "r4i"];
        match carried {
            Some(state) if state.len() == 4 => {
                for (name, output) in STATES.iter().zip(state) {
                    inputs.push(Input {
                        name,
                        dims: output.dims,
                        data: output.data.into(),
                    });
                }
            }
            _ => {
                for name in STATES {
                    inputs.push(Input {
                        name,
                        dims: vec![1, 1, 1, 1],
                        data: vec![0.0f32].into(),
                    });
                }
            }
        }
        // The frame is already at the working size; the model's own
        // downsample stays at one.
        inputs.push(Input {
            name: "downsample_ratio",
            dims: vec![1],
            data: vec![1.0f32].into(),
        });
        let mut outputs = self.run(inputs, &["pha", "r1o", "r2o", "r3o", "r4o"])?;
        let alpha = outputs.remove(0);
        if let Ok(mut state) = self.state.lock() {
            *state = Some(outputs);
        }
        probabilities(&alpha.data, size, width, height)
    }
}
pub fn planes(frame: &Frame, width: u32, height: u32) -> Vec<f32> {
    let (fw, fh) = (frame.width() as usize, frame.height() as usize);
    let (width, height) = (width as usize, height as usize);
    let pixels = frame.pixels();
    let mut data = vec![0f32; 3 * width * height];
    for c in 0..3 {
        for y in 0..height {
            let sy = (y * fh / height).min(fh - 1);
            for x in 0..width {
                let sx = (x * fw / width).min(fw - 1);
                data[(c * height + y) * width + x] =
                    f32::from(pixels[(sy * fw + sx) * BYTES_PER_PIXEL + c]) / 255.0;
            }
        }
    }
    data
}

/// A model's `0..=1` answer as a mask.
fn probabilities(data: &[f32], size: usize, width: u32, height: u32) -> Result<Mask, String> {
    let bytes: Vec<u8> = data
        .iter()
        .take(size)
        .map(|&p| (p.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
        .collect();
    Mask::from_bytes(width, height, bytes)
        .ok_or_else(|| "cutout: the model's answer is the wrong size".to_owned())
}
