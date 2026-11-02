//! Qualification-only original Stadium chains, including bypass/Nyquist and step responses.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::json;
fn applied(id: &str, params: serde_json::Value) -> AppliedFilter {
    AppliedFilter {
        id: id.into(),
        params: params
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), f64::from(v.as_f64().unwrap() as f32)))
            .collect(),
        enabled: true,
        keys: Default::default(),
    }
}
fn main() {
    let catalogue = Catalogue::builtin();
    assert_eq!(catalogue.get("arena").unwrap().id(), "concat.stadium");
    let mut cases = vec![];
    let mut push = |filters: Vec<AppliedFilter>, signal: &str| {
        let chain = catalogue.audio_chain(&filters);
        assert!(!chain.is_empty());
        cases.push(json!({"settings":filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect::<Vec<_>>(),"chain":chain,"signal":signal}));
    };
    for params in [
        json!({}),
        json!({"size":1}),
        json!({"size":10}),
        json!({"size":7.5}),
        json!({"size":1.125}),
    ] {
        push(vec![applied("concat.stadium", params)], "noise");
    }
    push(vec![applied("concat.stadium", json!({}))], "steps");
    push(vec![applied("concat.stadium", json!({"size":10}))], "steps");
    push(
        vec![
            applied("concat.stadium", json!({"size":7.5})),
            applied("concat.stadium", json!({"size":10})),
        ],
        "steps",
    );
    push(
        vec![
            applied("concat.room", json!({"size":37})),
            applied("concat.stadium", json!({"size":7.5})),
            applied("concat.cathedral", json!({"size":12.5})),
        ],
        "noise",
    );
    push(
        vec![
            applied("concat.cathedral", json!({"size":12.5})),
            applied("concat.stadium", json!({"size":7.5})),
            applied("concat.room", json!({"size":37})),
        ],
        "noise",
    );
    assert_eq!(cases.len(), 10);
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        serde_json::to_vec_pretty(
            &json!({"package":"concat.stadium","aliasVerified":"arena","cases":cases}),
        )
        .unwrap(),
    )
    .unwrap();
}
