//! Filmcraft Param::value_at semantics (MIT, pinned 52318524).
//! Attribution: docs/licenses/filmcraft-MIT.txt. Source-space typed authoring only.
use super::filmcraft_sequence_inspect::canonical_parameter;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
fn components(v: &Value) -> Vec<f64> {
    if let Some(x) = v["Float"].as_f64() {
        return vec![x];
    }
    if let Some(p) = v.get("Vec2") {
        return vec![
            p["x"].as_f64().unwrap_or(f64::NAN),
            p["y"].as_f64().unwrap_or(f64::NAN),
        ];
    }
    if let Some(c) = v["Color"].as_array() {
        return c.iter().map(|x| x.as_f64().unwrap()).collect();
    }
    if let Some(p) = v["Path"]["vertices"].as_array() {
        return p
            .iter()
            .flat_map(|v| {
                [
                    v["p"]["x"].as_f64().unwrap(),
                    v["p"]["y"].as_f64().unwrap(),
                    v["t_in"]["x"].as_f64().unwrap_or(0.),
                    v["t_in"]["y"].as_f64().unwrap_or(0.),
                    v["t_out"]["x"].as_f64().unwrap_or(0.),
                    v["t_out"]["y"].as_f64().unwrap_or(0.),
                ]
            })
            .collect();
    }
    vec![]
}
fn from_components(v: &Value, c: &[f64]) -> Result<Value> {
    if v.get("Float").is_some() {
        return Ok(json!({"Float":c[0]}));
    }
    if v.get("Vec2").is_some() {
        return Ok(json!({"Vec2":{"x":c[0],"y":c[1]}}));
    }
    if v.get("Color").is_some() {
        return Ok(json!({"Color":[c[0] as f32,c[1] as f32,c[2] as f32,c[3] as f32]}));
    }
    if v.get("Path").is_some() {
        return canonical_parameter(
            json!({"Path":{"closed":v["Path"]["closed"],"vertices":c.chunks_exact(6).map(|c|json!({"p":{"x":c[0],"y":c[1]},"t_in":{"x":c[2],"y":c[3]},"t_out":{"x":c[4],"y":c[5]}})).collect::<Vec<_>>()}}),
        );
    }
    Ok(v.clone())
}
// Retain the original operation order and Newton/bisection thresholds.
fn cubic(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    let bx = |s: f64| 3. * (1. - s) * (1. - s) * s * x1 + 3. * (1. - s) * s * s * x2 + s * s * s;
    let by = |s: f64| 3. * (1. - s) * (1. - s) * s * y1 + 3. * (1. - s) * s * s * y2 + s * s * s;
    let dbx = |s: f64| {
        3. * (1. - s) * (1. - s) * x1 + 6. * (1. - s) * s * (x2 - x1) + 3. * s * s * (1. - x2)
    };
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
        let (mut lo, mut hi) = (0., 1.);
        for _ in 0..60 {
            s = (lo + hi) / 2.;
            if bx(s) < x {
                lo = s
            } else {
                hi = s
            }
        }
    }
    by(s)
}
fn ease(a: &Value, b: &Value, u: f64) -> f64 {
    let left = a["interp"].as_str().unwrap_or("Linear");
    let right = b["interp"].as_str().unwrap_or("Linear");
    let out = matches!(
        left,
        "Bezier" | "AutoBezier" | "ContinuousBezier" | "EaseOut"
    );
    let incoming = matches!(
        right,
        "Bezier" | "AutoBezier" | "ContinuousBezier" | "EaseIn"
    ) || matches!(left, "Bezier" | "ContinuousBezier");
    if !out && !incoming {
        return u;
    }
    let x1 = if out {
        a["out_influence"]
            .as_f64()
            .unwrap_or(1. / 3.)
            .clamp(0.01, 1.)
    } else {
        1. / 3.
    };
    let y1 = if out { 0. } else { 1. / 3. };
    let x2 = if incoming {
        1. - b["in_influence"]
            .as_f64()
            .unwrap_or(1. / 3.)
            .clamp(0.01, 1.)
    } else {
        2. / 3.
    };
    let y2 = if incoming { 1. } else { 2. / 3. };
    cubic(x1, y1, x2, y2, u)
}
pub(super) fn sample(parameter: &Value, t: i64) -> Result<Value> {
    let base = canonical_parameter(parameter["value"].clone())?;
    let keys = parameter
        .get("keyframes")
        .map(|k| k.as_array().context("invalid parameter keys"))
        .transpose()?
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if keys.is_empty() {
        return Ok(base);
    }
    ensure!(
        keys.iter().all(|k| k["time"].as_i64().is_some()),
        "invalid source key time"
    );
    ensure!(
        keys.windows(2)
            .all(|k| k[0]["time"].as_i64() < k[1]["time"].as_i64()),
        "source keys must be ordered"
    );
    if t <= keys[0]["time"].as_i64().unwrap() {
        return canonical_parameter(keys[0]["value"].clone());
    }
    let last = keys.last().unwrap();
    if t >= last["time"].as_i64().unwrap() {
        return canonical_parameter(last["value"].clone());
    }
    let i = match keys.binary_search_by_key(&t, |k| k["time"].as_i64().unwrap()) {
        Ok(i) => return canonical_parameter(keys[i]["value"].clone()),
        Err(i) => i - 1,
    };
    let (a, b) = (&keys[i], &keys[i + 1]);
    let va = canonical_parameter(a["value"].clone())?;
    if a["interp"] == "Hold"
        || !["Float", "Vec2", "Color", "Path"]
            .iter()
            .any(|tag| va.get(tag).is_some())
    {
        return Ok(va);
    }
    let vb = canonical_parameter(b["value"].clone())?;
    let ca = components(&va);
    let cb = components(&vb);
    if ca.len() != cb.len() {
        return Ok(va);
    }
    let span = b["time"]
        .as_i64()
        .unwrap()
        .checked_sub(a["time"].as_i64().unwrap())
        .context("key span overflow")?;
    let offset = t
        .checked_sub(a["time"].as_i64().unwrap())
        .context("key offset overflow")?;
    let e = ease(a, b, offset as f64 / span as f64);
    let c: Vec<f64> = ca.iter().zip(cb).map(|(x, y)| x + (y - x) * e).collect();
    from_components(&va, &c)
}
