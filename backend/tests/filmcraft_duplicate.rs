use serde_json::{json,Value};use sha2::{Digest,Sha256};use video_kadr_backend::domain::filmcraft_duplicate::duplicate;
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-duplicate-oracle.json")).unwrap()}
fn strings(v:&Value)->Value{json!(v.as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>())}
#[test]
fn all_item_kinds_evolving_ids_and_cycle_refusal_match_original(){
 let o=oracle();assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),126);
 for case in o["cases"].as_array().unwrap(){let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let profile=case["profile"].as_u64().unwrap();if profile>0{let target=if profile==1{o["project"]["next_id"].clone()}else{o["sequence"].clone()};d["project"]["items"][o["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["item"]=target;}
  let mut p=case["params"].clone();p["document"]=d;p["context"]=json!({"projectSelection":case["selection"]});let actual=duplicate(p);assert_eq!(actual.is_ok(),case["ok"].as_bool().unwrap(),"case {case}: {:?}",actual.as_ref().err());
  assert_eq!(case["undoDelta"],if actual.is_ok(){1}else{0});
  if let Ok(actual)=actual{assert_eq!(format!("{:x}",Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap())),case["afterSha256"]);assert_eq!(actual["result"]["items"],strings(&case["result"]["items"]));assert_eq!(actual["context"]["projectSelection"],strings(&case["afterSelection"]));assert_eq!(actual["copies"].as_array().unwrap().len(),actual["result"]["items"].as_array().unwrap().len());for record in actual["copies"].as_array().unwrap(){let copy=&actual["document"]["project"]["items"][record["id"].as_str().unwrap()];let source=&actual["document"]["project"]["items"][record["sourceId"].as_str().unwrap()];assert_eq!(copy["kind"],source["kind"]);assert_eq!(copy["label"],source["label"]);assert_eq!(copy["name"],format!("{} Copy",source["name"].as_str().unwrap()));}for id in actual["result"]["items"].as_array().unwrap(){let copy=&actual["document"]["project"]["items"][id.as_str().unwrap()];assert_eq!(copy["metadata"],json!({}));assert!(copy.get("split").is_none());}}
 }
}
#[tokio::test]
async fn exact_new_ids_import_and_guard_exhaustion_without_partial_publication(){
 use video_kadr_backend::{error::ApiJson,handlers::filmcraft_project::{import,ImportRequest}};
 let o=oracle();let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let first=d["project"]["items"].as_object().unwrap().keys().next().unwrap().clone();d["project"]["next_id"]=json!(u64::MAX-2);
 let result=duplicate(json!({"document":serde_json::to_string(&d).unwrap(),"context":{"projectSelection":[first,first]}})).unwrap();assert_eq!(result["result"]["items"],json!([(u64::MAX-2).to_string(),(u64::MAX-1).to_string()]));assert_eq!(result["document"]["project"]["next_id"],u64::MAX);
 let request:ImportRequest=serde_json::from_value(json!({"document":result["documentText"]})).unwrap();let manifest=import(ApiJson(request)).await.unwrap().0;
 for id in result["result"]["items"].as_array().unwrap(){assert!(manifest["projectItems"].as_array().unwrap().iter().any(|item|item["id"]==*id));}
 let noop=duplicate(json!({"document":result["documentText"],"context":{"projectSelection":[u64::MAX.to_string()]}})).unwrap();assert_eq!(noop["result"]["items"],json!([]));assert_eq!(noop["document"],result["document"]);
 assert!(duplicate(json!({"document":result["documentText"],"context":{"projectSelection":[first]}})).is_err());
 d["project"]["next_id"]=json!(1);assert!(duplicate(json!({"document":d,"context":{"projectSelection":[first]}})).is_err());
}
#[tokio::test]
async fn file_media_copies_keep_payload_and_binding_provenance_in_manifest(){
 use video_kadr_backend::{error::ApiJson,handlers::filmcraft_project::{import,ImportRequest}};
 let o=oracle();let mut d=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});let source=d["project"]["items"].as_object().unwrap().iter().find(|(_,item)|item.pointer("/kind/Media").is_some()).unwrap().0.clone();
 d["project"]["items"][&source]["kind"]["Media"]["media"]=json!({"File":{"path":"/unopened/synthetic/source.mov"}});d["project"]["items"][&source]["kind"]["Media"]["offline"]=json!(true);
 let result=duplicate(json!({"document":d,"context":{"projectSelection":[source]}})).unwrap();let copy=result["result"]["items"][0].as_str().unwrap();assert_eq!(result["copies"],json!([{"id":copy,"sourceId":source}]));
 let request:ImportRequest=serde_json::from_value(json!({"document":result["documentText"]})).unwrap();let manifest=import(ApiJson(request)).await.unwrap().0;
 for id in [source.as_str(),copy]{let media=manifest["media"].as_array().unwrap().iter().find(|media|media["id"]==id).unwrap();assert_eq!(media["path"],"/unopened/synthetic/source.mov");assert_eq!(media["offline"],true);}
}
