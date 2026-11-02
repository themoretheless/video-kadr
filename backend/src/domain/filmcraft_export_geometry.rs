//! Match-source video geometry from Filmcraft export/settings.rs and pipeline.rs
//! (MIT, commit 5231852443363f001c3f6b396dd9b1e6461ae2be).
use super::transition_kernel_pack::{context::RenderContext, image::Image};
use anyhow::{ensure, Result};

pub fn video_dimensions(width: usize, height: usize) -> (usize, usize) {
    (width.max(2) & !1, height.max(2) & !1)
}

/// Source ExportSettings::resolve: f32 scaling, round, then a two-pixel minimum.
/// Codec-specific even cropping follows at the caller, after this resolution.
pub fn scaled_source_size(source: (usize, usize), scale: f32) -> Result<(usize, usize)> {
    ensure!(scale.is_finite()&&scale>0.,"export scale must be finite and positive");
    let width=((source.0 as f32*scale).round() as u32).max(2);
    let height=((source.1 as f32*scale).round() as u32).max(2);
    ensure!(width<=3840&&height<=2160,"scaled export dimensions exceed renderer budget");
    Ok((width as usize,height as usize))
}

use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scaling {
    #[default]
    ScaleToFit,
    ScaleToFill,
    StretchToFill,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Geometry {
    pub width: usize,
    pub height: usize,
    pub render_scale: f32,
    pub content: Option<(usize, usize)>,
    pub offset: (i64, i64),
}
impl Geometry {
    pub fn new(source: (usize,usize), output: (usize,usize), custom_size: bool, scaling: Scaling, pixel_aspect: Option<(u32,u32)>, max_quality: bool) -> Result<Self> {
        let (sw,sh)=(source.0.max(1) as f32,source.1.max(1) as f32);
        let (w,h)=output;
        let result=if !custom_size && pixel_aspect.is_none() && !max_quality {
            Self {width:w,height:h,render_scale:w as f32/sw,content:None,offset:(0,0)}
        } else {
            let par=pixel_aspect.map(|(n,d)|n.max(1)as f32/d.max(1)as f32).unwrap_or(1.);
            let (fx,fy)=(w as f32*par/sw,h as f32/sh);
            let (sx,sy)=match scaling {
                Scaling::ScaleToFit => (fx.min(fy),fx.min(fy)),
                Scaling::ScaleToFill => (fx.max(fy),fx.max(fy)),
                Scaling::StretchToFill => (fx,fy),
            };
            let (cw,ch)=((sw*sx/par).round().max(1.)as usize,(sh*sy).round().max(1.)as usize);
            let scale=if max_quality {1f32.max(sy).max(sx/par)}else{sy.max(sx/par)}.clamp(0.01,4.);
            Self {width:w,height:h,render_scale:scale,content:Some((cw,ch)),offset:((w as i64-cw as i64)/2,(h as i64-ch as i64)/2)}
        };
        ensure!(w>0&&w<=3840&&h>0&&h<=2160,"export canvas exceeds renderer budget");
        ensure!((sw*result.render_scale).round()<=3840.&&(sh*result.render_scale).round()<=2160.,"scaled export canvas exceeds renderer budget");
        if let Some((cw,ch))=result.content {ensure!(cw<=3840&&ch<=2160,"export picture exceeds renderer budget");}
        Ok(result)
    }
    pub fn fit(self, context:&RenderContext, mut image:Image)->Result<Image> {
        if let Some((w,h))=self.content {
            if image.w!=w {image=resample_axis(context,&image,w,true)?;}
            if image.h!=h {image=resample_axis(context,&image,h,false)?;}
        }
        if (image.w,image.h)!=(self.width,self.height)||self.offset!=(0,0) {
            image=place_at(context,&image,self.width,self.height,self.offset)?;
        }
        Ok(image)
    }
}

/// Match-source edge crop/pad; the general placement also supports centred fit/fill.
pub fn place(context: &RenderContext, image: &Image, width: usize, height: usize) -> Result<Image> {
    place_at(context,image,width,height,(0,0))
}
fn place_at(context:&RenderContext,image:&Image,width:usize,height:usize,(ox,oy):(i64,i64))->Result<Image> {
    ensure!(width <= 3840 && height <= 2160 && context.canvas_allowed(width,height),"invalid export canvas");
    context.image_work(&[image],||{
        let mut output=Image::new(width,height);
        let x0=ox.max(0)as usize;
        let x1=(ox+image.w as i64).clamp(0,width as i64)as usize;
        if x1<=x0 {return Ok(output)}
        for y in 0..height {
            if context.is_cancelled(){return Err("export placement cancelled".into())}
            let sy=y as i64-oy;
            if sy<0||sy>=image.h as i64 {continue}
            let sx=(x0 as i64-ox)as usize;
            output.px[(y*width+x0)*4..(y*width+x1)*4].copy_from_slice(&image.px[(sy as usize*image.w+sx)*4..(sy as usize*image.w+sx+x1-x0)*4]);
        }
        Ok(output)
    }).map_err(anyhow::Error::msg)
}
pub(crate) fn resample_axis(context:&RenderContext,image:&Image,n:usize,horizontal:bool)->Result<Image> {
    let (w,h)=if horizontal{(n,image.h)}else{(image.w,n)};
    ensure!(w<=3840&&h<=2160&&context.canvas_allowed(w,h),"export resample canvas exceeds budget");
    let source=if horizontal{image.w}else{image.h};
    let scale=source as f32/n as f32;
    let support=scale.max(1.);
    context.image_work(&[image],||{
    // At most n*(2*support+2) triangle coefficients plus per-output vector storage.
    let _weights=context.reserve_scratch(n*(2*support.ceil()as usize+2)*4+n*32);
    let weights:Vec<_>=(0..n).map(|i|{
        let center=(i as f32+0.5)*scale-0.5;
        let lo=((center-support).ceil()as i64).max(0)as usize;
        let hi=((center+support).floor()as i64).min(source as i64-1).max(lo as i64)as usize;
        let mut ws:Vec<f32>=(lo..=hi).map(|j|(1.-(j as f32-center).abs()/support).max(0.)).collect();
        let sum:f32=ws.iter().sum();
        if sum>0.{ws.iter_mut().for_each(|w|*w/=sum)}else{ws.fill(0.);if let Some(w)=ws.first_mut(){*w=1.;}}
        (lo,ws)
    }).collect();
        let mut output=Image::new(w,h);
        for y in 0..h {
            if context.is_cancelled(){return Err("export resample cancelled".into())}
            for x in 0..w {
                let (lo,weights)=if horizontal{&weights[x]}else{&weights[y]};
                let mut acc=[0f32;4];
                for (j,weight)in weights.iter().enumerate(){let(sx,sy)=if horizontal{(lo+j,y)}else{(x,lo+j)};for c in 0..4{acc[c]+=image.px[(sy*image.w+sx)*4+c]*weight;}}
                output.px[(y*w+x)*4..(y*w+x+1)*4].copy_from_slice(&acc);
            }
        }
        Ok(output)
    }).map_err(anyhow::Error::msg)
}
