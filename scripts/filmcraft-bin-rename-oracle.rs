// Invoke actual pinned original project.renameBin; expected mutations are never computed here.
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
fn hash(v:&Value)->String{format!("{:x}",Sha256::digest(serde_json::to_vec(v).unwrap()))}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut s=filmcraft_engine::Session::default();s.execute("file.openDemoProject",json!({}))?;
    let parent=std::sync::Arc::make_mut(&mut s.project).add_bin("Parent",None);
    let nested=std::sync::Arc::make_mut(&mut s.project).add_bin("Nested",Some(parent));
    let base=s.project.clone();let project=serde_json::to_value(base.as_ref())?;
    let targets=vec![json!(base.root.id.0),json!(1),json!(parent.0),json!(nested.0),json!(u64::MAX),json!(null),json!(false),json!(-1),json!(1.9),json!({})];
    let names=vec![json!("Renamed"),json!("  spaced \t"),json!("новое 🎬"),json!(""),json!(" \t\r\n"),json!("\u{85}wide\u{2003}"),json!("\u{feff}keep\u{feff}"),json!(null),json!(42),json!(true)];
    let mut cases=Vec::new();
    for target in targets {for name in &names {
        s.project=base.clone();let params=json!({"bin":target,"name":name});
        let result=s.execute("project.renameBin",params.clone());
        cases.push(json!({"params":params,"ok":result.is_ok(),"result":result.ok(),"afterSha256":hash(&serde_json::to_value(s.project.as_ref())?)}));
    }}
    std::fs::write(std::env::args().nth(1).ok_or("output required")?,serde_json::to_vec(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","project":project,"cases":cases}))?)?;Ok(())
}
