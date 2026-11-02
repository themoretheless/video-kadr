//! Instrumented exact f32 samples before final mix, never original output parity.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::{json,Value};
use std::collections::BTreeMap;
fn main(){
 let args:Vec<String>=std::env::args().collect();let id="concat.zoom-transition";
 let selection:Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_ZOOM_DIAGNOSTIC_CASES").unwrap()).unwrap()).unwrap();assert_eq!(selection["effect"],id);assert_eq!(selection["diagnosticSelection"],true);
 let catalogue=Catalogue::builtin();let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let mut cases=Vec::new();
 for c in selection["cases"].as_array().unwrap(){
  let width=c["width"].as_u64().unwrap()as u32;let height=c["height"].as_u64().unwrap()as u32;let parameter=c["parameter"].as_str().unwrap();let value=c["value"].as_f64().unwrap();let progress=c["progress"].as_f64().unwrap();
  let from:Vec<[f32;4]>=serde_json::from_value(c["from"].clone()).unwrap();let to:Vec<[f32;4]>=serde_json::from_value(c["to"].clone()).unwrap();let mut fields=BTreeMap::new();
  let mut observations=Vec::new();
  for source in ["a","b"]{for channel in 0..4{observations.push((format!("{source}{channel}"),format!("{source}[{channel}]")));}}
  for source in ["a","b"]{for channel in 0..2{let expression=format!("zoom_uv(uv, {}_scale)[{channel}]",if source=="a"{"out"}else{"in"});observations.push((format!("uv{source}{channel}"),expression));}}
  for(label,expression)in observations{
   let mut pass=catalogue.transition_pass(&id,&BTreeMap::from([(parameter.into(),value)]),progress).unwrap();let original="return mix(a, b, p);";assert!(pass.source.contains(original));
   let observer=format!("let bits = bitcast<u32>({expression}); return vec4<f32>(f32(bits & 255u), f32((bits >> 8u) & 255u), f32((bits >> 16u) & 255u), f32((bits >> 24u) & 255u));");
   pass.source=pass.source.replace(original,&observer).into();pass.key=format!("rotation-observer-{}-{label}",args[2]);
   let got=gpu.qualification_transition_image(&pass,width,height,&from,&to).unwrap();assert!(got.iter().flatten().all(|v|v.is_finite()&&*v>=0.&&*v<=255.&&v.fract()==0.));fields.insert(label,got);
  }
  cases.push(json!({"originalCase":c,"sampleBits":fields}));
 }
 std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"schemaVersion":1,"effect":id,"instrumented":true,"fullImage":true,"addedReadbackMethod":true,"upstreamProbes":0,"scope":"Exact float sample bits before mix, instrumented shader; not an original-output oracle","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
