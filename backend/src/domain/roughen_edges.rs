//! Filmcraft active Roughen Edges CPU primitive at source resolution.
//! MIT source attribution: docs/licenses/filmcraft-MIT.txt.
use anyhow::{ensure,Result};use serde::{Serialize,Deserialize};
#[derive(Debug,Clone,Copy,PartialEq,Serialize,Deserialize)]
#[serde(default,rename_all="camelCase",deny_unknown_fields)]
pub struct RoughenEdges {pub edge_type:u32,pub edge_color:[f32;4],pub border:f32,pub sharpness:f32,pub influence:f32,pub scale:f32,pub stretch:f32,pub offset:[f64;2],pub complexity:f32,pub evolution:f32,pub seed:f32}
impl Default for RoughenEdges {fn default()->Self{Self{edge_type:0,edge_color:[0.8,0.5,0.2,1.],border:8.,sharpness:1.,influence:1.,scale:100.,stretch:0.,offset:[0.;2],complexity:2.,evolution:0.,seed:0.}}}
impl RoughenEdges {
 pub fn validate(&self)->Result<()> {
 ensure!(self.edge_type<8&&self.edge_color.iter().all(|v|v.is_finite())&&self.offset.iter().all(|v|v.is_finite()&&v.abs()<=f32::MAX as f64)&&self.evolution.is_finite(),"Roughen Edges nonfinite parameter or invalid type");
 for(v,lo,hi)in[(self.border,0.,500.),(self.sharpness,0.,10.),(self.influence,0.,1.),(self.scale,10.,1000.),(self.stretch,-10.,10.),(self.complexity,1.,10.),(self.seed,0.,10000.)]{ensure!(v.is_finite()&&(lo..=hi).contains(&v),"Roughen Edges parameter out of range");}Ok(())
 }
 pub fn apply_linear(&self,frame:&mut super::linear_color_frame::LinearColorFrame,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {self.apply_linear_scaled(frame,1.,checkpoint)}
 pub(crate) fn apply_linear_scaled(&self,frame:&mut super::linear_color_frame::LinearColorFrame,px_scale:f32,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
 self.validate()?;ensure!(px_scale.is_finite()&&px_scale>0.,"invalid Roughen Edges pixel scale");checkpoint()?;let border=self.border*px_scale;let offset=self.offset.map(|v|v*px_scale as f64);if border<0.5{return Ok(());}let(w,h)=frame.dimensions();
 let a:Vec<f32>=frame.pixels().iter().map(|p|p[3]).collect();let depth=super::gaussian_blur::blur_plane(&a,w,h,border*0.5,0,false,checkpoint)?;
 let kind=self.edge_type;let sharp=self.sharpness.max(0.);let infl=self.influence.clamp(0.,1.);let scale=(self.scale/100.*30.*px_scale).max(0.5);let stretch=2f32.powf(self.stretch);let cplx=self.complexity.clamp(1.,10.);let evo=self.evolution/360.;let seed=self.seed.max(0.)as u64;let ec=std::array::from_fn::<f32,3,_>(|c|super::basic_grade::linear(self.edge_color[c].clamp(0.,1.)));let colored=matches!(kind,1|5|7);let mut index=0;
 frame.map_premultiplied(&mut|mut p|{let i=index;let(x,y)=(i%w,i/w);index+=1;
            if p[3] <= 1e-6 {
                return p;
            }
            let (u, v) = ((x as f32 + offset[0] as f32) / (scale * stretch), (y as f32 + offset[1] as f32) * stretch / scale);
            let mut n = fbm(u, v, evo, cplx, seed);
            if kind == 3 {
                n = 1.0 - 2.0 * n.abs();
            }
            let d = depth[i];
            let mut k = ((d - 0.5 + n * infl * 0.5) * (1.0 + sharp * 4.0) + 0.5).clamp(0.0, 1.0);
            if kind == 2 {
                k = if k > 0.5 { 1.0 } else { 0.0 };
            }
            if matches!(kind, 4..=7) {
                // rust / photocopy: holes eaten into the border region
                let hole = fbm(u * 2.3 + 11.0, v * 2.3, evo, cplx, seed + 1);
                if d < 0.97 && hole > 0.25 {
                    k *= 1.0 - smoothstep(0.25, 0.4, hole) * (1.0 - d);
                }
            }
            if colored && d < 0.97 {
                let t = (1.0 - d) * 1.5;
                let c = straight([p[0], p[1], p[2], p[3]]);
                let col = lerp3(c, ec, t.min(1.0));
                for c in 0..3 {
                    p[c] = col[c] * p[3];
                }
            }
            if matches!(kind, 6 | 7) {
                let c = straight([p[0], p[1], p[2], p[3]]);
                let l = if luma(enc(c)) > 0.5 { 1.0 } else { 0.0 };
                if kind == 6 {
                    for c in 0..3 {
                        p[c] = l * p[3];
                    }
                }
            }
            for v in p.iter_mut() {
                *v *= k;
            }
p},checkpoint)
 }
}
fn straight(p:[f32;4])->[f32;3]{if p[3]<=1e-6{[0.;3]}else{[p[0]/p[3],p[1]/p[3],p[2]/p[3]]}}
fn enc(c:[f32;3])->[f32;3]{c.map(|v|super::basic_grade::display(v.max(0.)))}
fn luma(c:[f32;3])->f32{0.2126*c[0]+0.7152*c[1]+0.0722*c[2]}
fn lerp3(a:[f32;3],b:[f32;3],t:f32)->[f32;3]{std::array::from_fn(|c|a[c]+(b[c]-a[c])*t)}
fn smoothstep(a:f32,b:f32,x:f32)->f32{let t=((x-a)/(b-a).max(1e-6)).clamp(0.,1.);t*t*(3.-2.*t)}
fn hash_u(x: i64, y: i64, z: i64, seed: u64) -> u64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (z as u64).wrapping_mul(0x1656_67B1_9E37_79F9)
        ^ seed.wrapping_mul(0x27D4_EB2F_1656_67C5);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^ (h >> 32)
}
#[inline]
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
#[inline]
fn grad3(h: u64, x: f32, y: f32, z: f32) -> f32 {
    // 12 edge directions of a cube (Perlin 2002)
    match h % 12 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        _ => -y - z,
    }
}

