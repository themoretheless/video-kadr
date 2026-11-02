//! Bounded calibrated two-view geometry. Translation is eliminated by the
//! essential constraint; only uniquely triangulated, distributed scenes qualify.
use super::*;
fn eigen<const N: usize>(mut a: [[f64; N]; N]) -> Option<([f64; N], [[f64; N]; N])> {
    let mut v = [[0.; N]; N];
    for i in 0..N {
        v[i][i] = 1.;
    }
    for _ in 0..(N * N * 80) {
        let (mut p, mut q, mut maximum) = (0, 1, 0.);
        for i in 0..N {
            for j in i + 1..N {
                if a[i][j].abs() > maximum {
                    maximum = a[i][j].abs();
                    p = i;
                    q = j;
                }
            }
        }
        if maximum < 1e-13 {
            let d = std::array::from_fn(|i| a[i][i]);
            return d.iter().all(|x| x.is_finite()).then_some((d, v));
        }
        let angle = 0.5 * (2. * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (c, s) = (angle.cos(), angle.sin());
        let (app, aqq, apq) = (a[p][p], a[q][q], a[p][q]);
        a[p][p] = c * c * app - 2. * s * c * apq + s * s * aqq;
        a[q][q] = s * s * app + 2. * s * c * apq + c * c * aqq;
        a[p][q] = 0.;
        a[q][p] = 0.;
        for k in 0..N {
            if k != p && k != q {
                let (x, y) = (a[k][p], a[k][q]);
                a[k][p] = c * x - s * y;
                a[p][k] = a[k][p];
                a[k][q] = s * x + c * y;
                a[q][k] = a[k][q];
            }
            let (x, y) = (v[k][p], v[k][q]);
            v[k][p] = c * x - s * y;
            v[k][q] = s * x + c * y;
        }
    }
    None
}
type M = [[f64; 3]; 3];
fn mv(m: M, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| dot3(m[i], v))
}
fn transpose(m: M) -> M {
    std::array::from_fn(|i| std::array::from_fn(|j| m[j][i]))
}
fn mm(a: M, b: M) -> M {
    let t = transpose(b);
    std::array::from_fn(|i| std::array::from_fn(|j| dot3(a[i], t[j])))
}
fn decompose(e: M) -> Option<(M, M)> {
    let (values, v) = eigen(mm(transpose(e), e))?;
    let mut order = [0, 1, 2];
    order.sort_by(|a, b| values[*b].total_cmp(&values[*a]));
    if values[order[1]] < 1e-12 {
        return None;
    }
    let v0 = std::array::from_fn(|i| v[i][order[0]]);
    let v1 = std::array::from_fn(|i| v[i][order[1]]);
    let v2 = cross3(v0, v1);
    let u0 = unit3(mv(e, v0))?;
    let raw = mv(e, v1);
    let u1 = unit3(std::array::from_fn(|i| raw[i] - dot3(raw, u0) * u0[i]))?;
    let u2 = cross3(u0, u1);
    Some((transpose([u0, u1, u2]), transpose([v0, v1, v2])))
}
fn fit(
    points: &[([f64; 3], [f64; 3], f64, [f64; 2])],
    ids: &[usize],
    require_rank: bool,
) -> Option<M> {
    let mut normal = [[0.; 9]; 9];
    for &id in ids {
        let (a, b, w, _) = points[id];
        let row: [f64; 9] = std::array::from_fn(|k| b[k / 3] * a[k % 3]);
        for i in 0..9 {
            for j in 0..9 {
                normal[i][j] += w * row[i] * row[j];
            }
        }
    }
    let (values, v) = eigen(normal)?;
    let mut order = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    // Plane-only and identical-ray designs do not identify independent translation.
    if require_rank && values[order[1]] < values[order[8]].abs() * 1e-6 {
        return None;
    }
    let e = std::array::from_fn(|i| std::array::from_fn(|j| v[i * 3 + j][order[0]]));
    let (u, v) = decompose(e)?;
    Some(std::array::from_fn(|i| {
        std::array::from_fn(|j| u[i][0] * v[j][0] + u[i][1] * v[j][1])
    }))
}
fn residual(e: M, a: [f64; 3], b: [f64; 3]) -> f64 {
    let ea = mv(e, a);
    let etb = mv(transpose(e), b);
    let x = dot3(b, ea);
    x.abs()
        / (dot3(ea, ea) + dot3(etb, etb) - 2. * x * x)
            .max(1e-12)
            .sqrt()
}

