//! Qualification only: execute the pinned unmodified upstream shader on Metal.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output=std::env::args().nth(1).expect("output path");
    let package=Catalogue::builtin().get("concat.chroma-key").expect("original package");
    let mut gpu=WgpuCompositor::new().expect("GPU compositor required");
    assert!(!gpu.on_software_adapter());
    let info=gpu.adapter_info();assert_eq!(format!("{:?}",info.backend),"Metal");
    let mut probes=Vec::new();
    for probe in &package.probes {
        let got=gpu.probe(&[package.probe_pass(probe).unwrap()],probe.input,8,0.).unwrap();
        probes.push(json!({"input":probe.input,"gpu":got,"sourceProbePassed":probe.check(got).is_ok()}));
    }
    let configurations=vec![json!({}),json!({"spill":0}),json!({"similarity":1,"softness":0,"spill":100}),json!({"color":0,"similarity":1,"softness":0,"spill":0}),json!({"color":4294967295u32,"similarity":60,"softness":50,"spill":100}),json!({"color":4701183})];
    let mut colors=vec![[0.,0.416332,0.0362356],[0.,0.18,0.012],[0.6,0.35,0.25],[0.18;3],[0.2,0.28,0.2],[0.,0.0464862,0.475033],[0.;3],[1.;3],[4.,0.5,-0.02],[10.,1.,0.2],[-0.1,0.3,0.8]];
    // Original shader executes every value; this only selects source colors.
    let green=[0f32,177./255.,64./255.];
    for target in [[0.5f32;3],[0.8,0.6,0.5]] {
        for n in 0..=32 {
            let t=n as f32/32.;
            colors.push(std::array::from_fn(|i|{let v=green[i]+t*(target[i]-green[i]);v.signum()*v.abs().powf(2.4)}));
        }
    }
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
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"softEdgeHdr":true,"adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"upstreamProbes":probes.len(),"probes":probes,"cases":cases})).unwrap()).unwrap();
}
