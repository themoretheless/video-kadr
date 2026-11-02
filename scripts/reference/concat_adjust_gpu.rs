//! External qualification adapter: original pinned Adjust package, unmodified Metal shader.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
fn settings(values: &[(&str, f64)]) -> BTreeMap<String, f64> {
    values.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let package = Catalogue::builtin().get("concat.adjust").unwrap();
    let mut gpu = WgpuCompositor::new().unwrap();
    assert!(!gpu.on_software_adapter());
    let adapter = gpu.adapter_info();
    assert_eq!(format!("{:?}", adapter.backend), "Metal");
    let mut probes = 0;
    for probe in &package.probes {
        probe
            .check(
                gpu.probe(&[package.probe_pass(probe).unwrap()], probe.input, 8, 0.)
                    .unwrap(),
            )
            .unwrap();
        probes += 1;
    }
    let mut configurations = vec![("identity".to_owned(), vec![BTreeMap::new()])];
    for (key, low, high) in [
        ("exposure", -3., 3.),
        ("brightness", -100., 100.),
        ("contrast", -100., 100.),
        ("saturation", -100., 100.),
        ("temperature", 2000., 11000.),
        ("tint", -100., 100.),
        ("shadows", -100., 100.),
        ("highlights", -100., 100.),
        ("whites", -100., 100.),
        ("blacks", -100., 100.),
        ("sharpen", 0., 100.),
        ("fade", 0., 100.),
        ("vignette", 0., 100.),
    ] {
        for value in [low, high] {
            configurations.push((format!("{key}/{value}"), vec![settings(&[(key, value)])]));
        }
    }
    let grade = settings(&[
        ("exposure", 1.),
        ("brightness", -12.),
        ("temperature", 5000.),
        ("tint", 10.),
        ("contrast", 30.),
        ("saturation", 30.),
        ("shadows", 25.),
        ("highlights", -18.),
        ("whites", 8.),
        ("blacks", -10.),
        ("fade", 8.),
        ("vignette", 30.),
        ("sharpen", 40.),
    ]);
    let mut wheels_curve = settings(&[
        ("lift.y", 0.5),
        ("gain.m", 0.2),
        ("gamma.x", -0.3),
        ("luma.0.x", 0.),
        ("luma.0.y", 0.),
        ("luma.1.x", 0.5),
        ("luma.1.y", 0.6),
        ("luma.2.x", 1.),
        ("luma.2.y", 1.),
        ("blue.0.x", 0.),
        ("blue.0.y", 0.2),
        ("blue.1.x", 1.),
        ("blue.1.y", 0.9),
    ]);
    configurations.push(("grade".into(), vec![grade.clone()]));
    configurations.push(("wheels-curves".into(), vec![wheels_curve.clone()]));
    wheels_curve.extend(grade.clone());
    configurations.push(("combined-single-pass".into(), vec![wheels_curve]));
    configurations.push((
        "two-pass".into(),
        vec![
            grade,
            settings(&[
                ("temperature", 11000.),
                ("tint", -100.),
                ("brightness", 5.),
                ("fade", 20.),
            ]),
        ],
    ));
    let colours: [[f32; 3]; 16] = [
        [0., 0., 0.],
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
    let mut cases = Vec::new();
    for (name, stages) in &configurations {
        let passes = stages
            .iter()
            .map(|v| package.pass(v, None).unwrap())
            .collect::<Vec<_>>();
        for rgb in colours {
            for alpha in [1., 0.5, 0.0625, 0.001] {
                let input = [rgb[0], rgb[1], rgb[2], alpha];
                let effective = input.map(|v| f16::from_f32(v).to_f32());
                let got = gpu.probe(&passes, input, 8, 0.).unwrap();
                assert!(
                    got.iter().all(|v| v.is_finite()),
                    "nonfinite {name} {input:?}: {got:?}"
                );
                let tolerance = got.map(|v| {
                    let bits = f16::from_f32(v.abs()).to_bits();
                    let lo = f16::from_bits(bits.saturating_sub(1)).to_f32();
                    let hi = f16::from_bits(bits.saturating_add(1).min(0x7bff)).to_f32();
                    ((v.abs() - lo).max(hi - v.abs()) * 2.).max(1e-6)
                });
                cases.push(json!({"name":name,"stages":stages,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
            }
        }
    }
    assert_eq!(configurations.len(), 31);
    assert_eq!(cases.len(), 1984);
    std::fs::write(output, serde_json::to_vec_pretty(&json!({"schemaVersion":1,"scope":"Uniform 8x8 working-light probe readback only; spatial sharpening/vignette image matrix remains separate","upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
