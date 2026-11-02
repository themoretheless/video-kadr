//! Qualification only: original pinned logarithmic crusher chains and rounding.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn applied(id:&str,p:&Value)->AppliedFilter{AppliedFilter{id:id.into(),params:p.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}}
fn main(){
    let cat=Catalogue::builtin();assert_eq!(cat.get("radio").unwrap().id(),"concat.radio");assert_eq!(cat.get("concat.lofi").unwrap().id(),"concat.lofi");assert!(cat.get("lofi").is_none());
    let mut cases=vec![];
    for signal in ["noise","steps"]{
        let mut plans:Vec<Vec<(&str,Value)>>=vec![];
        for value in [0.,30.,6.555,37.5,100.]{plans.push(vec![("concat.radio",json!({"noise":value}))]);}
        for value in [4.,8.,4.5,6.555,12.]{plans.push(vec![("concat.lofi",json!({"bits":value}))]);}
        plans.extend([
            vec![("concat.radio",json!({"noise":30})),("concat.lofi",json!({"bits":8}))],
            vec![("concat.lofi",json!({"bits":8})),("concat.radio",json!({"noise":30}))],
            vec![("concat.radio",json!({"noise":30})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
            vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.radio",json!({"noise":30}))],
            vec![("concat.lofi",json!({"bits":8})),("concat.distorted",json!({"drive":50}))],
            vec![("concat.distorted",json!({"drive":50})),("concat.lofi",json!({"bits":8}))],
            vec![("concat.radio",json!({"noise":30})),("concat.telephone",json!({"drive":3}))],
            vec![("concat.telephone",json!({"drive":3})),("concat.radio",json!({"noise":30}))],
            vec![("concat.lofi",json!({"bits":8})),("concat.lofi",json!({"bits":4}))],
            vec![("concat.radio",json!({"noise":100})),("concat.radio",json!({"noise":0}))],
        ]);
        for plan in plans{let filters=plan.iter().map(|(id,p)|applied(id,p)).collect::<Vec<_>>();let settings=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect::<Vec<_>>();cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));}
    }
    std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":{"radio":"concat.radio","lofiHasShortAlias":false},"cases":cases})).unwrap()).unwrap();
}
