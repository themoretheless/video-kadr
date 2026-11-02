//! External original spatial filter shader between explicitly synthetic input and pixel-selector passes.
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
 let package=Catalogue::builtin().get("concat.film-grain").unwrap();let auxiliary=Catalogue::builtin().get("concat.adjust").unwrap();
 let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let identity=package.pass(&BTreeMap::new(),None).unwrap();let aux=auxiliary.pass(&BTreeMap::new(),None).unwrap();
 let prefix_body=r#"
 let p=vec2<u32>(floor(uv*frame.size));let seed=u32(params.exposure);
 let rgb=vec3<f32>(f32((p.x*5u+p.y*3u+seed)%67u)*0.00390625-0.015625,f32((p.x*7u+p.y*11u+seed)%43u)*0.125-0.5,f32((p.x*13u+p.y*17u+seed)%31u)*0.03125-0.125);
 let which=(p.x+p.y*2u+seed)%4u;let a=select(select(select(0.0,0.125,which==1u),0.75,which==2u),1.0,which==3u);return vec4<f32>(rgb,a);
 "#;
 let mut cases=Vec::new();
 for side in [3u32,7,17] {for seed in [0u32,1] {
  let mut prefix=auxiliary.pass(&BTreeMap::from([("exposure".into(),seed as f64)]),None).unwrap();prefix.source=replace_effect(&aux.source,prefix_body).into();prefix.key="film-grain-synthetic-linear-prefix".into();
  let input:Vec<f32>=(0..side*side).flat_map(|i|{let x=i%side;let y=i/side;[((x*5+y*3+seed)%67)as f32*0.00390625-0.015625,((x*7+y*11+seed)%43)as f32*0.125-0.5,((x*13+y*17+seed)%31)as f32*0.03125-0.125,[0f32,0.125,0.75,1.][((x+y*2+seed)%4)as usize]]}).collect();
  for (amount,seconds) in [2.5,13.875,39.5].into_iter().flat_map(|a|[0.0625,0.375,2.125,-0.375,123.75].into_iter().map(move|t|(a,t))) {
   let effect=package.pass(&BTreeMap::from([("amount".into(),amount as f64)]),None).unwrap();assert_eq!(effect.source,identity.source);
   let mut expected=Vec::new();
   for pixel in 0..side*side {
    let x=pixel%side;let y=pixel/side;
    let mut selector=auxiliary.pass(&BTreeMap::from([("lift.x".into(),(x as f64+0.5)/side as f64*2.-1.),("lift.y".into(),(y as f64+0.5)/side as f64*2.-1.)]),None).unwrap();
    selector.source=replace_effect(&aux.source,"return textureLoad(source, vec2<i32>(floor((params.lift.xy*0.5+vec2<f32>(0.5))*frame.size)), 0);").into();selector.key="film-grain-integer-texel-selector".into();
    let got=gpu.probe(&[prefix.clone(),effect.clone(),selector],[0.,0.,0.,1.],side,seconds as f32).unwrap();assert!(got.iter().all(|v|v.is_finite()));expected.extend(got);
   }
   cases.push(json!({"side":side,"seed":seed,"seconds":seconds,"parameters":{"amount":amount},"inputWorking":input,"expectedWorking":expected}));
  }
 }}
 std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"fractionalInput":true,"upstreamProbes":0,"syntheticInputAndSelector":true,"scope":"Unmodified Film Grain shader with independent fractional parameters, dyadic toe/HDR/alpha input, new dimensions/times; integer selector","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
