//! Instrumented intermediate f32 bits. Never an original-output parity oracle.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main(){
 let args:Vec<String>=std::env::args().collect();let id="concat.clock-wipe";
 let catalogue=Catalogue::builtin();let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let mut cases=Vec::new();
 for side in [1u32,2,3,4,5,6,7,8,9,13,17,31]{for progress in [0.,0.333,0.375,0.5,0.731]{for (component,expression) in [("uvX","uv.x"),("uvY","uv.y"),("cx","c.x"),("cy","c.y"),("angle","a"),("radius","r"),("mask","swept * started")]{
  let mut pass=catalogue.transition_pass(id,&BTreeMap::new(),progress).unwrap();
  let original="return mix(from_at(uv), to_at(uv), swept * started);";assert!(pass.source.contains(original));
  let observed=format!("let bits = bitcast<u32>({expression}); return vec4<f32>(f32(bits & 255u), f32((bits >> 8u) & 255u), f32((bits >> 16u) & 255u), f32((bits >> 24u) & 255u));");
  pass.source=pass.source.replace(original,&observed).into();
  pass.key=format!("clock-wipe-diagnostic-{component}").into();
  let bytes=gpu.probe_transition(&pass,[1.,0.,0.,1.],[0.,0.,1.,1.],side,0.).unwrap();
  assert!(bytes.iter().all(|v|v.is_finite()&&*v>=0.&&*v<=255.));
  cases.push(json!({"side":side,"progress":progress,"component":component,"bytes":bytes}));
 }}}
 std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"schemaVersion":1,"effect":id,"instrumented":true,"upstreamProbes":cases.len(),"scope":"Clock Wipe intermediate bit observations at original raster centre; never original output parity","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
