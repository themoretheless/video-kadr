use serde_json::{json,Value};use sha2::{Digest,Sha256};
use video_kadr_backend::domain::filmcraft_labels::edit;
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-label-oracle.json")).unwrap()}
#[test]
fn generic_and_all_named_labels_match_original_selection_semantics(){
 let o=oracle();assert_eq!(video_kadr_backend::domain::filmcraft_labels::palette(),o["palette"]);assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),1637);
 for case in o["cases"].as_array().unwrap(){
  let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["locked"]=case["locked"].clone();
  let context=case["context"].as_u64().unwrap();let mut p=case["params"].clone();p["document"]=d;p["context"]=json!({"activeSequence":if context==1{Value::Null}else{o["sequence"].clone()},"selection":if context==2||context==3{vec![o["clip"].clone()]}else{vec![]},"projectSelection":if context==1||context==3{vec![o["item"].clone()]}else{vec![]}});
  let result=edit(case["command"].as_str().unwrap(),p);assert_eq!(result.is_ok(),case["ok"].as_bool().unwrap(),"{} c={context} {}: {:?}",case["command"],case["params"],result.as_ref().err());
  if let Ok(result)=result{assert_eq!(format!("{:x}",Sha256::digest(serde_json::to_vec(&result["document"]["project"]).unwrap())),case["afterSha256"],"{} c={context} {}",case["command"],case["params"]);assert_eq!(result["result"],case["result"])}
 }
}
#[tokio::test]
async fn exact_source_ids_and_label_manifest_roundtrip(){
 use video_kadr_backend::{error::ApiJson,handlers::filmcraft_project::{import,ImportRequest}};
 let o=oracle();let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let mut item=d["project"]["items"][o["item"].to_string()].clone();item["id"]=json!(u64::MAX);d["project"]["items"][u64::MAX.to_string()]=item;
 d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["id"]=json!(u64::MAX-1);
 let r=edit("edit.label.magenta",json!({"document":serde_json::to_string(&d).unwrap(),"sequence":o["sequence"].to_string(),"items":[u64::MAX.to_string()],"clips":[(u64::MAX-1).to_string()]})).unwrap();assert_eq!(r["result"]["labelled"],2);
 let request:ImportRequest=serde_json::from_value(json!({"document":r["documentText"]})).unwrap();let manifest=import(ApiJson(request)).await.unwrap().0;assert_eq!(manifest["labelPalette"],o["palette"]);
 assert_eq!(manifest["projectItems"].as_array().unwrap().iter().find(|row|row["id"]==u64::MAX.to_string()).unwrap()["label"],"Magenta");
 assert!(manifest["sequences"].as_array().unwrap().iter().flat_map(|s|s["clips"].as_array().unwrap()).any(|c|c["id"]==(u64::MAX-1).to_string()&&c["label"]=="Magenta"));
 assert!(edit("edit.label.orange",json!({"document":d})).is_err());
}
