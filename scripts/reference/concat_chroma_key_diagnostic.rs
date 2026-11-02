//! Instrumented helper-bit readback only; never an unmodified parity oracle.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main(){
 let output=std::env::args().nth(1).unwrap();let package=Catalogue::builtin().get("concat.chroma-key").unwrap();
 let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());assert_eq!(format!("{:?}",gpu.adapter_info().backend),"Metal");
 let pass=package.pass(&BTreeMap::from([("spill".into(),0.)]),None).unwrap();let mut cases=Vec::new();
 for input in [[0.1471329778432846f32,0.344148725271225,0.1050431877374649,1.],[0.2,0.28,0.2,1.]] {
  for expression in ["c.g","here.x","here.y","y","rgb.g","original_from_display(rgb).g"] {
   let mut p=pass.clone();p.key=format!("{}-{input:?}-{expression}",p.key);
   let old="return vec4<f32>(rgb, c.a * keep);";assert!(p.source.contains(old));
   let replacement=format!("let b=bitcast<u32>({expression}); return vec4<f32>(f32(b & 255u),f32((b >> 8u) & 255u),f32((b >> 16u) & 255u),f32((b >> 24u) & 255u));");
   // Preserve original conversion for measurement, bypass it only for byte packing.
   let source=p.source.replace(old,&replacement).replace("fn from_display(","fn original_from_display(");
   p.source=format!("{source}\nfn from_display(encoded:vec3<f32>)->vec3<f32>{{return encoded;}}\n").into();
   let got=gpu.probe(&[p],input,8,0.).unwrap();let bits=got.iter().enumerate().fold(0u32,|v,(i,b)|{assert_eq!(*b,b.round());v|(*b as u32)<<(i*8)});
   cases.push(json!({"input":input,"expression":expression,"bits":bits,"value":f32::from_bits(bits)}));
  }
 }
 std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"instrumented":true,"upstreamProbes":0,"cases":cases})).unwrap()).unwrap();
}
