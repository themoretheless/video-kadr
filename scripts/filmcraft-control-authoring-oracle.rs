use serde_json::{json,Value};
fn main() {
 let root=std::path::PathBuf::from(std::env::var("VIDEOKADR_CONTROL_OUTPUT").unwrap());
 let mut session=filmcraft_engine::Session::default();
 session.execute("file.openDemoProject",json!({})).unwrap();
 session.execute("playhead.set",json!({"seconds":600})).unwrap();
 let clip=session.execute("graphics.newText",json!({"text":"Headline"})).unwrap()["clip"].as_u64().unwrap();
 session.execute("graphics.newText",json!({"clip":clip,"text":"Subhead"})).unwrap();
 session.execute("graphics.newShape",json!({"clip":clip,"shape":"rectangle"})).unwrap();
 let path=root.join("base.fcgt");
 session.execute("graphics.template.export",json!({"clip":clip,"name":"Oracle","path":path,"controls":[]})).unwrap();
 let document:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
 let mut inputs=Vec::new();
 for (index,layer) in document["layers"].as_array().unwrap().iter().enumerate() {
  let effect:filmcraft_project::EffectInstance=serde_json::from_value(layer.clone()).unwrap();
  let name=filmcraft_project::graphic::layer_display_name(&effect,index);
  for param in layer["params"].as_object().unwrap().keys() {
   inputs.push(json!([{"layer":index,"param":param},{"layer":name.to_ascii_uppercase(),"param":param}]));
  }
  for param in ["color","fillColor","fontSize","visible","show"] {
   inputs.push(json!([{"layer":index,"param":param,"name":"Repeated label"},{"layer":index,"param":param,"name":"Repeated label","kind":"unknown"}]));
  }
  inputs.push(json!([{"layer":index,"param":"fillColor","kind":"COLOUR","id":"custom","min":9,"max":99}]));
 }
 inputs.push(json!([]));inputs.push(json!([{"layer":"missing"}]));inputs.push(json!([{"layer":0,"param":"missing"}]));
 let mut cases=Vec::new();
 for descriptors in inputs {
  let result=session.execute("graphics.template.export",json!({"clip":clip,"name":"Oracle","path":path,"controls":descriptors}));
  cases.push(match result {Ok(result)=>json!({"descriptors":descriptors,"controls":result["controls"]}),Err(_)=>json!({"descriptors":descriptors,"error":true})});
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","document":document,"cases":cases}));
}
