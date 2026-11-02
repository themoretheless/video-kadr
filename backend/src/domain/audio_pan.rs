//! Shared equal-power matrix for native PCM and FFmpeg audio compilers.
/// WeftCut eval::pan_coeffs: mono fan-out or stereo cross-feed, equal-power law.
pub fn equal_power_pan(pan: f64, channels: u32) -> [f64; 4] {
    let p = pan.clamp(-1.0, 1.0);
    let coefficients = |position: f64| {
        let angle = position * std::f64::consts::FRAC_PI_2;
        let snap = |value: f64| if value.abs() < 1e-7 { 0.0 } else { value };
        (snap(angle.cos()), snap(angle.sin()))
    };
    if channels == 1 {
        let (c, s) = coefficients((p + 1.0) / 2.0);
        [c, 0.0, s, 0.0]
    } else if p <= 0.0 {
        let (c, s) = coefficients(p + 1.0);
        [1.0, c, 0.0, s]
    } else {
        let (c, s) = coefficients(p);
        [c, 0.0, s, 1.0]
    }
}
