// Shared deterministic inputs only; original and native trackers compute outputs independently.
pub fn frame(width:usize,height:usize,step:i64,mode:&str)->Vec<u8>{
 let mut rgba=Vec::with_capacity(width*height*4);
 let angle=if matches!(mode,"rotation"|"mixed"){step as f64*0.7f64.to_radians()}else{0.};
 let scale=if mode=="mixed"{1.+step as f64*0.005}else{1.};let(c,s)=(angle.cos(),angle.sin());
 let(dx,dy)=if mode=="still"{(0.,0.)}else{(step as f64*2.25,step as f64*-1.3)};
 for y in 0..height{for x in 0..width{let(px,py)=(x as f64-dx-width as f64/2.,y as f64-dy-height as f64/2.);let(u,v)=((c*px+s*py)/scale+120.,(-s*px+c*py)/scale+90.);let value=if mode=="lost"&&step>=2{0.5}else{(0.5+0.2*(u*0.21).sin()*(v*0.17).cos()+0.1*((u+v)*0.43).sin()+0.08*(u*0.9-v*0.6).sin()).clamp(0.,1.)};let q=(value*255.).round() as u8;rgba.extend([q,q,q,255]);}}
 rgba
}
