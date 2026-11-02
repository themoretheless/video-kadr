//! FilmCraft Color Match Oklab statistics; MIT notice docs/licenses/filmcraft-MIT.txt.
/// sRGB-primaries linear RGB → Oklab.
pub fn oklab(c: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = c.map(|v| v.max(0.0));
    let l = 0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Tonal-range weights (shadows, midtones, highlights) of an Oklab lightness.
pub fn tonal_weights(l: f32) -> [f32; 3] {
    let s = smoothstep(0.55, 0.15, l);
    let h = smoothstep(0.45, 0.85, l);
    [s, (1.0 - s - h).max(0.0), h]
}

/// How much a pixel looks like skin (0..1).
pub fn skin_weight(lab: [f32; 3]) -> f32 {
    let c = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    if !(0.02..=0.2).contains(&c) || !(0.25..=0.92).contains(&lab[0]) {
        return 0.0;
    }
    let h = lab[2].atan2(lab[1]).to_degrees();
    let d = if h < 20.0 {
        20.0 - h
    } else if h > 75.0 {
        h - 75.0
    } else {
        0.0
    };
    (1.0 - d / 10.0).clamp(0.0, 1.0)
}

/// Statistics of a frame: per tonal range (weight, mean L, mean a, mean b), mean chroma, and
/// the mean (a, b) of its skin pixels with their total weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub bands: [[f32; 4]; 3],
    pub chroma: f32,
    pub skin: [f32; 3],
}

pub fn stats(
    img: &super::linear_color_frame::LinearColorFrame,
    skin_protect: bool,
    checkpoint: &mut dyn FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Stats> {
    checkpoint()?;
    let mut acc = [[0f64; 4]; 3];
    let mut chroma = (0f64, 0f64);
    let mut skin = [0f64; 3];
    for (index, p) in img.pixels().iter().enumerate() {
        if index % 4096 == 0 {
            checkpoint()?;
        }
        if p[3] <= 1e-3 {
            continue;
        }
        let lab = oklab([p[0] / p[3], p[1] / p[3], p[2] / p[3]]);
        let sk = skin_weight(lab);
        let cw = if skin_protect { 1.0 - 0.8 * sk } else { 1.0 } as f64;
        let w = tonal_weights(lab[0]);
        for k in 0..3 {
            let wk = w[k] as f64;
            acc[k][0] += wk;
            acc[k][1] += wk * lab[0] as f64;
            // colour means use the skin-protected weight (stored relative to wk)
            acc[k][2] += wk * cw * lab[1] as f64;
            acc[k][3] += wk * cw * lab[2] as f64;
        }
        chroma.0 += cw * ((lab[1] * lab[1] + lab[2] * lab[2]).sqrt()) as f64;
        chroma.1 += cw;
        skin[0] += sk as f64 * lab[1] as f64;
        skin[1] += sk as f64 * lab[2] as f64;
        skin[2] += sk as f64;
    }
    // colour weights per band for normalisation
    let mut cwsum = [0f64; 3];
    for (index, p) in img.pixels().iter().enumerate() {
        if index % 4096 == 0 {
            checkpoint()?;
        }
        if p[3] <= 1e-3 {
            continue;
        }
        let lab = oklab([p[0] / p[3], p[1] / p[3], p[2] / p[3]]);
        let cw = if skin_protect {
            1.0 - 0.8 * skin_weight(lab)
        } else {
            1.0
        } as f64;
        let w = tonal_weights(lab[0]);
        for k in 0..3 {
            cwsum[k] += w[k] as f64 * cw;
        }
    }
    let n = img.pixels().len().max(1) as f64;
    let bands = std::array::from_fn(|k| {
        let w = acc[k][0].max(1e-9);
        let cw = cwsum[k].max(1e-9);
        [
            (acc[k][0] / n) as f32,
            (acc[k][1] / w) as f32,
            (acc[k][2] / cw) as f32,
            (acc[k][3] / cw) as f32,
        ]
    });
    let sw = skin[2].max(1e-9);
    Ok(Stats {
        bands,
        chroma: (chroma.0 / chroma.1.max(1e-9)) as f32,
        skin: [
            (skin[0] / sw) as f32,
            (skin[1] / sw) as f32,
            (skin[2] / n) as f32,
        ],
    })
}

/// Residuals between graded statistics and the reference's (bands weighted by how much of both
/// frames they cover).
fn residuals(cur: &Stats, refs: &Stats) -> Vec<f32> {
    let mut r = Vec::with_capacity(10);
    for k in 0..3 {
        let w = (cur.bands[k][0].min(refs.bands[k][0]) * 3.0)
            .clamp(0.0, 1.0)
            .sqrt();
        r.push((cur.bands[k][1] - refs.bands[k][1]) * w);
        r.push((cur.bands[k][2] - refs.bands[k][2]) * w * 2.0);
        r.push((cur.bands[k][3] - refs.bands[k][3]) * w * 2.0);
    }
    r.push((cur.chroma - refs.chroma) * 2.0);
    r
}

/// The solved grade.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Match {
    pub shadows: [f32; 2],
    pub midtones: [f32; 2],
    pub highlights: [f32; 2],
    /// Lightness sliders (−100..100).
    pub lightness: [f32; 3],
    /// Basic Correction saturation (0..200).
    pub saturation: f32,
    /// Remaining residual norm (statistics distance) before and after.
    pub before: f32,
    pub after: f32,
}