/// 3-D gradient noise in about −1..1 (our own hash; Perlin's interpolant and gradient set).
pub(crate) fn noise3(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    let (xi, yi, zi) = (x.floor(), y.floor(), z.floor());
    let (xf, yf, zf) = (x - xi, y - yi, z - zi);
    let (xi, yi, zi) = (xi as i64, yi as i64, zi as i64);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let g = |dx: i64, dy: i64, dz: i64| grad3(hash_u(xi + dx, yi + dy, zi + dz, seed), xf - dx as f32, yf - dy as f32, zf - dz as f32);
    let l = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = l(g(0, 0, 0), g(1, 0, 0), u);
    let x10 = l(g(0, 1, 0), g(1, 1, 0), u);
    let x01 = l(g(0, 0, 1), g(1, 0, 1), u);
    let x11 = l(g(0, 1, 1), g(1, 1, 1), u);
    l(l(x00, x10, v), l(x01, x11, v), w) * 0.9
}

/// Fractal (fBm) noise with `octaves` (fractional octaves fade in), about −1..1.
pub(crate) fn fbm(x: f32, y: f32, z: f32, octaves: f32, seed: u64) -> f32 {
    let n = octaves.clamp(1.0, 10.0);
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    let mut i = 0;
    while (i as f32) < n {
        let k = (n - i as f32).min(1.0);
        sum += noise3(x * freq, y * freq, z * freq + i as f32 * 17.13, seed.wrapping_add(i as u64)) * amp * k;
        norm += amp * k;
        amp *= 0.5;
        freq *= 2.0;
        i += 1;
    }
    sum / norm.max(1e-6)
}


#[cfg(test)]mod tests{
 use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn identity_threshold_transparency_modes_and_cancellation(){
 let source=GroupSurface{width:3,height:3,rgba:(0..9).flat_map(|i|if i==4{[255,64,32,255]}else{[0,0,0,0]}).collect()};let original=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();let mut frame=original.clone();RoughenEdges{border:0.49,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels(),original.pixels());
 for kind in 0..8{let mut frame=original.clone();RoughenEdges{edge_type:kind,..Default::default()}.apply_linear(&mut frame,&mut||Ok(())).unwrap();for(i,p)in frame.pixels().iter().enumerate(){assert!(p.iter().all(|v|v.is_finite()));assert!(p[3]>=0.&&p[3]<=original.pixels()[i][3]);if i!=4{assert_eq!(*p,[0.;4]);}}}
 assert!(RoughenEdges::default().apply_linear(&mut frame,&mut||anyhow::bail!("cancelled")).is_err());assert!(RoughenEdges{edge_type:8,..Default::default()}.validate().is_err());assert!(RoughenEdges{complexity:0.,..Default::default()}.validate().is_err());
 }
}

