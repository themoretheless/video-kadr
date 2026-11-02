// Execute the pinned MIT engine, never the native implementation, for expected results.
use filmcraft_project::*;
use filmcraft_time::Tick;
use serde_json::{json};
use sha2::{Digest, Sha256};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut s = filmcraft_engine::Session::default();
    s.execute("file.openDemoProject", json!({}))?;
    let seq = s.state.active_sequence.unwrap();
    let p = std::sync::Arc::make_mut(&mut s.project);
    let media = p.items.values().find(|i| matches!(i.kind, ItemKind::Media(_))).unwrap().id;
    let second = p.add_item("Other sequence", p.item(seq).unwrap().label, p.item(seq).unwrap().kind.clone(), None);
    let parent = p.add_bin("Parent", None);
    let child = p.add_bin("Child", Some(parent));
    p.move_to_bin(&[media], Some(child));
    let sub = p.add_item("Dependent subclip", p.item(seq).unwrap().label, ItemKind::Subclip { parent: media, range: filmcraft_time::TimeRange::new(Tick(0), Tick(100)), restrict_trims: true }, None);
    // Preserve the original remove_item first-subtree behavior with duplicate references.
    p.root.children.push(BinEntry::Item(media));
    p.root.find_bin_mut(parent).unwrap().children.push(BinEntry::Item(sub));
    let clip = p.sequence(seq).unwrap().video_tracks[0].items[0].id;
    for sid in [seq, second] {
        let q = p.sequence_mut(sid).unwrap();
        let mut effect = find_effect("opacity").unwrap().instance();
        effect.masks.push(Mask::new("selected", MaskPath::rect(0., 0., 20., 30.)));
        q.video_tracks[0].items[0].effects = vec![effect];
        for t in q.all_tracks_mut() {
            if let Some(c) = t.items.first() {
                let c = c.id;
                for (i, (from, to)) in [(Some(c), None), (None, Some(c)), (Some(c), Some(ClipId(u64::MAX))), (None, None)].into_iter().enumerate() {
                    t.transitions.push(Transition { id: TransitionId(50000 + i as u64), effect: find_effect("opacity").unwrap().instance(), start: Tick(0), duration: Tick(20), from, to, align: TransitionAlign::default(), reverse: false });
                }
            }
        }
        let mut track = CaptionTrack::new(TrackId(60000), "Captions".into(), CaptionFormat::default());
        track.captions.push(Caption { id: ClipId(60001), start: Tick(0), duration: Tick(100), text: "Caption".into(), speaker: None, cue_id: None, settings: String::new() });
        q.caption_tracks.push(track);
    }
    let base = s.project.clone();
    let selections = [vec![], vec![media], vec![ItemId(parent.0)], vec![ItemId(u64::MAX)]];
    let params = vec![json!({}), json!({"items":null}), json!({"items":false}), json!({"items":[]}), json!({"items":[media.0]}), json!({"items":[seq.0]}), json!({"items":[seq.0,second.0]}), json!({"items":[parent.0]}), json!({"items":[child.0,parent.0]}), json!({"items":[parent.0,parent.0]}), json!({"items":[child.0,child.0]}), json!({"items":[0,u64::MAX]}), json!({"items":[true,1.5,"invalid",media.0,media.0]}), json!({"items":[sub.0,media.0]})];
    let mut cases = vec![];
    for active in [None, Some(seq), Some(second), Some(ItemId(u64::MAX))] {
        for selection in &selections {
            for params in &params {
                s.project = base.clone();
                s.history.undo.clear(); s.history.redo.clear();
                s.state.active_sequence = active;
                s.state.project_selection = selection.clone();
                s.state.selection = vec![clip, ClipId(u64::MAX), clip];
                s.state.source_item = Some(media);
                s.state.open_sequences = vec![second, seq, second, ItemId(u64::MAX)];
                s.state.caption_selection = vec![ClipId(60001), ClipId(u64::MAX), ClipId(60001)];
                s.state.selected_mask = Some(filmcraft_engine::masks::MaskSel { clip, effect: 0, mask: 0 });
                s.state.timeline_views.clear();
                for id in [seq,second,ItemId(u64::MAX)] { s.state.timeline_views.insert(id, SequenceView { pps: 33., scroll: 2., v_scroll: 0., a_scroll: 0., video_track_h: 30., audio_track_h: 20. }); }
                let context = |s: &filmcraft_engine::Session| json!({"activeSequence":s.state.active_sequence,"projectSelection":s.state.project_selection,"selection":s.state.selection,"sourceItem":s.state.source_item,"openSequences":s.state.open_sequences,"captionSelection":s.state.caption_selection,"selectedMask":s.state.selected_mask,"timelineViews":s.state.timeline_views});
                let before = context(&s);
                let result = s.execute("project.delete", params.clone());
                let after = serde_json::to_value(s.project.as_ref())?;
                cases.push(json!({"params":params,"context":before,"ok":result.is_ok(),"result":result.ok(),"afterContext":context(&s),"afterSha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&after)?)),"undoDelta":s.history.undo.len()}));
            }
        }
    }
    std::fs::write(std::env::args().nth(1).ok_or("output required")?, serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":base.as_ref(),"cases":cases}))?)?;
    Ok(())
}
