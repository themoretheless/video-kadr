// Original effects.apply command, including transitions and name disambiguation.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn hash(value:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(value).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut session=filmcraft_engine::Session::default();session.execute("file.openDemoProject",json!({}))?;
    let sequence=session.state.active_sequence.unwrap();let base=session.project.clone();
    let seq=base.sequence(sequence).unwrap();let video=seq.video_tracks[0].items[0].id.0;let audio=seq.audio_tracks[0].items[0].id.0;
    let mut queries=Vec::new();
    for def in filmcraft_project::effect_defs(){let audio_kind=matches!(def.kind,filmcraft_project::EffectKind::Audio|filmcraft_project::EffectKind::AudioTransition);let clip=if audio_kind{audio}else{video};queries.push(json!({"effect":def.id,"clip":clip,"clips":[clip],"edge":"out","frames":7}));}
    for effect in ["Gaussian Blur","GAUSSIAN_BLUR","Volume","motion","unknown_effect"] {for clip in [video,audio]{queries.push(json!({"effect":effect,"clip":clip}));}}
    for frames in [0,-1,1,13]{queries.push(json!({"effect":"cross_dissolve","clip":video,"edge":"in","frames":frames,"reverse":true}));}
    queries.push(json!({"effect":"gaussian_blur","clips":[]}));
    queries.push(json!({"effect":"gaussian_blur","clips":[u64::MAX]}));
    queries.push(json!({"effect":"wipe","clip":video,"edge":"out","params":{"direction":"from east"},"reverse":true}));
    queries.push(json!({"effect":"dip_to_color","clip":video,"edge":"out","params":{"color":"#33669980","hold":999.}}));
    queries.push(json!({"effect":"Volume","context":{"selection":[audio]}}));
    queries.push(json!({"effect":"cross_dissolve","context":{"playhead":0,"targetedTracks":[seq.video_tracks[0].id.0],"videoTransitionDuration":11.,"videoTransitionUnit":"frames"}}));
    queries.push(json!({"effect":"constant_power","context":{"playhead":0,"targetedTracks":[seq.audio_tracks[0].id.0],"audioTransitionDuration":0.125,"audioTransitionUnit":"seconds"}}));
    queries.push(json!({"effect":"cross_dissolve","context":{"targetedTracks":[]}}));
    let mut cases=Vec::new();
    for params in queries {
        session.project=base.clone();session.state.playheads.insert(sequence,filmcraft_time::Tick(0));session.state.selection.clear();
        session.state.targeting.remove(&sequence);
        session.prefs.timeline.video_transition_duration=30.;session.prefs.timeline.video_transition_unit="frames".into();session.prefs.timeline.audio_transition_duration=1.;session.prefs.timeline.audio_transition_unit="seconds".into();
        if let Some(context)=params.get("context") {
            if let Some(selection)=context["selection"].as_array(){session.state.selection=selection.iter().map(|id|filmcraft_project::ClipId(id.as_u64().unwrap())).collect();}
            if let Some(time)=context["playhead"].as_i64(){session.state.playheads.insert(sequence,filmcraft_time::Tick(time));}
            if let Some(targets)=context["targetedTracks"].as_array(){session.state.targeting.insert(sequence,filmcraft_engine::Targeting{targeted:targets.iter().map(|id|filmcraft_project::TrackId(id.as_u64().unwrap())).collect(),..Default::default()});}
            if let Some(value)=context["videoTransitionDuration"].as_f64(){session.prefs.timeline.video_transition_duration=value;}
            if let Some(value)=context["videoTransitionUnit"].as_str(){session.prefs.timeline.video_transition_unit=value.into();}
            if let Some(value)=context["audioTransitionDuration"].as_f64(){session.prefs.timeline.audio_transition_duration=value;}
            if let Some(value)=context["audioTransitionUnit"].as_str(){session.prefs.timeline.audio_transition_unit=value.into();}
        }
        let result=session.execute("effects.apply",params.clone());let project=serde_json::to_value(session.project.as_ref())?;
        cases.push(json!({"params":params,"ok":result.is_ok(),"result":result.ok(),"projectSha256":hash(&project)}));
    }
    let output=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":sequence.0,"videoClip":video,"audioClip":audio,"project":serde_json::to_value(base.as_ref())?,"cases":cases});
    let mut output=output.as_object().unwrap().clone();let cases=output.remove("cases").unwrap();let mut text=serde_json::to_string(&output)?;text.pop();text.push_str(",\"cases\":[\n");
    for(i,case)in cases.as_array().unwrap().iter().enumerate(){if i>0{text.push_str(",\n");}text.push_str(&serde_json::to_string(case)?);}text.push_str("\n]}\n");std::fs::write(std::env::args().nth(1).ok_or("output required")?,text)?;Ok(())
}
