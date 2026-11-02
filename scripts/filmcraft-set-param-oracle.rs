// Actual original effects.setParam across definitions, clocks, masks and animation.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn hash(value:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(value).unwrap()))}
fn sample(value:&filmcraft_project::ParamValue)->Value {use filmcraft_project::ParamValue as V;match value {
 V::Float(v)=>json!(v+0.125),V::Vec2(_)=>json!([17.25,-9.5]),V::Color(_)=>json!([0.1,0.2,0.3,0.4]),V::Bool(v)=>json!(!v),V::Choice(_)=>json!(0),V::Text(_)=>json!("changed\nsource"),V::Curve(_)=>json!([[0.,0.],[0.5,0.75],[1.,1.],"ignored",[0.1]]),V::Path(_)=>json!([[1.,2.],[3.,4.],[5.,6.]])}}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut session=filmcraft_engine::Session::default();session.execute("file.openDemoProject",json!({}))?;
    let sequence=session.state.active_sequence.unwrap();let base=session.project.clone();let clip=base.sequence(sequence).unwrap().video_tracks[0].items[0].id;
    let mut instances=Vec::new();let mut jobs:Vec<(usize,Value,Value)>=Vec::new();
    for definition in filmcraft_project::effect_defs(){let mut effect=definition.instance();effect.enabled=false;let index=instances.len();instances.push(effect.clone());for(param,value)in &effect.params {jobs.push((index,json!({"clip":clip.0,"effect":definition.id,"param":param,"value":sample(&value.value),"frame":7}),json!({})));}}
    let mut motion=filmcraft_project::find_effect("motion").unwrap().instance();motion.enabled=false;
    motion.params.get_mut("scale").unwrap().keyframes=vec![filmcraft_project::Keyframe::new(filmcraft_time::Tick(0),filmcraft_project::ParamValue::Float(25.)),filmcraft_project::Keyframe::new(filmcraft_time::Tick(15*254016000000),filmcraft_project::ParamValue::Float(75.))];
    motion.params.get_mut("scale").unwrap().keyframes[0].interp=filmcraft_project::Interpolation::Hold;
    let index=instances.len();instances.push(motion.clone());
    for (speed,reverse,held,filters,duration)in [(2.,false,None,false,4),(2.,true,None,false,4),(2.,true,Some(8),false,4),(2.,true,Some(8),true,4),(0.,false,None,false,4),(-2.,false,None,false,4),(2.,true,None,false,0),(2.,true,None,false,-1)] {
        for time in [-254016000000i64,3*254016000000,30*254016000000] {
            jobs.push((index,json!({"clip":clip.0,"effect":"motion","param":"scale","value":123.,"time":time,"merge":true,"begin":true}),json!({"start":0,"duration":duration*254016000000i64,"source_in":7*254016000000i64,"speed":speed,"reverse":reverse,"frame_hold":held.map(|v|v*254016000000i64),"hold_filters":filters})));
        }
    }
    for params in [json!({"frame":12}),json!({"seconds":0.5}),json!({"timecode":"00:00:00:12"}),json!({"timecode":"invalid"})] {
        let mut p=json!({"clip":clip.0,"effect":0,"param":"scale","value":44.});for(k,v)in params.as_object().unwrap(){p[k]=v.clone();}jobs.push((index,p,json!({})));
    }
    motion.params.remove("scale");let legacy=instances.len();instances.push(motion);jobs.push((legacy,json!({"clip":clip.0,"effect":"motion","param":"scale","value":51.}),json!({})));
    let mut opacity=filmcraft_project::find_effect("opacity").unwrap().instance();opacity.enabled=false;opacity.masks.push(filmcraft_project::mask::Mask::new("Test",filmcraft_project::mask::MaskPath::rect(0.,0.,10.,10.)));
    let mask=instances.len();instances.push(opacity.clone());
    for param in ["path","feather","opacity","expansion"] {let value=sample(&opacity.masks[0].param(param).unwrap().value);jobs.push((mask,json!({"clip":clip.0,"effect":"opacity","mask":0,"param":param,"value":value}),json!({})));}
    let color=filmcraft_project::effect_defs().iter().find_map(|d|d.params.iter().find(|p|matches!(p.default,filmcraft_project::ParamValue::Color(_))).map(|p|(d,p))).unwrap();
    let ci=instances.len();instances.push(color.0.instance());jobs.push((ci,json!({"clip":clip.0,"effect":color.0.id,"param":color.1.id,"value":"#33669980"}),json!({})));
    for (effect,param,value)in [(json!("missing"),"scale",json!(1)),(json!(0),"scale",json!("wrong")),(json!("motion"),"missing",json!(1))]{jobs.push((index,json!({"clip":clip.0,"effect":effect,"param":param,"value":value}),json!({})));}
    let mut cases=Vec::new();
    for (instance,params,changes)in jobs {
        session.project=base.clone();session.history.undo.clear();session.history.redo.clear();session.history.merge_key=None;session.state.playheads.insert(sequence,filmcraft_time::Tick(0));
        let item=&mut std::sync::Arc::make_mut(&mut session.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0];
        let mut raw=serde_json::to_value(&*item)?;for(k,v)in changes.as_object().unwrap(){raw[k]=v.clone();}*item=serde_json::from_value(raw)?;item.effects=vec![instances[instance].clone()];
        // Record the actual typed source input, including skipped-default fields.
        let before_item=serde_json::to_value(&*item)?;let mut canonical_changes=serde_json::Map::new();let mut removals=Vec::new();
        for key in changes.as_object().unwrap().keys(){if let Some(value)=before_item.get(key){canonical_changes.insert(key.clone(),value.clone());}else{removals.push(key.clone());}}
        let result=session.execute("effects.setParam",params.clone());let project=serde_json::to_value(session.project.as_ref())?;
        cases.push(json!({"instance":instance,"params":params,"clipChanges":canonical_changes,"clipRemovals":removals,"ok":result.is_ok(),"projectSha256":hash(&project)}));
    }
    let mut output=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":sequence.0,"clip":clip.0,"project":serde_json::to_value(base.as_ref())?,"instances":instances}).as_object().unwrap().clone();
    let instances=output.remove("instances").unwrap();let mut text=serde_json::to_string(&output)?;text.pop();text.push_str(",\"instances\":[\n");for(i,v)in instances.as_array().unwrap().iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(v)?);}text.push_str("\n],\"cases\":[\n");for(i,v)in cases.iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(v)?);}text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
