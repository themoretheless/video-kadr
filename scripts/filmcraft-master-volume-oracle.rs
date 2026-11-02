use filmcraft_project::{Project,SequenceSettings,Param};
use filmcraft_project::mixer::AutomationMode;
use filmcraft_time::Tick;
use serde_json::{json,Value};
fn main(){let scalar:Value=serde_json::from_str(include_str!("../backend/tests/fixtures/filmcraft-scalar-param.json")).unwrap();let mut cases=vec![];
let modes=AutomationMode::ALL;
let mut p=Project::new("Master automation");let id=p.new_sequence("Sequence",SequenceSettings::default(),0,0,None);
let seq=p.sequence_mut(id).unwrap();seq.master_volume_db=-13.25;
for source in scalar["cases"].as_array().unwrap(){for mode in modes {
seq.master_mixer.mode=mode;seq.master_mixer.lanes.insert("volume".into(),serde_json::from_value::<Param>(source["parameter"].clone()).unwrap());
let at=source["at"].as_str().unwrap().parse::<i64>().unwrap();
cases.push(json!({"sequence":{"master_volume_db":seq.master_volume_db,"master_mixer":seq.master_mixer},"at":at.to_string(),"bits":seq.master_volume_at(Tick(at)).to_bits(),"reads":mode.reads(),"writes":mode.writes()}));
}}
for mode in modes {for parameter in [None,Some(json!({"value":{"Float":12.5},"keyframes":[]})),Some(json!({"value":{"Float":-96.0},"keyframes":[]}))]{
seq.master_mixer.mode=mode;seq.master_mixer.lanes.clear();if let Some(parameter)=parameter{seq.master_mixer.lanes.insert("volume".into(),serde_json::from_value(parameter).unwrap());}
cases.push(json!({"sequence":{"master_volume_db":seq.master_volume_db,"master_mixer":seq.master_mixer},"at":"9007199254740993","bits":seq.master_volume_at(Tick(9007199254740993)).to_bits(),"reads":mode.reads(),"writes":mode.writes()}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
