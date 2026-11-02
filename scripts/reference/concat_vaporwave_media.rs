//! Real original Metal per-pixel reference for a four-frame lossless media qualification.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let p = Catalogue::builtin().get("concat.vaporwave").unwrap();
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
    let parameters = json!({});
    let values = BTreeMap::new();
    let pass = p.pass(&values, None).unwrap();
    let mut cases = Vec::new();
    for f in 0..4u32 {
        let mut input = Vec::new();
        let mut expected = Vec::new();
        for pixel in 0..256u32 {
            let rgb = [
                ((pixel * 37 + f * 13) % 256) as u8,
                ((pixel * 73 + f * 29) % 256) as u8,
                ((pixel * 11 + f * 47) % 256) as u8,
            ];
            let alpha=[0u8,16,128,255][pixel as usize%4];
            input.extend_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
            let light = rgb.map(|v| {
                let x = f64::from(v) / 255.;
                (if x <= 0.04045 {
                    x / 12.92
                } else {
                    ((x + 0.055) / 1.055).powf(2.4)
                }) as f32
            });
            let output = gpu
                .probe(
                    std::slice::from_ref(&pass),
                    [light[0], light[1], light[2], alpha as f32/255.],
                    8,
                    f as f32 / 4.,
                )
                .unwrap();
            for &v in &output[..3] {
                let v = f64::from(v);
                let encoded = if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1. / 2.4) - 0.055
                };
                expected.push((encoded.clamp(0., 1.) * 255.).round() as u8);
            }
            assert_eq!(output[3], half::f16::from_f32(alpha as f32/255.).to_f32());
            expected.push(alpha);
        }
        cases.push(json!({"frame":f,"timeUs":f*250000,"inputRgba":input,"expectedRgba":expected}));
    }
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"parameters":parameters,"width":16,"height":16,"fps":4,"cases":cases})).unwrap()).unwrap();
}
