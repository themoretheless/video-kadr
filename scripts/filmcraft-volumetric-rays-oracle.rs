use filmcraft_project::{Param,ParamValue};use filmcraft_render::{effects::{apply,FxCtx},image::Image};use serde_json::json;
fn main(){let mut cases=vec![];for(w,h)in[(1,7),(7,1),(6,4),(18,10),(961,3),(1921,3),(961,1)]{for only in [false,true] {for(threshold,length,intensity,color)in [(60.,50.,100.,[1f32,0.95,0.8,1.]),(0.,0.,0.,[1.;4]),(0.,100.,1000.,[0.25,0.75,0.125,0.]),(100.,100.,1000.,[1.;4]),(0.,0.6,37.5,[0.,0.,0.,1.]),(20.,1.,150.,[1.,0.,0.,0.]),(5.,3.25,75.,[0.3,0.8,1.,0.]),(0.,100.,1000.,[1.;4])] {for center in [None,Some([Some(1.123456789012),None]),Some([None,Some(-0.75)]),Some([Some(-3.25),Some(4.125)])] {for scale in [0.5f32,1.] {
let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();let mut out=Image{w,h,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="volumetric_rays").unwrap().instance();for(id,value)in[("threshold",ParamValue::Float(threshold)),("length",ParamValue::Float(length)),("intensity",ParamValue::Float(intensity)),("color",ParamValue::Color(color)),("only",ParamValue::Bool(only))]{e.params.insert(id.into(),Param::new(value));}
if let Some(c)=center{e.params.insert("center".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(c[0].unwrap_or(f64::NAN),c[1].unwrap_or(f64::NAN)))));}
let cx=FxCtx{t:filmcraft_time::Tick::ZERO,px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut out,&e,&cx);cases.push(json!({"parameters":{"threshold":threshold,"length":length,"intensity":intensity,"color":color,"only":only,"center":center},"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}let mut media=vec![];
for only in [false,true] {for tick in [0u64,250000,500000] {
let channels=[("threshold",60.,5.),("length",50.,100.),("intensity",100.,350.)];
let colors=[("colorRed",1.,0.25),("colorGreen",0.95,0.75),("colorBlue",0.8,0.125),("colorAlpha",1.,0.)];
let curve=|a:f64,b:f64|json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":a},{"tick":500000,"value":b}]}});
let mut animation:serde_json::Map<String,serde_json::Value>=channels.iter().chain(colors.iter()).map(|(name,a,b)|(name.to_string(),curve(*a,*b))).collect();
animation.insert("centerX".into(),curve(8.,2.123456789012));animation.insert("centerY".into(),curve(4.,-0.75));
let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,128]}).collect();let mut frames=vec![];
for index in 0..(1000000-tick).div_ceil(100000){let local=(tick+index*100000).min(500000);let t=local as f64/500000.;let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();let mut frame=Image{w:16,h:16,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="volumetric_rays").unwrap().instance();
e.params.insert("only".into(),Param::new(ParamValue::Bool(only)));
e.params.insert("center".into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(8.+(2.123456789012-8.)*t,4.+(-0.75-4.)*t))));
for(id,a,b)in channels {e.params.insert(id.into(),Param::new(ParamValue::Float(a+(b-a)*t)));}
let color=colors.map(|(_,a,b)|(a+(b-a)*t)as f32);e.params.insert("color".into(),Param::new(ParamValue::Color(color)));
let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());}
media.push(json!({"localTick":tick,"parameters":{"only":only},"animation":animation,"rgba":rgba,"bits":frames[0],"frames":frames}));
}}
println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));}
