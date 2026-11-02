use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use video_kadr_backend::domain::{filmcraft_bin_rename::{rename,list},filmcraft_project_file::SourceProjectDocument};
fn oracle()->Value{serde_json::from_str(include_str!("fixtures/filmcraft-bin-rename-oracle.json")).unwrap()}
#[test]
fn matches_actual_original_bin_command() {
    let o=oracle();assert_eq!(o["sourceCommit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");assert_eq!(o["cases"].as_array().unwrap().len(),100);
    for case in o["cases"].as_array().unwrap() {
        let doc=json!({"format":"filmcraft.project","schema_version":12,"project":o["project"]});
        let mut params=case["params"].clone();params["document"]=doc;
        let result=rename(params);assert_eq!(result.is_ok(),case["ok"].as_bool().unwrap(),"{}: {result:?}",case["params"]);
        if let Ok(actual)=result {
            assert_eq!(format!("{:x}",Sha256::digest(serde_json::to_vec(&actual["document"]["project"]).unwrap())),case["afterSha256"]);
            assert_eq!(actual["result"]["name"],case["result"]["name"]);
            assert_eq!(actual["result"]["bin"],case["result"]["bin"].as_u64().unwrap().to_string());
        }
    }
}
#[tokio::test]
async fn nested_bin_import_retains_exact_ids_parent_and_undo_snapshot() {
    use video_kadr_backend::{error::ApiJson,handlers::filmcraft_project::{import,ImportRequest}};
    let mut doc=json!({"format":"filmcraft.project","schema_version":12,"project":oracle()["project"]});
    doc["project"]["root"]["children"].as_array_mut().unwrap().push(json!({"Bin":{"id":u64::MAX,"name":"Exact","children":[{"Bin":{"id":u64::MAX-1,"name":"Nested","children":[]}}]}}));
    let original=serde_json::to_string(&doc).unwrap();
    let changed=rename(json!({"document":original,"bin":(u64::MAX-1).to_string(),"name":" \u{85} новое 🎬 \u{2003}"})).unwrap();
    let request:ImportRequest=serde_json::from_value(json!({"document":changed["documentText"]})).unwrap();
    let manifest=import(ApiJson(request)).await.unwrap().0;
    let row=manifest["projectBins"].as_array().unwrap().iter().find(|b|b["id"]==(u64::MAX-1).to_string()).unwrap();
    assert_eq!(row["name"],"новое 🎬");assert_eq!(row["parentId"],u64::MAX.to_string());assert_eq!(row["depth"],2);
    let mut expected=doc.clone();expected["project"]["root"]["children"].as_array_mut().unwrap().last_mut().unwrap()["Bin"]["children"][0]["Bin"]["name"]=json!("новое 🎬");
    assert_eq!(changed["document"],expected);
    let parsed=SourceProjectDocument::read(original.as_bytes()).unwrap();assert_ne!(list(&parsed).unwrap(),manifest["projectBins"]);
    doc["project"]["root"]["children"].as_array_mut().unwrap().push(json!({"Bin":{"id":u64::MAX,"name":"Duplicate","children":[]}}));
    assert!(rename(json!({"document":doc,"bin":u64::MAX.to_string(),"name":"x"})).is_err());
}
