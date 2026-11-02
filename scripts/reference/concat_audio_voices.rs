//! Qualification only: original pinned pass/EQ voice chains and source aliases.
use concat_effects::Catalogue;
use concat_project::model::AppliedFilter;
use serde_json::{json,Value};
fn applied(id:&str,p:&Value)->AppliedFilter{AppliedFilter{id:id.into(),params:p.as_object().unwrap().iter().map(|(k,v)|(k.clone(),f64::from(v.as_f64().unwrap() as f32))).collect(),enabled:true,keys:Default::default()}}
fn main(){
    let cat=Catalogue::builtin();let aliases=[("submerged","concat.underwater"),("telephone","concat.telephone"),("bullhorn","concat.megaphone"),("loudspeaker","concat.megaphone"),("record","concat.vinyl"),("gramophone","concat.vinyl"),("breathy","concat.whisper"),("intimate","concat.whisper")];
    for(alias,id)in aliases{assert_eq!(cat.get(alias).unwrap().id(),id);}
    let mut cases=vec![];
    for signal in ["noise","steps"]{
        let mut plans:Vec<Vec<(&str,Value)>>=vec![];
        for(id,key,min,default)in[("concat.underwater","depth",1.,5.),("concat.telephone","drive",0.,3.),("concat.megaphone","drive",1.,5.),("concat.vinyl","warmth",0.,5.),("concat.whisper","air",1.,5.)]{for value in [min,default,6.555,10.]{plans.push(vec![(id,json!({key:value}))]);}}
        plans.extend([
            vec![("concat.telephone",json!({"drive":3})),("concat.underwater",json!({"depth":5}))],
            vec![("concat.underwater",json!({"depth":5})),("concat.telephone",json!({"drive":3}))],
            vec![("concat.megaphone",json!({"drive":5})),("concat.whisper",json!({"air":5}))],
            vec![("concat.whisper",json!({"air":5})),("concat.megaphone",json!({"drive":5}))],
            vec![("concat.vinyl",json!({"warmth":5})),("concat.bass",json!({"gain":5}))],
            vec![("concat.bass",json!({"gain":5})),("concat.vinyl",json!({"warmth":5}))],
            vec![("concat.distorted",json!({"drive":50})),("concat.telephone",json!({"drive":3}))],
            vec![("concat.telephone",json!({"drive":3})),("concat.distorted",json!({"drive":50}))],
            vec![("concat.echo",json!({"delay":0.05,"decay":0.2})),("concat.whisper",json!({"air":5}))],
            vec![("concat.whisper",json!({"air":5})),("concat.echo",json!({"delay":0.05,"decay":0.2}))],
        ]);
        for plan in plans{let filters=plan.iter().map(|(id,p)|applied(id,p)).collect::<Vec<_>>();let settings=filters.iter().map(|f|json!({"id":f.id,"params":f.params})).collect::<Vec<_>>();cases.push(json!({"signal":signal,"settings":settings,"chain":cat.audio_chain(&filters)}));}
    }
    std::fs::write(std::env::args().nth(1).unwrap(),serde_json::to_vec_pretty(&json!({"aliasVerified":aliases,"cases":cases})).unwrap()).unwrap();
}
