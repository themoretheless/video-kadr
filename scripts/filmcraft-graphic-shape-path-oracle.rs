use filmcraft_project::graphic::ShapeProps;
use filmcraft_render::graphic_clip::shape_path;
use filmcraft_text::{Mask,raster::fill_into};
use serde_json::json;
fn main(){let mut cases=vec![];
for shape in 0..4{for profile in 0..3{for authored in[false,true]{for matrix in[[1.,0.,0.,1.,32.,24.],[0.75,0.2,-0.3,1.25,12.25,18.75],[1.25,0.,0.,0.5,-4.,46.]]{
let(size,sides,radius)=match profile{0=>((20.5,12.25),3,0.),1=>((0.,12.),6,3.5),_=>((35.,19.),64,100.)};let points=if authored{vec![[-10.,-5.],[7.,-8.],[12.,3.],[-4.,6.]]}else{vec![]};
let spec=ShapeProps{shape,size,sides,corner_radius:radius,points:points.clone()};let path=shape_path(&spec);let mut mask=Mask::new(64,48);fill_into(&path.transformed(&matrix),&mut mask,0.,0.);
cases.push(json!({"parameters":{"shape":shape,"size":[size.0,size.1],"sides":sides,"cornerRadius":radius,"points":points},"matrix":matrix,"contours":path.contours,"coverageBits":mask.a.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