fn graded_stats(
    img: &super::linear_color_frame::LinearColorFrame,
    base: &super::lumetri::Lumetri,
    x: &[f32; 10],
    library: &[super::project_luts::ProjectLut],
    skin: bool,
    checkpoint: &mut dyn FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Stats> {
    let mut p = base.clone();
    p.animation = None;
    p.enabled = true;
    p.basic.enabled = true;
    p.wheels_on = true;
    for (wheel, i) in [
        (&mut p.grade.shadows, 0),
        (&mut p.grade.midtones, 2),
        (&mut p.grade.highlights, 4),
    ] {
        wheel.x = f64::from(x[i]);
        wheel.y = f64::from(x[i + 1]);
        wheel.lightness = (x[6 + i / 2] as f64 * 100.0) as f32;
    }
    p.basic.saturation = ((1.0 + x[9] as f64) * 100.0) as f32;
    let mut g = img.clone();
    p.apply_linear_probe_with_library(
        &mut g,
        img.dimensions().0 as f32 / 1920.0,
        library,
        checkpoint,
    )?;
    stats(&g, skin, checkpoint)
}
pub fn solve(
    current: &super::linear_color_frame::LinearColorFrame,
    reference: &super::linear_color_frame::LinearColorFrame,
    base: &super::lumetri::Lumetri,
    skin_protect: bool,
    checkpoint: &mut dyn FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Match> {
    solve_with_library(current, reference, base, skin_protect, &[], checkpoint)
}
pub fn solve_with_library(
    current: &super::linear_color_frame::LinearColorFrame,
    reference: &super::linear_color_frame::LinearColorFrame,
    base: &super::lumetri::Lumetri,
    skin_protect: bool,
    library: &[super::project_luts::ProjectLut],
    checkpoint: &mut dyn FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Match> {
    super::project_luts::validate(library)?;
    base.validate_library(library)?;
    let cur = current.shrink_for_match(checkpoint)?;
    let refs = stats(
        &reference.shrink_for_match(checkpoint)?,
        skin_protect,
        checkpoint,
    )?;
    let mut x = [0f32; 10];
    let lambda_reg = 0.02f32;
    let cost = |x: &[f32; 10],
                checkpoint: &mut dyn FnMut() -> anyhow::Result<()>|
     -> anyhow::Result<(Vec<f32>, f32)> {
        let r = residuals(
            &graded_stats(&cur, base, x, library, skin_protect, checkpoint)?,
            &refs,
        );
        let c = r.iter().map(|v| v * v).sum::<f32>()
            + lambda_reg * lambda_reg * x.iter().map(|v| v * v).sum::<f32>();
        Ok((r, c))
    };
    let (mut r, mut c) = cost(&x, checkpoint)?;
    let before = c.sqrt();
    let mut mu = 1e-3f32;
    for _ in 0..12 {
        // numerical Jacobian (forward differences)
        let h = 1e-2f32;
        let mut jac = vec![[0f32; 10]; r.len()];
        for j in 0..10 {
            let mut xp = x;
            xp[j] += h;
            let (rp, _) = cost(&xp, checkpoint)?;
            for (i, row) in jac.iter_mut().enumerate() {
                row[j] = (rp[i] - r[i]) / h;
            }
        }
        // normal equations with Tikhonov regularisation and LM damping
        let mut a = [[0f64; 10]; 10];
        let mut g = [0f64; 10];
        for (i, row) in jac.iter().enumerate() {
            for p in 0..10 {
                g[p] += (row[p] * r[i]) as f64;
                for q in 0..10 {
                    a[p][q] += (row[p] * row[q]) as f64;
                }
            }
        }
        for p in 0..10 {
            g[p] += (lambda_reg * lambda_reg * x[p]) as f64;
            a[p][p] += (lambda_reg * lambda_reg) as f64 + mu as f64 * (1.0 + a[p][p]);
        }
        let Some(dx) = solve_linear(a, g.map(|v| -v)) else {
            break;
        };
        let mut xn = x;
        for p in 0..10 {
            xn[p] = (x[p] + dx[p] as f32).clamp(-1.0, 1.0);
        }
        let (rn, cn) = cost(&xn, checkpoint)?;
        if cn < c {
            x = xn;
            r = rn;
            let improved = c - cn;
            c = cn;
            mu = (mu * 0.3).max(1e-6);
            if improved < 1e-7 {
                break;
            }
        } else {
            mu *= 10.0;
            if mu > 1e4 {
                break;
            }
        }
    }
    if skin_protect {
        // limit the hue shift of the current frame's skin tones to 8°
        let s0 = stats(&cur, true, checkpoint)?;
        if s0.skin[2] > 0.002 {
            let hue = |s: &Stats| s.skin[1].atan2(s.skin[0]).to_degrees();
            let h0 = hue(&s0);
            let mut k = 1.0f32;
            for _ in 0..8 {
                let xs: [f32; 10] = std::array::from_fn(|i| if i < 6 { x[i] * k } else { x[i] });
                let s1 = graded_stats(&cur, base, &xs, library, true, checkpoint)?;
                let mut d = (hue(&s1) - h0).abs();
                if d > 180.0 {
                    d = 360.0 - d;
                }
                if d <= 8.0 {
                    break;
                }
                k *= 0.7;
            }
            for v in x.iter_mut().take(6) {
                *v *= k;
            }
            c = cost(&x, checkpoint)?.1;
        }
    }
    Ok(Match {
        shadows: [x[0], x[1]],
        midtones: [x[2], x[3]],
        highlights: [x[4], x[5]],
        lightness: [x[6] * 100.0, x[7] * 100.0, x[8] * 100.0],
        saturation: ((1.0 + x[9]) * 100.0).clamp(0.0, 200.0),
        before,
        after: c.sqrt(),
    })
}

/// Gaussian elimination with partial pivoting.
fn solve_linear(mut a: [[f64; 10]; 10], mut b: [f64; 10]) -> Option<[f64; 10]> {
    for col in 0..10 {
        let piv = (col..10).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[piv][col].abs() < 1e-14 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        for row in col + 1..10 {
            let f = a[row][col] / a[col][col];
            for k in col..10 {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0f64; 10];
    for i in (0..10).rev() {
        let s: f64 = (i + 1..10).map(|k| a[i][k] * x[k]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_independent_filmcraft_solver() {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-color-match.json"
        ))
        .unwrap();
        assert_eq!(f["commit"], "5231852443363f001c3f6b396dd9b1e6461ae2be");
        assert_eq!(f["cases"].as_array().unwrap().len(), 6);
        let frame = |bytes: serde_json::Value| {
            let image = super::super::group_surface::GroupSurface {
                width: 7,
                height: 5,
                rgba: serde_json::from_value(bytes).unwrap(),
            };
            super::super::linear_color_frame::LinearColorFrame::from_surface(&image, &mut || Ok(()))
                .unwrap()
        };
        let current = frame(f["input"].clone());
        for case in f["cases"].as_array().unwrap() {
            let expected: Match = serde_json::from_value(case["expected"].clone()).unwrap();
            let actual = solve(
                &current,
                &frame(case["reference"].clone()),
                &super::super::lumetri::Lumetri::default(),
                case["skin"].as_bool().unwrap(),
                &mut || Ok(()),
            )
            .unwrap();
            for (a, b) in actual
                .shadows
                .iter()
                .chain(&actual.midtones)
                .chain(&actual.highlights)
                .zip(
                    expected
                        .shadows
                        .iter()
                        .chain(&expected.midtones)
                        .chain(&expected.highlights),
                )
            {
                assert!((a - b).abs() < 0.001, "wheel {a} != {b}");
            }
            for (a, b) in actual
                .lightness
                .iter()
                .chain(std::iter::once(&actual.saturation))
                .zip(
                    expected
                        .lightness
                        .iter()
                        .chain(std::iter::once(&expected.saturation)),
                )
            {
                assert!((a - b).abs() < 0.02, "slider {a} != {b}");
            }
            assert!((actual.before - expected.before).abs() < 1e-4);
            assert!((actual.after - expected.after).abs() < 1e-4);
        }
    }
    #[test]
    fn derivative_probes_cross_boundaries_but_authored_values_stay_strict() {
        let image = super::super::group_surface::GroupSurface {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 255, 255],
        };
        let frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &image,
            &mut || Ok(()),
        )
        .unwrap();
        let mut base = super::super::lumetri::Lumetri::default();
        let x = [1.01, 0.0, 0.0, 0.0, 0.0, 0.0, 1.01, 0.0, 0.0, 1.01];
        assert!(graded_stats(&frame, &base, &x, &[], false, &mut || Ok(())).is_ok());
        base.grade.shadows.x = 1.01;
        assert!(base.validate().is_err());
        assert!(solve(&frame, &frame, &base, false, &mut || Ok(())).is_err());
    }
    #[test]
    fn identical_frames_need_no_match_and_cancel_is_propagated() {
        let image = super::super::group_surface::GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![80, 120, 160, 255, 140, 100, 70, 128],
        };
        let frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &image,
            &mut || Ok(()),
        )
        .unwrap();
        let base = super::super::lumetri::Lumetri::default();
        let m = solve(&frame, &frame, &base, true, &mut || Ok(())).unwrap();
        assert!(m.before < 1e-5 && m.after < 1e-5);
        assert!((m.saturation - 100.0).abs() < 1e-3);
        assert!(m.shadows.iter().all(|v| v.abs() < 1e-5));
        assert!(solve(&frame, &frame, &base, false, &mut || anyhow::bail!(
            "cancelled"
        ))
        .is_err());
    }
    #[test]
    fn opaque_white_and_hidden_rgb_statistics() {
        let image = super::super::group_surface::GroupSurface {
            width: 2,
            height: 1,
            rgba: vec![255, 255, 255, 255, 255, 0, 0, 0],
        };
        let frame = super::super::linear_color_frame::LinearColorFrame::from_surface(
            &image,
            &mut || Ok(()),
        )
        .unwrap();
        let s = stats(&frame, false, &mut || Ok(())).unwrap();
        assert!((s.bands[2][0] - 0.5).abs() < 1e-6);
        assert!((s.bands[2][1] - 1.0).abs() < 1e-5);
        assert!(s.chroma < 1e-6);
        assert!(stats(&frame, true, &mut || anyhow::bail!("cancelled")).is_err());
    }
}
