// Independent original schema probe and complete migration chain.
use filmcraft_project::{Project,SequenceSettings};
use filmcraft_format::{detect_version,migrate_with,MIGRATIONS};
use serde_json::json;
fn main(){let mut p=Project::new("Preserved authoring document");p.new_sequence("Sequence",SequenceSettings::default(),1,1,None);
let mut project=serde_json::to_value(&p).unwrap();project["opaque_extension"]=json!({"tick":9007199254740993i64,"id":18446744073709551615u64,"data":[null,true,"exact"]});
let mut documents=vec![];
for version in 2..=12 {documents.push(json!({"format":"filmcraft.project","schema_version":version,"generator":"Original fixture","project":project,"view":{"opaque":{"selected":9007199254740993i64}},"extension":[1,2,3]}));}
for version in [None,Some(0),Some(1),Some(2)] {let mut legacy=project.clone();if let Some(v)=version{legacy["version"]=json!(v);}documents.push(legacy);}
for version in [0u64,13,4294967295,4294967296]{documents.push(json!({"format":"filmcraft.project","schema_version":version,"project":project}));}
for v in [json!(-1),json!("12"),json!(1.5)] {documents.push(json!({"schema_version":v,"project":project}));}
for v in [json!(null),json!([]),json!({}),json!({"format":"other","schema_version":12,"project":project}),json!({"items":null,"root":{}}),json!({"items":{},"root":null})] {documents.push(v);}
for version in [3,9,13]{let mut legacy=project.clone();legacy["version"]=json!(version);documents.push(legacy);}
let cases:Vec<_>=documents.into_iter().map(|document|{let mut c=json!({"document":document});match detect_version(&document){Ok(found)=>{c["found"]=json!(found);match migrate_with(MIGRATIONS,document,found){Ok(v)=>c["migrated"]=v,Err(e)=>c["error"]=json!(e.to_string())}},Err(e)=>c["error"]=json!(e.to_string())}c}).collect();
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
