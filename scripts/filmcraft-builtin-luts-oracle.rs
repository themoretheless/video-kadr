//! Export pinned upstream generated builtins without depending on host color math.
use serde_json::json;
fn main(){
 let entries:Vec<_>=filmcraft_render::luts::builtins().iter().map(|b|{
  let lut=b.generate();let cube=lut.cube.unwrap();
  json!({"id":b.id,"label":b.label,"input":filmcraft_render::luts::input_builtins().any(|i|i.id==b.id),"size":cube.size,"data":cube.data})
 }).collect();
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","entries":entries}));
}
