//! External original-package qualification only; no shader copied into product.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let package = Catalogue::builtin().get("concat.color-wheels").unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let adapter = gpu.adapter_info();
    assert_eq!(format!("{:?}", adapter.backend), "Metal");
    let mut probes = 0;
    for p in &package.probes {
        let pass = package.probe_pass(p).unwrap();
        p.check(gpu.probe(&[pass], p.input, 8, 0.).unwrap())
            .unwrap();
        probes += 1;
    }
    let zero = [0.; 3];
    let settings: Vec<Vec<[[f32; 3]; 3]>> = vec![
        vec![[zero, zero, zero]],
        vec![[[0., 1., 0.], zero, zero]],
        vec![[[1., 0., 0.], zero, zero]],
        vec![[[0., -1., 0.], zero, zero]],
        vec![[[-1., 0., 0.], zero, zero]],
        vec![[[0., 0., -1.], zero, zero]],
        vec![[[0., 0., 1.], zero, zero]],
        vec![[zero, [0., 1., 0.], zero]],
        vec![[zero, [1., 0., 0.5], zero]],
        vec![[zero, [0., 0., -1.], zero]],
        vec![[zero, [0., 0., 1.], zero]],
        vec![[zero, zero, [0., 1., 0.]]],
        vec![[zero, zero, [0., 0., -1.]]],
        vec![[zero, zero, [0., 0., 1.]]],
        vec![[[1., 1., 1.], [-1., 1., -1.], [1., -1., 1.]]],
        vec![
            [[0.2, -0.6, 0.1], [-0.1, 0.4, -0.25], [0.35, 0.45, 0.1]],
            [[0., 0., -0.2], zero, [0., 0., 0.3]],
        ],
    ];
    let colours: [[f32; 3]; 16] = [
        [0.36, 0.09, 0.18],
        [4., 0.5, -0.02],
        [0.; 3],
        [1e-8, 1e-5, 0.001],
        [0.1799, 0.1801, 1.],
        [0.01, 0.1, 10.],
        [128., 1024., 65504.],
        [-0.25, -0.001, 0.4],
        [0.0001, 0.002, 0.02],
        [0.4, 0.7, 1.2],
        [0.3, 0.2, 0.12],
        [0.8, 0.5, 0.4],
        [0.12, 0.11, 0.1],
        [0.25; 3],
        [1., 0.2, 0.05],
        [0.1, 0.4, 0.2],
    ];
    let mut cases = Vec::new();
    for wheels in settings {
        let passes = wheels
            .iter()
            .map(|values| {
                let mut params = BTreeMap::new();
                for (i, name) in ["lift", "gamma", "gain"].into_iter().enumerate() {
                    for (j, key) in ["x", "y", "m"].into_iter().enumerate() {
                        params.insert(format!("{name}.{key}"), f64::from(values[i][j]));
                    }
                }
                package.pass(&params, None).unwrap()
            })
            .collect::<Vec<_>>();
        for rgb in colours {
            for alpha in [1., 0.5, 0.0625, 0.001] {
                let input = [rgb[0], rgb[1], rgb[2], alpha];
                let effective = input.map(|v| f16::from_f32(v).to_f32());
                let got = gpu.probe(&passes, input, 8, 0.).unwrap();
                assert!(got.iter().all(|v| v.is_finite()));
                let tolerance = got.map(|v| {
                    let b = f16::from_f32(v.abs()).to_bits();
                    let lo = f16::from_bits(b.saturating_sub(1)).to_f32();
                    let hi = f16::from_bits(b.saturating_add(1).min(0x7bff)).to_f32();
                    ((v.abs() - lo).max(hi - v.abs()) * 2.).max(1e-6)
                });
                cases.push(json!({"wheels":wheels,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
            }
        }
    }
    assert_eq!(cases.len(), 1024);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"upstreamProbes":probes,"cases":cases})).unwrap()).unwrap();
}
