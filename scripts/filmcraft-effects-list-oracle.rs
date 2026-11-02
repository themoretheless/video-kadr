// Execute the actual pinned source command, not a replica of its implementation.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn hash(value:&Value)->String {format!("{:x}",Sha256::digest(serde_json::to_vec(value).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut session=filmcraft_engine::Session::default();
    let mut queries=vec![json!({}),json!({"detail":true}),json!({"kind":true,"folder":42,"detail":"true","unknown":true}),json!({"kind":"unknown"}),json!({"folder":"video transitions"}),json!({"folder":""})];
    for kind in ["Video","Audio","VideoTransition","AudioTransition","vIdEoTrAnSiTiOn"] {for detail in [false,true]{queries.push(json!({"kind":kind,"detail":detail}));}}
    for folder in ["Video Transitions/Wipe","Video Transitions","Video Effects/Color Correction","Audio Effects","does-not-exist"] {queries.push(json!({"folder":folder,"detail":true}));}
    let mut cases=Vec::new();
    for params in queries {let result=session.execute("effects.list",params.clone())?;cases.push(json!({"params":params,"count":result.as_array().unwrap().len(),"sha256":hash(&result)}));}
    let definitions=serde_json::to_value(filmcraft_project::effect_defs())?;
    let output=json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","schemaVersion":1,"definitionCount":definitions.as_array().unwrap().len(),"definitionsSha256":hash(&definitions),"cases":cases});
    std::fs::write(std::env::args().nth(1).ok_or("output path required")?,serde_json::to_vec_pretty(&output)?)?;Ok(())
}
