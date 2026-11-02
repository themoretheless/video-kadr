//! Qualification only: original pinned distortion compiler and mixed chain ordering.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json, Value};
fn applied(id: &str, params: &Value) -> AppliedFilter {
    AppliedFilter { id:id.into(),params:params.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default() }
}
fn main() {
    let cat=Catalogue::builtin();
    assert_eq!(cat.get("concat.distorted").unwrap().id(),"concat.distorted");
    assert!(cat.get("distorted").is_none());
    let mut cases=vec![];
    for signal in ["noise","steps"] {
        let mut plans:Vec<Vec<(&str,Value)>>=[0.,1.,37.5,50.,100.].into_iter().map(|drive|vec![("concat.distorted",json!({"drive":drive}))]).collect();
        plans.extend([
            vec![("concat.distorted",json!({"drive":50})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
            vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.distorted",json!({"drive":50}))],
            vec![("concat.distorted",json!({"drive":1})),("concat.distorted",json!({"drive":100}))],
        ]);
        for plan in plans {
            let chain=cat.audio_chain(&plan.iter().map(|(id,p)|applied(id,p)).collect::<Vec<_>>());
            let settings=plan.iter().map(|(id,p)|json!({"id":id,"params":p})).collect::<Vec<_>>();
            cases.push(json!({"chain":chain,"signal":signal,"settings":settings}));
        }
    }
    std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":null,"cases":cases})).unwrap()).unwrap();
}
