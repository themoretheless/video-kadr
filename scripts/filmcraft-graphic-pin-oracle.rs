//! Original responsive pin graph fixture generator.
use filmcraft_project::graphic_design::{Pin,PinNode,PinTarget,resolve_pins};
use serde_json::json;
fn case(nodes:Vec<PinNode>)->serde_json::Value {
 let frame=[0.,0.,64.,48.];let bounds=resolve_pins(&nodes,frame);
 json!({"frame":frame,"nodes":nodes.iter().map(|n|json!({"uid":n.uid,"pin":n.pin,"bounds":n.bounds,"resizable":n.resizable})).collect::<Vec<_>>(),"bits":bounds.iter().map(|b|b.map(f64::to_bits)).collect::<Vec<_>>()})
}
fn main(){let mut cases=vec![];
 for mask in 0..16 {for resize in [false,true] {for target in [PinTarget::Frame,PinTarget::Layer(2),PinTarget::Layer(99),PinTarget::Layer(0),PinTarget::Layer(1)] {for reverse in [false,true] {
  let pin=|to|Pin{to,left:mask&1!=0,top:mask&2!=0,right:mask&4!=0,bottom:mask&8!=0,offsets:[1.25,-2.5,3.75,4.5]};
  let mut nodes=vec![PinNode{uid:1,pin:Some(pin(target)),bounds:[3.25,-2.5,9.5,18.],resizable:resize},PinNode{uid:2,pin:Some(pin(PinTarget::Frame)),bounds:[20.,10.,50.,40.],resizable:true}];if reverse{nodes.reverse()}cases.push(case(nodes));
 }}}}
 for mask in 0..8{let mut nodes=vec![];for i in 0..3{nodes.push(PinNode{uid:i+1,pin:Some(Pin{to:PinTarget::Layer((i+1)%3+1),left:true,top:true,right:mask&(1<<i)!=0,bottom:false,offsets:[1.,2.,3.,4.]}),bounds:[i as f64*10.,0.,i as f64*10.+5.,8.],resizable:true});}cases.push(case(nodes));}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));
}
