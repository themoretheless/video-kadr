use filmcraft_project::graphic_design::{GraphicMeta,remap_time};
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
for scale in [1,100000] {for intro in [-5,0,3,20] {for outro in [-2,0,4,25] {for design_in in [-7,0,11] {for design_duration in [0,10,40] {for d in [0,2,10,50] {for u in [-2,0,1,3,4,9,10,20,49,50,51] {
let (intro,outro,design_in,design_duration,u,d)=(intro*scale,outro*scale,design_in*scale,design_duration*scale,u*scale,d*scale);
let meta=GraphicMeta{intro:Tick(intro),outro:Tick(outro),design_in:Tick(design_in),design_duration:Tick(design_duration),..Default::default()};
cases.push(json!({"scale":scale,"meta":{"intro":intro,"outro":outro,"designIn":design_in,"designDuration":design_duration},"u":u,"d":d,"expected":remap_time(&meta,Tick(u),Tick(d)).0}));
}}}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
