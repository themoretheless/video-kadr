// Execute actual original rack commands against an in-memory demo project.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn hash(value:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(value).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut session=filmcraft_engine::Session::default();session.execute("file.openDemoProject",json!({}))?;
    let sequence=session.state.active_sequence.unwrap();
    let clip=session.project.sequence(sequence).unwrap().video_tracks[0].items[0].id;
    let base=serde_json::to_value(session.project.as_ref())?;
    let mut cases=Vec::new();
    for definition in filmcraft_project::effect_defs() {
        let mut effect=definition.instance();effect.enabled=false;effect.post_fader=true;effect.essential=true;
        for param in effect.params.values_mut(){param.keyframes.push(filmcraft_project::keyframe::Keyframe::new(filmcraft_time::Tick(0),param.value.clone()));}
        let before=serde_json::to_value(vec![effect.clone()])?;
        std::sync::Arc::make_mut(&mut session.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0].effects=vec![effect];
        let result=session.execute("effects.reset",json!({"clip":clip.0,"index":0}));
        let after=serde_json::to_value(&session.project.sequence(sequence).unwrap().video_tracks[0].items[0].effects)?;
        cases.push(json!({"command":"effects.reset","index":0,"before":before,"ok":result.is_ok(),"afterSha256":hash(&after)}));
    }
    for (command,index,effect) in [("effects.toggleEnabled",0,"motion"),("effects.toggleEnabled",99,"motion"),("effects.remove",0,"motion"),("effects.remove",0,"gaussian_blur"),("effects.remove",99,"gaussian_blur")] {
        let instance=filmcraft_project::find_effect(effect).unwrap().instance();let before=serde_json::to_value(vec![instance.clone()])?;
        std::sync::Arc::make_mut(&mut session.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0].effects=vec![instance];
        let result=session.execute(command,json!({"clip":clip.0,"index":index}));
        let after=serde_json::to_value(&session.project.sequence(sequence).unwrap().video_tracks[0].items[0].effects)?;
        cases.push(json!({"command":command,"index":index,"before":before,"ok":result.is_ok(),"afterSha256":hash(&after)}));
    }
    for command in ["effects.reset","effects.toggleEnabled","effects.remove"] {
        let mut instance=filmcraft_project::find_effect("gaussian_blur").unwrap().instance();instance.effect="future_unknown_effect".into();
        let before=serde_json::to_value(vec![instance.clone()])?;
        std::sync::Arc::make_mut(&mut session.project).sequence_mut(sequence).unwrap().video_tracks[0].items[0].effects=vec![instance];
        let result=session.execute(command,json!({"clip":clip.0,"index":0}));
        let after=serde_json::to_value(&session.project.sequence(sequence).unwrap().video_tracks[0].items[0].effects)?;
        cases.push(json!({"command":command,"index":0,"before":before,"ok":result.is_ok(),"afterSha256":hash(&after)}));
    }
    let output=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":sequence.0,"clip":clip.0,"project":base,"cases":cases});
    // One case per line avoids huge pretty-printed parameter dumps.
    let mut output=output.as_object().unwrap().clone();let cases=output.remove("cases").unwrap();
    let mut text=serde_json::to_string(&output)?;text.pop();text.push_str(",\"cases\":[\n");
    for (i,case)in cases.as_array().unwrap().iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(case)?);}
    text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
