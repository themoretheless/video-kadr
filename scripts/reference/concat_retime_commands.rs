//! Qualification only: execute unchanged pinned project commands and undo/redo.
use concat_project::{Editor,Command};
use serde_json::{json,Value};
fn snapshot(editor:&Editor)->Value{serde_json::to_value(editor.project()).unwrap()}
fn main(){
 let plans:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).unwrap()).unwrap()).unwrap();
 let mut rows=vec![];
 for plan in plans.as_array().unwrap(){
  let mut editor=Editor::from_document(&plan["document"]).unwrap();
  let before=snapshot(&editor);
  let command:Command=serde_json::from_value(plan["command"].clone()).unwrap();
  let outcome=editor.apply(command).unwrap();let after=snapshot(&editor);
  if outcome.applied{assert!(editor.undo());assert_eq!(snapshot(&editor),before);assert!(editor.redo());assert_eq!(snapshot(&editor),after);}else{assert_eq!(after,before);assert!(!editor.can_undo());}
  rows.push(json!({"name":plan["name"],"command":plan["command"],"before":before["timelines"][0]["clips"],"after":after["timelines"][0]["clips"],"applied":outcome.applied,"undoRedoVerified":true}));
 }
 std::fs::write(std::env::args().nth(2).unwrap(),serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
 println!("{} original command scenes with undo/redo",rows.len());
}
