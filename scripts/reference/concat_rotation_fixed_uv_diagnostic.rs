//! Fixed uniform coordinates isolate sampling from rotation arithmetic.
//! Instrumented diagnostic, never an original transition parity oracle.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let id = format!("concat.{}", args[2]);
    let input: Value = serde_json::from_slice(&std::fs::read(
        std::env::var("CONCAT_ROTATION_FIXED_UV_REQUESTS").unwrap(),
    ).unwrap()).unwrap();
    assert_eq!(input["effect"], id);
    assert_eq!(input["instrumented"], true);
    let catalogue = Catalogue::builtin();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let info = gpu.adapter_info();
    assert_eq!(format!("{:?}", info.backend), "Metal");
    let parameter = if args[2] == "spin" { "turns" } else { "twist" };
    let mut cases = Vec::new();
    for request in input["requests"].as_array().unwrap() {
        let width = request["width"].as_u64().unwrap() as u32;
        let height = request["height"].as_u64().unwrap() as u32;
        let pixels: Vec<[f32; 4]> = serde_json::from_value(request["pixels"].clone()).unwrap();
        let uv: [f32; 2] = serde_json::from_value(request["uv"].clone()).unwrap();
        assert!(uv.iter().all(|x| x.is_finite()));
        let mut variants = BTreeMap::new();
        for (label, expression) in [
            ("from", "from_at(fixed_uv)"), ("to", "to_at(fixed_uv)"),
            ("from_level", "textureSampleLevel(from_texture, from_sampler, fixed_uv, 0.0)"),
            ("to_level", "textureSampleLevel(to_texture, to_sampler, fixed_uv, 0.0)"),
        ] {
            let mut channels = Vec::new();
            for channel in 0..4 {
                let mut pass = catalogue.transition_pass(&id, &BTreeMap::new(), 0.5).unwrap();
                pass.params[..4].copy_from_slice(&uv[0].to_le_bytes());
                pass.progress = uv[1];
                let original = "return mix(a, b, p);";
                assert!(pass.source.contains(original));
                let observer = format!("let fixed_uv = vec2<f32>(params.{parameter}, progress); let sampled = {expression}; let bits = bitcast<u32>(sampled[{channel}]); return vec4<f32>(f32(bits & 255u), f32((bits >> 8u) & 255u), f32((bits >> 16u) & 255u), f32((bits >> 24u) & 255u));");
                pass.source = pass.source.replace(original, &observer).into();
                pass.key = format!("fixed-uv-{}-{label}-{channel}", args[2]);
                let image = gpu.qualification_transition_image(&pass, width, height, &pixels, &pixels).unwrap();
                assert!(image.iter().all(|pixel| *pixel == image[0]));
                let bytes = image[0].map(|v| { assert!(v >= 0. && v <= 255. && v.fract() == 0.); v as u8 });
                channels.push(f32::from_le_bytes(bytes));
            }
            variants.insert(label, channels);
        }
        cases.push(json!({"request":request,"samples":variants}));
    }
    std::fs::write(&args[1], serde_json::to_vec_pretty(&json!({
        "schemaVersion":1,"effect":id,"instrumented":true,"upstreamProbes":0,
        "scope":"Fixed uniform UV sampling with identical source textures; not transition parity",
        "adapter":{"name":info.name,"backend":format!("{:?}",info.backend)},"cases":cases
    })).unwrap()).unwrap();
}
