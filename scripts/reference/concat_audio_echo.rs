//! Qualification only: original package chain rendering, not production DSP.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::json;
use std::collections::BTreeMap;
fn applied(params: serde_json::Value) -> AppliedFilter {
    AppliedFilter {
        id: "concat.echo".into(),
        params: params
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), f64::from(v.as_f64().unwrap() as f32)))
            .collect::<BTreeMap<_, _>>(),
        enabled: true,
        keys: Default::default(),
    }
}
fn main() {
    let cat = Catalogue::builtin();
    let package = cat.get("concat.echo").unwrap();
    assert_eq!(cat.get("echo").unwrap().id(), package.id());
    let mut cases = vec![];
    for parameters in [
        vec![json!({})],
        vec![json!({"delay":0.05,"decay":0.1})],
        vec![json!({"delay":1.0,"decay":0.9})],
        vec![json!({"delay":0.0505,"decay":0.125})],
        vec![
            json!({"delay":0.05,"decay":0.2}),
            json!({"delay":0.375,"decay":0.75}),
        ],
        vec![
            json!({"delay":0.375,"decay":0.75}),
            json!({"delay":0.05,"decay":0.2}),
        ],
    ] {
        let filters = parameters.iter().cloned().map(applied).collect::<Vec<_>>();
        let chain = cat.audio_chain(&filters);
        assert!(!chain.is_empty());
        let effective = filters
            .iter()
            .map(|f| json!({"id":"concat.echo","params":f.params}))
            .collect::<Vec<_>>();
        cases.push(json!({"settings":effective,"chain":chain}));
    }
    let output = std::env::args().nth(1).unwrap();
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"package":"concat.echo","aliasVerified":"echo","cases":cases}),
        )
        .unwrap(),
    )
    .unwrap();
}
