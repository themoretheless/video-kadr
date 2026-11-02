//! Qualification only: unchanged pinned original shader, sampled at dust and background pixels.
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

fn hash(x: f32, y: f32, seed: f32) -> f32 {
    let mut h = x.to_bits()
        ^ y.to_bits().wrapping_mul(0x9e3779b9)
        ^ seed.to_bits().wrapping_mul(0x85ebca6b);
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    (h >> 8) as f32 / 16777216.
}
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let package = Catalogue::builtin().get("concat.falling-dust").unwrap();
    let auxiliary = Catalogue::builtin().get("concat.adjust").unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let adapter = gpu.adapter_info();
    assert_eq!(format!("{:?}", adapter.backend), "Metal");
    let mut probes = 0;
    for p in &package.probes {
        p.check(
            gpu.probe(&[package.probe_pass(p).unwrap()], p.input, 8, 0.)
                .unwrap(),
        )
        .unwrap();
        probes += 1;
    }
    let original = package.pass(&BTreeMap::new(), None).unwrap();
    let identity = auxiliary.pass(&BTreeMap::new(), None).unwrap();
    let mut cases = vec![];
    for side in [32u32, 256] {
        for time in [0f32, 0.125, 1.25, 13.75] {
            let mut points = vec![
                [0, 0],
                [side - 1, 0],
                [side / 2, side / 2],
                [side - 1, side - 1],
            ];
            for k in 0..28 {
                let k = k as f32;
                let a = hash(k, 1., 3.);
                let b = hash(k, 2., 5.);
                let c = hash(k, 3., 7.);
                let raw_y = a - time * (0.02 + b * 0.05);
                let raw_x = c + (time * (0.3 + a) + k).sin() * 0.02;
                let x = ((raw_x - raw_x.floor()) * side as f32).floor() as u32;
                let y = ((raw_y - raw_y.floor()) * side as f32).floor() as u32;
                points.push([x.min(side - 1), y.min(side - 1)]);
                if k < 3. {
                    points.push([(x + 1).min(side - 1), y.min(side - 1)]);
                }
            }
            for parameters in [
                json!({"dust":0,"leak":0}),
                json!({"dust":60,"leak":50}),
                json!({"dust":100,"leak":100}),
                json!({"dust":100,"leak":0}),
            ] {
                let effect = package
                    .pass(
                        &serde_json::from_value::<BTreeMap<String, f64>>(parameters.clone())
                            .unwrap(),
                        None,
                    )
                    .unwrap();
                assert_eq!(effect.source, original.source);
                for input in [
                    [0.08f32, 0.1, 0.9, 1.],
                    [4., -0.02, 0.5, 0.0625],
                    [0.36, 0.09, 0.18, 0.],
                ] {
                    let effective = input.map(|v| half::f16::from_f32(v).to_f32());
                    for &[x, y] in &points {
                        let values = BTreeMap::from([
                            ("lift.x".into(), (x as f64 + 0.5) / side as f64 * 2. - 1.),
                            ("lift.y".into(), (y as f64 + 0.5) / side as f64 * 2. - 1.),
                        ]);
                        let mut selector = auxiliary.pass(&values, None).unwrap();
                        selector.source = replace_effect(
                            &identity.source,
                            "return sample(params.lift.xy*0.5+vec2<f32>(0.5));",
                        )
                        .into();
                        selector.key = "qualification-only-dust-selector-v1".into();
                        let got = gpu
                            .probe(&[effect.clone(), selector], input, side, time)
                            .unwrap();
                        assert!(got.iter().all(|v| v.is_finite()));
                        let tolerance = got.map(|v| {
                            let b = half::f16::from_f32(v.abs()).to_bits();
                            let lo = half::f16::from_bits(b.saturating_sub(1)).to_f32();
                            let hi = half::f16::from_bits(b.saturating_add(1).min(0x7bff)).to_f32();
                            ((v.abs() - lo).max(hi - v.abs()) * 2.).max(1e-6)
                        });
                        cases.push(json!({"side":side,"x":x,"y":y,"time":time,"parameters":parameters,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 3360);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"package":"concat.falling-dust","upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"scope":"Unchanged original dust shader; synthetic exact pixel-center selector only. Includes each of 28 mote centers and background at four clocks and two resolutions; signed extended and hidden RGB.","syntheticInputAndSelector":true,"cases":cases})).unwrap()).unwrap();
}