#[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub enum RoughenEdgesChannel {Border,Sharpness,Influence,Scale,Stretch,Complexity,Evolution,Seed,OffsetX,OffsetY,EdgeColorR,EdgeColorG,EdgeColorB,EdgeColorA}
pub type RoughenEdgesAnimation=std::collections::BTreeMap<RoughenEdgesChannel,super::composition::AnimatableValue>;
pub fn validate_animation(a:&RoughenEdgesAnimation)->Result<()>{for(c,v)in a{v.validate()?;let(lo,hi)=match c{RoughenEdgesChannel::Border=>(0.,500.),RoughenEdgesChannel::Sharpness=>(0.,10.),RoughenEdgesChannel::Influence=>(0.,1.),RoughenEdgesChannel::Scale=>(10.,1000.),RoughenEdgesChannel::Stretch=>(-10.,10.),RoughenEdgesChannel::Complexity=>(1.,10.),RoughenEdgesChannel::Seed=>(0.,10000.),_=>(-(f32::MAX as f64),f32::MAX as f64)};ensure!(super::composition::animatable_values_in(v,lo,hi,true),"Roughen Edges animation out of range");}Ok(())}
pub fn sample_animation(mut p:RoughenEdges,a:&RoughenEdgesAnimation,t: impl Into<super::animation_sample_time::AnimationSampleTime>)->Result<RoughenEdges>{
    let t: super::animation_sample_time::AnimationSampleTime = t.into();
p.validate()?;validate_animation(a)?;for(c,v)in a{let v=super::nested_composition::sample_value(v,t)?;match c{RoughenEdgesChannel::Border=>p.border=v as f32,RoughenEdgesChannel::Sharpness=>p.sharpness=v as f32,RoughenEdgesChannel::Influence=>p.influence=v as f32,RoughenEdgesChannel::Scale=>p.scale=v as f32,RoughenEdgesChannel::Stretch=>p.stretch=v as f32,RoughenEdgesChannel::Complexity=>p.complexity=v as f32,RoughenEdgesChannel::Evolution=>p.evolution=v as f32,RoughenEdgesChannel::Seed=>p.seed=v as f32,RoughenEdgesChannel::OffsetX=>p.offset[0]=v,RoughenEdgesChannel::OffsetY=>p.offset[1]=v,RoughenEdgesChannel::EdgeColorR=>p.edge_color[0]=v as f32,RoughenEdgesChannel::EdgeColorG=>p.edge_color[1]=v as f32,RoughenEdgesChannel::EdgeColorB=>p.edge_color[2]=v as f32,RoughenEdgesChannel::EdgeColorA=>p.edge_color[3]=v as f32,}}p.validate()?;Ok(p)}

#[cfg(test)]mod persistence_tests{
 use super::*;
 #[test]fn fourteen_channels_roundtrip_source_choices_and_local_clock(){
 let values=[("border",4.),("sharpness",2.),("influence",0.5),("scale",123.),("stretch",-2.),("complexity",3.),("evolution",720.),("seed",4.),("offsetX",1.23456789012345),("offsetY",-2.125),("edgeColorR",-1.),("edgeColorG",2.),("edgeColorB",0.5),("edgeColorA",0.)];
 let animation:serde_json::Map<String,serde_json::Value>=values.into_iter().map(|(name,v)|{let d=if name=="influence"{0.25}else if name.starts_with("offset"){0.0}else{0.5};(name.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":0,"value":v-d},{"tick":500000,"value":v+d}]}}))}).collect();
 let effect:super::super::composition::VideoEffect=serde_json::from_value(serde_json::json!({"kind":"roughen_edges","parameters":{"edgeType":7},"animation":animation})).unwrap();assert_eq!(effect,serde_json::from_value(serde_json::to_value(&effect).unwrap()).unwrap());
 let super::super::composition::VideoEffect::RoughenEdges{parameters,animation:Some(a)}=effect else{panic!("RoughenEdges")};assert_eq!(a.len(),14);let p=sample_animation(parameters,&a,250000).unwrap();assert_eq!(p.edge_type,7);assert_eq!(p.offset,[1.23456789012345,-2.125]);assert_eq!(p.edge_color,[-1.,2.,0.5,0.]);assert_eq!((p.border,p.sharpness,p.influence,p.scale,p.stretch,p.complexity,p.evolution,p.seed),(4.,2.,0.5,123.,-2.,3.,720.,4.));
 let invalid:RoughenEdgesAnimation=serde_json::from_value(serde_json::json!({"influence":{"mode":"constant","value":1.1}})).unwrap();assert!(validate_animation(&invalid).is_err());
 }
}

#[cfg(test)]mod original_tests {use super::*;use crate::domain::{group_surface::GroupSurface,linear_color_frame::LinearColorFrame};
 #[test]fn original_working_frame_rgba_bits(){
 #[derive(Deserialize)]struct Case{parameters:RoughenEdges,width:usize,height:usize,rgba:Vec<u8>,bits:Vec<u32>}
 #[derive(Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-roughen-edges.json")).unwrap();assert_eq!(f.cases.len(),256);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();c.parameters.apply_linear(&mut frame,&mut||Ok(())).unwrap();assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original RoughenEdges {i}");}
 }
}
