use serde_json::{json,Value};use sha2::{Digest,Sha256};
use video_kadr_backend::domain::filmcraft_bin_edit::edit;
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-bin-edit-oracle.json")).unwrap()}
#[test]
fn create_group_and_move_match_original_engine(){
 let o=oracle();assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),238);
 for case in o["cases"].as_array().unwrap(){
  let mut p=case["params"].clone();p["document"]=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});p["context"]=json!({"projectSelection":if case["selected"]==true{o["selection"].clone()}else{json!([])}});
  let actual=edit(case["command"].as_str().unwrap(),p);assert_eq!(actual.is_ok(),case["ok"].as_bool().unwrap(),"{} {}: {:?}",case["command"],case["params"],actual.as_ref().err());
  if let Ok(actual)=actual {
   assert_eq!(format!("{:x}",Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap())),case["afterSha256"],"{} {}",case["command"],case["params"]);
   let mut expected=case["result"].clone();if let Some(id)=expected["bin"].as_u64(){expected["bin"]=json!(id.to_string())}assert_eq!(actual["result"],expected);
  }
 }
}
#[test]
fn exact_id_allocation_exhaustion_and_selection_validation(){
 let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":oracle()["project"]});d["project"]["next_id"]=json!(u64::MAX-1);
 let r=edit("file.newBin",json!({"document":serde_json::to_string(&d).unwrap(),"name":"Exact"})).unwrap();assert_eq!(r["result"]["bin"],(u64::MAX-1).to_string());assert_eq!(r["document"]["project"]["next_id"],u64::MAX);
 assert!(edit("file.newBin",json!({"document":r["documentText"]})).is_err());
 assert!(edit("project.moveToBin",json!({"document":d,"context":{"projectSelection":[null]}})).is_err());
 d["project"]["next_id"]=json!(1);assert!(edit("file.newBin",json!({"document":d})).is_err());
}
#[tokio::test]
async fn moved_membership_is_imported_without_changing_source_items(){
 use video_kadr_backend::{error::ApiJson,handlers::filmcraft_project::{import,ImportRequest}};
 let o=oracle();let d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let item=o["selection"][0].as_u64().unwrap().to_string();
 let created=edit("file.newBin",json!({"document":d,"name":"Target"})).unwrap();let bin=created["result"]["bin"].clone();
 let moved=edit("project.moveToBin",json!({"document":created["documentText"],"items":[item],"bin":bin})).unwrap();
 let request:ImportRequest=serde_json::from_value(json!({"document":moved["documentText"]})).unwrap();let manifest=import(ApiJson(request)).await.unwrap().0;
 let rows=manifest["projectBins"].as_array().unwrap();let target=rows.iter().find(|row|row["id"]==bin).unwrap();assert_eq!(target["itemIds"],json!([item]));
 assert_eq!(rows.iter().filter(|row|row["itemIds"].as_array().unwrap().iter().any(|v|v==&item)).count(),1);
 assert_eq!(moved["document"]["project"]["items"],d["project"]["items"]);
}
