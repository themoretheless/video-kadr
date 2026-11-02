use filmcraft_time::{Tick,FrameRate};use serde::{Deserialize,Serialize};use serde_json::{Value,json};
#[derive(Clone,Copy,Deserialize)]#[serde(rename_all="camelCase")]enum TrimKind{Trim,Ripple,Roll}
#[derive(Deserialize)]struct EditPoint{out:bool,kind:TrimKind}
#[derive(Clone,Copy,Serialize)]#[serde(rename_all="camelCase")]struct TrimShift{out_shift:Tick,in_shift:Tick}
struct State{edit_points:Vec<EditPoint>,trim_shift:TrimShift}
struct Session{state:State}
__SHIFT__
fn main(){let source:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let projects:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(2).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for (source_case,case) in source["cases"].as_array().unwrap().iter().enumerate(){for initial in [(0,0),(-254016000000i64,508032000000i64),(9007199254740993i64,-9007199254740993i64)]{
let shift=TrimShift{out_shift:Tick(initial.0),in_shift:Tick(initial.1)};let mut s=Session{state:State{edit_points:serde_json::from_value(case["points"].clone()).unwrap(),trim_shift:shift}};
if !case["error"].as_bool().unwrap(){let first=case["applied"].as_array().unwrap().first().and_then(Value::as_i64).map(Tick).unwrap_or_default();add_shift(&mut s,first);}
let base=&projects["cases"][case["sourceCase"].as_u64().unwrap() as usize];let seq=base["sequence"].as_u64().unwrap().to_string();let rate:FrameRate=serde_json::from_value(base["document"]["project"]["items"][seq]["kind"]["Sequence"]["settings"]["frame_rate"].clone()).unwrap();
cases.push(json!({"expectedFrames":{"outShift":rate.frame_at(s.state.trim_shift.out_shift),"inShift":rate.frame_at(s.state.trim_shift.in_shift)},"sourceCase":source_case,"initial":shift,"expected":s.state.trim_shift}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
