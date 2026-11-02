//! Filmcraft roll/crawl timing and offsets. Attribution: docs/licenses/filmcraft-MIT.txt.
use serde::{Deserialize, Serialize};
/// Roll / crawl direction (Responsive Design – Time ▸ Roll).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RollMode {
    #[default]
    Off,
    /// Moves up (credits).
    Roll,
    /// Moves right to left (tickers).
    CrawlLeft,
    /// Moves left to right.
    CrawlRight,
}

impl RollMode {
    pub const ALL: [RollMode; 4] = [
        RollMode::Off,
        RollMode::Roll,
        RollMode::CrawlLeft,
        RollMode::CrawlRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RollMode::Off => "Off",
            RollMode::Roll => "Roll",
            RollMode::CrawlLeft => "Crawl Left",
            RollMode::CrawlRight => "Crawl Right",
        }
    }
    pub fn from_name(s: &str) -> Option<RollMode> {
        let n = s.to_ascii_lowercase().replace([' ', '_', '-'], "");
        RollMode::ALL.into_iter().find(|m| {
            m.label().to_ascii_lowercase().replace(' ', "") == n
                || (n == "none" && *m == RollMode::Off)
        })
    }
}

/// Roll / crawl options. Times are clip-relative durations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Roll {
    pub mode: RollMode,
    /// The content starts just outside the frame (below it for a roll, right of it for a left
    /// crawl, left of it for a right crawl); otherwise it starts where it was laid out.
    pub start_off_screen: bool,
    /// The content leaves the frame completely; otherwise it stops when its last edge reaches the
    /// frame edge (content that fits stays where it was laid out).
    pub end_off_screen: bool,
    /// Hold before moving.
    pub preroll: i64,
    /// Accelerate over this long.
    pub ease_in: i64,
    /// Decelerate over this long.
    pub ease_out: i64,
    /// Hold after moving.
    pub postroll: i64,
}

/// Fraction (0..=1) of the roll's travel at clip time `u` of a clip `d` long: still during the
/// preroll, then constant speed with linear acceleration over `ease_in` and deceleration over
/// `ease_out` (a trapezoidal speed profile), still during the postroll.
pub fn roll_progress(r: &Roll, u: i64, d: i64) -> f64 {
    let ta = r.preroll.max(0) as f64;
    let tb = (d as i128 - r.postroll.max(0) as i128) as f64;
    let t_total = tb - ta;
    let u = u as f64;
    if t_total <= 0.0 {
        return if u <= ta { 0.0 } else { 1.0 };
    }
    let tau = (u - ta).clamp(0.0, t_total);
    let ei = (r.ease_in.max(0) as f64).min(t_total);
    let eo = (r.ease_out.max(0) as f64).min(t_total - ei);
    let v = 1.0 / (t_total - ei / 2.0 - eo / 2.0);
    let s = if tau < ei {
        v * tau * tau / (2.0 * ei)
    } else if tau <= t_total - eo {
        v * (ei / 2.0 + (tau - ei))
    } else {
        let rest = t_total - tau;
        1.0 - v * rest * rest / (2.0 * eo)
    };
    s.clamp(0.0, 1.0)
}

/// Offset (canvas px) of the whole graphic for a roll / crawl at clip time `u` of a clip `d`
/// long. `canvas` is the frame size and `content` the union of the layers' bounds
/// `[x0, y0, x1, y1]` where they were laid out.
pub fn roll_offset(r: &Roll, u: i64, d: i64, canvas: (u32, u32), content: [f64; 4]) -> (f64, f64) {
    let (w, h) = (canvas.0 as f64, canvas.1 as f64);
    let [x0, y0, x1, y1] = content;
    let lerp = |a: f64, b: f64| a + (b - a) * roll_progress(r, u, d);
    match r.mode {
        RollMode::Off => (0.0, 0.0),
        RollMode::Roll => {
            let from = if r.start_off_screen { h - y0 } else { 0.0 };
            let to = if r.end_off_screen {
                -y1
            } else {
                (h - y1).min(0.0)
            };
            (0.0, lerp(from, to))
        }
        RollMode::CrawlLeft => {
            let from = if r.start_off_screen { w - x0 } else { 0.0 };
            let to = if r.end_off_screen {
                -x1
            } else {
                (w - x1).min(0.0)
            };
            (lerp(from, to), 0.0)
        }
        RollMode::CrawlRight => {
            let from = if r.start_off_screen { -x1 } else { 0.0 };
            let to = if r.end_off_screen {
                w - x0
            } else {
                (-x0).max(0.0)
            };
            (lerp(from, to), 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_roll_and_crawl_all_progress_and_offset_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-roll.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 6480);
        for (index, case) in cases.iter().enumerate() {
            let roll: Roll = serde_json::from_value(case["roll"].clone()).unwrap();
            let u = case["u"].as_i64().unwrap();
            let d = case["d"].as_i64().unwrap();
            let bounds: [f64; 4] = serde_json::from_value(case["bounds"].clone()).unwrap();
            assert_eq!(
                roll_progress(&roll, u, d).to_bits(),
                case["progressBits"].as_u64().unwrap(),
                "progress{index}"
            );
            let (x, y) = roll_offset(&roll, u, d, (64, 48), bounds);
            let expected: [u64; 2] = serde_json::from_value(case["offsetBits"].clone()).unwrap();
            assert_eq!([x.to_bits(), y.to_bits()], expected, "offset{index}");
        }
    }
}
