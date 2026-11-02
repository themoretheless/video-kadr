//! Instrumented coordinate arithmetic only; never an unmodified Pixelate parity oracle.
//! Removing sampling can change Metal optimization; use only to reject simple arithmetic hypotheses.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn replace_effect(source: &str, body: &str) -> String {
    let start = source.find("fn effect(").unwrap();
    let open = start + source[start..].find('{').unwrap();
    let mut depth = 1;
    let mut end = open + 1;
    for (i, c) in source[open + 1..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            end = open + 1 + i + 1;
            break;
        }
    }
    assert_eq!(depth, 0);
    format!(
        "{}fn effect(uv: vec2<f32>) -> vec4<f32> {{ {} }}{}",
        &source[..start],
        body,
        &source[end..]
    )
}
fn main() {
 let output=std::env::args().nth(1).unwrap();
 let package=Catalogue::builtin().get("concat.pixelate").unwrap();
 let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());assert_eq!(format!("{:?}",gpu.adapter_info().backend),"Metal");
 let mut cases=Vec::new();
 for numerator in 2..=64 {
  for expression in ["uv.x", "p.x", "fract(p).x"] {
   let mut pass=package.pass(&BTreeMap::from([("size".into(),numerator as f64)]),None).unwrap();
   let source=replace_effect(&pass.source,"return sample_premultiplied(vec2<f32>(params.size*0.125)/frame.size);");
   let start=source.find("fn sample_premultiplied(").unwrap();
   let open=start+source[start..].find('{').unwrap();let mut end=open+1;let mut depth=1;
   for (i,c) in source[open+1..].char_indices(){if c=='{'{depth+=1;}if c=='}'{depth-=1;}if depth==0{end=open+1+i+1;break;}}
   let body=format!("let p=uv*frame.size-vec2<f32>(0.5);let b=bitcast<u32>({expression});return vec4<f32>(f32(b&255u),f32((b>>8u)&255u),f32((b>>16u)&255u),f32((b>>24u)&255u));");
   pass.source=format!("{}{}{}",&source[..open+1],body,&source[end-1..]).into();pass.key=format!("pixelate-coordinate-diagnostic-{numerator}-{expression}").into();
   let got=gpu.probe(&[pass],[0.,0.,0.,0.5],13,0.).unwrap();let bits=got.iter().enumerate().fold(0u32,|v,(i,b)|{assert_eq!(*b,b.round());v|(*b as u32)<<(i*8)});
   cases.push(json!({"numerator":numerator,"expression":expression,"bits":bits,"value":f32::from_bits(bits)}));
  }
 }
 std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"instrumented":true,"upstreamProbes":0,"cases":cases})).unwrap()).unwrap();
}
