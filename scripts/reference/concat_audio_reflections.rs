//! Qualification-only original reflection package chain compiler.
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
    let cat = Catalogue::builtin();
    let mut cases = vec![];
    let mut aliases = serde_json::Map::new();
    let mut append = |filters: Vec<AppliedFilter>| {
        let chain = cat.audio_chain(&filters);
        assert!(!chain.is_empty());
        let settings = filters
            .iter()
            .map(|f| json!({"id":f.id,"params":f.params}))
            .collect::<Vec<_>>();
        cases.push(json!({"settings":settings,"chain":chain}));
    };
    for name in ["room", "hall", "plate", "cathedral"] {
        let id = format!("concat.{name}");
        assert_eq!(cat.get(name).unwrap().id(), id);
        aliases.insert(name.into(), json!(id));
        for params in [
            json!({}),
            json!({"size":0}),
            json!({"size":100}),
            json!({"size":37}),
            json!({"size":12.5}),
        ] {
            append(vec![applied(&id, params)]);
        }
        append(vec![
            applied(&id, json!({"size":37})),
            applied(&id, json!({"size":100})),
        ]);
    }
    let all = [
        "concat.echo",
        "concat.room",
        "concat.hall",
        "concat.plate",
        "concat.cathedral",
    ];
    append(
        all.iter()
            .map(|id| {
                applied(
                    id,
                    if *id == "concat.echo" {
                        json!({"delay":0.05,"decay":0.125})
                    } else {
                        json!({"size":37})
                    },
                )
            })
            .collect(),
    );
    append(
        all.iter()
            .rev()
            .map(|id| {
                applied(
                    id,
                    if *id == "concat.echo" {
                        json!({"delay":0.05,"decay":0.125})
                    } else {
                        json!({"size":37})
                    },
                )
            })
            .collect(),
    );
    assert_eq!(cases.len(), 26);
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        serde_json::to_vec_pretty(
            &json!({"package":"concat.reflections","aliasVerified":aliases,"cases":cases}),
        )
        .unwrap(),
    )
    .unwrap();
}
