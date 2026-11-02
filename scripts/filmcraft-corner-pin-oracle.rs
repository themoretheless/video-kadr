use filmcraft_project::{Param,ParamValue};
use filmcraft_render::{effects::{apply,FxCtx},image::Image};
use serde_json::json;
fn main(){
 let mut cases=vec![];
 for(w,h)in[(1,7),(7,1),(6,4),(18,10)]{for variant in 0..6{for scale in[0.5f32,1.]{
  let rgba:Vec<u8>=(0..w*h).flat_map(|i|[[64,128,192,128],[0,0,0,0],[0,255,0,255],[255,0,0,64],[128,64,255,1]][(i+i/w)%5]).collect();
  let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;[filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[1]as f32/255.)*a,filmcraft_color::srgb_to_linear(p[2]as f32/255.)*a,a]}).collect();
  let mut out=Image{w,h,px};
  let mut e=filmcraft_project::effect_defs().iter().find(|d|d.id=="corner_pin").unwrap().instance();
  let mut parameters=serde_json::Map::new();
  let defaults=[[0.,0.],[w as f64,0.],[0.,h as f64],[w as f64,h as f64]];
  for(i,(id,key))in[("upper_left","upperLeft"),("upper_right","upperRight"),("lower_left","lowerLeft"),("lower_right","lowerRight")].into_iter().enumerate(){
   let p=match variant{
    0=>[f64::NAN,f64::NAN],
    1=>defaults[i],
    2=>[defaults[i][0]+1.25,defaults[i][1]-0.75],
    3=>[defaults[i][0]*0.6+(i%2)as f64*0.3,defaults[i][1]*0.8+(i/2)as f64*0.7],
    4=>[0.,0.],
    _=>[1.123456789012+i as f64,f64::NAN],
   };
   e.params.insert(id.into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(p[0],p[1]))));
   parameters.insert(key.into(),json!([if p[0].is_nan(){None}else{Some(p[0])},if p[1].is_nan(){None}else{Some(p[1])}]));
  }
  let cx=FxCtx{t:filmcraft_time::Tick(0),px_scale:scale,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
  apply(&mut out,&e,&cx);
  cases.push(json!({"parameters":parameters,"width":w,"height":h,"rgba":rgba,"scale":scale,"bits":out.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>()}));
 }}}
 let mut media=vec![];
 for tick in[0u64,250000,500000]{
  let rgba:Vec<u8>=(0..256).flat_map(|i|{let v=if i%16<8&&i/16<12{255}else{32};[v,v,v,if i%16<8&&i/16<12{128}else{0}]}).collect();
  let mut frames=vec![];
  for index in 0..(1000000-tick).div_ceil(100000){
   let local=(tick+index*100000).min(500000);let t=local as f64/500000.;
   let px=rgba.chunks_exact(4).flat_map(|p|{let a=p[3]as f32/255.;let v=filmcraft_color::srgb_to_linear(p[0]as f32/255.)*a;[v,v,v,a]}).collect();
   let mut frame=Image{w:16,h:16,px};
   let mut e=filmcraft_project::effect_defs().iter().find(|e|e.id=="corner_pin").unwrap().instance();
   for(n,x0,y0,x1,y1)in[("upper_left",0.,0.,2.,1.),("upper_right",16.,0.,14.,2.),("lower_left",0.,16.,1.,14.),("lower_right",16.,16.,15.,15.)]{
    e.params.insert(n.into(),Param::new(ParamValue::Vec2(filmcraft_geom::Vec2::new(x0+(x1-x0)*t,y0+(y1-y0)*t))));
   }
   let cx=FxCtx{t:filmcraft_time::Tick(local as i64),px_scale:1.,seconds:0.,timecode:"",clip_name:"",project:None,env:None,working:filmcraft_color::WorkingSpace::Rec709};
   apply(&mut frame,&e,&cx);frames.push(frame.px.iter().map(|v|v.to_bits()).collect::<Vec<_>>());
  }
  media.push(json!({"localTick":tick,"rgba":rgba,"bits":frames[0],"frames":frames}));
 }
 println!("{}",json!({"commit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases,"media":media}));
}
