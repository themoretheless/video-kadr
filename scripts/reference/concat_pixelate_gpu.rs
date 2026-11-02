//! Qualification only: execute the pinned unmodified upstream shader on Metal.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output=std::env::args().nth(1).expect("output path");
    let package=Catalogue::builtin().get("concat.pixelate").expect("original package");
    let mut gpu=WgpuCompositor::new().expect("GPU compositor required");
    assert!(!gpu.on_software_adapter());
    let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
    let mut probes=Vec::new();
    for probe in &package.probes {
        let got=gpu.probe(&[package.probe_pass(probe).unwrap()],probe.input,8,0.).unwrap();
        probes.push(json!({"input":probe.input,"gpu":got,"sourceProbePassed":probe.check(got).is_ok()}));
    }
    let configurations=vec![json!({"size":2}),json!({"size":16}),json!({"size":64})];
    let colors=vec![[0.18f32;3],[4.,1.,0.05],[10.,0.5,-0.02],[-0.1,0.3,0.8],[0.;3],[1.;3],[0.5,0.2,0.1],[0.14713298,0.34414873,0.10504319]];
    let mut cases=Vec::new();
    for settings in configurations {
        let params=serde_json::from_value::<BTreeMap<String,f64>>(settings.clone()).unwrap();
        let pass=package.pass(&params,None).unwrap();
        for &rgb in &colors {
            for alpha in [0f32,0.0625,0.5,1.] {
                let input=[rgb[0],rgb[1],rgb[2],alpha];
                let got=gpu.probe(&[pass.clone()],input,8,0.).unwrap();
                assert!(got.iter().all(|v|v.is_finite()));
                cases.push(json!({"settings":settings,"input":input,"gpu":got}));
            }
        }
    }
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"uniformPixelate":true,"adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"upstreamProbes":probes.len(),"probes":probes,"cases":cases})).unwrap()).unwrap();
}
