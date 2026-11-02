//! External qualification: unmodified pinned Curves package and original Metal compositor.
use concat_effects::Catalogue;
use concat_render::WgpuCompositor;
use half::f16;
use serde_json::json;
use std::collections::BTreeMap;
type Curve = Vec<[f64; 2]>;
type Settings = [Curve; 4];
fn settings(luma: Curve, red: Curve, green: Curve, blue: Curve) -> Settings {
    [luma, red, green, blue]
}
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let p = Catalogue::builtin().get("concat.curves").unwrap();
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
    let identity = vec![[0., 0.], [1., 1.]];
    let s = vec![[0., 0.], [0.25, 0.18], [0.75, 0.84], [1., 1.]];
    let partial = vec![[0.2, 0.3], [0.8, 0.7]];
    let peak = vec![[0., 0.], [0.5, 1.], [1., 0.]];
    let blue = vec![[0., 0.3], [0.3, 0.8], [0.6, 0.2], [1., 0.9]];
    let eight = vec![
        [0., 0.],
        [0.01, 0.8],
        [0.02, 0.9],
        [0.1, 0.92],
        [0.2, 0.95],
        [0.4, 0.98],
        [0.8, 0.99],
        [1., 1.],
    ];
    let sets: Vec<Vec<Settings>> = vec![
        vec![Default::default()],
        vec![settings(s.clone(), vec![], vec![], vec![])],
        vec![settings(partial.clone(), vec![], vec![], vec![])],
        vec![settings(vec![[0., 0.2], [1., 1.]], vec![], vec![], vec![])],
        vec![settings(vec![], vec![[0., 1.], [1., 0.]], vec![], vec![])],
        vec![settings(vec![], vec![], peak.clone(), vec![])],
        vec![settings(vec![], vec![], vec![], blue.clone())],
        vec![settings(vec![], eight.clone(), vec![], vec![])],
        vec![settings(
            s.clone(),
            partial.clone(),
            peak.clone(),
            blue.clone(),
        )],
        vec![settings(vec![[0.4, 0.9]], vec![], vec![], vec![])],
        vec![settings(
            vec![[1., 1.], [0., 0.], [0.4, 0.2]],
            vec![],
            vec![],
            vec![],
        )],
        vec![settings(
            vec![[0., 0.], [0.5, 0.25], [0.5000004, 0.75], [1., 1.]],
            vec![],
            vec![],
            vec![],
        )],
        vec![settings(
            vec![[0., 0.], [0.5, 0.4], [0.500002, 0.6], [1., 1.]],
            vec![],
            vec![],
            vec![],
        )],
        vec![settings(eight, vec![], vec![], vec![])],
        vec![settings(
            identity.clone(),
            identity.clone(),
            identity.clone(),
            identity,
        )],
        vec![
            settings(s, vec![], vec![], blue),
            settings(partial, vec![], peak, vec![]),
        ],
    ];
    let mut colours: Vec<[f32; 3]> = vec![
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
    for green in [0.1484375f32, 0.14855957, 0.14868164, 0.14880371] {
        colours.push([0.5f32.powf(2.4), -green.powf(2.4), 0.]);
    }
    let mut cases = Vec::new();
    for stages in sets {
        let passes = stages
            .iter()
            .map(|set| {
                let mut values = BTreeMap::new();
                for (name, points) in ["luma", "red", "green", "blue"].into_iter().zip(set) {
                    for (i, point) in points.iter().enumerate() {
                        values.insert(format!("{name}.{i}.x"), point[0]);
                        values.insert(format!("{name}.{i}.y"), point[1]);
                    }
                }
                p.pass(&values, None).unwrap()
            })
            .collect::<Vec<_>>();
        for &rgb in &colours {
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
                cases.push(json!({"stages":stages,"input":input,"effectiveInput":effective,"gpu":got,"tolerance":tolerance}));
            }
        }
    }
    assert_eq!(cases.len(), 1280);
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"schemaVersion":1,"upstreamProbes":probes,"adapter":{"name":adapter.name,"backend":format!("{:?}",adapter.backend)},"cases":cases})).unwrap()).unwrap();
}
