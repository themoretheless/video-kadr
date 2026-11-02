//! Qualification adapter only: invokes the unmodified, pinned upstream compositor.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).expect("output path");
    let package = Catalogue::builtin()
        .get("concat.winter-cool")
        .expect("original package");
    let mut gpu = WgpuCompositor::new().expect("GPU compositor required; no skipped oracle");
    assert!(!gpu.on_software_adapter(), "hardware oracle required");
    let adapter = gpu.adapter_info();
    assert_eq!(
        format!("{:?}", adapter.backend),
        "Metal",
        "this qualification requires Metal"
    );
    let mut probes = 0;
    for probe in &package.probes {
        let pass = package.probe_pass(probe).expect("original probe shader");
        let got = gpu
            .probe(&[pass], probe.input, 8, 0.0)
            .expect("original GPU readback");
        probe.check(got).unwrap();
        probes += 1;
    }
    let colours: [[f32; 3]; 22] = [
        [0.36, 0.09, 0.18],
        [4.0, 0.5, -0.02],
        [0.0, 0.0, 0.0],
        [1e-8, 1e-5, 0.001],
        [0.1799, 0.1801, 1.0],
        [0.01, 0.1, 10.0],
        [128.0, 1024.0, 65504.0],
        [-0.25, -0.001, 0.4],
        [0.0001, 0.002, 0.02],
        [0.4, 0.7, 1.2],
        [1.0,0.05,0.01],
        [0.6,0.3,0.1],
        [0.3,0.2,0.2],
        [0.4,0.4,0.4],
        [0.05,0.8,0.03],
        [0.15,0.4,0.04],
        [0.4,0.8,0.3],
        [0.01,0.8,0.2],
        [0.01,0.3,0.9],
        [0.02,0.7,0.8],
        [0.08,0.1,0.9],
        [0.02,0.4,0.45],

    ];
    let mut cases = Vec::new();
    for count in [1usize,2,3,4] {
        let passes=(0..count).map(|_|package.pass(&BTreeMap::new(),None).unwrap()).collect::<Vec<_>>();
        for rgb in colours {
            for alpha in [1.0f32, 0.5, 0.0625, 0.001] {
                let input = [rgb[0], rgb[1], rgb[2], alpha];
                let effective = input.map(|v| f16::from_f32(v).to_f32());
                let got = gpu
                    .probe(&passes, input, 8, 0.0)
                    .expect("hardware winter-cool readback");
                assert!(got.iter().all(|v| v.is_finite()));
                // Two output-half ULPs; the stack also has an intermediate f16 boundary.
                let tolerance = got.map(|v| {
                    let bits = f16::from_f32(v.abs()).to_bits();
                    let low = f16::from_bits(bits.saturating_sub(1)).to_f32();
                    let high = f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();
                    ((v.abs() - low).max(high - v.abs()) * 2.0).max(1e-6)
                });
                cases.push(json!({"passes":count,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
            }
        }
    }
    assert_eq!(cases.len(), 352);
    let data = json!({"schemaVersion":1,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend),"deviceType":format!("{:?}",adapter.device_type)},"upstreamProbes":probes,"cases":cases});
    std::fs::write(output, serde_json::to_vec_pretty(&data).unwrap()).unwrap();
    eprintln!(
        "original GPU probes={probes}, generated={} winter-cool cases",
        352
    );
}
