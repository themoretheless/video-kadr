//! Test-only adapter for the pinned upstream brush oracle; no production use.
use std::path::Path;
pub enum Data {
    Float(Vec<f32>),
    Int(Vec<i64>),
}
impl From<Vec<f32>> for Data {
    fn from(v: Vec<f32>) -> Self {
        Self::Float(v)
    }
}
impl From<Vec<i64>> for Data {
    fn from(v: Vec<i64>) -> Self {
        Self::Int(v)
    }
}
pub struct Input {
    pub name: &'static str,
    pub dims: Vec<usize>,
    pub data: Data,
}
pub struct Output {
    pub dims: Vec<usize>,
    pub data: Vec<f32>,
}
pub struct Model(ort::session::Session);
impl Model {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        (move || -> anyhow::Result<Self> {
            Ok(Self(
                ort::session::Session::builder()?
                    .with_intra_threads(2)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?
                    .commit_from_file(path)?,
            ))
        })()
        .map_err(|e| e.to_string())
    }
    pub fn run(&mut self, mut inputs: Vec<Input>, names: &[&str]) -> Result<Vec<Output>, String> {
        (move||->anyhow::Result<Vec<Output>>{
 let take=|inputs:&mut Vec<Input>,name:&str|->anyhow::Result<Input>{let i=inputs.iter().position(|v|v.name==name).ok_or_else(||anyhow::anyhow!("missing input"))?;Ok(inputs.remove(i))};
 let floats=|input:Input|->anyhow::Result<ort::value::Tensor<f32>>{let Data::Float(v)=input.data else {anyhow::bail!("expected floats")};Ok(ort::value::Tensor::from_array((input.dims,v.into_boxed_slice()))?)};
 let outputs=if names==["image_embeddings","image_positional_embeddings"] {let input=floats(take(&mut inputs,"pixel_values")?)?;self.0.run(ort::inputs!["pixel_values"=>input])?}else{
 let points=floats(take(&mut inputs,"input_points")?)?;let input=take(&mut inputs,"input_labels")?;let Data::Int(labels)=input.data else{anyhow::bail!("expected labels")};let labels=ort::value::Tensor::from_array((input.dims,labels.into_boxed_slice()))?;
 let image=floats(take(&mut inputs,"image_embeddings")?)?;let position=floats(take(&mut inputs,"image_positional_embeddings")?)?;
 self.0.run(ort::inputs!["input_points"=>points,"input_labels"=>labels,"image_embeddings"=>image,"image_positional_embeddings"=>position])?
 };
 names.iter().map(|name|{let(shape,data)=outputs[*name].try_extract_tensor::<f32>()?;Ok(Output{dims:shape.iter().map(|v|*v as usize).collect(),data:data.to_vec()})}).collect()
 })().map_err(|e|e.to_string())
    }
}
