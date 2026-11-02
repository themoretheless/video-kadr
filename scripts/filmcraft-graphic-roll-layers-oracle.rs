use filmcraft_project::{EffectInstance,graphic_design::{Roll,RollMode,GraphicMeta}};
use filmcraft_time::Tick;
use filmcraft_geom::Affine;
use filmcraft_render::{graphic_clip::{apply_pins,apply_roll,layer_bounds,layer_matrix,xform,raster_layer,composite_raster},image::Image};
use serde_json::json;
fn main(){let input:serde_json::Value=serde_json::from_reader(std::fs::File::open(std::env::args().nth(1).unwrap()).unwrap()).unwrap();let mut cases=vec![];
for seed in [0,48,128,240] {let effects:Vec<EffectInstance>=serde_json::from_value(input["cases"][seed]["effects"].clone()).unwrap();for mode in [RollMode::Roll,RollMode::CrawlLeft,RollMode::CrawlRight] {for start in [false,true] {for end in [false,true] {for u in [0,5,10] {
let roll=Roll{mode,start_off_screen:start,end_off_screen:end,preroll:Tick(1),ease_in:Tick(2),ease_out:Tick(3),postroll:Tick(1)};
let mut specs:Vec<_>=effects.iter().filter_map(|effect|filmcraft_project::graphic::eval_layer(effect,Tick::ZERO,(64,48))).collect();apply_pins(&mut specs,(64,48));apply_roll(&GraphicMeta{roll:roll.clone(),..Default::default()},&mut specs,Tick(u),Tick(10),(64,48));
let bounds:Vec<_>=specs.iter().map(|s|layer_bounds(s).map(f64::to_bits)).collect();let matrices:Vec<_>=specs.iter().map(|s|{let a=layer_matrix(s);[a.a,a.b,a.c,a.d,a.e,a.f].map(f64::to_bits)}).collect();let mut image=Image::new(80,64);
for spec in &specs {let matrix=Affine::IDENTITY.then_apply(&layer_matrix(spec));if let Some(r)=raster_layer(spec,&xform(&matrix),80,64){composite_raster(&mut image,&r,0,0)}}
cases.push(json!({"seed":seed,"effects":effects,"roll":roll,"u":u,"d":10,"boundsBits":bounds,"matrixBits":matrices,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
