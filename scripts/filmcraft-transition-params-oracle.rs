use serde_json::{Value,json};use filmcraft_project::{ParamValue,EffectKind};
type Result<T>=std::result::Result<T,String>;
fn bad(cmd:&str,msg:impl std::fmt::Display)->String{format!("{cmd}: {msg}")}
// No transition definition uses paths; this unexercised adapter only satisfies the engine helper signature.
mod masks{pub fn path_from_json(_: &serde_json::Value)->Option<filmcraft_project::mask::MaskPath>{None}}
__CONVERT__
__PARAMS__
fn main(){let mut catalog=vec![];let mut cases=vec![];
for def in filmcraft_project::effect::effect_defs().iter().filter(|def|matches!(def.kind,EffectKind::VideoTransition|EffectKind::AudioTransition)){
 assert!(def.params.iter().all(|p|!matches!(p.default,ParamValue::Path(_))));
 catalog.push(json!({"definition":def,"instance":def.instance()}));
 let mut patches=vec![Value::Null,json!({}),json!({"unknown":1})];
 for p in &def.params{for v in [Value::Null,json!(true),json!(false),json!(-1),json!(0),json!(1),json!(999999),json!(4294967296_u64),json!(0.25),json!("invalid"),json!("#12abef80"),json!([0.1,0.2]),json!([0.1,0.2,0.3]),json!([0.1,0.2,0.3,0.4]),json!({})]{let mut patch=json!({});patch[p.id]=v;patches.push(patch);}
 if let filmcraft_project::ParamKind::Choice(opts)=&p.kind{for option in *opts{let mut patch=json!({});patch[p.id]=json!(option.to_lowercase());patches.push(patch);}}
 }
 for patch in patches{let before=def.instance();let mut after=before.clone();let error=set_transition_params(&mut after,&patch,"transition").is_err();cases.push(json!({"before":before,"params":patch,"expected":if error{None}else{Some(after)},"error":error}));}
}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","catalog":catalog,"cases":cases}));}
