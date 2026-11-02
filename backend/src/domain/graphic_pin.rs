//! Original Filmcraft responsive-position pins (MIT).
//! Attribution: docs/licenses/filmcraft-MIT.txt.
use serde::{Deserialize, Serialize};
/// What a pinned layer follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PinTarget {
    /// The video frame (the graphic's canvas).
    #[default]
    Frame,
    /// Another layer of the same clip by its stable UID.
    Layer(u64),
}

/// Responsive Design – Position: which edges of this layer follow the same edges of the target,
/// and their distances (this layer's edge minus the target's edge, canvas pixels) captured when
/// the pin was made.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pin {
    pub to: PinTarget,
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
    /// Distances `[left, top, right, bottom]`.
    pub offsets: [f64; 4],
}

impl Pin {
    pub fn any(&self) -> bool {
        self.left || self.top || self.right || self.bottom
    }
    /// A pin of `edges` (left, top, right, bottom) from layer bounds `own` to target bounds `target`.
    pub fn new(to: PinTarget, edges: [bool; 4], own: [f64; 4], target: [f64; 4]) -> Pin {
        Pin {
            to,
            left: edges[0],
            top: edges[1],
            right: edges[2],
            bottom: edges[3],
            offsets: [
                own[0] - target[0],
                own[1] - target[1],
                own[2] - target[2],
                own[3] - target[3],
            ],
        }
    }
}

/// One layer for [`resolve_pins`]: its uid, pin, unpinned bounds `[x0, y0, x1, y1]` (canvas px)
/// and whether pinning opposite edges may resize it (shapes) or only move it (text).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinNode {
    pub uid: u64,
    pub pin: Option<Pin>,
    pub bounds: [f64; 4],
    pub resizable: bool,
}

/// New bounds of every layer after pinning (same order as `nodes`). Targets are resolved before
/// the layers pinned to them; pins in a cycle, or to a missing layer, are ignored.
pub fn resolve_pins(nodes: &[PinNode], frame: [f64; 4]) -> Vec<[f64; 4]> {
    let mut out: Vec<Option<[f64; 4]>> = vec![None; nodes.len()];
    fn solve(
        i: usize,
        nodes: &[PinNode],
        frame: [f64; 4],
        out: &mut Vec<Option<[f64; 4]>>,
        visiting: &mut Vec<usize>,
    ) -> [f64; 4] {
        if let Some(b) = out[i] {
            return b;
        }
        let n = &nodes[i];
        let own = n.bounds;
        let Some(pin) = n.pin.as_ref().filter(|p| p.any()) else {
            out[i] = Some(own);
            return own;
        };
        if visiting.contains(&i) {
            return own; // cycle: this pin is ignored
        }
        visiting.push(i);
        let target = match pin.to {
            PinTarget::Frame => Some(frame),
            PinTarget::Layer(uid) => nodes
                .iter()
                .position(|m| m.uid == uid && uid != 0)
                .filter(|&j| j != i)
                .map(|j| solve(j, nodes, frame, out, visiting)),
        };
        visiting.pop();
        let Some(t) = target else {
            out[i] = Some(own);
            return own;
        };
        let axis = |lo_on: bool, hi_on: bool, lo: usize, hi: usize| -> (f64, f64) {
            let size = own[hi] - own[lo];
            match (lo_on, hi_on) {
                (true, true) if n.resizable => (t[lo] + pin.offsets[lo], t[hi] + pin.offsets[hi]),
                (true, _) => (t[lo] + pin.offsets[lo], t[lo] + pin.offsets[lo] + size),
                (false, true) => (t[hi] + pin.offsets[hi] - size, t[hi] + pin.offsets[hi]),
                (false, false) => (own[lo], own[hi]),
            }
        };
        let (x0, x1) = axis(pin.left, pin.right, 0, 2);
        let (y0, y1) = axis(pin.top, pin.bottom, 1, 3);
        let b = [x0, y0, x1, y1];
        out[i] = Some(b);
        b
    }
    let mut visiting = Vec::new();
    (0..nodes.len())
        .map(|i| solve(i, nodes, frame, &mut out, &mut visiting))
        .collect()
}

impl Pin {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.offsets.iter().all(|value| value.is_finite()),
            "invalid pin offsets"
        );
        Ok(())
    }
}
/// Bounded admission for the original resolver; successful results retain original arithmetic.
pub fn resolve_pins_checked(nodes: &[PinNode], frame: [f64; 4]) -> anyhow::Result<Vec<[f64; 4]>> {
    anyhow::ensure!(nodes.len() <= 512, "too many pinned layers");
    anyhow::ensure!(
        frame.iter().all(|value| value.is_finite()),
        "invalid pin frame"
    );
    for node in nodes {
        anyhow::ensure!(
            node.bounds.iter().all(|value| value.is_finite()),
            "invalid pin bounds"
        );
        if let Some(pin) = &node.pin {
            pin.validate()?;
        }
    }
    let result = resolve_pins(nodes, frame);
    anyhow::ensure!(
        result.iter().flatten().all(|value| value.is_finite()),
        "pin bounds overflow"
    );
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_pin_graph_all_bounds_bits() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-graphic-pin.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 328);
        for (index, case) in cases.iter().enumerate() {
            let nodes: Vec<PinNode> = serde_json::from_value(case["nodes"].clone()).unwrap();
            let frame = serde_json::from_value(case["frame"].clone()).unwrap();
            let actual = resolve_pins_checked(&nodes, frame).unwrap();
            let expected: Vec<[u64; 4]> = serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(
                actual
                    .iter()
                    .map(|bounds| bounds.map(f64::to_bits))
                    .collect::<Vec<_>>(),
                expected,
                "case {index}"
            );
        }
    }
    #[test]
    fn pin_admission_rejects_nonfinite_overflow_and_unbounded_graphs() {
        let node = PinNode {
            uid: 1,
            pin: Some(Pin::new(
                PinTarget::Frame,
                [true; 4],
                [1., 2., 3., 4.],
                [0., 0., 10., 10.],
            )),
            bounds: [1., 2., 3., 4.],
            resizable: true,
        };
        let mut invalid = node.clone();
        invalid.pin.as_mut().unwrap().offsets[0] = f64::NAN;
        assert!(resolve_pins_checked(&[invalid], [0.; 4]).is_err());
        assert!(resolve_pins_checked(&vec![node.clone(); 513], [0.; 4]).is_err());
        let mut overflow = node;
        overflow.pin.as_mut().unwrap().offsets[0] = f64::MAX;
        assert!(resolve_pins_checked(&[overflow], [f64::MAX; 4]).is_err());
    }
}
