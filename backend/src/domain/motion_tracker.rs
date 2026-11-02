//! Native tracking kernels with the renderer's existing point/affine API.
//! MIT attribution: docs/licenses/filmcraft-MIT.txt.
use super::warp_stabilizer::{Affine, Vec2};
pub use kernels::{Gray, Prepared, TrackMethod, MAX_FEATURES};
use video_kadr_motion_kernels as kernels;
fn points(input: &[Vec2]) -> Vec<kernels::geometry::Vec2> {
    input
        .iter()
        .map(|p| kernels::geometry::Vec2::new(p.x, p.y))
        .collect()
}
fn affine(m: kernels::geometry::Affine) -> Affine {
    Affine {
        a: m.a,
        b: m.b,
        c: m.c,
        d: m.d,
        e: m.e,
        f: m.f,
    }
}
pub fn features(g: &Gray, region: &[Vec2], max: usize, min_dist: f32) -> Vec<[f32; 2]> {
    kernels::features(g, &points(region), max, min_dist)
}
pub fn fit_ls(method: TrackMethod, src: &[Vec2], dst: &[Vec2]) -> Option<Affine> {
    kernels::fit_ls(method, &points(src), &points(dst)).map(affine)
}
pub fn fit_robust(method: TrackMethod, src: &[Vec2], dst: &[Vec2]) -> Option<(Affine, usize)> {
    kernels::fit_robust(method, &points(src), &points(dst)).map(|(m, n)| (affine(m), n))
}
#[derive(Clone, Debug)]
pub struct Step {
    pub transform: Affine,
    pub features: usize,
    pub tracked: usize,
    pub inliers: usize,
}
pub fn track_step(
    a: &Prepared,
    b: &Prepared,
    region: &[Vec2],
    method: TrackMethod,
) -> Option<Step> {
    kernels::track_step(a, b, &points(region), method).map(|step| Step {
        transform: affine(step.transform),
        features: step.features,
        tracked: step.tracked,
        inliers: step.inliers,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use rayon::prelude::*;

    /// A deterministic texture: smooth blobs + fine detail.
    fn texture(u: f64, v: f64) -> f32 {
        let s = (u * 0.21).sin() * (v * 0.17).cos()
            + 0.5 * ((u + v) * 0.43).sin()
            + 0.35 * ((u * 0.9 - v * 0.6).sin() * (v * 0.75).cos());
        let h = {
            let (iu, iv) = ((u / 6.0).floor() as i64, (v / 6.0).floor() as i64);
            let mut x = (iu.wrapping_mul(73_856_093) ^ iv.wrapping_mul(19_349_663)) as u64;
            x ^= x >> 13;
            x = x.wrapping_mul(0x5bd1_e995);
            (x >> 40) as f64 / (1u64 << 24) as f64
        };
        (0.5 + 0.22 * s + 0.12 * h).clamp(0.0, 1.0) as f32
    }

    /// Frame with a textured disc (radius 70, object coords centred) under `pose`, over a static
    /// differently-textured background.
    fn frame(w: usize, h: usize, pose: &Affine) -> Gray {
        let inv = pose.inverse().unwrap();
        let px = (0..w * h)
            .into_par_iter()
            .map(|i| {
                // 2×2 supersampling
                let mut acc = 0.0;
                for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let p = Vec2::new((i % w) as f64 + ox - 0.5, (i / w) as f64 + oy - 0.5);
                    let o = inv.apply(p);
                    acc += if o.length() < 70.0 {
                        texture(o.x + 200.0, o.y + 300.0)
                    } else {
                        0.25 + 0.1 * texture(p.x * 0.5 + 900.0, p.y * 0.5)
                    };
                }
                acc / 4.0
            })
            .collect();
        Gray::new(w, h, px)
    }

    fn pose(t: f64, rot_per: f64, scale_per: f64) -> Affine {
        Affine::motion(
            Vec2::new(150.0 + 3.2 * t, 120.0 - 1.7 * t),
            Vec2::new(1.0 + scale_per * t, 1.0 + scale_per * t),
            rot_per * t,
            Vec2::ZERO,
        )
    }

    fn circle(c: Vec2, r: f64) -> Vec<Vec2> {
        (0..48)
            .map(|i| {
                c + Vec2::new(
                    (i as f64 / 48.0 * std::f64::consts::TAU).cos() * r,
                    (i as f64 / 48.0 * std::f64::consts::TAU).sin() * r,
                )
            })
            .collect()
    }

    fn run(method: TrackMethod, rot: f64, scale: f64, frames: usize) -> f64 {
        let (w, h) = (320, 240);
        let mut region = circle(Vec2::new(150.0, 120.0), 50.0);
        let mut prev = Prepared::new(frame(w, h, &pose(0.0, rot, scale)));
        let mut worst = 0.0f64;
        let probe = [
            Vec2::new(30.0, 0.0),
            Vec2::new(-20.0, 25.0),
            Vec2::new(0.0, -40.0),
        ];
        let mut tracked: Vec<Vec2> = probe
            .iter()
            .map(|o| pose(0.0, rot, scale).apply(*o))
            .collect();
        for k in 1..=frames {
            let next = Prepared::new(frame(w, h, &pose(k as f64, rot, scale)));
            let st = track_step(&prev, &next, &region, method).expect("tracked");
            region = region.iter().map(|p| st.transform.apply(*p)).collect();
            tracked = tracked.iter().map(|p| st.transform.apply(*p)).collect();
            for (o, t) in probe.iter().zip(&tracked) {
                let truth = pose(k as f64, rot, scale).apply(*o);
                worst = worst.max((truth - *t).length());
            }
            prev = next;
        }
        worst
    }

    #[test]
    fn tracks_translation() {
        let e = run(TrackMethod::Position, 0.0, 0.0, 10);
        assert!(e < 0.5, "max drift {e} px");
    }

    #[test]
    fn tracks_rotation() {
        let e = run(TrackMethod::PositionRotation, 2.0, 0.0, 10);
        assert!(e < 0.75, "max drift {e} px");
    }

    #[test]
    fn tracks_similarity() {
        let e = run(TrackMethod::PositionScaleRotation, -1.5, 0.012, 10);
        assert!(e < 0.75, "max drift {e} px");
    }

    #[test]
    fn ransac_rejects_outliers() {
        let truth = Affine::motion(Vec2::new(5.0, -3.0), Vec2::new(1.1, 1.1), 7.0, Vec2::ZERO);
        let mut src = Vec::new();
        let mut dst = Vec::new();
        for i in 0..40 {
            let p = Vec2::new((i * 37 % 100) as f64, (i * 61 % 90) as f64);
            src.push(p);
            dst.push(if i % 4 == 0 {
                p + Vec2::new(20.0, -13.0)
            } else {
                truth.apply(p)
            });
        }
        let (m, inl) = fit_robust(TrackMethod::PositionScaleRotation, &src, &dst).unwrap();
        assert_eq!(inl, 30);
        for p in &src {
            assert!((m.apply(*p) - truth.apply(*p)).length() < 1e-6);
        }
    }
    #[test]
    fn cached_sampling_pyramids_and_features_match_original_bits() {
        use sha2::{Digest, Sha256};
        // Golden digests from the pre-cache implementation (SHA256
        // 97a34d0718e8ad93f870fdcc79f5ee7754cee763752014305be68eab3121cfce).
        let mut samples_features = Sha256::new();
        let mut pyramids = Sha256::new();
        for (w, h) in [(1, 1), (2, 2), (31, 25), (63, 48), (240, 180)] {
            for seed in [1u32, 17, 91] {
                let pixels = (0..w * h)
                    .map(|i| {
                        let n = (i as u32)
                            .wrapping_add(seed)
                            .wrapping_mul(1664525)
                            .wrapping_add(1013904223);
                        (n >> 8) as f32 / 16777216.
                    })
                    .collect();
                let image = Gray::new(w, h, pixels);
                let prepared = Prepared::new(image.clone());
                for level in prepared.levels() {
                    pyramids.update((level.w as u64).to_le_bytes());
                    pyramids.update((level.h as u64).to_le_bytes());
                    for sample in &level.px {
                        pyramids.update(sample.to_bits().to_le_bytes());
                    }
                }
                for x in [
                    -5.25,
                    -0.25,
                    -0.,
                    0.,
                    0.125,
                    w as f32 - 1.25,
                    w as f32 - 0.125,
                    w as f32 + 3.5,
                ] {
                    for y in [
                        -4.75,
                        -0.25,
                        -0.,
                        0.,
                        0.375,
                        h as f32 - 1.75,
                        h as f32 - 0.125,
                        h as f32 + 2.25,
                    ] {
                        samples_features.update(image.sample(x, y).to_bits().to_le_bytes());
                    }
                }
                for region in [
                    vec![
                        Vec2::new(0., 0.),
                        Vec2::new(w as f64, 0.),
                        Vec2::new(w as f64, h as f64),
                        Vec2::new(0., h as f64),
                    ],
                    vec![
                        Vec2::new(-4., -3.),
                        Vec2::new(w as f64 + 3., 2.),
                        Vec2::new(w as f64 - 1., h as f64 + 4.),
                        Vec2::new(3., h as f64 - 2.),
                    ],
                ] {
                    let points = features(&image, &region, 160, 5.);
                    samples_features.update((points.len() as u64).to_le_bytes());
                    for point in points {
                        for value in point {
                            samples_features.update(value.to_bits().to_le_bytes());
                        }
                    }
                }
            }
        }
        assert_eq!(
            format!("{:x}", samples_features.finalize()),
            "5aec0b4e12db110b6ff6239f66f2685e6e481e8115c62d054c214a768e6ac9c0"
        );
        assert_eq!(
            format!("{:x}", pyramids.finalize()),
            "910b3cbd07180f32d1b7949093f335fb03bb4061d72f1e9200b9479413025e91"
        );
    }
}
