//! Camera centres from semantic masks on equirectangular panoramas.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Centre {
    pub pan: f64,
    pub tilt: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov: Option<f64>,
}
/// Circular longitude avoids averaging a seam-split subject into the opposite hemisphere.
pub fn mask_centre(width: usize, height: usize, mask: &[u8]) -> Result<Centre> {
    ensure!(
        width >= 2
            && height >= 2
            && width
                .checked_mul(height)
                .is_some_and(|n| n == mask.len() && n <= 256 * 256),
        "invalid semantic mask dimensions"
    );
    let (mut mass, mut cos, mut sin, mut vertical) = (0., 0., 0., 0.);
    for y in 0..height {
        for x in 0..width {
            let weight = f64::from(mask[y * width + x].saturating_sub(127));
            if weight == 0. {
                continue;
            }
            let angle = ((x as f64 + 0.5) / width as f64 - 0.5) * std::f64::consts::TAU;
            mass += weight;
            cos += angle.cos() * weight;
            sin += angle.sin() * weight;
            vertical += ((y as f64 + 0.5) / height as f64 - 0.5) * weight;
        }
    }
    ensure!(
        mass >= 128. * 4. && cos.hypot(sin) / mass >= 0.2,
        "semantic mask absent or ambiguous; no camera keys generated"
    );
    Ok(Centre {
        pan: sin.atan2(cos).to_degrees(),
        tilt: vertical / mass * 180.,
        fov: None,
    })
}

/// Opt-in perspective framing. FOV is horizontal, as in the native spherical renderer.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Framing {
    pub aspect_ratio: f64,
    pub target_fill: f64,
    pub initial_fov: f64,
}
impl Framing {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.aspect_ratio.is_finite()
                && (0.125..=8.).contains(&self.aspect_ratio)
                && self.target_fill.is_finite()
                && (0.2..=0.9).contains(&self.target_fill)
                && self.initial_fov.is_finite()
                && (10.0..=179.0).contains(&self.initial_fov),
            "invalid semantic auto-framing parameters"
        );
        Ok(())
    }
}

/// Fit all retained mask pixel corners in the camera tangent plane. The rotation
/// is identical to spherical_video (Ry * Rx); no longitude bounding box crosses
/// the seam, and near-pole longitude widths naturally contract on the sphere.
pub fn mask_fov(
    width: usize,
    height: usize,
    mask: &[u8],
    centre: Centre,
    framing: Framing,
) -> Result<f64> {
    framing.validate()?;
    ensure!(
        width >= 2
            && height >= 2
            && width
                .checked_mul(height)
                .is_some_and(|n| n == mask.len() && n <= 256 * 256)
            && centre.pan.is_finite()
            && (-180.0..=180.0).contains(&centre.pan)
            && centre.tilt.is_finite()
            && (-90.0..=90.0).contains(&centre.tilt),
        "invalid semantic framing mask/centre"
    );
    let rotation = super::vr_sphere::rotation(centre.tilt, centre.pan, 0.);
    let mut extent = 0.0_f64;
    let mut retained = 0;
    for y in 0..height {
        for x in 0..width {
            if mask[y * width + x] < 128 {
                continue;
            }
            retained += 1;
            for dy in [0, 1] {
                for dx in [0, 1] {
                    let d = super::vr_sphere::dir(
                        ((x + dx) as f64 / width as f64 - 0.5) * std::f64::consts::TAU,
                        (0.5 - (y + dy) as f64 / height as f64) * std::f64::consts::PI,
                    );
                    let local: [f64; 3] = std::array::from_fn(|axis| {
                        (0..3).map(|row| rotation[row][axis] * d[row]).sum()
                    });
                    ensure!(
                        local[2] > 0.000001,
                        "semantic target crosses the camera hemisphere; auto-framing unavailable"
                    );
                    // A circumscribed tangent-plane circle fits for every authored roll.
                    // target_fill is an upper bound, rather than exact subject occupancy.
                    extent = extent
                        .max(local[0].hypot(local[1]) / local[2] * framing.aspect_ratio.max(1.));
                }
            }
        }
    }
    ensure!(retained >= 4, "semantic target absent for auto-framing");
    let required = 2. * (extent / framing.target_fill).atan().to_degrees();
    ensure!(
        required <= 179.,
        "semantic target exceeds maximum perspective FOV"
    );
    Ok(required.max(10.))
}

/// Preserve the authored first FOV, then ease toward framing with a bounded zoom
/// speed. This avoids an instantaneous zoom at the first generated key.
pub fn frame_sequence(
    centres: &mut [Centre],
    desired: &[f64],
    times_us: &[u64],
    initial_fov: f64,
) -> Result<()> {
    ensure!(
        centres.len() >= 2
            && centres.len() <= 300
            && centres.len() == desired.len()
            && centres.len() == times_us.len()
            && times_us.windows(2).all(|t| t[0] < t[1])
            && initial_fov.is_finite()
            && (10.0..=179.0).contains(&initial_fov)
            && desired
                .iter()
                .all(|v| v.is_finite() && (10.0..=179.0).contains(v)),
        "invalid semantic zoom sequence"
    );
    let mut previous = initial_fov;
    centres[0].fov = Some(previous);
    for i in 1..centres.len() {
        let seconds = (times_us[i] - times_us[i - 1]) as f64 / 1_000_000.;
        let eased = previous + (desired[i] - previous) * (1. - (-seconds / 0.35).exp());
        previous =
            (previous + (eased - previous).clamp(-30. * seconds, 30. * seconds)).clamp(10., 179.);
        centres[i].fov = Some(previous);
    }
    Ok(())
}
