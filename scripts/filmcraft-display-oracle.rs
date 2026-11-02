use serde_json::json;
fn main(){let mut cases=vec![];
for a in [-0.5f32,0.,0.000001,0.001,0.25,0.5,1.,1.5,2.]{
 let values:Vec<f32>=(0..4096).map(|i|i as f32/4095.).chain([-0.25,1.25,0.0031308,0.0031309,0.5/4095.,1.5/4095.]).collect();
 let mut src=vec![];for v in values{src.extend([v*a,(1.-v)*a,v*a*0.37,a]);}
 let mut out=vec![0u8;src.len()];filmcraft_frame::linear_premul_to_srgb8(&src,&mut out);
 cases.push(json!({"width":1,"height":src.len()/4,"bits":src.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"rgba":out}));
}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
