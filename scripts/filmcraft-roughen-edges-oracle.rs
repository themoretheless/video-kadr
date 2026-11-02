//! Original active Roughen Edges fixtures; pinned MIT source.
use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for kind in 0u32..8{for(border,sharp,infl,scale,stretch,cplx,evo,seed)in[(0.,1.,1.,100.,0.,2.,0.,0.),(8.,1.,1.,100.,0.,2.,0.,0.),(2.5,10.,0.5,10.,-2.,3.5,720.,4.5),(500.,0.,0.,1000.,10.,10.,-45.,10000.)]{for color in[[0.8,0.5,0.2,1.],[-1.,2.,0.5,0.]]{
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut image=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="roughen_edges").unwrap().instance();
e.params.insert("edge_type".into(),Param::new(ParamValue::Choice(kind)));e.params.insert("edge_color".into(),Param::new(ParamValue::Color(color)));e.params.insert("offset".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(1.23456789012345,-2.125))));
for(n,v)in[("border",border),("sharpness",sharp),("influence",infl),("scale",scale),("stretch",stretch),("complexity",cplx),("evolution",evo),("seed",seed)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);cases.push(json!({"parameters":{"edgeType":kind,"edgeColor":color,"offset":[1.23456789012345,-2.125],"border":border,"sharpness":sharp,"influence":infl,"scale":scale,"stretch":stretch,"complexity":cplx,"evolution":evo,"seed":seed},"width":w,"height":h,"rgba":rgba,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}let mut media=vec![];
for kind in 0u32..8 {for tick in [0u64,250000,500000] {
 let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
 for index in 0..(1000000-tick).div_ceil(100000) {
  let local=(tick+index*100000).min(500000);let t=local as f64/500000.;
  let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="roughen_edges").unwrap().instance();
  e.params.insert("edge_type".into(),Param::new(ParamValue::Choice(kind)));e.params.insert("edge_color".into(),Param::new(ParamValue::Color([t as f32,(1.-t)as f32,(0.5*t)as f32,t as f32])));e.params.insert("offset".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(-1.25+t*3.375,0.5-t))));
  for(n,v)in[("border",t*8.),("sharpness",1.+t),("influence",t),("scale",100.+t*100.),("stretch",-2.+t*4.),("complexity",2.+t*1.5),("evolution",t*720.),("seed",t*4.5)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
  let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
 }
 media.push(json!({"edgeType":kind,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
