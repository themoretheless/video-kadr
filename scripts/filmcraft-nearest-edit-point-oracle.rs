use filmcraft_project::{Project,Sequence,ItemId,ClipId,TrackId};
use filmcraft_time::Tick;
use serde::Serialize;
use serde_json::{Value,json};
#[derive(Clone,Copy,PartialEq,Eq,Serialize)]#[serde(rename_all="camelCase")]enum TrimKind{Trim,Ripple,Roll}
#[derive(Clone,Copy,Serialize)]struct EditPoint{clip:ClipId,out:bool,kind:TrimKind}
struct Targeting{targeted:Vec<TrackId>}
struct State{active_sequence:Option<ItemId>,targeting:std::collections::HashMap<ItemId,Targeting>,edit_points:Vec<EditPoint>,selection:Vec<ClipId>}
struct Session{project:Project,state:State,time:Tick}
#[derive(Debug)]enum EngineError{NoSequence}
type Result<T>=std::result::Result<T,EngineError>;
impl Session{fn active_sequence(&self)->Option<&Sequence>{self.project.sequence(self.state.active_sequence?)}fn playhead(&self)->Tick{self.time}}
fn reset_shift(_: &mut Session){}
__HELPERS__
__NEAREST__
fn main(){let input:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for (source_case,case) in input["cases"].as_array().unwrap().iter().enumerate().step_by(36){let project:Project=serde_json::from_value(case["document"]["project"].clone()).unwrap();let id=ItemId(case["sequence"].as_u64().unwrap());let seq=project.sequence(id).unwrap();
for targeted in [vec![],vec![seq.video_tracks[0].id],vec![seq.audio_tracks[0].id],vec![TrackId(999999)]]{for kind in ["trimIn","trimOut","rippleIn","rippleOut","roll","unknown"]{for time in [-254016000000i64,0,508032000000,762048000000,1524096000000]{
let mut session=Session{project:project.clone(),state:State{active_sequence:Some(id),targeting:std::collections::HashMap::from([(id,Targeting{targeted:targeted.clone()})]),edit_points:vec![],selection:vec![ClipId(1001)]},time:Tick(time)};
let expected=select_nearest(&mut session,&json!({"kind":kind})).unwrap();assert!(session.state.selection.is_empty());cases.push(json!({"sourceCase":source_case,"kind":kind,"time":time,"targeted":targeted,"expected":expected}));
}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
