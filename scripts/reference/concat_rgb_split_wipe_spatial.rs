//! Original GPU pipeline on full straight-HDR images, readback-only extension.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main(){
 let output=std::env::args().nth(1).unwrap();let catalogue=Catalogue::builtin();let id="concat.rgb-split-wipe";
 let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let mut cross_checks=0;
 for side in [1u32,2,5]{for progress in [0.,0.25,0.5,1.]{
  let pass=catalogue.transition_pass(id,&BTreeMap::from([("spread".into(),60.)]),progress).unwrap();let a=[-0.25,0.5,2.,0.5];let b=[1.,-1.,0.25,0.25];
  let pixels=gpu.qualification_transition_image(&pass,side,side,&vec![a;(side*side)as usize],&vec![b;(side*side)as usize]).unwrap();let point=gpu.probe_transition(&pass,a,b,side,0.).unwrap();assert_eq!(pixels[(side/2*side+side/2)as usize].map(f32::to_bits),point.map(f32::to_bits));cross_checks+=1;
 }}
 let mut cases=Vec::new();
 for(width,height)in[(1u32,1u32),(2,3),(8,8),(13,7),(7,13),(16,9)]{for seed in 0u32..4{
  let from:Vec<[f32;4]>=(0..width*height).map(|i|{let x=i%width;let y=i/width;[((x*3+y+seed)%13)as f32*0.25-0.5,((x+y*3+seed)%9)as f32*0.25,((x+y+seed)%11)as f32*0.25,[0.,0.25,0.5,1.][((x+y+seed)%4)as usize]]}).collect();
  let to:Vec<[f32;4]>=(0..width*height).map(|i|{let x=i%width;let y=i/width;[((x+y*2+seed)%11)as f32*0.25,((x*2+y+seed)%13)as f32*0.25-0.5,((x*3+y+seed)%9)as f32*0.25,[1.,0.5,0.25,0.][((x+y+seed)%4)as usize]]}).collect();
  for spread in [0.,37.5,60.,100.]{for progress in [0.,0.25,0.5,1.]{
   let pass=catalogue.transition_pass(id,&BTreeMap::from([("spread".into(),spread)]),progress).unwrap();let expected=gpu.qualification_transition_image(&pass,width,height,&from,&to).unwrap();assert!(expected.iter().flatten().all(|v|v.is_finite()));
   cases.push(json!({"width":width,"height":height,"seed":seed,"spread":spread,"progress":progress,"from":from,"to":to,"expected":expected}));
  }}
 }}
 std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"effect":id,"upstreamProbes":cross_checks,"fullImage":true,"addedReadbackMethod":true,"scope":"Unmodified original shader and encode_transition in isolated renderer copy with appended upload/readback method; whole straight-HDR textures","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
