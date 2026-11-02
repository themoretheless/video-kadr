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
    let package = Catalogue::builtin().get(&format!("concat.{name}")).unwrap();
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
    let _identity = package.pass(&BTreeMap::new(), None).unwrap();
    let auxiliary_identity = auxiliary.pass(&BTreeMap::new(), None).unwrap();
    let prefix_body = r#"
 let p=vec2<u32>(floor(uv*frame.size));let seed=u32(params.exposure);
 let rgb=vec3<f32>(f32((p.x*37u+p.y*11u+seed*13u)%256u),f32((p.x*7u+p.y*53u+seed*29u)%256u),f32((p.x*79u+p.y*17u+seed*47u)%256u))/255.0;
 let lo=rgb/12.92;let hi=pow((rgb+vec3<f32>(0.055))/1.055,vec3<f32>(2.4));let light=select(hi,lo,rgb<=vec3<f32>(0.04045));
 let which=(p.x+p.y+seed)%4u;var alpha=0.0;if(which==1u){alpha=16.0/255.0;}else if(which==2u){alpha=128.0/255.0;}else if(which==3u){alpha=1.0;}
 return vec4<f32>(unpack2x16float(pack2x16float(light.xy)),unpack2x16float(pack2x16float(vec2<f32>(light.z,alpha))));
 "#;
    let selector_body = "return sample(params.lift.xy*0.5+vec2<f32>(0.5));";
    let configurations = vec![
        vec![json!({"kind":"concat_sharpen","parameters":{"amount":0.2}})],
        vec![json!({"kind":"concat_sharpen","parameters":{"amount":1}})],
        vec![json!({"kind":"concat_sharpen","parameters":{"amount":3}})],
        vec![
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
        ],
        vec![
            json!({"kind":"concat_exposure","parameters":{"stops":1.25}}),
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
        ],
        vec![
            json!({"kind":"concat_vignette","parameters":{"strength":80}}),
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
        ],
        vec![
            json!({"kind":"concat_curves","parameters":{"luma":[[0,0],[0.25,0.18],[0.75,0.84],[1,1]]}}),
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
        ],
        vec![
            json!({"kind":"concat_adjust","parameters":{"brightness":-8,"temperature":5000,"tint":10,"sharpen":80,"fade":13}}),
            json!({"kind":"concat_sharpen","parameters":{"amount":1}}),
        ],
    ];
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
                        [0, 16, 128, 255][((x + y + seed) % 4) as usize],
                    ]
                })
                .collect();
            for stages in &configurations {
                let mut passes = vec![];
                for stage in stages {
                    let id = stage["kind"]
                        .as_str()
                        .unwrap()
                        .replacen("concat_", "concat.", 1);
                    let pkg = Catalogue::builtin().get(&id).unwrap();
                    let mut values = BTreeMap::new();
                    for (k, v) in stage["parameters"].as_object().unwrap() {
                        if let Some(points) = v.as_array() {
                            for (i, p) in points.iter().enumerate() {
                                values.insert(format!("{k}.{i}.x"), p[0].as_f64().unwrap());
                                values.insert(format!("{k}.{i}.y"), p[1].as_f64().unwrap());
                            }
                        } else {
                            values.insert(k.clone(), v.as_f64().unwrap());
                        }
                    }
                    let pass = pkg.pass(&values, None).unwrap();
                    assert_eq!(
                        pass.source,
                        pkg.pass(&BTreeMap::new(), None).unwrap().source
                    );
                    passes.push(pass);
                }
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
                    let mut chain = vec![prefix.clone()];
                    chain.extend(passes.clone());
                    chain.push(selector);
                    let got = gpu.probe(&chain, [0., 0., 0., 1.], side, 0.).unwrap();
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
                cases.push(json!({"side":side,"seed":seed,"stages":stages,"inputRgba":input,"expectedWorking":working,"expectedRgba":expected}));
            }
        }
    }
    assert_eq!(cases.len(), 64);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"package":format!("concat.{name}"),"upstreamProbes":probes,"zeroAlphaColoredInput":true,"scope":"Original unchanged spatial filter chain on synthetic nonuniform RNE-half input; synthetic prefix and exact pixel-center readback selector are qualification-only","syntheticInputAndSelector":true,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
