//! Qualification only: pinned Concat shelf packages and precision-changing orders.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn applied(id:&str,params:&Value)->AppliedFilter{AppliedFilter{id:id.into(),params:params.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}}
fn main(){
    let cat=Catalogue::builtin();for name in ["bass","treble","bright"]{assert_eq!(cat.get(name).unwrap().id(),format!("concat.{name}"));}
    let mut cases=vec![];
    for signal in ["noise","steps"]{
        let mut plans:Vec<Vec<(&str,Value)>>=vec![];
        for (id,default) in [("concat.bass",5.),("concat.treble",4.),("concat.bright",5.)]{for gain in [0.,default,6.555,12.]{plans.push(vec![(id,json!({"gain":gain}))]);}}
        plans.extend([
            vec![("concat.bass",json!({"gain":5})),("concat.treble",json!({"gain":4}))],
            vec![("concat.treble",json!({"gain":4})),("concat.bass",json!({"gain":5}))],
            vec![("concat.bright",json!({"gain":5})),("concat.bright",json!({"gain":12}))],
            vec![("concat.distorted",json!({"drive":50})),("concat.bass",json!({"gain":5}))],
            vec![("concat.bass",json!({"gain":5})),("concat.distorted",json!({"drive":50}))],
            vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.bright",json!({"gain":5}))],
            vec![("concat.bright",json!({"gain":5})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
        ]);
        for plan in plans{let filters=plan.iter().map(|(id,p)|applied(id,p)).collect::<Vec<_>>();let settings=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect::<Vec<_>>();cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));}
    }
    std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":["bass","treble","bright"],"cases":cases})).unwrap()).unwrap();
}
