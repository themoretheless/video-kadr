//! Bounded premultiplied linear f32 working frame for integrated color processing.
//! FilmCraft Image/SDR GradeSpace semantics, MIT notice docs/licenses/filmcraft-MIT.txt.
use super::{
    basic_grade::{display, linear},
    group_surface::GroupSurface,
};
use anyhow::{ensure, Result};
#[derive(Debug, Clone)]
pub struct LinearColorFrame {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 4]>,
    template: Vec<u8>,
    concat_hidden: Option<super::concat_hidden_rgb::HiddenRgb>,
    concat_hidden_checked: bool,
}
impl LinearColorFrame {
    /// Adopt source working values without an SDR byte conversion.
    pub(crate) fn from_working(width:usize,height:usize,values:&[f32],checkpoint:&mut dyn FnMut()->Result<()>)->Result<Self>{
        let count=width.checked_mul(height).and_then(|n|n.checked_mul(4)).ok_or_else(||anyhow::anyhow!("working frame size overflow"))?;
        ensure!(width>0&&height>0&&values.len()==count,"invalid working frame shape");
        let mut pixels=Vec::with_capacity(count/4);
        for row in values.chunks(width*4){checkpoint()?;for p in row.chunks_exact(4){ensure!(p.iter().all(|v|v.is_finite()),"nonfinite working frame");pixels.push([p[0],p[1],p[2],p[3]]);}}
        Ok(Self{width,height,pixels,template:vec![0;count],concat_hidden:None,concat_hidden_checked:false})
    }
    /// Original full-resolution source-provider n-by-n box decimation.
    pub(crate) fn box_decimated(&self,n:usize,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Self>{
        ensure!(n.is_power_of_two()&&n<=16,"invalid source decimation");let(width,height)=((self.width/n).max(1),(self.height/n).max(1));let mut pixels=Vec::with_capacity(width*height);let inv=1./(n*n)as f32;
        for oy in 0..height{checkpoint()?;for ox in 0..width{let mut acc=[0f32;4];for dy in 0..n{let y=(oy*n+dy).min(self.height-1);for dx in 0..n{let x=(ox*n+dx).min(self.width-1);let p=self.pixels[y*self.width+x];for k in 0..4{acc[k]+=p[k];}}}pixels.push(acc.map(|v|v*inv));}}
        Ok(Self{width,height,pixels,template:vec![0;width*height*4],concat_hidden:None,concat_hidden_checked:false})
    }
    pub(crate) fn pixels(&self) -> &[[f32;4]] { &self.pixels }
    /// Original Filmcraft two-by-two working-pixel reduction, preserving linear alpha.
    pub(crate) fn downsample2(&self, checkpoint: &mut dyn FnMut()->Result<()>) -> Result<Self> {
        let (width,height)=((self.width/2).max(1),(self.height/2).max(1));
        let mut pixels=Vec::with_capacity(width*height);
        for y in 0..height {checkpoint()?;for x in 0..width {
            let get=|dx:usize,dy:usize|self.pixels[(2*y+dy).min(self.height-1)*self.width+(2*x+dx).min(self.width-1)];
            let(a,b,c,d)=(get(0,0),get(1,0),get(0,1),get(1,1));
            pixels.push(std::array::from_fn(|k|(a[k]+b[k]+c[k]+d[k])*0.25));
        }}
        Ok(Self{width,height,pixels,template:vec![0;width*height*4],concat_hidden:None,concat_hidden_checked:false})
    }
    pub(crate) fn shrink_for_match(&self, checkpoint: &mut dyn FnMut()->Result<()>) -> Result<Self> {
        let mut out=self.clone();
        while out.width>96 {
            let (w,h)=((out.width/2).max(1),(out.height/2).max(1));
            let mut pixels=Vec::with_capacity(w*h);
            for y in 0..h {checkpoint()?;for x in 0..w {
                let get=|dx:usize,dy:usize|out.pixels[(2*y+dy).min(out.height-1)*out.width+(2*x+dx).min(out.width-1)];
                let (a,b,c,d)=(get(0,0),get(1,0),get(0,1),get(1,1));
                pixels.push(std::array::from_fn(|k|(a[k]+b[k]+c[k]+d[k])*0.25));
            }}
            out=Self{width:w,height:h,pixels,template:vec![0;w*h*4],concat_hidden:None,concat_hidden_checked:false};
        }
        Ok(out)
    }
    /// Transform working pixels without converting through an RGBA8 surface.
    pub(crate) fn transformed(&self,width:usize,height:usize,matrix:super::group_transform::GroupAffine,checkpoint:&mut dyn FnMut()->Result<()>) -> Result<Self> {
        ensure!(width>0&&height>0,"empty working transform canvas");
        ensure!(matrix.0.iter().all(|v|v.is_finite()),"nonfinite working transform");
        let [a,b,c,d,x,y]=matrix.0;let det=a*d-b*c;
        ensure!(det.is_finite()&&det!=0.,"singular working transform");
        let factor=1./det;let inverse=[d*factor,-c*factor,-b*factor,a*factor];
        let minify=1./(a*a+b*b).sqrt().min((c*c+d*d).sqrt()).max(1e-6);
        if minify>=2. && self.width>=4 && self.height>=4 {
            let(w,h)=((self.width/2).max(1),(self.height/2).max(1));let mut pixels=Vec::with_capacity(w*h);
            for row in 0..h{checkpoint()?;for column in 0..w{let get=|dx:usize,dy:usize|self.pixels[((2*row+dy).min(self.height-1))*self.width+(2*column+dx).min(self.width-1)];let(p,q,r,t)=(get(0,0),get(1,0),get(0,1),get(1,1));pixels.push(std::array::from_fn(|k|(p[k]+q[k]+r[k]+t[k])*0.25));}}
            let half=Self{width:w,height:h,pixels,template:vec![0;w*h*4],concat_hidden:None,concat_hidden_checked:false};
            return half.transformed(width,height,super::group_transform::GroupAffine([a*2.,b*2.,c*2.,d*2.,x,y]),checkpoint);
        }
        let ie=-(inverse[0]*x+inverse[1]*y);let iff=-(inverse[2]*x+inverse[3]*y);
        let corners=[[0.,0.],[self.width as f64,0.],[0.,self.height as f64],[self.width as f64,self.height as f64]].map(|p|matrix.point(p));
        let x0=corners.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min).floor().max(0.) as usize;
        let x1=corners.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max).ceil().max(0.) as usize;
        let y0=corners.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min).floor().max(0.) as usize;
        let y1=corners.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max).ceil().max(0.) as usize;
        ensure!(inverse.iter().all(|v|v.is_finite()),"nonfinite working inverse");
        let size=width.checked_mul(height).and_then(|n|n.checked_mul(4)).ok_or_else(||anyhow::anyhow!("working transform size overflow"))?;
        let mut pixels=Vec::with_capacity(size/4);
        for row in 0..height{checkpoint()?;for column in 0..width{
            if column<x0 || column>=x1 || row<y0 || row>=y1 {pixels.push([0.;4]);continue;}
            let px=column as f64+0.5;let py=row as f64+0.5;
            let(u,v)=if b==0.&&c==0.{(inverse[0]*px+ie,inverse[3]*py+iff)}else{(inverse[0]*px+inverse[1]*py+ie,inverse[2]*px+inverse[3]*py+iff)};
            ensure!(u.is_finite()&&v.is_finite(),"nonfinite working sample coordinate");
            pixels.push(super::gradient_wipe::transparent_sample(self,u as f32,v as f32));
        }}
        Ok(Self{width,height,pixels,template:vec![0;size],concat_hidden:None,concat_hidden_checked:false})
    }
    pub(crate) fn composite_working(&mut self,child:&Self,matrix:super::group_transform::GroupAffine,opacity:f32,blend:super::composition::BlendMode,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        use super::composition::BlendMode;
        ensure!(opacity.is_finite()&&(0.0..=1.).contains(&opacity),"invalid working opacity");
        let source=child.transformed(self.width,self.height,matrix,checkpoint)?;let mut index=0;
        self.map_premultiplied(&mut|b|{let s=source.pixels[index].map(|v|v*opacity);index+=1;let sa=s[3];let ba=b[3];if sa<=0.{return b;}if blend==BlendMode::Normal||ba<=0.{return std::array::from_fn(|k|s[k]+b[k]*(1.-sa));}
            let mut out=[0.;4];out[3]=sa+ba-sa*ba;
            for k in 0..3 {let cs=display(s[k]/sa);let cb=display(b[k]/ba);let mixed=match blend {BlendMode::Normal=>cs,BlendMode::Multiply=>cb*cs,BlendMode::Screen=>cb+cs-cb*cs,BlendMode::Overlay=>if cb<=0.5{cs*2.*cb}else{let t=2.*cb-1.;cs+t-cs*t},BlendMode::Darken=>cb.min(cs),BlendMode::Lighten=>cb.max(cs),BlendMode::Difference=>(cb-cs).abs(),BlendMode::Addition=>(cb+cs).min(1.)};out[k]=s[k]*(1.-ba)+b[k]*(1.-sa)+sa*ba*linear(mixed.clamp(0.,1.));}
            out
        },checkpoint)
    }
    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }
    fn prepare_concat_hidden(&mut self,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        if self.concat_hidden_checked{return Ok(())}
        let Some(mask)=super::concat_hidden_rgb::HiddenRgb::mask(&self.pixels,self.width,checkpoint)? else{self.concat_hidden=None;self.concat_hidden_checked=true;return Ok(())};
        if !self.concat_hidden.as_ref().is_some_and(|old|old.matches(&mask)){
            let colors=super::concat_hidden_rgb::HiddenRgb::new(mask,&self.pixels,&self.template,self.width,self.concat_hidden.as_ref(),checkpoint)?;self.concat_hidden=Some(colors);
        }
        self.concat_hidden_checked=true;Ok(())
    }
    fn concat_proxy(&self,index:usize)->[f32;4]{let p=self.pixels[index];if !super::concat_hidden_rgb::HiddenRgb::target_zero(p){return p}let color=if p[3]>0.{std::array::from_fn(|i|p[i]/p[3])}else{self.concat_hidden.as_ref().and_then(|hidden|hidden.get(index)).expect("prepared transparent color")};[color[0],color[1],color[2],1.]}
    fn store_concat_result(&mut self,index:usize,output:[f32;4])->Result<()>{
        ensure!(output.iter().all(|v|v.is_finite()),"nonfinite Concat pixel");
        if super::concat_hidden_rgb::HiddenRgb::target_zero(self.pixels[index]){ensure!(output[3]==1.,"Concat RGB pass changed proxy alpha");self.concat_hidden.as_mut().ok_or_else(||anyhow::anyhow!("missing transparent colors"))?.set(index,[output[0],output[1],output[2]])?;self.pixels[index]=[0.;4];}
        else{self.pixels[index]=output;}Ok(())
    }
    /// Preserve straight half-precision hidden colors across Concat-only RGB passes.
    pub(crate) fn map_concat_rgb_pixels(&mut self,operation:&mut dyn FnMut([f32;4],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        self.prepare_concat_hidden(checkpoint)?;for y in 0..self.height{checkpoint()?;for x in 0..self.width{let index=y*self.width+x;let output=operation(self.concat_proxy(index),x,y);self.store_concat_result(index,output)?;}}Ok(())
    }
    /// Frozen straight values for spatial Concat sampling; includes hidden RGB.
    pub(crate) fn concat_straight_snapshot(&mut self,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Vec<[f32;4]>> {
        Self::concat_alpha_scratch_bytes(self.pixels.len())?;
        self.prepare_concat_hidden(checkpoint)?;let mut out=Vec::with_capacity(self.pixels.len());
        for y in 0..self.height {checkpoint()?;for x in 0..self.width {let index=y*self.width+x;let p=self.concat_proxy(index);out.push([p[0]/p[3],p[1]/p[3],p[2]/p[3],self.pixels[index][3]]);}}
        Ok(out)
    }
    /// Conservative temporary allocation admission, not a physical RSS reading.
    pub(crate) fn concat_alpha_scratch_bytes(pixels:usize)->Result<usize> {
        let bytes=pixels.checked_mul(80).and_then(|v|v.checked_add(256)).ok_or_else(||anyhow::anyhow!("Concat alpha scratch size overflow"))?;
        ensure!(bytes<=1024*1024*1024,"Concat alpha scratch exceeds 1 GiB budget");Ok(bytes)
    }
    /// Alpha-changing straight-linear Concat pass. Stage results before commit so
    /// cancellation cannot leave new transparent pixels without their RGB.
    pub(crate) fn map_concat_straight_pixels(&mut self,operation:&mut dyn FnMut([f32;4],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        Self::concat_alpha_scratch_bytes(self.pixels.len())?;
        self.prepare_concat_hidden(checkpoint)?;
        let mut output=Vec::with_capacity(self.pixels.len());let mut hidden=Vec::new();
        for y in 0..self.height {checkpoint()?;for x in 0..self.width {
            let index=y*self.width+x;let proxy=self.concat_proxy(index);
            let rgb=std::array::from_fn::<_,3,_>(|i|proxy[i]/proxy[3]);
            let result=operation([rgb[0],rgb[1],rgb[2],self.pixels[index][3]],x,y);
            ensure!(result.iter().all(|v|v.is_finite()) && (0. ..=1.).contains(&result[3]) && result[..3].iter().all(|v|v.abs()<=65504.),"invalid Concat RGBA result");
            if super::concat_hidden_rgb::HiddenRgb::target_zero(result) {
                hidden.push((index,[result[0],result[1],result[2]]));output.push([0.;4]);
            } else {output.push([result[0]*result[3],result[1]*result[3],result[2]*result[3],result[3]]);}
        }}
        let hidden=super::concat_hidden_rgb::HiddenRgb::from_entries(output.len(),&hidden)?;
        checkpoint()?;self.pixels=output;self.concat_hidden=hidden;self.concat_hidden_checked=true;Ok(())
    }
    /// Frozen original RGB samples; retains the existing three-component row footprint.
    pub(crate) fn map_original_rgb_neighborhood<const N:usize>(&mut self,sample:&mut dyn FnMut([f32;4])->[f32;3],operation:&mut dyn FnMut([f32;4],[[f32;3];N],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        self.map_original_sample_neighborhood::<N,[f32;3]>(1,&mut |p,_|sample(p),operation,checkpoint)
    }
    /// Frozen RGBA samples expose real alpha separately from the hidden-color proxy.
    pub(crate) fn map_original_rgba_neighborhood<const N:usize>(&mut self,sample:&mut dyn FnMut([f32;4],f32)->[f32;4],operation:&mut dyn FnMut([f32;4],[[f32;4];N],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        self.map_original_sample_neighborhood::<N,[f32;4]>(1,sample,operation,checkpoint)
    }
    /// Sparse integer-spaced RGBA taps, retaining every intervening original row.
    pub(crate) fn map_original_rgba_neighborhood_step<const N:usize>(&mut self,spacing:usize,sample:&mut dyn FnMut([f32;4],f32)->[f32;4],operation:&mut dyn FnMut([f32;4],[[f32;4];N],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        self.map_original_sample_neighborhood::<N,[f32;4]>(spacing,sample,operation,checkpoint)
    }
    pub(crate) fn map_original_sample_neighborhood<const N:usize,T:Copy>(&mut self,spacing:usize,sample:&mut dyn FnMut([f32;4],f32)->T,operation:&mut dyn FnMut([f32;4],[T;N],usize,usize)->[f32;4],checkpoint:&mut dyn FnMut()->Result<()>)->Result<()>{
        ensure!(spacing==1||spacing==2||spacing==4||spacing==5||spacing==6||spacing==7,"unsupported Concat tap spacing");
        self.prepare_concat_hidden(checkpoint)?;
        let side:usize=match N{9=>3,25=>5,49=>7,_=>anyhow::bail!("unsupported RGB neighborhood")};let radius=side/2*spacing;let row_count=(side-1)*spacing+1;
        let width=self.width;let height=self.height;let mut rows=Vec::with_capacity(row_count);
        for n in 0..row_count{checkpoint()?;let y=(n as isize-radius as isize).clamp(0,height as isize-1)as usize;rows.push((y*width..(y+1)*width).map(|index|sample(self.concat_proxy(index),self.pixels[index][3])).collect::<Vec<T>>());}
        for y in 0..height{
            checkpoint()?;
            for x in 0..width{
                let neighbors:[T;N]=std::array::from_fn(|n|{let nx=(x as isize+((n%side)*spacing)as isize-radius as isize).clamp(0,width as isize-1)as usize;rows[n/side*spacing][nx]});
                let index=y*width+x;let output=operation(self.concat_proxy(index),neighbors,x,y);self.store_concat_result(index,output)?;
            }
            if y+1<height{
                rows.rotate_left(1);let last=row_count-1;rows[last].clear();let incoming=y+radius+1;
                if incoming<height{rows[last].extend((incoming*width..(incoming+1)*width).map(|index|sample(self.concat_proxy(index),self.pixels[index][3])));}
                else{let(head,tail)=rows.split_at_mut(last);tail[0].extend_from_slice(&head[last-1]);}
            }
        }Ok(())
    }
    pub(crate) fn map_premultiplied(&mut self, operation:&mut dyn FnMut([f32;4])->[f32;4], checkpoint:&mut dyn FnMut()->Result<()>) -> Result<()> {
        self.concat_hidden_checked=false;
        for row in self.pixels.chunks_mut(self.width) {
            checkpoint()?;
            for pixel in row {
                let output=operation(*pixel);
                ensure!(output.iter().all(|value|value.is_finite()), "nonfinite premultiplied effect pixel");
                *pixel=output;
            }
        }
        Ok(())
    }
    pub fn from_surface(
        image: &GroupSurface,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Self> {
        image.validate()?;
        checkpoint()?;
        let mut pixels = Vec::with_capacity(image.width * image.height);
        for row in image.rgba.chunks(image.width * 4) {
            checkpoint()?;
            for p in row.chunks_exact(4) {
                let a = p[3] as f32 / 255.0;
                pixels.push([
                    linear(p[0] as f32 / 255.0) * a,
                    linear(p[1] as f32 / 255.0) * a,
                    linear(p[2] as f32 / 255.0) * a,
                    a,
                ]);
            }
        }
        Ok(Self {
            width: image.width,
            height: image.height,
            pixels,
            template: image.rgba.clone(),
            concat_hidden:None,concat_hidden_checked:false,
        })
    }
    /// Read a scalar mask without mutating or quantizing the working frame.
    pub fn signal_mask(
        &self,
        operation: &mut dyn FnMut([f32; 3]) -> Result<f32>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Vec<f32>> {
        checkpoint()?;
        let mut mask = Vec::with_capacity(self.pixels.len());
        for row in self.pixels.chunks(self.width) {
            checkpoint()?;
            for p in row {
                let value = if p[3] <= 1e-6 {
                    0.0
                } else {
                    operation(
                        [p[0], p[1], p[2]].map(|q| display((q / p[3]).max(0.0)).clamp(0.0, 1.0)),
                    )?
                };
                ensure!(value.is_finite(), "nonfinite color mask");
                mask.push(value);
            }
        }
        Ok(mask)
    }
    /// Same SDR grading signal semantics as FilmCraft: encode nonnegative linear, decode clamped unit signal.
    pub(crate) fn map_linear_to_signal(&mut self, operation:&mut dyn FnMut([f32;3],usize,usize)->Result<[f32;3]>,checkpoint:&mut dyn FnMut()->Result<()>)->Result<()> {
        for (y,row) in self.pixels.chunks_mut(self.width).enumerate(){checkpoint()?;for (x,p) in row.iter_mut().enumerate(){let a=p[3];if a<=1e-6 {continue;}
            let out=operation([p[0]/a,p[1]/a,p[2]/a],x,y)?;
            ensure!(out.iter().all(|v|v.is_finite()),"nonfinite grading signal");
            for c in 0..3 {p[c]=linear(out[c].clamp(0.0,1.0))*a;}
        }}Ok(())
    }
    pub fn map_signal(
        &mut self,
        operation: &mut dyn FnMut([f32; 3], usize, usize) -> Result<[f32; 3]>,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        for (y, row) in self.pixels.chunks_mut(self.width).enumerate() {
            checkpoint()?;
            for (x, p) in row.iter_mut().enumerate() {
                let a = p[3];
                if a <= 1e-6 {
                    continue;
                }
                let v = [p[0], p[1], p[2]].map(|q| display((q / a).max(0.0)));
                let out = operation(v, x, y)?;
                ensure!(
                    out.iter().all(|v| v.is_finite()),
                    "nonfinite grading signal"
                );
                for c in 0..3 {
                    p[c] = linear(out[c].clamp(0.0, 1.0)) * a;
                }
            }
        }
        Ok(())
    }
    /// Apply the limiter in linear working space, preserving alpha and super-white values.
    pub fn video_limiter(
        &mut self,
        limiter: &super::video_limiter::VideoLimiter,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        limiter.validate()?;
        for row in self.pixels.chunks_mut(self.width) {
            checkpoint()?;
            for pixel in row {
                let alpha=pixel[3];
                if alpha <= 1e-6 { continue; }
                let output=limiter.apply([pixel[0]/alpha,pixel[1]/alpha,pixel[2]/alpha])?;
                for channel in 0..3 {pixel[channel]=output[channel]*alpha;}
            }
        }
        Ok(())
    }
    pub fn sharpen(
        &mut self,
        amount: f32,
        scale: f32,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            amount.is_finite() && (-1.0..=1.0).contains(&amount),
            "invalid linear sharpen amount"
        );
        ensure!(
            scale.is_finite() && scale > 0.0 && scale <= 1.0,
            "invalid linear sharpen scale"
        );
        checkpoint()?;
        if amount.abs() <= 1e-3 {
            return Ok(());
        }
        for c in 0..3 {
            let mut blurred = Vec::with_capacity(self.pixels.len());
            for row in self.pixels.chunks(self.width) {
                checkpoint()?;
                blurred.extend(row.iter().map(|p| p[c]));
            }
            // Sharpen uses the RGBA Gaussian kernel in Filmcraft, whose rolling
            // sum subtracts and adds separately and multiplies by the reciprocal.
            // HSL mask blur has a different floating-point evaluation order.
            let blurred = super::gaussian_blur::blur_plane(
                &blurred,
                self.width,
                self.height,
                1.2 * scale.max(0.35),
                0,
                true,
                checkpoint,
            )?;
            for (y, row) in self.pixels.chunks_mut(self.width).enumerate() {
                checkpoint()?;
                for (x, p) in row.iter_mut().enumerate() {
                    let q = (p[c] + (p[c] - blurred[y * self.width + x]) * amount).max(0.0);
                    ensure!(q.is_finite(), "nonfinite linear sharpen");
                    p[c] = q;
                }
            }
        }
        Ok(())
    }
    pub fn to_surface(&self, checkpoint: &mut dyn FnMut() -> Result<()>) -> Result<GroupSurface> {
        checkpoint()?;
        let mut rgba = self.template.clone();
        for (y, row) in rgba.chunks_mut(self.width * 4).enumerate() {
            checkpoint()?;
            for (x, p) in row.chunks_exact_mut(4).enumerate() {
                let v = self.pixels[y * self.width + x];
                ensure!(v.iter().all(|v| v.is_finite()), "nonfinite working frame");
                p[3] = (v[3] * 255.0 + 1e-4).round().clamp(0.0,255.0) as u8;
                if v[3] <= 1e-6 {
                    if v[3]<=0.{if let Some(color)=self.concat_hidden.as_ref().and_then(|hidden|hidden.get(y*self.width+x)){for c in 0..3{p[c]=(display(color[c].max(0.))*255.+1e-4).round().clamp(0.,255.)as u8;}}}
                    continue;
                }
                for c in 0..3 {
                    // Stabilize exact half-byte ties against f32 transfer/premultiply roundoff.
                    p[c] = (display((v[c] / v[3]).max(0.0)) * 255.0 + 1e-4)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
        }
        Ok(GroupSurface {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_mask_reads_fractional_signal_without_mutating_frame() {
        let source = GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 128, 12, 34, 56, 0],
        };
        let mut f = LinearColorFrame::from_surface(&source, &mut || Ok(())).unwrap();
        f.map_signal(
            &mut |v, _, _| Ok(v.map(|q| q + 0.2 / 255.0)),
            &mut || Ok(()),
        )
        .unwrap();
        let before = f.to_surface(&mut || Ok(())).unwrap();
        let mask = f.signal_mask(&mut |v| Ok(v[0]), &mut || Ok(())).unwrap();
        assert!((mask[0] - 0.2 / 255.0).abs() < 1e-6);
        assert_eq!(mask[1], 0.0);
        assert_eq!(f.to_surface(&mut || Ok(())).unwrap(), before);
        assert!(f
            .signal_mask(&mut |_| Ok(f32::NAN), &mut || Ok(()))
            .is_err());
    }
    #[test]
    fn signal_chain_retains_sub_byte_changes_until_output() {
        let image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![100, 100, 100, 128],
        };
        let mut f = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        for _ in 0..3 {
            f.map_signal(
                &mut |v, _, _| Ok(v.map(|q| q + 0.2 / 255.0)),
                &mut || Ok(()),
            )
            .unwrap();
        }
        assert_eq!(
            f.to_surface(&mut || Ok(())).unwrap().rgba,
            vec![101, 101, 101, 128]
        );
    }
    #[test]
    fn all_bytes_roundtrip_and_hidden_color() {
        let rgba: Vec<u8> = (0..256u16)
            .flat_map(|i| {
                [
                    i as u8,
                    (255 - i) as u8,
                    (i / 2) as u8,
                    if i % 3 == 0 { 0 } else { 128 },
                ]
            })
            .collect();
        let original = GroupSurface {
            width: 256,
            height: 1,
            rgba,
        };
        let f = LinearColorFrame::from_surface(&original, &mut || Ok(())).unwrap();
        assert_eq!(f.to_surface(&mut || Ok(())).unwrap(), original);
    }
    #[test]
    fn limiter_preserves_alpha_hidden_rgb_and_superwhites() {
        let mut frame=LinearColorFrame {
            width:3,height:1,
            pixels:vec![[2.0,2.0,2.0,0.5],[0.0,0.0,0.0,0.0],[0.0625,0.125,0.25,0.25]],
            template:vec![255,255,255,128,23,45,67,0,137,188,255,64],concat_hidden:None,concat_hidden_checked:false,
        };
        let alpha:Vec<_>=frame.pixels.iter().map(|p|p[3].to_bits()).collect();
        let limiter=super::super::video_limiter::VideoLimiter{clip_level:9,compression:0,axis:2,..Default::default()};
        frame.video_limiter(&limiter,&mut || Ok(())).unwrap();
        assert_eq!(frame.pixels.iter().map(|p|p[3].to_bits()).collect::<Vec<_>>(),alpha);
        assert!(frame.pixels[0][0]>0.5); // 109IRE retained above white before delivery.
        let output=frame.to_surface(&mut || Ok(())).unwrap();
        assert_eq!(&output.rgba[4..8],&[23,45,67,0]);
        assert_eq!(output.rgba[3],128);
        assert_eq!(output.rgba[11],64);
        let warning=super::super::video_limiter::VideoLimiter{gamut_warning:true,compression:0,axis:2,..Default::default()};
        frame.video_limiter(&warning,&mut || Ok(())).unwrap();
        assert_eq!(frame.pixels[0],[0.5,0.0,0.0,0.5]);
        assert!(frame.video_limiter(&limiter,&mut || anyhow::bail!("cancel")).is_err());
    }
    #[test]
    fn rejects_nonfinite_operation_and_cancellation() {
        let image = GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![100; 4],
        };
        let mut f = LinearColorFrame::from_surface(&image, &mut || Ok(())).unwrap();
        assert!(f
            .map_signal(&mut |_, _, _| Ok([f32::NAN; 3]), &mut || Ok(()))
            .is_err());
        assert!(f.to_surface(&mut || anyhow::bail!("cancel")).is_err());
    }
}

#[cfg(test)]mod working_transform_tests {use super::*;use crate::domain::group_transform::GroupAffine;
#[test]fn fractional_coverage_retains_working_bits_and_cancels(){
 let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:1,height:1,rgba:vec![255,255,255,128]},&mut||Ok(())).unwrap();frame.map_premultiplied(&mut|p|p.map(|v|v*0.33333334),&mut||Ok(())).unwrap();
 let identity=frame.transformed(1,1,GroupAffine::IDENTITY,&mut||Ok(())).unwrap();assert_eq!(identity.pixels(),frame.pixels());
 let shifted=frame.transformed(2,1,GroupAffine([1.,0.,0.,1.,0.5,0.]),&mut||Ok(())).unwrap();for p in shifted.pixels(){assert_eq!(*p,frame.pixels()[0].map(|v|v*0.5));}
 let quantized=LinearColorFrame::from_surface(&frame.to_surface(&mut||Ok(())).unwrap(),&mut||Ok(())).unwrap();assert_ne!(frame.pixels(),quantized.pixels());
 assert!(frame.transformed(1,1,GroupAffine::IDENTITY,&mut||anyhow::bail!("cancelled")).is_err());
 assert!(frame.transformed(1,1,GroupAffine([0.;6]),&mut||Ok(())).is_err());
}
}
#[cfg(test)]mod original_transform_tests {use super::*;
#[test]fn original_working_transform_bits(){
 #[derive(serde::Deserialize)]struct Case{width:usize,height:usize,rgba:Vec<u8>,matrix:[f64;6],attenuation:f32,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-working-transform.json")).unwrap();assert_eq!(f.cases.len(),40);
 for(i,c)in f.cases.into_iter().enumerate(){let mut frame=LinearColorFrame::from_surface(&GroupSurface{width:c.width,height:c.height,rgba:c.rgba},&mut||Ok(())).unwrap();frame.map_premultiplied(&mut|p|p.map(|v|v*c.attenuation),&mut||Ok(())).unwrap();let out=frame.transformed(18,10,crate::domain::group_transform::GroupAffine(c.matrix),&mut||Ok(())).unwrap();assert_eq!(out.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original transform {i}");}
}
}
#[cfg(test)]mod original_blend_tests {use super::*;
#[test]fn original_working_blend_bits(){
 #[derive(serde::Deserialize)]struct Case{blend:crate::domain::composition::BlendMode,opacity:f32,rgba:Vec<u8>,child:Vec<u8>,bits:Vec<u32>}
 #[derive(serde::Deserialize)]struct Fixture{cases:Vec<Case>}
 let f:Fixture=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-working-blend.json")).unwrap();assert_eq!(f.cases.len(),24);
 for(i,c)in f.cases.into_iter().enumerate(){let mut out=LinearColorFrame::from_surface(&GroupSurface{width:6,height:4,rgba:c.rgba},&mut||Ok(())).unwrap();let child=LinearColorFrame::from_surface(&GroupSurface{width:6,height:4,rgba:c.child},&mut||Ok(())).unwrap();out.composite_working(&child,crate::domain::group_transform::GroupAffine::IDENTITY,c.opacity,c.blend,&mut||Ok(())).unwrap();assert_eq!(out.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),c.bits,"original blend {i}");}
}
}
