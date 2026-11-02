//! Driver-only arithmetic: direct original MoveClips calls avoid a second JSON f64 parse.
use concat_project::{commands::ClipMove, Command, Editor};
use serde_json::{json, Value};
fn snapshot(editor: &Editor) -> Value { serde_json::to_value(editor.project()).unwrap() }
fn main() {
    let plans: Vec<Value> = serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
    let mut rows = vec![];
    for plan in plans {
        let mut editor = Editor::from_document(&plan["document"]).unwrap();
        let before = snapshot(&editor);
        let old = editor.project().active().clip("c").unwrap().start;
        let wire_target = plan["wireTarget"].as_u64().unwrap();
        let delta = (wire_target as i128 - (old * 1e6).round() as i128) as f64 / 1e6;
        let command = Command::MoveClips { moves: vec![ClipMove { clip_id: "c".into(), track_id: "v".into(), start: old + delta }] };
        let command_json = serde_json::to_value(&command).unwrap();
        let outcome = editor.apply(command).unwrap();
        let after = snapshot(&editor);
        if outcome.applied {
            assert!(editor.undo()); assert_eq!(snapshot(&editor), before);
            assert!(editor.redo()); assert_eq!(snapshot(&editor), after);
        } else { assert_eq!(after, before); assert!(!editor.can_undo()); }
        rows.push(json!({"name":plan["name"],"wireTarget":wire_target,"command":command_json,"before":before["timelines"][0]["clips"],"after":after["timelines"][0]["clips"],"applied":outcome.applied,"undoRedoVerified":true}));
    }
    std::fs::write(std::env::args().nth(2).unwrap(),serde_json::to_vec(&rows).unwrap()).unwrap();
    println!("{} direct original move scenes with undo/redo",rows.len());
}
