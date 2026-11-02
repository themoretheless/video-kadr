//! Metadata-only Sweet plans from the pinned original catalogue/compiler.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::json;
fn main() {
    let catalogue = Catalogue::builtin();
    let mut cases = Vec::new();
    for (amount, pitch) in [
        (0f64, 0f64),
        (1., 0.),
        (25., 0.),
        (50., 0.),
        (65., 0.),
        (75., 0.),
        (100., 0.),
        (65., 0.5),
        (65., 8.),
        (100., 8.),
        (37.55, 0.),
    ] {
        let amount = f64::from(amount as f32);
        let pitch = f64::from(pitch as f32);
        let filter = AppliedFilter {
            id: "concat.sweet".into(),
            params: [("amount".into(), amount), ("pitch".into(), pitch)]
                .into_iter()
                .collect(),
            enabled: true,
            keys: Default::default(),
        };
        let settings = json!({"id":filter.id,"params":filter.params});
        let chain = catalogue.audio_chain(&[filter]);
        let prefix = chain.split(',').take(4).collect::<Vec<_>>().join(",");
        let shift = if pitch > 0. {
            pitch
        } else {
            1.2 + 3.8 * (amount / 100.)
        };
        cases.push(json!({"settings":[settings],"amount":amount,"pitch":pitch,"derivedShift":shift,"prefix":prefix,"chain":chain}));
    }
    std::fs::write(
        std::env::args().nth(1).unwrap(),
        serde_json::to_vec_pretty(&json!({"cases":cases})).unwrap(),
    )
    .unwrap();
}
