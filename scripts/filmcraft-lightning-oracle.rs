use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use filmcraft_geom::Vec2;
use serde_json::json;
fn main(){let mut cases=vec![];
for alpha in [0u8,128,255]{for mode in 0..5{for profile in 0..4{for seconds in [-0.125,0.,2.25]{for scale in [0.5f32,1.] {
 let(width,height)=(32usize,18usize);
 let(start,end,segments,amplitude,detail,detail_amplitude,branching,speed,width_px,core,outside,inside,seed)=match profile{
 0=>([None,None],[None,None],7.,10.,2.,0.3,0.3,10.,10.,0.4,[0.35,0.3,1.,1.],[1.,1.,1.,1.],0.),
 1=>([Some(2.25),None],[None,Some(15.25)],5.75,37.5,3.5,0.125,0.75,37.5,1.25,0.25,[0.2,0.7,0.4,0.25],[1.,0.25,0.5,0.],17.75),
 2=>([Some(1.),Some(1.)],[Some(30.),Some(16.)],2.,100.,10.,1.,1.,100.,2.5,0.,[1.,0.,0.,0.],[0.,1.,0.,0.5],5.),
 _=>([Some(-4.25),Some(9.)],[Some(36.5),Some(9.)],1.,0.,0.,0.,0.,0.,0.,1.,[0.,0.,1.,1.],[1.,1.,1.,1.],777.75)};
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="lightning").unwrap().instance();
 for(n,v)in[("start",ParamValue::Vec2(Vec2::new(start[0].unwrap_or(f64::NAN),start[1].unwrap_or(f64::NAN)))),("end",ParamValue::Vec2(Vec2::new(end[0].unwrap_or(f64::NAN),end[1].unwrap_or(f64::NAN)))),("segments",ParamValue::Float(segments)),("amplitude",ParamValue::Float(amplitude)),("detail",ParamValue::Float(detail)),("detail_amplitude",ParamValue::Float(detail_amplitude)),("branching",ParamValue::Float(branching)),("speed",ParamValue::Float(speed)),("width",ParamValue::Float(width_px)),("core",ParamValue::Float(core)),("outside",ParamValue::Color(outside)),("inside",ParamValue::Color(inside)),("seed",ParamValue::Float(seed)),("mode",ParamValue::Choice(mode))]{e.params.insert(n.into(),Param::new(v));}
 let cx=FxCtx{t:filmcraft_time::Tick::from_seconds_f64(seconds),px_scale:scale,seconds,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&e,&cx);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"start":start,"end":end,"segments":segments,"amplitude":amplitude,"detail":detail,"detailAmplitude":detail_amplitude,"branching":branching,"speed":speed,"width":width_px,"core":core,"outside":outside,"inside":inside,"seed":seed,"mode":mode},"seconds":seconds,"pixelScale":scale,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
