use filmcraft_project::Param;
use filmcraft_time::Tick;
use serde_json::{json,Value};
fn main(){let source:Value=serde_json::from_str(include_str!("../backend/tests/fixtures/filmcraft-scalar-param.json")).unwrap();let mut cases=vec![];
for case in source["cases"].as_array().unwrap(){for a in 0..3 {for b in 0..3 {
let mut raw=case["parameter"].clone();let keys=raw["keyframes"].as_array_mut().unwrap();
for (i,key) in keys.iter_mut().enumerate(){let kind=if i==0{a}else{b};if kind==1 {key["value"]=json!({"Choice":if i==0{1}else{7}});}else if kind==2 {key["value"]=json!({"Bool":i!=0});}}
let param:Param=serde_json::from_value(raw.clone()).unwrap();let at=case["at"].as_str().unwrap().parse::<i64>().unwrap();
cases.push(json!({"parameter":raw,"at":at.to_string(),"effectBits":param.f64_at(Tick(at)).to_bits(),"mixerBits":param.scalar_at(Tick(at)).to_bits()}));
}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
