//! Unmodified original two-input transition shader on constant signed HDR/alpha frames.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main(){
 let args:Vec<String>=std::env::args().collect();let id=format!("concat.{}",args[2].replace('_',"-"));
 let catalogue=Catalogue::builtin();let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let inputs=[[0.,0.,0.,0.],[1.,0.,0.,0.25],[0.,0.,1.,0.75],[1.,1.,1.,1.],[-0.25,0.5,2.,0.5],[2.,-1.,0.25,1.],[0.125,0.25,0.5,0.],[0.333,0.777,1.125,0.333]];
 let mut cases=Vec::new();
 for i in 0..inputs.len(){let from=inputs[i];let to=inputs[(i+3)%inputs.len()];
 for side in [1u32,2,5]{for shape in 0..3{for softness in [0.,15.,70.5,100.]{for progress in [0.,0.125,0.25,0.499,0.5,0.501,0.75,0.875,1.]{
  let parameters=BTreeMap::from([("shape".into(),shape as f64),("softness".into(),softness)]);
  let pass=catalogue.transition_pass(&id,&parameters,progress).unwrap();
  let expected=gpu.probe_transition(&pass,from,to,side,0.).unwrap();assert!(expected.iter().all(|v|v.is_finite()));
  cases.push(json!({"from":from,"to":to,"side":side,"softness":softness,"shape":shape,"progress":progress,"expected":expected}));
 }}}}}
 std::fs::write(&args[1],serde_json::to_vec_pretty(&json!({"schemaVersion":1,"effect":id,"upstreamProbes":cases.len(),"scope":"Unmodified original two-input transition on constant signed HDR straight-alpha textures","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
