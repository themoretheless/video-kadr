use serde_json::{json,Value};use video_kadr_backend::domain::filmcraft_selection::select;
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-selection-oracle.json")).unwrap()}
fn strings(v:&Value)->Value{json!(v.as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>())}
#[test]
fn matching_and_project_commands_match_actual_source_without_document_edits(){
 let o=oracle();assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),576);
 for case in o["cases"].as_array().unwrap(){
  assert_eq!(case["unchanged"],true);let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});for tracks in ["video_tracks","audio_tracks"]{for track in d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"][tracks].as_array_mut().unwrap(){track["locked"]=case["locked"].clone();}}
  let mut p=case["params"].clone();p["document"]=d;p["context"]=json!({"activeSequence":if case["active"]==true{o["sequence"].clone()}else{Value::Null},"selection":case["selection"],"projectSelection":case["projectSelection"]});
  let actual=select(case["command"].as_str().unwrap(),p);assert_eq!(actual.is_ok(),case["ok"].as_bool().unwrap(),"case {case}: {:?}",actual.as_ref().err());
  if let Ok(actual)=actual{let mut expected=case["result"].clone();if expected.get("clips").is_some(){expected["clips"]=strings(&expected["clips"])}assert_eq!(actual["result"],expected);assert_eq!(actual["context"]["selection"],strings(&case["afterSelection"]));assert_eq!(actual["context"]["projectSelection"],strings(&case["afterProjectSelection"]));assert!(actual.get("documentText").is_none());if case["command"]=="project.selectAll"{assert!(actual["context"]["projectSelection"].as_array().unwrap().contains(&json!(o["orphan"].as_u64().unwrap().to_string())));}}
 }
}
#[test]
fn exact_ids_repeated_selection_and_unlinked_map_order(){
 let o=oracle();let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let id=u64::MAX-3;let clip=u64::MAX-2;let extra=u64::MAX-1;
 let mut item=d["project"]["items"].as_object().unwrap().values().next().unwrap().clone();item["id"]=json!(id);d["project"]["items"][id.to_string()]=item;d["project"]["next_id"]=json!(u64::MAX);
 let q=&mut d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"];q["video_tracks"][0]["items"][0]["id"]=json!(clip);q["video_tracks"][0]["items"][0]["item"]=json!(id);let mut copy=q["video_tracks"][0]["items"][0].clone();copy["id"]=json!(extra);q["audio_tracks"][0]["items"].as_array_mut().unwrap().push(copy);
 let result=select("edit.selectAllMatching",json!({"document":serde_json::to_string(&d).unwrap(),"sequence":o["sequence"].to_string(),"clips":[clip.to_string()],"context":{"projectSelection":[id.to_string()]}})).unwrap();assert_eq!(result["result"]["clips"],json!([clip.to_string(),extra.to_string()]));assert_eq!(result["context"]["projectSelection"],json!([id.to_string()]));
 let result=select("project.selectAll",json!({"document":d})).unwrap();assert_eq!(result["context"]["projectSelection"].as_array().unwrap().last().unwrap(),&json!(id.to_string()));
 let result=select("project.select",json!({"document":d,"items":[id.to_string(),u64::MAX.to_string(),id.to_string(),false,1.5],"context":{"selection":[clip.to_string()]}})).unwrap();assert_eq!(result["context"]["projectSelection"],json!([id.to_string(),u64::MAX.to_string(),id.to_string()]));assert_eq!(result["context"]["selection"],json!([clip.to_string()]));
}
