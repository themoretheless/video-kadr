//! Independent calibrated spherical-video geometry; no vendor code or camera profiles.
use super::{linear_color_frame::LinearColorFrame, vr_sphere};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    #[default]
    Equirectangular,
    DualFisheye,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    #[default]
    Perspective,
    Equirectangular,
    TinyPlanet,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceLayout {
    #[default]
    Packed,
    DualVideoStreams,
}
/// Desired world-space camera path when an orientation track is supplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StabilizationMode {
    Disabled,
    #[default]
    Locked,
    Smooth,
    Horizon,
}
/// Equidistant lens. Centre and radius are fractions of full source width/height.
/// Rotation maps lens coordinates (+Z optical axis, +Y up) into camera coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lens {
    pub center: [f64; 2],
    pub radius: [f64; 2],
    pub fov: f64,
    pub rotation: [f64; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Orientation {
    pub time_seconds: f64,
    /// Camera-to-world quaternion [w,x,y,z].
    pub quaternion: [f64; 4],
}
pub(crate) struct LensProjection {
    generic: [[[f64;3];3];2],
    calibrated: Option<[[[f64;3];3];2]>,
    rolling: Option<super::rolling_shutter::RollingFrame>,
}
#[derive(Clone,Copy)]
pub(crate) struct LensSample {pub pixel:[f32;4],pub weight:f64,pub priority:f64,pub source_row:f64}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SphericalVideo {
    pub input: Input,
    pub source_layout: SourceLayout,
    pub output: Output,
    pub pan: f32,
    pub tilt: f32,
    pub roll: f32,
    pub fov: f32,
    pub lenses: [Lens; 2],
    pub camera_profile: Option<super::insta360_camera_profile::CameraProfile>,
    pub stitching: super::spherical_optical_flow::Stitching,
    pub orientation: Vec<Orientation>,
    pub orientation_offset_seconds: f64,
    pub stabilization_mode: StabilizationMode,
    pub smooth_seconds: f64,
    pub rolling_shutter: Option<super::rolling_shutter::RollingShutter>,
    pub source_frame_clock: Option<super::rolling_shutter::SourceFrameClock>,
}
impl Default for SphericalVideo {
    fn default() -> Self {
        Self {
            input: Input::Equirectangular,
            source_layout: SourceLayout::Packed,
            output: Output::Perspective,
            pan: 0.,
            tilt: 0.,
            roll: 0.,
            fov: 90.,
            lenses: [
                Lens {
                    center: [0.25, 0.5],
                    radius: [0.25, 0.5],
                    fov: 190.,
                    rotation: [0., 0., 0.],
                },
                Lens {
                    center: [0.75, 0.5],
                    radius: [0.25, 0.5],
                    fov: 190.,
                    rotation: [0., 180., 0.],
                },
            ],
            camera_profile: None,
            stitching: Default::default(),
            orientation: vec![],
            orientation_offset_seconds: 0.,
            stabilization_mode: StabilizationMode::Locked,
            smooth_seconds: 0.5,
            rolling_shutter: None,
            source_frame_clock: None,
        }
    }
}
fn qdot(a: [f64; 4], b: [f64; 4]) -> f64 {
    (0..4).map(|i| a[i] * b[i]).sum()
}
fn normalize(q: [f64; 4]) -> [f64; 4] {
    let n = qdot(q, q).sqrt();
    q.map(|v| v / n)
}
fn rotate(q: [f64; 4], d: [f64; 3]) -> [f64; 3] {
    let u = [q[1], q[2], q[3]];
    let c = vr_sphere::cross(u, d);
    let cc = vr_sphere::cross(u, c);
    std::array::from_fn(|i| d[i] + 2. * (q[0] * c[i] + cc[i]))
}
fn orientation_at(track: &[Orientation], time: f64) -> [f64; 4] {
    if track.is_empty() {
        return [1., 0., 0., 0.];
    }
    let index = track.partition_point(|s| s.time_seconds <= time);
    if index == 0 {
        return normalize(track[0].quaternion);
    }
    if index == track.len() {
        return normalize(track[index - 1].quaternion);
    }
    let (a, b) = (track[index - 1], track[index]);
    let a = normalize(a.quaternion);
    let mut bq = normalize(b.quaternion);
    let mut dot = qdot(a, bq);
    if dot < 0. {
        bq = bq.map(|v| -v);
        dot = -dot;
    }
    let t =
        (time - track[index - 1].time_seconds) / (b.time_seconds - track[index - 1].time_seconds);
    if dot > 0.9995 {
        return normalize(std::array::from_fn(|i| a[i] + (bq[i] - a[i]) * t));
    }
    let angle = dot.clamp(-1., 1.).acos();
    let den = angle.sin();
    std::array::from_fn(|i| (a[i] * ((1. - t) * angle).sin() + bq[i] * (t * angle).sin()) / den)
}
/// Continuous heading in two conditioned horizontal-axis charts. Forward
/// azimuth is retained while observable; near a vertical forward axis we use
/// the right axis, matching the phase at the transition pose. The selected
/// chart returns to forward once its horizontal norm exceeds .6. This avoids both pole
/// flips and the artificial yaw produced by globally integrating world-up spin
/// during ordinary optical roll. Heading inside/after a pole is path-dependent;
/// this is an independent convention, not a vendor heading model.
fn horizon_orientation(track: &[Orientation], time: f64) -> [f64; 4] {
    if track.is_empty() {return [1.,0.,0.,0.];}
    let first=normalize(track[0].quaternion);
    let mut axis=if heading_chart(first,false).1>=0.3 {false}else{true};
    let mut previous=heading_chart(first,axis).0;
    let mut heading=previous;
    for pair in track.windows(2) {
        if time<=pair[0].time_seconds {break;}
        let a=normalize(pair[0].quaternion);let b=normalize(pair[1].quaternion);
        // Fixed full-segment subdivision makes arbitrary query times share the
        // same transition history. A <=.05rad step cannot skip the .3 pole cap.
        let steps=((2.*qdot(a,b).abs().clamp(0.,1.).acos()/0.05).ceil() as usize).max(1);
        let end=time.min(pair[1].time_seconds);
        for step in 1..=steps {
            let sample=(pair[0].time_seconds+(pair[1].time_seconds-pair[0].time_seconds)*step as f64/steps as f64).min(end);
            let q=orientation_at(track,sample);
            let (angle,norm)=heading_chart(q,axis);
            heading+=(angle-previous+PI).rem_euclid(TAU)-PI;
            previous=angle;
            if (axis && heading_chart(q,false).1>=0.6) || (norm<0.3 && heading_chart(q,!axis).1>0.6) {
                axis=!axis;previous=heading_chart(q,axis).0;
            }
            if sample>=end {break;}
        }
        if time<=pair[1].time_seconds {break;}
    }
    [(heading/2.).cos(),0.,(heading/2.).sin(),0.]
}
fn heading_chart(q:[f64;4],right:bool)->(f64,f64) {
    let v=rotate(q,if right {[1.,0.,0.]}else{[0.,0.,1.]});
    (if right {(-v[2]).atan2(v[0])}else{v[0].atan2(v[2])},v[0].hypot(v[2]))
}

impl SphericalVideo {
    pub fn validate(&self) -> Result<()> {
        if let Some(clock)=self.source_frame_clock {clock.validate()?;}
        if let Some(rs)=&self.rolling_shutter{rs.validate()?;ensure!(self.input==Input::DualFisheye,"rolling shutter requires original dual fisheye sensor rows");if rs.readout_seconds>0.{ensure!(self.orientation.len()>=2,"rolling shutter requires an orientation track");}}
        self.stitching.validate()?;
        ensure!(self.stitching.mode != super::spherical_optical_flow::Mode::OpticalFlow || self.input == Input::DualFisheye,"optical stitching requires dual fisheye input");
        if let Some(profile)=&self.camera_profile {
            ensure!(self.input == Input::DualFisheye, "camera profile requires dual fisheye input");
            profile.validate()?;
        }
        ensure!(self.source_layout != SourceLayout::DualVideoStreams || self.input == Input::DualFisheye,"dual video streams require dual fisheye input");
        ensure!(
            [self.pan, self.tilt, self.roll, self.fov]
                .iter()
                .all(|v| v.is_finite()),
            "nonfinite spherical camera"
        );
        ensure!(
            (10. ..=179.).contains(&self.fov),
            "spherical field of view must be 10..179 degrees"
        );
        ensure!(
            self.orientation_offset_seconds.is_finite(),
            "invalid orientation offset"
        );
        ensure!(self.smooth_seconds.is_finite() && (0.01..=10.).contains(&self.smooth_seconds), "smoothing must be 0.01..10 seconds");
        ensure!(
            self.orientation.len() <= 20000,
            "too many orientation samples"
        );
        let mut last = None;
        for s in &self.orientation {
            ensure!(
                s.time_seconds.is_finite()
                    && s.time_seconds >= 0.
                    && last.is_none_or(|t| s.time_seconds > t),
                "orientation times must increase"
            );
            ensure!(
                s.quaternion.iter().all(|v| v.is_finite())
                    && (0.99..=1.01).contains(&qdot(s.quaternion, s.quaternion)),
                "orientation must be a unit quaternion"
            );
            last = Some(s.time_seconds);
        }
        for l in self.lenses {
            ensure!(
                l.center
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v))
                    && l.radius
                        .iter()
                        .all(|v| v.is_finite() && (0.001..=1.).contains(v))
                    && l.rotation.iter().all(|v| v.is_finite())
                    && l.fov.is_finite()
                    && (180. ..=240.).contains(&l.fov),
                "invalid equidistant lens calibration"
            );
        }
        Ok(())
    }
    pub(crate) fn lens_projection(&self)->LensProjection {
        LensProjection {
            generic:self.lenses.map(|l|vr_sphere::transpose(vr_sphere::rotation(l.rotation[0],l.rotation[1],l.rotation[2]))),
            calibrated:self.camera_profile.as_ref().map(|p|p.optical_rotations()),
            rolling:None,
        }
    }
    /// Common midpoint-body direction sampler. Rolling-shutter correction can
    /// adjust the ray using the returned source row, without bypassing flow.
    pub(crate) fn lens_sample_at(&self,src:&vr_sphere::Image,projection:&LensProjection,lens:usize,d:[f64;3],_time_seconds:f64)->Option<LensSample> {
        let Some(rolling)=&projection.rolling else{return self.lens_sample(src,projection,lens,d)};
        // Source row is implicit in the calibrated projection: solve it in sensor
        // coordinates, with a bounded iteration. Unresolvable rays are not blended.
        let mut row=self.lens_sample(src,projection,lens,d)?.source_row;
        for _ in 0..10 {
            let ray=rotate(rolling.rotation(lens,row),d);
            let sample=self.lens_sample(src,projection,lens,ray)?;
            if (sample.source_row-row).abs()<=0.05/src.h as f64{return Some(sample)}
            row=0.2*row+0.8*sample.source_row;
        }
        None
    }
    pub(crate) fn lens_sample(&self,src:&vr_sphere::Image,projection:&LensProjection,lens:usize,d:[f64;3])->Option<LensSample> {
        if let Some(profile)=&self.camera_profile {
            let optical=vr_sphere::mvec(&projection.calibrated.as_ref()?[lens],d);
            let pixel=profile.lenses[lens].project_optical(optical)?;
            let half=(profile.reference_size[0]/2)as f64;let height=profile.reference_size[1]as f64;
            let local_x=pixel[0]-lens as f64*half;let v=pixel[1]/(height-1.);
            if !(0. ..=half-1.).contains(&local_x)||!(0. ..=1.).contains(&v){return None}
            let decoded_half=src.w as f64*0.5;
            let sx=lens as f64*decoded_half+local_x/(half-1.)*(decoded_half-1.)+0.5;
            let sy=v*(src.h as f64-1.)+0.5;
            Some(LensSample{pixel:sample_clamp(src,sx,sy),weight:(optical[2]-100f64.to_radians().cos()).clamp(1e-8,0.1),priority:(optical[2]-100f64.to_radians().cos()).max(0.),source_row:v})
        }else{
            let l=self.lenses[lens];let optical=vr_sphere::mvec(&projection.generic[lens],d);
            let theta=optical[2].clamp(-1.,1.).acos();let half=l.fov.to_radians()*0.5;
            if theta>half+1e-12{return None}
            let radial=(theta/half).min(1.);let az=optical[1].atan2(optical[0]);
            let u=l.center[0]+l.radius[0]*radial*az.cos();let v=l.center[1]-l.radius[1]*radial*az.sin();
            if !(0. ..=1.).contains(&u)||!(0. ..=1.).contains(&v){return None}
            Some(LensSample{pixel:sample_clamp(src,u*src.w as f64,v*src.h as f64),weight:(half-theta).clamp(1e-12,0.1),priority:(half-theta).max(0.),source_row:v})
        }
    }
    fn desired_orientation(&self,time_seconds:f64,source_pose:[f64;4])->[f64;4] {
        match self.stabilization_mode {
            StabilizationMode::Disabled => source_pose,
            StabilizationMode::Locked => [1.,0.,0.,0.],
            StabilizationMode::Smooth => smooth_orientation(&self.orientation,time_seconds+self.orientation_offset_seconds,self.smooth_seconds),
            StabilizationMode::Horizon => horizon_orientation(&self.orientation,time_seconds+self.orientation_offset_seconds),
        }
    }
    /// A measured body-panorama target becomes a key in the SAME stabilized
    /// virtual frame used by the renderer, including source held-frame timing.
    pub fn map_tracking_centre(&self,mut centre:super::spherical_tracking::Centre,clip_time:f64)->Result<super::spherical_tracking::Centre>{
        self.validate()?;ensure!(clip_time.is_finite()&&clip_time>=0.&&centre.pan.is_finite()&&(-180. ..=180.).contains(&centre.pan)&&centre.tilt.is_finite()&&(-90. ..=90.).contains(&centre.tilt),"invalid tracked body direction");
        let source_time=self.source_frame_clock.map(|c|c.presentation_time(clip_time)).transpose()?.unwrap_or(clip_time);
        let source_q=orientation_at(&self.orientation,source_time+self.orientation_offset_seconds);
        let desired=self.desired_orientation(clip_time,source_q);
        let body=vr_sphere::dir(centre.pan.to_radians(),-centre.tilt.to_radians());
        let target=rotate([desired[0],-desired[1],-desired[2],-desired[3]],rotate(source_q,body));
        centre.pan=target[0].atan2(target[2]).to_degrees();centre.tilt=-target[1].clamp(-1.,1.).asin().to_degrees();Ok(centre)
    }
    /// Reproject a saved source-normalized mask into the body panorama used by
    /// neural identity inference. RGB-derived seam flow and source-row timing
    /// match the prepared inference frame; probabilities use a max union rather
    /// than attenuating a first-stream seed with an unobserved second lens.
    pub fn project_tracking_mask(&self,mask:&[u8],width:usize,height:usize,time_seconds:f64,source:&super::group_surface::GroupSurface,checkpoint:&mut dyn FnMut()->Result<()>)->Result<Vec<u8>> {
        self.validate()?;ensure!(self.input==Input::DualFisheye&&self.output==Output::Equirectangular&&self.stabilization_mode==StabilizationMode::Disabled&&self.pan==0.&&self.tilt==0.&&self.roll==0.,"tracking mask projection requires a body panorama");
        ensure!(width>1&&height>1&&width.checked_mul(height)==Some(mask.len())&&mask.len()<=256*256,"invalid semantic seed plane");
        let frame=LinearColorFrame::from_surface(source,checkpoint)?;let(w,h)=frame.dimensions();ensure!(w>1&&h>1&&w%2==0,"invalid packed lens raster");
        let rgb=vr_sphere::Image{w,h,px:frame.pixels().to_vec()};
        let at=|x:i64,y:i64|mask[y.clamp(0,height as i64-1)as usize*width+x.clamp(0,width as i64-1)as usize]as f32/255.;
        let mut pixels=Vec::with_capacity(w*h);
        for y in 0..h{checkpoint()?;for x in 0..w{
            if self.source_layout==SourceLayout::DualVideoStreams&&x>=w/2{pixels.push([0.;4]);continue;}
            let reference_width=if self.source_layout==SourceLayout::DualVideoStreams{w/2}else{w};
            let sx=(x as f64+0.5)/reference_width as f64*width as f64-0.5;let sy=(y as f64+0.5)/h as f64*height as f64-0.5;
            let(x0,y0)=(sx.floor()as i64,sy.floor()as i64);let(fx,fy)=((sx-sx.floor())as f32,(sy-sy.floor())as f32);
            let p=(at(x0,y0)*(1.-fx)+at(x0+1,y0)*fx)*(1.-fy)+(at(x0,y0+1)*(1.-fx)+at(x0+1,y0+1)*fx)*fy;pixels.push([p;4]);
        }}
        let probability=vr_sphere::Image{w,h,px:pixels};let mut projection=self.lens_projection();
        let source_time=self.source_frame_clock.map(|c|c.presentation_time(time_seconds)).transpose()?.unwrap_or(time_seconds);
        if let Some(rs)=self.rolling_shutter.as_ref().filter(|r|r.readout_seconds>0.){projection.rolling=Some(super::rolling_shutter::RollingFrame::prepare(rs,&self.orientation,source_time+self.orientation_offset_seconds,h,checkpoint)?);}
        let flow=if self.stitching.mode==super::spherical_optical_flow::Mode::OpticalFlow&&self.stitching.strength>0.{Some(super::spherical_optical_flow::Flow::prepare_with_priority(|lens,u,v|self.lens_sample_at(&rgb,&projection,lens,vr_sphere::dir((u-0.5)*TAU,(0.5-v)*PI),time_seconds).map(|s|(s.pixel,s.priority)),self.stitching,checkpoint)?)}else{None};
        let mut output=Vec::with_capacity(256*256);
        for y in 0..256{checkpoint()?;for x in 0..256{let uv=[(x as f64+0.5)/256.,(y as f64+0.5)/256.];let d=vr_sphere::dir((uv[0]-0.5)*TAU,(0.5-uv[1])*PI);let displacement=flow.as_ref().and_then(|f|f.displacement(uv[0],uv[1]));let mut best=0f32;
            let mut colors=[None,None];let mut masks=[None,None];
            for lens in 0..2{let warped=if let Some((delta,confidence))=displacement{let factor=(if lens==0{-0.5}else{0.5})*self.stitching.strength*confidence;vr_sphere::dir((uv[0]+delta[0]*factor-0.5)*TAU,(0.5-(uv[1]+delta[1]*factor).clamp(0.,1.))*PI)}else{d};
                masks[lens]=self.lens_sample_at(&probability,&projection,lens,warped,time_seconds).or_else(||self.lens_sample_at(&probability,&projection,lens,d,time_seconds));
                if flow.is_some(){colors[lens]=self.lens_sample_at(&rgb,&projection,lens,warped,time_seconds).or_else(||self.lens_sample_at(&rgb,&projection,lens,d,time_seconds));}
            }
            let weights=if flow.is_some(){super::spherical_optical_flow::seam_weights(colors.map(|s|s.map_or([0.;4],|s|s.pixel)),colors.map(|s|s.map_or(0.,|s|s.weight)),self.stitching.strength*self.stitching.occlusion_strength,displacement.map_or(0.,|(_,c)|c),flow.as_ref().and_then(|f|f.ownership(uv[0],uv[1])))}else{[1.,1.]};
            for lens in 0..2{if weights[lens]>1e-9{if let Some(s)=masks[lens]{best=best.max(s.pixel[3]);}}}
            output.push((best.clamp(0.,1.)*255.).round()as u8);
        }}Ok(output)
    }
    pub fn sample(
        &self,
        a: Option<&vr_sphere::VrSphereAnimation>,
        time: super::animation_sample_time::AnimationSampleTime,
    ) -> Result<Self> {
        let mut p = self.clone();
        if let Some(a) = a {
            let sampled = vr_sphere::sample_animation(
                vr_sphere::VrSphere {
                    tilt: p.tilt,
                    pan: p.pan,
                    roll: p.roll,
                    fov: p.fov,
                    ..Default::default()
                },
                a,
                time,
                false,
            )?;
            p.tilt = sampled.tilt;
            p.pan = sampled.pan;
            p.roll = sampled.roll;
            p.fov = sampled.fov;
        }
        p.validate()?;
        Ok(p)
    }
    pub fn apply_linear(
        &self,
        frame: &mut LinearColorFrame,
        time_seconds: f64,
        checkpoint: &mut dyn FnMut() -> Result<()>,
    ) -> Result<()> {
        self.validate()?;
        ensure!(time_seconds.is_finite(), "invalid spherical sample time");
        checkpoint()?;
        let (w, h) = frame.dimensions();
        let src = vr_sphere::Image {
            w,
            h,
            px: frame.pixels().to_vec(),
        };
        let mut out = Vec::with_capacity(w * h);
        let camera = vr_sphere::rotation(self.tilt as f64, self.pan as f64, self.roll as f64);
        let capture_frame_seconds=self.source_frame_clock.map(|clock|clock.presentation_time(time_seconds)).transpose()?.unwrap_or(time_seconds);
        let mut projection = self.lens_projection();
        if let Some(rs)=self.rolling_shutter.as_ref().filter(|r|r.readout_seconds>0.) {
            projection.rolling=Some(super::rolling_shutter::RollingFrame::prepare(rs,&self.orientation,capture_frame_seconds+self.orientation_offset_seconds,h,checkpoint)?);
        }
        let flow = if self.input == Input::DualFisheye && self.stitching.mode == super::spherical_optical_flow::Mode::OpticalFlow && self.stitching.strength > 0. {
            Some(super::spherical_optical_flow::Flow::prepare_with_priority(|lens,u,v| {
                let d=vr_sphere::dir((u-0.5)*TAU,(0.5-v)*PI);
                self.lens_sample_at(&src,&projection,lens,d,time_seconds).map(|s|(s.pixel,s.priority))
            },self.stitching,checkpoint)?)
        } else { None };
        let q = orientation_at(
            &self.orientation,
            capture_frame_seconds + self.orientation_offset_seconds,
        );
        let desired = self.desired_orientation(time_seconds,q);
        let inverse = multiply([q[0], -q[1], -q[2], -q[3]], desired);
        let extent = (self.fov as f64 * PI / 360.).tan();
        for y in 0..h {
            checkpoint()?;
            for x in 0..w {
                let nx = (2. * (x as f64 + 0.5) / w as f64 - 1.) * extent;
                let ny = (1. - 2. * (y as f64 + 0.5) / h as f64) * extent * h as f64 / w as f64;
                let d = match self.output {
                    Output::Perspective => {
                        let n = (nx * nx + ny * ny + 1.).sqrt();
                        [nx / n, ny / n, 1. / n]
                    }
                    Output::Equirectangular => vr_sphere::dir(
                        ((x as f64 + 0.5) / w as f64 - 0.5) * TAU,
                        (0.5 - (y as f64 + 0.5) / h as f64) * PI,
                    ),
                    Output::TinyPlanet => {
                        let r = nx * nx + ny * ny;
                        [2. * nx / (1. + r), (r - 1.) / (1. + r), 2. * ny / (1. + r)]
                    }
                };
                let d = rotate(inverse, vr_sphere::mvec(&camera, d));
                let pixel = if self.input == Input::Equirectangular {
                    vr_sphere::sample_dir(&src, d)
                } else {
                    let uv=if flow.is_some(){[d[0].atan2(d[2])/TAU+0.5,0.5-d[1].clamp(-1.,1.).asin()/PI]}else{[0.,0.]};
                    let displacement=flow.as_ref().and_then(|field|field.displacement(uv[0],uv[1]));
                    let mut sum=[0.;4];let mut total=0.;
                    let mut samples=[None,None];
                    for lens in 0..2 {
                        let warped=if let Some((delta,confidence))=displacement {
                            let factor=(if lens==0{-0.5}else{0.5})*self.stitching.strength*confidence;
                            vr_sphere::dir(((uv[0]+delta[0]*factor)-0.5)*TAU,(0.5-(uv[1]+delta[1]*factor).clamp(0.,1.))*PI)
                        }else{d};
                        let sample=self.lens_sample_at(&src,&projection,lens,warped,time_seconds).or_else(||self.lens_sample_at(&src,&projection,lens,d,time_seconds));
                        samples[lens]=sample;
                    }
                    let mut weights=samples.map(|s|s.map_or(0.,|s|s.weight));
                    if flow.is_some(){weights=super::spherical_optical_flow::seam_weights(samples.map(|s|s.map_or([0.;4],|s|s.pixel)),weights,self.stitching.strength*self.stitching.occlusion_strength,displacement.map_or(0.,|(_,c)|c),flow.as_ref().and_then(|f|f.ownership(uv[0],uv[1])));}
                    for lens in 0..2 {if let Some(sample)=samples[lens] {total+=weights[lens];for k in 0..4{sum[k]+=sample.pixel[k]*weights[lens] as f32;}}
                    }
                    if total>0.{sum.map(|v|v/total as f32)}else{[0.;4]}
                };
                out.push(pixel);
            }
        }
        checkpoint()?;
        let mut i = 0;
        frame.map_premultiplied(
            &mut |_| {
                let p = out[i];
                i += 1;
                p
            },
            checkpoint,
        )
    }
}
fn sample_clamp(src: &vr_sphere::Image, x: f64, y: f64) -> [f32; 4] {
    let x = (x - 0.5).clamp(0., (src.w - 1) as f64);
    let y = (y - 0.5).clamp(0., (src.h - 1) as f64);
    let (ix, iy) = (x.floor() as usize, y.floor() as usize);
    let (tx, ty) = ((x - ix as f64) as f32, (y - iy as f64) as f32);
    let get = |x: usize, y: usize| src.px[y.min(src.h - 1) * src.w + x.min(src.w - 1)];
    let (a, b, c, d) = (
        get(ix, iy),
        get(ix + 1, iy),
        get(ix, iy + 1),
        get(ix + 1, iy + 1),
    );
    std::array::from_fn(|k| {
        (a[k] * (1. - tx) + b[k] * tx) * (1. - ty) + (c[k] * (1. - tx) + d[k] * tx) * ty
    })
}

fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0]*b[0]-a[1]*b[1]-a[2]*b[2]-a[3]*b[3],
     a[0]*b[1]+a[1]*b[0]+a[2]*b[3]-a[3]*b[2],
     a[0]*b[2]-a[1]*b[3]+a[2]*b[0]+a[3]*b[1],
     a[0]*b[3]+a[1]*b[2]-a[2]*b[1]+a[3]*b[0]]
}
/// Symmetric Gaussian quaternion average: no causal phase lag, antipodal-safe.
/// Quadrature weights preserve the duration of irregularly sampled observations.
fn smooth_orientation(track: &[Orientation], time: f64, sigma: f64) -> [f64; 4] {
    let reference = orientation_at(track, time);
    if track.len() < 2 { return reference; }
    let time = time.clamp(track[0].time_seconds, track[track.len()-1].time_seconds);
    let start = track.partition_point(|s| s.time_seconds < time - 3. * sigma);
    let end = track.partition_point(|s| s.time_seconds <= time + 3. * sigma);
    let mut sum = [0.; 4];
    for i in start..end {
        let sample = track[i];
        let left = if i == 0 { sample.time_seconds } else { (track[i-1].time_seconds + sample.time_seconds) * 0.5 };
        let right = if i+1 == track.len() { sample.time_seconds } else { (track[i+1].time_seconds + sample.time_seconds) * 0.5 };
        let dt = (sample.time_seconds-time)/sigma;
        let weight = (-0.5*dt*dt).exp() * (right-left) * if qdot(reference,sample.quaternion) < 0. { -1. } else { 1. };
        for k in 0..4 { sum[k] += sample.quaternion[k] * weight; }
    }
    if qdot(sum,sum) < 1e-20 { reference } else { normalize(sum) }
}

