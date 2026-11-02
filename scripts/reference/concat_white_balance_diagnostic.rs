//! Instrumentation only: original helper expressions, bit-exact f32 readback via byte channels.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let package = Catalogue::builtin().get("concat.white-balance").unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    assert_eq!(format!("{:?}", gpu.adapter_info().backend), "Metal");
    let mut cases = Vec::new();
    for temperature in [2000., 2222., 4000., 4500., 6500., 11000.] {
        for tint in [-100., 0., 100.] {
            let pass = package
                .pass(
                    &BTreeMap::from([("temperature".into(), temperature), ("tint".into(), tint)]),
                    None,
                )
                .unwrap();
            for expression in ["planckian(params.temperature).x","planckian(params.temperature).y","planckian(params.temperature * 1.01).x","planckian(params.temperature * 1.01).y","normalize(planckian(params.temperature * 1.01) - planckian(params.temperature)).x","normalize(planckian(params.temperature * 1.01) - planckian(params.temperature)).y","white_cones(params.temperature, params.tint).x","white_cones(params.temperature, params.tint).y","white_cones(params.temperature, params.tint).z","white_cones(6500.0, 0.0).x","white_cones(6500.0, 0.0).y","white_cones(6500.0, 0.0).z","white_balance(vec3<f32>(-0.25,-0.0010004043579101562,0.39990234375),params.temperature,params.tint).x","white_balance(vec3<f32>(-0.25,-0.0010004043579101562,0.39990234375),params.temperature,params.tint).y","white_balance(vec3<f32>(-0.25,-0.0010004043579101562,0.39990234375),params.temperature,params.tint).z"]{
 let mut diagnostic=pass.clone();diagnostic.key=format!("{}-diagnostic-{expression}",pass.key);
 let old="return vec4<f32>(white_balance(c.rgb, params.temperature, params.tint), c.a);";
 assert!(pass.source.contains(old));
 let replacement=format!("let b=bitcast<u32>({expression}); return vec4<f32>(f32(b & 255u),f32((b >> 8u) & 255u),f32((b >> 16u) & 255u),f32((b >> 24u) & 255u));");
 diagnostic.source=pass.source.replace(old,&replacement).into();
 let bytes=gpu.probe(&[diagnostic],[0.2,0.3,0.4,1.],8,0.).unwrap();let bits=bytes.iter().enumerate().fold(0u32,|v,(i,b)|{assert_eq!(*b,b.round());v|(*b as u32)<<(i*8)});
 cases.push(json!({"temperature":temperature,"tint":tint,"expression":expression,"value":f32::from_bits(bits),"bits":bits}));
 }
        }
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"schemaVersion":1,"upstreamProbes":0,"instrumented":true,"cases":cases}),
        )
        .unwrap(),
    )
    .unwrap();
}
