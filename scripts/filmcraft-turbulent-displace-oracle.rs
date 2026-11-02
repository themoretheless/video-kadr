use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];
for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for displacement in 0..9u32{for pinning in 0..4u32{for profile in 0..3{for cycle in[false,true]{for seed in[0.,42.75]{for scale in[0.5f32,1.]{
let(amount,size,complexity,evolution,cycle_revs,offset)=match profile{0=>(0.,100.,1.,0.,1.,[None,None]),1=>(2.5,3.25,3.75,-450.,2.25,[Some(3.123456789012),None]),_=>(-7.5,2.,10.,810.,2.,[Some(-0.75),Some(2.25)])};
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="turbulent_displace").unwrap().instance();
for(n,v)in[("displacement",displacement),("pinning",pinning)]{e.params.insert(n.into(),Param::new(ParamValue::Choice(v)));}for(n,v)in[("amount",amount),("size",size),("complexity",complexity),("evolution",evolution),("cycle_revs",cycle_revs),("seed",seed)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}e.params.insert("cycle".into(),Param::new(ParamValue::Bool(cycle)));e.params.insert("offset".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(offset[0].unwrap_or(f64::NAN),offset[1].unwrap_or(f64::NAN)))));
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"displacement":displacement,"pinning":pinning,"amount":amount,"size":size,"complexity":complexity,"evolution":evolution,"cycleRevs":cycle_revs,"seed":seed,"cycle":cycle,"offset":offset},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}}
 let mut media=vec![];
 for displacement in 0..9u32{for cycle in[false,true]{for tick in[0u64,250000,500000]{
 let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();let mut frames=vec![];
 for index in 0..(1000000-tick).div_ceil(100000){
 let local=tick+index*100000;let t=local.min(500000)as f64/500000.;
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};
 let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="turbulent_displace").unwrap().instance();
 for(n,v)in[("amount",-2.+t*7.),("size",2.+t*3.),("complexity",1.+t*5.5),("evolution",-450.+t*1260.),("cycle_revs",1.+t*1.25),("seed",t*42.75)]{e.params.insert(n.into(),Param::new(ParamValue::Float(v)));}
 e.params.insert("cycle".into(),Param::new(ParamValue::Bool(cycle)));for(n,v)in[("displacement",displacement),("pinning",displacement%4)]{e.params.insert(n.into(),Param::new(ParamValue::Choice(v)));}e.params.insert("offset".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(t*3.,1.+t*3.))));
 let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:local as f64/1000000.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
 }
 media.push(json!({"displacement":displacement,"pinning":displacement%4,"cycle":cycle,"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }}}
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
