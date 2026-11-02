use serde_json::json;
fn main(){
 let effects:serde_json::Map<String,serde_json::Value>=["graphic_text","graphic_shape"].into_iter().map(|id|{
  let definition=filmcraft_project::find_effect(id).expect("graphic effect");
  let params:serde_json::Map<String,serde_json::Value>=definition.params.iter().map(|parameter|(parameter.id.to_string(),json!(parameter.kind))).collect();
  (id.into(),json!(params))
 }).collect();
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","effects":effects}));
}
