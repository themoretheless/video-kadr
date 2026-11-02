//! Independent pinned FilmCraft Color Match oracle.
use filmcraft_render::{image::Image,color_match::solve};
use serde_json::json;
fn main(){
 let (w,h)=(7,5);
 let input:Vec<u8>=(0..w*h).flat_map(|i|[(i*73%256) as u8,(i*139%256) as u8,(i*211%256) as u8,if i%7==0 {0}else if i%3==0 {128}else{255}]).collect();
 let image=|bytes:&[u8]|{let mut img=Image::new(w,h);for(p,q)in bytes.chunks_exact(4).zip(img.px.chunks_exact_mut(4)){let a=p[3] as f32/255.0;for c in 0..3 {q[c]=filmcraft_color::srgb_to_linear(p[c] as f32/255.0)*a;}q[3]=a;}img};
 let current=image(&input);let base=filmcraft_project::effect::find_effect("lumetri").unwrap().instance();let mut cases=vec![];
 for scenario in 0..3{
  let shift=scenario==1;
  let reference:Vec<u8>=input.chunks_exact(4).flat_map(|p|[if scenario==2{255}else if shift{p[0].saturating_add(22)}else{p[0]},if scenario==2{0}else if shift{p[1].saturating_sub(12)}else{p[1]},if scenario==2{0}else if shift{p[2].saturating_add(9)}else{p[2]},p[3]]).collect();
  for skin in [false,true]{let m=solve(&current,&image(&reference),&base,skin);cases.push(json!({"reference":reference,"skin":skin,"expected":{"shadows":m.shadows,"midtones":m.midtones,"highlights":m.highlights,"lightness":m.lightness,"saturation":m.saturation,"before":m.before,"after":m.after}}));}
 }
 println!("{}",serde_json::to_string_pretty(&json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","width":w,"height":h,"input":input,"cases":cases})).unwrap());
}
