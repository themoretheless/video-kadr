//! External unmodified original scalar spatial packages on uniform working-light probes.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let name = std::env::args().nth(2).unwrap();
    let p = Catalogue::builtin().get(&format!("concat.{name}")).unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let adapter = gpu.adapter_info();
    assert_eq!(format!("{:?}", adapter.backend), "Metal");
    let mut probes = 0;
    for probe in &p.probes {
        probe
            .check(
                gpu.probe(&[p.probe_pass(probe).unwrap()], probe.input, 8, 0.)
                    .unwrap(),
            )
            .unwrap();
        probes += 1;
    }
    let (key, values) = match name.as_str() {
        "sharpen" => ("amount", [0.2, 1., 3.]),
        "vignette" => ("strength", [10., 50., 100.]),
        _ => panic!("unsupported package"),
    };
    let colours: [[f32; 3]; 16] = [
        [0.; 3],
        [0.18; 3],
        [1.; 3],
        [0.3, 0.2, 0.12],
        [4., 3., 2.],
        [0.08, 0.07, 0.06],
        [0.0001, 0.002, 0.02],
        [-0.25, -0.001, 0.4],
        [1e-8, 1e-5, 0.001],
        [0.01, 0.1, 10.],
        [0.4, 0.7, 1.2],
        [1., 0.2, 0.05],
        [0.1, 0.4, 0.2],
        [0.1799, 0.1801, 1.],
        [0.8, 0.5, 0.4],
        [16., 8., -0.02],
    ];
    let stages = vec![
        vec![values[0]],
        vec![values[1]],
        vec![values[2]],
        vec![values[2], values[0]],
    ];
    let mut cases = vec![];
    for stages in stages {
        let params = stages
            .iter()
            .map(|v| BTreeMap::from([(key.to_owned(), *v)]))
            .collect::<Vec<_>>();
        let passes = params
            .iter()
            .map(|v| p.pass(v, None).unwrap())
            .collect::<Vec<_>>();
        for rgb in colours {
            for alpha in [1., 0.5, 0.0625, 0.001] {
                let input = [rgb[0], rgb[1], rgb[2], alpha];
                let effective = input.map(|v| f16::from_f32(v).to_f32());
                let got = gpu.probe(&passes, input, 8, 0.).unwrap();
                assert!(got.iter().all(|v| v.is_finite()));
                let tolerance = got.map(|v| {
                    let bits = f16::from_f32(v.abs()).to_bits();
                    let lo = f16::from_bits(bits.saturating_sub(1)).to_f32();
                    let hi = f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();
                    ((v.abs() - lo).max(hi - v.abs()) * 2.).max(1e-6)
                });
                cases.push(json!({"stages":params,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
            }
        }
    }
    assert_eq!(cases.len(), 256);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"package":format!("concat.{name}"),"upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
