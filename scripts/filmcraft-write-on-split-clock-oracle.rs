use filmcraft_project::{EffectInstance,Param,ParamValue,Keyframe,Interpolation};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use filmcraft_geom::Vec2;
use filmcraft_time::Tick;
use serde_json::json;
fn main(){let mut cases=vec![];
for alpha in [0u8,128,255]{for style in 0..3{for profile in 0..4{for origin_us in [0i64,20_000_000]{for seconds in [-0.5,0.,0.125,0.5,1.25]{for scale in [0.5f32,1.] {
 let(width,height)=(32usize,18usize);
 let(size,hardness,opacity,stroke_length,spacing,color) = match profile {0=>(8.,75.,100.,0.,0.01,[1.,1.,1.,1.]),1=>(3.5,0.,42.125,0.,0.125,[0.2,0.7,0.4,0.25]),2=>(6.25,100.,100.,0.25,0.05,[1.,0.,0.25,0.]),_=>(1.,37.5,0.,0.125,0.2,[0.5,0.25,1.,0.75])};
 let keys:Vec<(i64,[f64;2])>=match profile {0=>vec![],1=>vec![(0,[2.25,7.5]),(1000000,[29.75,11.25])],2=>vec![(-250000,[3.,3.]),(250000,[28.,3.]),(750000,[28.,15.]),(1250000,[3.,15.])],_=>vec![(0,[-4.25,9.]),(1000000,[36.5,9.])]};
 let keys:Vec<(i64,[f64;2])>=keys.into_iter().map(|(at,p)|(at+origin_us,p)).collect();
 let hold=profile==2;
 let rgba:Vec<u8>=(0..width*height).flat_map(|i|[(i*17%256)as u8,(i*31%256)as u8,(i*7%256)as u8,alpha]).collect();
 let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
 let mut image=Image{w:width,h:height,px};let mut effect:EffectInstance=filmcraft_project::effect_defs().iter().find(|e|e.id=="write_on").unwrap().instance();
 for(name,value)in[("size",ParamValue::Float(size)),("hardness",ParamValue::Float(hardness)),("opacity",ParamValue::Float(opacity)),("stroke_length",ParamValue::Float(stroke_length)),("spacing",ParamValue::Float(spacing)),("style",ParamValue::Choice(style)),("color",ParamValue::Color(color))]{effect.params.insert(name.into(),Param::new(value));}
 if !keys.is_empty(){let mut brush=Param::new(ParamValue::Vec2(Vec2::new(2.,7.)));brush.keyframes=keys.iter().map(|(tick,p)|Keyframe{time:Tick::from_units(*tick,1000000),value:ParamValue::Vec2(Vec2::new(p[0],p[1])),interp:if hold{Interpolation::Hold}else{Interpolation::Linear},out_influence:1./3.,in_influence:1./3.}).collect();effect.params.insert("brush".into(),brush);}
 let context=FxCtx{t:Tick::from_units(origin_us,1000000)+Tick::from_seconds_f64(seconds),px_scale:scale,seconds,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};apply(&mut image,&effect,&context);
 cases.push(json!({"width":width,"height":height,"rgba":rgba,"parameters":{"brush":[null,null],"size":size,"hardness":hardness,"opacity":opacity,"strokeLength":stroke_length,"spacing":spacing,"style":style,"color":color},"keys":keys,"hold":hold,"seconds":seconds,"parameterTick":(Tick::from_units(origin_us,1000000)+Tick::from_seconds_f64(seconds)).0.to_string(),"originUs":origin_us,"pixelScale":scale,"bits":image.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
}}}}}}println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases}));}
