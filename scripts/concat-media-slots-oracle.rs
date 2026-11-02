//! Test-only driver for the unmodified pinned Concat project engine.
use concat_project::{commands::{self,IdMint},from_document,Command};
use serde_json::{json,Value};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let input:Value=serde_json::from_slice(&std::fs::read(std::env::args().nth(1).ok_or("input")?)?)?;
 let mut cases=vec![];
 for case in input.as_array().ok_or("case array")? {
  let mut project=from_document(&case["document"]).ok_or("invalid fixture document")?;
  let mut before=serde_json::to_value(&project)?;before["version"]=json!(1);
  let command:Command=serde_json::from_value(case["command"].clone())?;
  let mut mint=IdMint::default();mint.adopt_project(&project);
  let result=commands::apply(&mut project,&mut mint,command);
  let mut after=serde_json::to_value(&project)?;after["version"]=json!(1);
  cases.push(json!({"document":before,"command":case["command"],"ok":result.is_ok(),"applied":result.as_ref().map(|v|v.applied).ok(),"error":result.err().map(|v|format!("{v:?}")),"after":after}));
 }
 std::fs::write(std::env::args().nth(2).ok_or("output")?,serde_json::to_vec(&json!({"sourceCommit":"78e4be101c2d3c41a7dc73c112c6b1f11b9ce170","cases":cases}))?)?;
 Ok(())
}
