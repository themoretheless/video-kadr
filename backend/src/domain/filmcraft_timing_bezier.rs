//! Filmcraft original parameter timing solver. MIT: docs/licenses/filmcraft-MIT.txt.
pub fn sample(p1:[f64;2],p2:[f64;2],x:f64) -> f64 {
    let [x1,y1]=p1;let [x2,y2]=p2;
    let bx = |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s * s * x2 + s * s * s;
    let by = |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s * s * y2 + s * s * s;
    let dbx = |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * x1 + 6.0 * (1.0 - s) * s * (x2 - x1) + 3.0 * s * s * (1.0 - x2);
    let mut s = x;
    for _ in 0..8 {
        let d = dbx(s);
        if d.abs() < 1e-9 {
            break;
        }
        let ns = s - (bx(s) - x) / d;
        if !(0.0..=1.0).contains(&ns) {
            break;
        }
        s = ns;
    }
    if (bx(s) - x).abs() > 1e-7 {
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..60 {
            s = (lo + hi) / 2.0;
            if bx(s) < x { lo = s } else { hi = s }
        }
    }
    by(s)
}
