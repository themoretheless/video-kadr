pub use filmcraft_render::{SourceProvider,MAX_NEST_DEPTH,transitions,remix};
#[path="__AUDIO_PATH__"] pub mod audio;
#[path="__AUDIO_FX_PATH__"] pub mod audio_fx;
mod mixer;
use filmcraft_project::{Project,SequenceSettings,EffectKind,ParamKind};
use filmcraft_project::mixer::AutomationMode;
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
for definition in filmcraft_project::effect_defs().iter().filter(|d|d.kind==EffectKind::Audio&&!d.intrinsic){for scenario in 0..3 {for mode in [AutomationMode::Read,AutomationMode::Off] {for post in [false,true] {for start in [0i64,1,-17] {for latency in [0usize,97] {
let mut effect=definition.instance();effect.post_fader=post;
if scenario>0 {if let Some(param)=definition.params.iter().find(|p|matches!(p.kind,ParamKind::Float{..})){if let ParamKind::Float{min,max,..}=param.kind {
effect.params.get_mut(param.id).unwrap().keyframes=serde_json::from_value(json!([{"time":0,"value":{"Float":min},"interp":"EaseIn"},{"time":T/100,"value":if scenario==1{json!({"Float":max})}else{json!({"Choice":1})},"interp":"EaseOut"}])).unwrap();}}}
let mut p=Project::new("Insert state");let id=p.new_sequence("Sequence",SequenceSettings::default(),0,1,None);let seq=p.sequence_mut(id).unwrap();seq.audio_tracks[0].effects=vec![effect.clone()];seq.audio_tracks[0].mixer.mode=mode;
let pcm=mixer::inspect_insert_pcm(seq,post,start,193,48000,latency);
cases.push(json!({"effects":[effect],"mixer":{"mode":mode},"post":post,"start":start,"contentLatency":latency,"pcm":pcm}));
}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
