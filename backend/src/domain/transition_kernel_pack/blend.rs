// Normal premultiplied source-over from Filmcraft (MIT; see LICENSE-MIT).
use super::image::Image;
#[derive(Clone,Copy)]pub enum Blend{Normal}
pub fn composite(dst:&mut Image,src:&Image,opacity:f32,_mode:Blend){
 for(d,s)in dst.px.chunks_exact_mut(4).zip(src.px.chunks_exact(4)){
  if super::context::cancelled(){return;}
  let sp=[s[0]*opacity,s[1]*opacity,s[2]*opacity,s[3]*opacity];
  if sp[3]<=0.0{continue;}
  let k=1.0-sp[3];for c in 0..4{d[c]=sp[c]+d[c]*k;}
 }
}
