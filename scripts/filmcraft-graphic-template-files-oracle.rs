use filmcraft_project::gtemplate::{builtin_templates,GraphicsTemplate};
use serde_json::{Value,json};
fn main(){
 let mut cases=vec![];
 for template in builtin_templates(){
  let original=serde_json::to_value(template).unwrap();
  for variant in ["original","bom","animated","font","newer","duplicate_uid","missing_target","missing_param","bad_font","bad_kind","bad_value","bad_keys"] {
   let mut raw=original.clone();
   match variant {
    "animated"=>{raw["layers"][0]["params"]["opacity"]["keyframes"]=json!([{"time":-254016000000i64,"value":{"Float":0.0},"interp":"Linear"},{"time":254016000000i64,"value":{"Float":100.0},"interp":"Hold"}]);},
    "font"=>raw["resources"]=json!({"fonts":[{"family":"Test","data":"TWFu","license":"OFL-1.1"}]}),
    "newer"=>raw["version"]=json!(2),
    "duplicate_uid"=>{if raw["layers"].as_array().unwrap().len()>1{raw["layers"][1]["layer"]["uid"]=raw["layers"][0]["layer"]["uid"].clone();}else{raw["layers"][0]["layer"]["uid"]=json!(0);}},
    "missing_target"=>raw["controls"][0]["layer"]=json!(999),
    "missing_param"=>raw["controls"][0]["param"]=json!("missing"),
    "bad_font"=>raw["resources"]=json!({"fonts":[{"data":"@@@"}]}),
    "bad_kind"=>raw["controls"][0]["kind"]=json!("unknown"),
    "bad_value"=>raw["layers"][0]["params"]["opacity"]["value"]=json!({"Float":"bad"}),
    "bad_keys"=>raw["layers"][0]["params"]["opacity"]["keyframes"]=json!([{"time":"bad"}]),
    _=>{}
   }
   let mut bytes=serde_json::to_vec(&raw).unwrap();
   if variant=="bom" {bytes.splice(0..0,[0xef,0xbb,0xbf]);}
   let accepted=GraphicsTemplate::from_bytes(&bytes).is_ok();
   cases.push(json!({"id":original["id"],"variant":variant,"document":raw,"bom":variant=="bom","accepted":accepted}));
  }
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
