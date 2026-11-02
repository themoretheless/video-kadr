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
fn replace_fragment(source:&str,body:&str)->String {
 let start=source.find("fn fs_main(").unwrap();let open=start+source[start..].find('{').unwrap();let mut depth=1;let mut end=open+1;
 for(i,c)in source[open+1..].char_indices(){match c{'{'=>depth+=1,'}'=>depth-=1,_=>{}}if depth==0{end=open+1+i+1;break;}}
 assert_eq!(depth,0);format!("{}{}{}",&source[..open+1],body,&source[end-1..])
}
fn main() {
 let output=std::env::args().nth(1).unwrap();
 let package=Catalogue::builtin().get("concat.zoom-blur").unwrap();let auxiliary=Catalogue::builtin().get("concat.adjust").unwrap();
 let mut gpu=WgpuCompositor::new().unwrap();assert!(!gpu.on_software_adapter());let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
 let identity=package.pass(&BTreeMap::new(),None).unwrap();let aux=auxiliary.pass(&BTreeMap::new(),None).unwrap();
 let prefix_body=r#"
 let p=vec2<u32>(floor(uv*frame.size));let seed=u32(params.exposure);
 let rgb=vec3<f32>(f32((p.x*3u+p.y+seed)%9u)*0.25-0.25,f32((p.x+p.y*3u+seed)%7u)*0.25,f32((p.x+p.y+seed)%11u)*0.25);
 let which=(p.x+p.y+seed)%4u;let a=select(select(select(0.0,0.25,which==1u),0.5,which==2u),1.0,which==3u);
 return vec4<f32>(rgb-select(vec3<f32>(0.0),vec3<f32>(1.5),seed>=2u),a);
 "#;
 let mut cases=Vec::new();
 for side in [1u32,2,8,13] {for seed in [0u32,1,2,3] {
  let mut prefix=auxiliary.pass(&BTreeMap::from([("exposure".into(),seed as f64)]),None).unwrap();prefix.source=replace_effect(&aux.source,prefix_body).into();prefix.key="zoom-blur-synthetic-linear-prefix".into();
  let offset=if seed>=2 {1.5} else {0.};
  let input:Vec<f32>=(0..side*side).flat_map(|i| {let x=i%side;let y=i/side;[((x*3+y+seed)%9)as f32*0.25-0.25-offset,((x+y*3+seed)%7)as f32*0.25-offset,((x+y+seed)%11)as f32*0.25-offset,[0f32,0.25,0.5,1.][((x+y+seed)%4)as usize]]}).collect();
  for (amount,cx,cy) in [(0.,50.,50.),(25.,50.,50.),(100.,50.,50.),(70.,0.,100.),(50.,100.,0.)] {
   let effect=package.pass(&BTreeMap::from([("amount".into(),amount),("x".into(),cx),("y".into(),cy)]),None).unwrap();assert_eq!(effect.source,identity.source);
   for stage in ["fine","middle","uvx","uvy"] {
   let mut effect=effect.clone();
   let diagnostic=match stage {
    "uvx"|"uvy"=>format!("let bits=bitcast<u32>(uv.{});return vec4<f32>(f32(bits&2047u),f32((bits>>11u)&2047u),f32(bits>>22u),1.0);",if stage=="uvx" {"x"}else{"y"}),
    _=>format!("return textureLoad({stage}_picture, vec2<i32>(floor(uv*frame.size)), 0);")
   };
   effect.source=replace_fragment(&identity.source,&format!("let uv=in.uv;{}",diagnostic)).into();
   effect.key=format!("zoom-stage-diagnostic-{stage}").into();
   let mut expected=Vec::new();
   for pixel in 0..side*side {
    let x=pixel%side;let y=pixel/side;
    let mut selector=auxiliary.pass(&BTreeMap::from([("lift.x".into(),(x as f64+0.5)/side as f64*2.-1.),("lift.y".into(),(y as f64+0.5)/side as f64*2.-1.)]),None).unwrap();
    selector.source=replace_fragment(&aux.source,"return textureLoad(source, vec2<i32>(floor((params.lift.xy*0.5+vec2<f32>(0.5))*frame.size)), 0);").into();selector.key="zoom-blur-integer-texel-selector".into();
    let got=gpu.probe(&[prefix.clone(),effect.clone(),selector],[0.,0.,0.,1.],side,0.).unwrap();assert!(got.iter().all(|v|v.is_finite()));expected.extend(got);
   }
   cases.push(json!({"side":side,"seed":seed,"stage":stage,"parameters":{"amount":amount,"x":cx,"y":cy},"inputWorking":input,"expectedWorking":expected}));
   }
  }
 }}
 std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"instrumented":true,"rawEntryReadback":true,"upstreamProbes":0,"syntheticInputAndSelector":true,"scope":"Instrumented final effect returns integer fine/middle texture; original intermediate bodies unchanged; diagnostic only","adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases})).unwrap()).unwrap();
}
