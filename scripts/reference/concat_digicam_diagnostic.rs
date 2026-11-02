//! Prefix readback diagnostic: original Digicam passes remain unchanged.
use concat_effects::Catalogue;use concat_render::WgpuCompositor;use serde_json::{json,Value};use std::collections::BTreeMap;
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
fn main(){
let output=std::env::args().nth(1).unwrap();let fixture:Value=serde_json::from_slice(&std::fs::read(std::env::var("CONCAT_DIAGNOSTIC_FIXTURE").unwrap()).unwrap()).unwrap();let index:usize=std::env::var("CONCAT_DIAGNOSTIC_CASE").unwrap().parse().unwrap();let c=&fixture["cases"][index];
let package=Catalogue::builtin().get("concat.digicam").unwrap();let auxiliary=Catalogue::builtin().get("concat.adjust").unwrap();let identity=auxiliary.pass(&BTreeMap::new(),None).unwrap();let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let adapter=gpu.adapter_info();assert_eq!(format!("{:?}",adapter.backend),"Metal");let mut probes=0;for probe in &package.probes{probe.check(gpu.probe(&[package.probe_pass(probe).unwrap()],probe.input,8,0.).unwrap()).unwrap();probes+=1;}
let input:[f32;4]=serde_json::from_value(c["input"].clone()).unwrap();let passes=c["settings"].as_array().unwrap().iter().map(|v|package.pass(&serde_json::from_value::<BTreeMap<String,f64>>(v.clone()).unwrap(),None).unwrap()).collect::<Vec<_>>();let mut stages=vec![];
for count in 1..=passes.len(){let mut working=vec![];for y in 0..8{for x in 0..8{let values=BTreeMap::from([("lift.x".into(),(x as f64+0.5)/8.*2.-1.),("lift.y".into(),(y as f64+0.5)/8.*2.-1.)]);let mut selector=auxiliary.pass(&values,None).unwrap();selector.source=replace_effect(&identity.source,"return sample(params.lift.xy*0.5+vec2<f32>(0.5));").into();selector.key="diagnostic-only-prefix-selector".into();let mut chain=passes[..count].to_vec();chain.push(selector);working.extend(gpu.probe(&chain,input,8,c["time"].as_f64().unwrap()as f32).unwrap());}}stages.push(json!({"prefixLength":count,"working":working}));}
let final_gpu:[f32;4]=serde_json::from_value(c["gpu"].clone()).unwrap();for i in 0..4{assert_eq!(stages.last().unwrap()["working"][36*4+i].as_f64().unwrap()as f32,final_gpu[i],"selector changed original package output");}
std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"instrumented":true,"upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":[{"caseIndex":index,"original":c,"stages":stages}]})).unwrap()).unwrap();}
