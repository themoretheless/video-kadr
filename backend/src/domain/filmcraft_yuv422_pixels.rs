//! SDR ProRes 10-bit 4:2:2 and PCM24 conversion from Filmcraft export/lib.rs
//! and job.rs (MIT, commit 5231852443363f001c3f6b396dd9b1e6461ae2be).
use super::group_surface::GroupSurface;
use anyhow::{ensure,Result};
pub fn yuv422_10(surface:&GroupSurface)->Result<Vec<u8>> {
    surface.validate()?;
    let(w,h)=(surface.width,surface.height);
    ensure!(w%2==0,"4:2:2 export requires even width");
    let pixels=w*h;let mut out=vec![0u8;pixels*4];
    for row in 0..h {for cx in 0..w/2 {
        let(mut u,mut v)=(0f32,0f32);
        for dx in 0..2 {
            let p=row*w+cx*2+dx;let at=p*4;
            let(r,g,b)=(surface.rgba[at]as f32/255.,surface.rgba[at+1]as f32/255.,surface.rgba[at+2]as f32/255.);
            let y=0.2126*r+0.7152*g+0.0722*b;
            let code=(64.+876.*y).round().clamp(4.,1019.)as u16;
            out[p*2..p*2+2].copy_from_slice(&code.to_le_bytes());
            u+=(b-y)/1.8556;v+=(r-y)/1.5748;
        }
        let c=row*(w/2)+cx;
        let cb=(512.+896.*(u*0.5)).round().clamp(4.,1019.)as u16;
        let cr=(512.+896.*(v*0.5)).round().clamp(4.,1019.)as u16;
        out[pixels*2+c*2..pixels*2+c*2+2].copy_from_slice(&cb.to_le_bytes());
        out[pixels*3+c*2..pixels*3+c*2+2].copy_from_slice(&cr.to_le_bytes());
    }}Ok(out)
}
pub fn pcm24(raw_f32:&[u8])->Result<Vec<u8>> {
    ensure!(raw_f32.len()%4==0,"unaligned PCM float input");
    let mut out=Vec::with_capacity(raw_f32.len()/4*3);
    for bytes in raw_f32.chunks_exact(4){
        let sample=f32::from_le_bytes(bytes.try_into().unwrap()).clamp(-1.,1.);
        out.extend_from_slice(&((sample*8_388_607.).round()as i32).to_le_bytes()[..3]);
    }Ok(out)
}
