//! Qualification only: execute unchanged pinned original Catalogue::upgrade.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::json;
fn main() {
    let output = std::env::args().nth(1).unwrap();
    let catalogue = Catalogue::builtin();
    let mut cases = Vec::new();
    for id in [
        "concat.box-blur",
        "box-blur",
        "concat.green-screen",
        "green-screen",
        "concat.blue-screen",
        "blue-screen",
        "concat.sepia",
        "custom.fx",
    ] {
        for n in 0..6 {
            for enabled in [false, true] {
                for keyed in [false, true] {
                    let mut input = json!({"id":id,"enabled":enabled});
                    if n > 0 {
                        input["params"] = json!({"radius":([0.,0.,1.,5.9,50.,1000.][n]),"similarity":([0.,-5.,0.,25.,40.,100.][n]),"softness":([0.,-2.,0.,10.,40.,100.][n]),"unused":17});
                    }
                    if keyed {
                        input["keys"] = json!({"radius":[{"at":0.7,"value":6.,"ease":"inOut"},{"at":0.2,"value":0.,"ease":[-1.,-2.,2.,3.]},{"bad":true}],"similarity":[{"at":0.8,"value":100.,"ease":"out"},{"at":0.1,"value":-5.,"ease":null}],"softness":[{"at":0.3,"value":2.,"ease":"unknown"}],"unused":[{"at":0.6,"value":9.}],"bad":"not-an-array"});
                    }
                    let mut link: AppliedFilter = serde_json::from_value(input.clone()).unwrap();
                    let upgraded = catalogue.upgrade(&mut link);
                    cases.push(json!({"input":input,"expected":{"upgraded":upgraded,"link":link}}));
                }
            }
        }
    }
    let mut animation_cases = Vec::new();
    let mut tracks = vec![json!([]), json!([{"at":0.4,"value":3.,"ease":"in"}])];
    for curve in [
        [0., 0., 1., 1.],
        [0.42, 0., 1., 1.],
        [0., 0., 0.58, 1.],
        [0.42, 0., 0.58, 1.],
        [0., 1., 1., 0.],
        [0., -2., 1., 3.],
        [1., 2., 0., -1.],
        [0., 1., 0., 1.],
        [1., 0., 1., 1.],
    ] {
        tracks.push(
            json!([{"at":0.1,"value":1.,"ease":"linear"},{"at":0.9,"value":10.,"ease":curve}]),
        );
    }
    tracks.push(json!([{"at":1.5,"value":9.},{"at":0.5,"value":3.,"ease":"in"},{"at":0.5,"value":7.,"ease":"out"},{"at":-0.5,"value":2.}]));
    for keys in tracks {
        let link: AppliedFilter =
            serde_json::from_value(json!({"id":"concat.gaussian-blur","keys":{"radius":keys}}))
                .unwrap();
        for progress in [-0.25, 0., 1e-9, 1e-7, 0.1, 0.5, 0.9, 1., 1.25]
            .into_iter()
            .chain((0..=256).map(|i| i as f64 / 256.))
        {
            let expected = link.value_at("radius", progress, 7.);
            animation_cases.push(json!({"keys":keys,"progress":progress,"rest":7.,"expectedBits":expected.to_bits()}));
        }
    }
    let mut chroma_parameter_cases = Vec::new();
    let inputs = vec![
        json!({"id":"green-screen","params":{"similarity":0,"softness":0},"keys":{"similarity":[{"at":0,"value":0},{"at":0.5,"value":30,"ease":"inOut"},{"at":1,"value":0}],"softness":[{"at":0,"value":0},{"at":1,"value":25,"ease":"out"}]}}),
        json!({"id":"blue-screen","keys":{"similarity":[{"at":0,"value":2},{"at":1,"value":100,"ease":"in"}]}}),
        json!({"id":"concat.chroma-key","keys":{"color":[{"at":0,"value":0},{"at":1,"value":4294967295u32,"ease":[0.42,-2.,0.58,3.]}],"similarity":[{"at":0,"value":1},{"at":1,"value":60,"ease":[0.42,-2.,0.58,3.]}],"softness":[{"at":0,"value":0},{"at":1,"value":50,"ease":"out"}],"spill":[{"at":0,"value":0},{"at":1,"value":100,"ease":"inOut"}]}}),
    ];
    for input in inputs {
        let mut link: AppliedFilter = serde_json::from_value(input.clone()).unwrap();
        catalogue.upgrade(&mut link);
        let package = catalogue.get("concat.chroma-key").unwrap();
        for progress in (0..=128).map(|i| i as f64 / 128.) {
            let pass = package.pass(&link.params_at(progress), None).unwrap();
            chroma_parameter_cases.push(json!({"input":input,"progress":progress,"uniformPrefix":pass.params[..28].to_vec()}));
        }
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"schemaVersion":1,"cases":cases,"animationCases":animation_cases,"chromaParameterCases":chroma_parameter_cases}),
        )
        .unwrap(),
    )
    .unwrap();
}
