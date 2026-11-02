// Execute the pinned original command; never compute expected mutations locally.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
fn hash(v: &Value) -> String { format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap())) }
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = filmcraft_engine::Session::default();
    s.execute("file.openDemoProject", json!({}))?;
    let seq = s.state.active_sequence.unwrap();
    let clip = s.project.sequence(seq).unwrap().video_tracks[0].items[0].id;
    // Use actual source constructors for every item variant, including variants
    // absent from the demo. Expected mutations still execute only the command.
    {
        let p = std::sync::Arc::make_mut(&mut s.project);
        let media = p.items.values().find(|i|matches!(i.kind, filmcraft_project::ItemKind::Media(_))).unwrap().clone();
        p.add_item("Subclip", media.label, filmcraft_project::ItemKind::Subclip {
            parent: media.id, range: filmcraft_time::TimeRange::new(filmcraft_time::Tick(0), filmcraft_time::Tick(254016000000)), restrict_trims: false
        }, None);
        p.add_item("Graphic", media.label, filmcraft_project::ItemKind::Graphic {
            width:1920, height:1080, rate:filmcraft_time::FrameRate::default()
        }, None);
        p.add_item("Adjustment", media.label, filmcraft_project::ItemKind::AdjustmentLayer {
            width:1920, height:1080, rate:filmcraft_time::FrameRate::default(), duration:filmcraft_time::Tick(254016000000)
        }, None);
    }
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
    let mut item_kinds = std::collections::BTreeSet::new();
    for (id, item) in &base.items {
        let kind = serde_json::to_value(&item.kind)?.as_object().unwrap().keys().next().unwrap().clone();
        item_kinds.insert(kind.clone());
        for name in ["", "новое 🎬"] {
            s.project = base.clone(); s.state.active_sequence = None; s.state.selection.clear();
            let params = json!({"item":id.0,"name":name});
            let result = s.execute("clip.rename", params.clone());
            cases.push(json!({"locked":false,"active":false,"selected":false,"kind":kind,"params":params,"ok":result.is_ok(),"afterSha256":hash(&serde_json::to_value(s.project.as_ref())?)}));
        }
    }
    let output = json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","sequence":seq.0,"clip":clip.0,"project":project,"itemKinds":item_kinds,"cases":cases});
    std::fs::write(std::env::args().nth(1).ok_or("output required")?, serde_json::to_vec(&output)?)?;
    Ok(())
}
