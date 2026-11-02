use serde_json::{json,Value};use video_kadr_backend::domain::filmcraft_label_group::select;
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-label-group-oracle.json")).unwrap()}
fn strings(v:&Value)->Value{json!(v.as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>())}
#[test]
fn selection_and_unchanged_document_match_actual_original(){
 let o=oracle();assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),300);
 for case in o["cases"].as_array().unwrap(){
  assert_eq!(case["unchanged"],true);
  let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});for tracks in ["video_tracks","audio_tracks"]{for track in d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"][tracks].as_array_mut().unwrap(){track["locked"]=case["locked"].clone();}}
  let mut p=case["params"].clone();p["document"]=d;p["context"]=json!({"activeSequence":if case["active"]==true{o["sequence"].clone()}else{Value::Null},"selection":case["selection"],"projectSelection":case["projectSelection"]});
  let actual=select(p);assert_eq!(actual.is_ok(),case["ok"].as_bool().unwrap(),"case {case}: {:?}",actual.as_ref().err());
  if let Ok(actual)=actual{let mut expected=case["result"].clone();for key in ["clips","items"]{if expected.get(key).is_some(){expected[key]=strings(&expected[key]);}}
   assert_eq!(actual["result"],expected);assert_eq!(actual["context"]["selection"],strings(&case["afterSelection"]));assert_eq!(actual["context"]["projectSelection"],strings(&case["afterProjectSelection"]));assert!(actual.get("documentText").is_none());assert!(actual.get("document").is_none());
  }
 }
}
#[test]
fn tree_order_duplicates_and_unlinked_items_keep_exact_ids(){
 let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":oracle()["project"]});let template=d["project"]["items"].as_object().unwrap().values().next().unwrap().clone();
 for id in [u64::MAX,u64::MAX-1]{let mut item=template.clone();item["id"]=json!(id);d["project"]["items"][id.to_string()]=item;}
 d["project"]["root"]["children"].as_array_mut().unwrap().extend([json!({"Item":u64::MAX}),json!({"Item":u64::MAX})]);
 let result=select(json!({"document":serde_json::to_string(&d).unwrap(),"context":{"projectSelection":[u64::MAX.to_string()]}})).unwrap();let rows=result["result"]["items"].as_array().unwrap();
 assert_eq!(rows.iter().filter(|v|**v==u64::MAX.to_string()).count(),2);assert_eq!(&rows[rows.len()-2..],&[json!(u64::MAX.to_string()),json!(u64::MAX.to_string())]);assert!(!rows.contains(&json!((u64::MAX-1).to_string())));
}
