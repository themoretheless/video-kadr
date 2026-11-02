//! Qualification adapter only: invokes the unmodified, pinned upstream compositor.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).expect("output path");
    let package = Catalogue::builtin()
        .get("concat.mono")
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
    let mut cases=Vec::new();let mut configurations:Vec<Vec<serde_json::Value>>=Vec::new();
 for colour in [-100.,40.,100.] {for contrast in [0.,40.,100.] {for grain in [0.,75.] {configurations.push(vec![json!({"colour":colour,"contrast":contrast,"grain":grain})]);}}}
 configurations.push(vec![json!({"colour":-75.,"contrast":100.,"grain":75.}),json!({"colour":100.,"contrast":25.,"grain":25.})]);
 for settings in configurations {let passes=settings.iter().map(|v|package.pass(&serde_json::from_value::<BTreeMap<String,f64>>(v.clone()).unwrap(),None).unwrap()).collect::<Vec<_>>();
 for time in [0f32,0.125,1.25] {for rgb in colours {for alpha in [1f32,0.5,0.0625,0.001] {
 let input=[rgb[0],rgb[1],rgb[2],alpha];let effective=input.map(|v|f16::from_f32(v).to_f32());let got=gpu.probe(&passes,input,8,time).unwrap();assert!(got.iter().all(|v|v.is_finite()));
 let tolerance=got.map(|v|{let bits=f16::from_f32(v.abs()).to_bits();let low=f16::from_bits(bits.saturating_sub(1)).to_f32();let high=f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();((v.abs()-low).max(high-v.abs())*2.).max(1e-6)});
 cases.push(json!({"settings":settings,"time":time,"samplePixel":[4,4],"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
 }}}}
 assert_eq!(cases.len(),5016);std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
