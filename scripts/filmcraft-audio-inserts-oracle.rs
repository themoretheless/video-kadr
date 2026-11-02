pub use filmcraft_render::{SourceProvider,MAX_NEST_DEPTH,transitions,remix};
#[path="__AUDIO_PATH__"] pub mod audio;
#[path="__AUDIO_FX_PATH__"] pub mod audio_fx;
mod mixer;
use filmcraft_project::{Project,SequenceSettings,EffectKind,ParamKind};
use filmcraft_project::mixer::AutomationMode;
use serde_json::json;
const T:i64=254016000000;
fn main(){let mut cases=vec![];
for definition in filmcraft_project::effect_defs().iter().filter(|d|d.kind==EffectKind::Audio&&!d.intrinsic){for scenario in 0..4 {for mode in AutomationMode::ALL {for master in [false,true] {for at in [0i64,17,T/2] {
let mut effect=definition.instance();
if scenario==0 {if let Some(param)=definition.params.iter().find(|p|matches!(p.kind,ParamKind::Float{..})){if let ParamKind::Float{min,max,..}=param.kind {effect.params.get_mut(param.id).unwrap().keyframes=serde_json::from_value(json!([{"time":17,"value":{"Float":min},"interp":"EaseIn"},{"time":T,"value":{"Float":max},"interp":"EaseOut"}])).unwrap();}}}
let mut effects=vec![effect.clone()];
if scenario==1 {effects[0].enabled=false;effects.push(effect.clone());}
if scenario==2 {effects[0].params.insert("bypass".into(),serde_json::from_value(json!({"value":{"Bool":true},"keyframes":[{"time":0,"value":{"Bool":false},"interp":"Linear"}]})).unwrap());effects[0].post_fader=true;}
if scenario==3 {let mut unknown=effect.clone();unknown.effect="unknown-effect".into();effects.push(unknown);let mut post=effect.clone();post.post_fader=true;effects.push(post);let mut disabled=effect.clone();disabled.enabled=false;effects.push(disabled);let mut defaults=effect.clone();defaults.params.clear();effects.push(defaults);effects.push(effect.clone());}
let mut p=Project::new("Insert selection");let id=p.new_sequence("Sequence",SequenceSettings::default(),0,1,None);let seq=p.sequence_mut(id).unwrap();
if master {seq.master_effects=effects.clone();seq.master_mixer.mode=mode;}else{seq.audio_tracks[0].effects=effects.clone();seq.audio_tracks[0].mixer.mode=mode;}
let expected=mixer::inspect_inserts(seq,master,at);
cases.push(json!({"effects":effects,"mixer":{"mode":mode},"master":master,"at":at.to_string(),"expected":expected}));
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
