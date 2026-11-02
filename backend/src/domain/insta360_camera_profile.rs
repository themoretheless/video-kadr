//! Source-owned Insta360 calibration. No model-name defaults or equidistant approximation.
//! Field/token interpretation is qualified against Studio 6.0.2 OffsetParser and
//! native polynomial/RadtanPro projections; pose/packing must be resolved by the caller.
use super::insta360_metadata::Metadata;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraProfile {
    pub model: String,
    pub reference_size: [u32; 2],
    pub lenses: [OmniLens; 2],
    pub flags: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OmniLens {
    /// Legacy native angular radius polynomial (radians). Absent for v6 RadtanPro.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angular_polynomial: Option<[f64; 4]>,
    pub xi: f64,
    pub focal: [f64; 2],
    /// Principal point in the full, side-by-side reference raster.
    pub principal: [f64; 2],
    pub euler_degrees: [f64; 3],
    pub translation: [f64; 3],
    pub distortion: [f64; 13],
    pub lens_type: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraProfileReport {
    pub status: &'static str,
    pub render_ready: bool,
    pub reason: String,
    pub calibration_field: Option<u32>,
    pub profile: Option<CameraProfile>,
}
pub fn inspect(metadata: &Metadata) -> CameraProfileReport {
    // Prefer the current calibration over the factory original. Never downgrade a
    // present but unrecognized newer calibration to a different, older model.
    for field in [197, 111, 112, 5, 17] {
        if let Some(c) = metadata
            .calibrations
            .iter()
            .find(|c| c.field_number == field)
        {
            let result = if matches!(field, 111 | 112) {
                parse_v6(&c.value)
            } else if matches!(field, 5 | 17) {
                parse_legacy(&c.value).and_then(|mut p| {
                    let crop = metadata.window_crop_info.as_ref().context(
                        "legacy calibration requires qualified source window crop metadata",
                    )?;
                    p.apply_window_crop(crop)?;
                    Ok(p)
                })
            } else {
                Err(anyhow::anyhow!(
                    "calibration field {field} is not a qualified v6 model"
                ))
            };
            return match result {
                Ok(profile) => CameraProfileReport {
                    status: "available",
                    render_ready: true,
                    reason: "Embedded source-owned calibration; pair both video lens tracks before stitching".into(),
                    calibration_field: Some(field),
                    profile: Some(profile),
                },
                Err(e) => CameraProfileReport {
                    status: "unsupported_calibration",
                    render_ready: false,
                    reason: e.to_string(),
                    calibration_field: Some(field),
                    profile: None,
                },
            };
        }
    }
    CameraProfileReport {
        status: "missing_calibration",
        render_ready: false,
        reason:
            "No supported embedded calibration; camera model alone does not define lens parameters"
                .into(),
        calibration_field: None,
        profile: None,
    }
}
pub fn parse_v6(text: &str) -> Result<CameraProfile> {
    ensure!(text.len() <= 16_384, "camera calibration exceeds budget");
    let tokens: Vec<_> = text.split('_').collect();
    ensure!(
        tokens.len() == 56 && tokens[0] == "2",
        "unsupported v6 camera calibration shape"
    );
    let flags: u32 = tokens[55].parse().context("invalid v6 flags")?;
    ensure!(flags >> 16 == 6, "calibration is not version 6");
    let mut reference = None;
    let mut parsed = Vec::with_capacity(2);
    for i in 0..2 {
        let s = &tokens[1 + i * 27..1 + (i + 1) * 27];
        let mut values = [0.; 24];
        for (v, s) in values.iter_mut().zip(&s[..24]) {
            *v = s.parse::<f64>().context("invalid calibration number")?;
            ensure!(v.is_finite(), "nonfinite calibration number");
        }
        let size = [s[24].parse::<u32>()?, s[25].parse::<u32>()?];
        ensure!(
            reference.is_none_or(|r| r == size),
            "lens reference sizes disagree"
        );
        reference = Some(size);
        parsed.push(OmniLens {
            angular_polynomial: None,
            xi: values[0],
            focal: [values[1], values[2]],
            principal: [values[3], values[4]],
            euler_degrees: values[5..8].try_into().unwrap(),
            translation: values[8..11].try_into().unwrap(),
            distortion: values[11..24].try_into().unwrap(),
            lens_type: s[26].parse()?,
        });
    }
    let profile = CameraProfile {
        model: "insta360_v6_omni".into(),
        reference_size: reference.unwrap(),
        lenses: parsed.try_into().unwrap(),
        flags,
    };
    profile.validate()?;
    Ok(profile)
}
/// Qualified legacy branch: source-owned two-lens calibration, native lens71.
/// The five native angle-in-degrees coefficients are decoded from the installed
/// Studio's lens71 branch; they are not fitted from a camera model name.
pub fn parse_legacy(text: &str) -> Result<CameraProfile> {
    ensure!(text.len() <= 16_384, "camera calibration exceeds budget");
    let tokens: Vec<_> = text.split('_').collect();
    ensure!(
        tokens.len() == 16 && tokens[0] == "2",
        "unsupported legacy calibration shape"
    );
    let flags: u32 = tokens[15].parse().context("invalid legacy flags")?;
    ensure!(flags == 1095, "unqualified legacy lens/gyro flags");
    let reference_size = [tokens[13].parse::<u32>()?, tokens[14].parse::<u32>()?];
    let degrees = 180. / std::f64::consts::PI;
    // Native lens71 polynomial: [0, .02249, -.0000579, .000001932, -.00000001295].
    let angular = [
        1.,
        -5.79e-5 * degrees / 0.02249,
        1.932e-6 * degrees.powi(2) / 0.02249,
        -1.295e-8 * degrees.powi(3) / 0.02249,
    ];
    let theta = 100f64.to_radians();
    let edge =
        theta * (angular[0] + theta * (angular[1] + theta * (angular[2] + theta * angular[3])));
    let mut lenses = Vec::with_capacity(2);
    for i in 0..2 {
        let mut values = [0.; 6];
        for (value, token) in values.iter_mut().zip(&tokens[1 + 6 * i..7 + 6 * i]) {
            *value = token.parse::<f64>().context("invalid legacy number")?;
            ensure!(value.is_finite(), "nonfinite legacy number");
        }
        ensure!(values[0] > 0., "invalid legacy lens radius");
        let focal = values[0] / edge;
        lenses.push(OmniLens {
            angular_polynomial: Some(angular),
            xi: 0.,
            focal: [focal; 2],
            principal: [values[1], values[2]],
            euler_degrees: values[3..6].try_into().unwrap(),
            translation: [0.; 3],
            distortion: [0.; 13],
            lens_type: 71,
        });
    }
    let profile = CameraProfile {
        model: "insta360_legacy_polynomial".into(),
        reference_size,
        lenses: lenses.try_into().unwrap(),
        flags,
    };
    profile.validate()?;
    Ok(profile)
}
impl CameraProfile {
    /// Native OffsetConvert::convertOffset: resize calibration to sensor window,
    /// then center crop and translate its principal point into the active raster.
    pub fn apply_window_crop(
        &mut self,
        crop: &super::insta360_metadata::WindowCropInfo,
    ) -> Result<()> {
        ensure!(
            self.model == "insta360_legacy_polynomial",
            "window conversion is only qualified for legacy model"
        );
        let [sw, sh] = crop.source_size;
        let [dw, dh] = crop.destination_size;
        ensure!(
            sw > 0
                && sh > 0
                && dw > 0
                && dh > 0
                && dw <= sw
                && dh <= sh
                && sw <= 65536
                && sh <= 65536,
            "unsupported window crop dimensions"
        );
        ensure!(
            crop.offset[0].unsigned_abs() as u64 * 2 <= (sw - dw) as u64
                && crop.offset[1].unsigned_abs() as u64 * 2 <= (sh - dh) as u64,
            "window crop outside source"
        );
        self.validate()?;
        let old_half = self.reference_size[0] as f64 / 2.;
        let scale = [
            sw as f64 / old_half,
            sh as f64 / self.reference_size[1] as f64,
        ];
        for (i, lens) in self.lenses.iter_mut().enumerate() {
            let local = [lens.principal[0] - i as f64 * old_half, lens.principal[1]];
            lens.focal = std::array::from_fn(|axis| lens.focal[axis] * scale[axis]);
            lens.principal = [
                local[0] * scale[0] - (sw - dw) as f64 * 0.5
                    + crop.offset[0] as f64
                    + i as f64 * dw as f64,
                local[1] * scale[1] - (sh - dh) as f64 * 0.5 + crop.offset[1] as f64,
            ];
        }
        self.reference_size = [dw.checked_mul(2).context("window crop overflow")?, dh];
        self.validate()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (self.model == "insta360_v6_omni" && self.flags >> 16 == 6)
                || (self.model == "insta360_legacy_polynomial" && self.flags == 1095),
            "unsupported camera profile model"
        );
        let [w, h] = self.reference_size;
        ensure!(
            (32..=65536).contains(&w)
                && (16..=32768).contains(&h)
                && w % 2 == 0
                && (self.model != "insta360_v6_omni" || w == 2 * h),
            "unsupported camera profile reference layout"
        );
        for l in &self.lenses {
            ensure!(
                l.xi.is_finite()
                    && (0. ..=8.).contains(&l.xi)
                    && l.focal
                        .iter()
                        .all(|v| v.is_finite() && *v > 0. && *v <= w as f64 * 8.)
                    && l.principal.iter().all(|v| v.is_finite())
                    && (0. ..=w as f64).contains(&l.principal[0])
                    && (0. ..=h as f64).contains(&l.principal[1])
                    && l.euler_degrees
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 360.)
                    && l.translation
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 100.)
                    && l.distortion
                        .iter()
                        .all(|v| v.is_finite() && v.abs() <= 1000.)
                    && if self.model == "insta360_v6_omni" {
                        l.lens_type == 193 && l.angular_polynomial.is_none()
                    } else {
                        l.lens_type == 71
                            && l.xi == 0.
                            && l.distortion == [0.; 13]
                            && l.translation == [0.; 3]
                            && l.angular_polynomial.is_some_and(|p| {
                                p[0] == 1. && p.iter().all(|v| v.is_finite() && v.abs() < 100.)
                            })
                    },
                "unsupported or invalid omnidirectional lens"
            );
        }
        Ok(())
    }
    /// Native static-dewarp convention transformed into the editor's right/up/forward frame.
    pub fn optical_rotations(&self) -> [[[f64; 3]; 3]; 2] {
        std::array::from_fn(|i| {
            let l = &self.lenses[i];
            let [a, b, c] = l.euler_degrees.map(f64::to_radians);
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = (b + std::f64::consts::FRAC_PI_2).sin_cos();
            let (sc, cc) = c.sin_cos();
            // Ry(b+90) * Rz(a) * Rx(c).
            let ry = [[cb, 0., sb], [0., 1., 0.], [-sb, 0., cb]];
            let rz = [[ca, -sa, 0.], [sa, ca, 0.], [0., 0., 1.]];
            let rx = [[1., 0., 0.], [0., cc, -sc], [0., sc, cc]];
            let mut rotation = matmul(matmul(ry, rz), rx);
            if i == 0 {
                rotation = matmul(rotation, [[-1., 0., 0.], [0., -1., 0.], [0., 0., 1.]]);
            }
            // Native spherical axes: [editor Z, -editor X, editor Y].
            matmul(rotation, [[0., 0., 1.], [-1., 0., 0.], [0., 1., 0.]])
        })
    }
}
fn matmul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}
impl OmniLens {
    /// Project a ray already expressed in this lens's optical coordinates, +Z
    /// forward, +X right, +Y down. Returns full reference raster coordinates.
    /// Explicit native angular or13-term radial/tangential model; no approximation fallback.
    pub fn project_optical(&self, ray: [f64; 3]) -> Option<[f64; 2]> {
        if !ray.iter().all(|v| v.is_finite()) {
            return None;
        }
        let n = ray.iter().map(|v| v * v).sum::<f64>().sqrt();
        if let Some(a) = self.angular_polynomial {
            if n <= 0. {
                return None;
            }
            let radial = ray[0].hypot(ray[1]);
            let theta = radial.atan2(ray[2]);
            // Native static-dewarp expands the lens71 200-degree FOV by1.05.
            if theta >= 105f64.to_radians() {
                return None;
            }
            let radius = theta * (a[0] + theta * (a[1] + theta * (a[2] + theta * a[3])));
            let xy = if radial > 0. {
                [ray[0] / radial, ray[1] / radial]
            } else {
                [0., 0.]
            };
            let uv = std::array::from_fn(|i| self.principal[i] + self.focal[i] * radius * xy[i]);
            return uv.iter().all(|v| v.is_finite()).then_some(uv);
        }
        // Qualified lens type 193 resolves to 200 degrees. Native static dewarp
        // expands it by 1.05 before testing the cosine with a 0.01 tolerance.
        if n <= 0. || ray[2] / n < (105f64.to_radians().cos() - 0.01) {
            return None;
        }
        let den = ray[2] + self.xi * n;
        if den <= 1e-12 {
            return None;
        }
        let x = ray[0] / den;
        let y = ray[1] / den;
        let r = x * x + y * y;
        let r4 = r * r;
        let k = &self.distortion;
        let radial = 1. + r * (k[0] + r * (k[1] + r * (k[2] + r * (k[3] + r * k[4]))));
        let p = k[5] + r * k[7];
        let q = k[6] + r * k[8];
        let xd = x * radial + (r + 2. * x * x) * p + 2. * x * y * q + k[9] * r + k[11] * r4;
        let yd = y * radial + (r + 2. * y * y) * q + 2. * x * y * p + k[10] * r + k[12] * r4;
        let uv = [
            self.principal[0] + self.focal[0] * xd,
            self.principal[1] + self.focal[1] * yd,
        ];
        uv.iter().all(|v| v.is_finite()).then_some(uv)
    }
}
