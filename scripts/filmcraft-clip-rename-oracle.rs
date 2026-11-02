// Execute the pinned original command; never compute expected mutations locally.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn hash(v: &Value) -> String { format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap())) }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = filmcraft_engine::Session::default();
    s.execute("file.openDemoProject", json!({}))?;
    let seq = s.state.active_sequence.unwrap();
    let clip = s.project.sequence(seq).unwrap().video_tracks[0].items[0].id;
    let base = s.project.clone();
    let project = serde_json::to_value(base.as_ref())?;
    let mut cases = Vec::new();
    for locked in [false, true] {
        for active in [false, true] {
            for selected in [false, true] {
                for name in [json!("Renamed"),json!(""),json!("новое 🎬 / ~"),json!(null),json!(42)] {
                    for target in [json!({}),json!({"clip":clip.0}),json!({"item":seq.0}),json!({"item":seq.0,"clip":u64::MAX}),json!({"clip":u64::MAX}),json!({"item":u64::MAX}),json!({"clip":null}),json!({"item":false})] {
                        s.project = base.clone();
                        std::sync::Arc::make_mut(&mut s.project).sequence_mut(seq).unwrap().video_tracks[0].locked = locked;
                        s.state.active_sequence = active.then_some(seq);
                        s.state.selection = if selected {vec![clip]} else {vec![]};
                        let mut params = target;
                        params["name"] = name.clone();
                        let result = s.execute("clip.rename", params.clone());
                        let after = serde_json::to_value(s.project.as_ref())?;
                        cases.push(json!({"locked":locked,"active":active,"selected":selected,"params":params,"ok":result.is_ok(),"afterSha256":hash(&after)}));
                    }
                }
            }
        }
    }
    let output = json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"clip":clip.0,"project":project,"cases":cases});
    std::fs::write(std::env::args().nth(1).ok_or("output required")?, serde_json::to_vec(&output)?)?;
    Ok(())
}
