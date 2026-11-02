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
    let output = std::env::args().nth(1).unwrap();
    let name = std::env::args().nth(2).unwrap();
    let package = Catalogue::builtin().get(&format!("concat.{}",name.replace('_',"-"))).unwrap();
    let auxiliary = Catalogue::builtin().get("concat.adjust").unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let adapter = gpu.adapter_info();
    assert_eq!(format!("{:?}", adapter.backend), "Metal");
    let mut probes = 0;
    for probe in &package.probes {
        probe
            .check(
                gpu.probe(&[package.probe_pass(probe).unwrap()], probe.input, 8, 0.)
                    .unwrap(),
            )
            .unwrap();
        probes += 1;
    }
    let identity = package.pass(&BTreeMap::new(), None).unwrap();
    let auxiliary_identity = auxiliary.pass(&BTreeMap::new(), None).unwrap();
    let prefix_body = r#"
 let p=vec2<u32>(floor(uv*frame.size));let seed=u32(params.exposure);
 let rgb=vec3<f32>(f32((p.x*37u+p.y*11u+seed*13u)%256u),f32((p.x*7u+p.y*53u+seed*29u)%256u),f32((p.x*79u+p.y*17u+seed*47u)%256u))/255.0;
 let lo=rgb/12.92;let hi=pow((rgb+vec3<f32>(0.055))/1.055,vec3<f32>(2.4));let light=select(hi,lo,rgb<=vec3<f32>(0.04045));
 let alpha=1.0;
 return vec4<f32>(unpack2x16float(pack2x16float(light.xy)),unpack2x16float(pack2x16float(vec2<f32>(light.z,alpha))));
 "#;
    let selector_body = "return sample(params.lift.xy*0.5+vec2<f32>(0.5));";
    assert_eq!(name,"soft_light");
    let configurations=vec![json!({"lift":0,"warmth":0}),json!({"lift":50,"warmth":40}),json!({"lift":100,"warmth":100}),json!({"lift":75,"warmth":80})];
    let mut cases = vec![];
    for side in [1u32, 2, 8, 16] {
        for seed in 0..2u32 {
            let mut input_values = BTreeMap::new();
            input_values.insert("exposure".into(), f64::from(seed));
            let mut prefix = auxiliary.pass(&input_values, None).unwrap();
            prefix.source = replace_effect(&auxiliary_identity.source, prefix_body).into();
            prefix.key = "qualification-only-adjust-input-v1".into();
            let input: Vec<u8> = (0..side * side)
                .flat_map(|i| {
                    let x = i % side;
                    let y = i / side;
                    [
                        ((x * 37 + y * 11 + seed * 13) % 256) as u8,
                        ((x * 7 + y * 53 + seed * 29) % 256) as u8,
                        ((x * 79 + y * 17 + seed * 47) % 256) as u8,
                        255,
                    ]
                })
                .collect();
            for parameters in &configurations {
                let values: BTreeMap<String, f64> =
                    serde_json::from_value(parameters.clone()).unwrap();
                let effect = package.pass(&values, None).unwrap();
                assert_eq!(
                    effect.source, identity.source,
                    "original package source changed"
                );
                let mut expected = vec![];
                let mut working = vec![];
                for pixel in 0..side * side {
                    let x = pixel % side;
                    let y = pixel / side;
                    let mut selector_values = BTreeMap::new();
                    selector_values.insert(
                        "lift.x".into(),
                        (f64::from(x) + 0.5) / f64::from(side) * 2. - 1.,
                    );
                    selector_values.insert(
                        "lift.y".into(),
                        (f64::from(y) + 0.5) / f64::from(side) * 2. - 1.,
                    );
                    let mut selector = auxiliary.pass(&selector_values, None).unwrap();
                    selector.source =
                        replace_effect(&auxiliary_identity.source, selector_body).into();
                    selector.key = "qualification-only-adjust-selector-v1".into();
                    let got = gpu
                        .probe(
                            &[prefix.clone(), effect.clone(), selector],
                            [0., 0., 0., 1.],
                            side,
                            0.,
                        )
                        .unwrap();
                    assert!(got.iter().all(|v| v.is_finite()));
                    working.extend(got);
                    for &v in &got[..3] {
                        let v = f64::from(v);
                        let encoded = if v <= 0.0031308 {
                            v * 12.92
                        } else {
                            1.055 * v.powf(1. / 2.4) - 0.055
                        };
                        expected.push((encoded.clamp(0., 1.) * 255.).round() as u8);
                    }
                    expected.push((got[3] * 255.).round() as u8);
                }
                cases.push(json!({"side":side,"seed":seed,"parameters":parameters,"inputRgba":input,"expectedWorking":working,"expectedRgba":expected}));
            }
        }
    }
    assert_eq!(cases.len(), 32);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"package":format!("concat.{}",name.replace('_',"-")),"upstreamProbes":probes,"scope":"Original unchanged spatial filter shader on synthetic nonuniform RNE-half input; synthetic prefix and exact pixel-center readback selector are qualification-only","syntheticInputAndSelector":true,"opaqueInput":true,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