#[cfg(test)]
mod horizon_heading_tests {
    use super::*;
    fn pitch_yaw(pitch:f64,yaw:f64)->[f64;4] {
        multiply([(yaw/2.).cos(),0.,(yaw/2.).sin(),0.],[(pitch/2.).cos(),(pitch/2.).sin(),0.,0.])
    }
    fn same(a:[f64;4],b:[f64;4]) {assert!((qdot(a,b).abs()-1.).abs()<1e-10,"{a:?} != {b:?}");}
    #[test]
    fn pure_pitch_crosses_both_poles_without_artificial_heading() {
        let track:Vec<_>=(0..=360).map(|i|Orientation{time_seconds:i as f64,quaternion:pitch_yaw((i as f64).to_radians(),0.)}).collect();
        for t in [0.,89.5,90.,90.5,179.5,180.,180.5,270.,360.] {same(horizon_orientation(&track,t),[1.,0.,0.,0.]);}
    }
    #[test]
    fn world_yaw_is_preserved_at_vertical_and_upside_down() {
        for pitch in [0f64,90.,180.,270.] {
            let track:Vec<_>=(0..=120).map(|i|Orientation{time_seconds:i as f64,quaternion:pitch_yaw(pitch.to_radians(),(i as f64).to_radians())}).collect();
            for t in [0.,0.5,60.,119.5,120.] {
                same(horizon_orientation(&track,t),multiply(horizon_orientation(&track,0.),pitch_yaw(0.,t.to_radians())));
            }
        }
    }
    #[test]
    fn optical_roll_at_normal_tilt_does_not_invent_pan() {
        let track:Vec<_>=(0..=180).map(|i| {
            let r=(i as f64).to_radians()/2.;
            Orientation{time_seconds:i as f64,quaternion:multiply(pitch_yaw(PI/4.,0.),[r.cos(),0.,0.,r.sin()])}
        }).collect();
        for t in [0.,45.,90.,135.,180.] {same(horizon_orientation(&track,t),[1.,0.,0.,0.]);}
    }
    #[test]
    fn recovered_forward_chart_rejects_post_pole_optical_roll() {
        let mut track:Vec<_>=[0f64,100.,45.].iter().enumerate().map(|(i,p)|Orientation{time_seconds:i as f64,quaternion:pitch_yaw(p.to_radians(),0.)}).collect();
        for i in 1..=180 {let r=(i as f64).to_radians()/2.;track.push(Orientation{time_seconds:2.+i as f64/100.,quaternion:multiply(pitch_yaw(PI/4.,0.),[r.cos(),0.,0.,r.sin()])});}
        let reference=horizon_orientation(&track,2.);
        for t in [2.,2.45,2.9,3.35,3.8] {same(horizon_orientation(&track,t),reference);}
    }
    #[test]
    fn antipodes_and_frame_order_do_not_change_heading() {
        let track:Vec<_>=(0..=200).map(|i|Orientation{time_seconds:i as f64/100.,quaternion:pitch_yaw((i as f64).to_radians(),(i as f64/4.).to_radians())}).collect();
        let signed:Vec<_>=track.iter().enumerate().map(|(i,s)|Orientation{quaternion:s.quaternion.map(|v|if i%2==0 {-v}else{v}),..*s}).collect();
        for t in [2.,0.,1.799,0.9,1.8] {same(horizon_orientation(&track,t),horizon_orientation(&signed,t));}
        for i in 1..200 {let a=horizon_orientation(&track,i as f64/100.);let b=horizon_orientation(&track,(i+1) as f64/100.);assert!(qdot(a,b).abs()>0.999);}
    }
    #[test]
    fn upside_down_start_and_empty_tracks_are_defined() {
        let track=vec![Orientation{time_seconds:0.,quaternion:pitch_yaw(PI,0.)},Orientation{time_seconds:1.,quaternion:pitch_yaw(PI-0.2,0.)}];
        same(horizon_orientation(&track,0.),[0.,0.,1.,0.]);
        same(horizon_orientation(&track,0.5),[0.,0.,1.,0.]);
        same(horizon_orientation(&[],0.),[1.,0.,0.,0.]);
    }
}