pub(super) fn observe(
    matches: &[Correspondence],
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<YawObservation>> {
    let points: Vec<_> = matches
        .iter()
        .filter_map(|m| {
            if m.confidence < 0.25
                || !m.from.iter().chain(&m.to).all(|v| v.is_finite())
                || !(0.12..=0.88).contains(&m.from[1])
                || !(0.12..=0.88).contains(&m.to[1])
            {
                return None;
            }
            Some((ray(m.from), ray(m.to), m.confidence, m.from))
        })
        .collect();
    if points.len() < 50 {
        return Ok(None);
    }
    let limit = 0.45f64.to_radians();
    let mut seed = 0x948f2075b3d18ea7u64;
    let mut best: Option<(M, f64)> = None;
    for h in 0..512 {
        if h % 16 == 0 {
            checkpoint()?;
        }
        let mut ids = Vec::with_capacity(8);
        while ids.len() < 8 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let id = seed as usize % points.len();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        let Some(e) = fit(&points, &ids, false) else {
            continue;
        };
        let score = points
            .iter()
            .filter(|p| residual(e, p.0, p.1) < limit)
            .map(|p| p.2)
            .sum();
        if best.is_none_or(|(_, b)| score > b) {
            best = Some((e, score));
        }
    }
    let Some((mut e, _)) = best else {
        return Ok(None);
    };
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids = points
            .iter()
            .enumerate()
            .filter(|(_, p)| residual(e, p.0, p.1) < limit)
            .map(|(i, _)| i)
            .collect();
        if ids.len() < 50 || ids.len() * 100 < points.len() * 70 {
            return Ok(None);
        }
        let Some(next) = fit(&points, &ids, true) else {
            return Ok(None);
        };
        e = next;
    }
    let Some((u, v)) = decompose(e) else {
        return Ok(None);
    };
    let vt = transpose(v);
    let t = [u[0][2], u[1][2], u[2][2]];
    let mut candidates = Vec::new();
    for w in [
        [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]],
        [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]],
    ] {
        let r = mm(mm(u, w), vt);
        let Some(q) = matrix_quat(r) else {
            continue;
        };
        for sign in [-1., 1.] {
            let t = t.map(|x| x * sign);
            let mut positive = Vec::new();
            let mut parallax = Vec::new();
            for &id in &ids {
                let p = &points[id];
                let a = mv(r, p.0);
                let c = dot3(a, p.1);
                let det = 1. - c * c;
                if det < 1e-8 {
                    continue;
                }
                let d = -dot3(a, t);
                let f = dot3(p.1, t);
                let depth1 = (d + c * f) / det;
                let depth2 = (f + c * d) / det;
                if depth1 > 0. && depth2 > 0. {
                    positive.push(id);
                    parallax.push(c.clamp(-1., 1.).acos());
                }
            }
            candidates.push((positive.len(), q, positive, parallax));
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    if candidates.len() < 2 {
        return Ok(None);
    }
    let winner = &mut candidates[0];
    if winner.0 * 100 < ids.len() * 85 {
        return Ok(None);
    }
    let count = winner.0;
    if candidates[1].0 * 100 > ids.len() * 20 {
        return Ok(None);
    }
    let winner = &mut candidates[0];
    if winner.3.is_empty() || median(&mut winner.3) < 0.7f64.to_radians() {
        return Ok(None);
    }
    let q = winner.1;
    let radians = 2. * q[2].atan2(q[0]);
    if q[0] < 7.5f64.to_radians().cos() || radians.abs() > 9f64.to_radians() {
        return Ok(None);
    }
    // Leave-quarter-out stability catches a small epipolar residual with an
    // unstable rotational solution. No uncertain two-view yaw is accumulated.
    for skip in 0..4 {
        checkpoint()?;
        let subset: Vec<_> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 4 != skip)
            .map(|(_, i)| *i)
            .collect();
        let Some(ee) = fit(&points, &subset, true) else {
            return Ok(None);
        };
        let Some((uu, vv)) = decompose(ee) else {
            return Ok(None);
        };
        let mut closest: Option<[f64; 4]> = None;
        for ww in [
            [[0., -1., 0.], [1., 0., 0.], [0., 0., 1.]],
            [[0., 1., 0.], [-1., 0., 0.], [0., 0., 1.]],
        ] {
            if let Some(qq) = matrix_quat(mm(mm(uu, ww), transpose(vv))) {
                if closest.is_none_or(|old| qq[0] > old[0]) {
                    closest = Some(qq);
                }
            }
        }
        let Some(qq) = closest else {
            return Ok(None);
        };
        if wrap(2. * qq[2].atan2(qq[0]) - radians).abs() > 0.15f64.to_radians() {
            return Ok(None);
        }
    }
    let mut sectors = [false; 12];
    let mut bands = [false; 3];
    for &id in &winner.2 {
        let p = points[id].3;
        sectors[((p[0].rem_euclid(1.) * 12.) as usize).min(11)] = true;
        bands[(((p[1] - 0.12) / 0.76 * 3.) as usize).min(2)] = true;
    }
    let longitude_sectors = sectors.iter().filter(|v| **v).count();
    let latitude_bands = bands.iter().filter(|v| **v).count();
    if longitude_sectors < 6 || latitude_bands < 2 {
        return Ok(None);
    }
    let residual_degrees = (ids
        .iter()
        .map(|i| residual(e, points[*i].0, points[*i].1).powi(2))
        .sum::<f64>()
        / ids.len() as f64)
        .sqrt()
        .to_degrees();
    Ok(Some(YawObservation {
        radians,
        inliers: count,
        matches: points.len(),
        longitude_sectors,
        latitude_bands,
        residual_degrees,
    }))
}
