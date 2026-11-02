// Adapted from WeftCut MIT, pinned d1db8020164c51ee49df9cf0143a5cbf250951a4.
// License: docs/licenses/weftcut-MIT.txt.
// Distance-based motion over an adaptively flattened path. The same samples
// and lookup serve native callers and the Wasm preview. No heap is required.
pub const MAX_NODES: usize = 128;
pub const MAX_SAMPLES: usize = (MAX_NODES - 1) * 512 + 1;
pub const TOLERANCE: f64 = 0.01;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub fn plus(self, b: Self) -> Self {
        Self {
            x: self.x + b.x,
            y: self.y + b.y,
        }
    }
    pub fn minus(self, b: Self) -> Self {
        Self {
            x: self.x - b.x,
            y: self.y - b.y,
        }
    }
    pub fn scaled(self, s: f64) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
    pub fn distance(self, b: Self) -> f64 {
        let d = self.minus(b);
        (d.x * d.x + d.y * d.y).sqrt()
    }
    pub fn unit(self) -> Self {
        let n = self.distance(Self::ZERO);
        if n > 0.0 {
            self.scaled(1.0 / n)
        } else {
            Self::ZERO
        }
    }
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub point: Point,
    pub incoming: Point,
    pub outgoing: Point,
    pub cubic: bool,
}
impl Node {
    pub const ZERO: Self = Self {
        point: Point::ZERO,
        incoming: Point::ZERO,
        outgoing: Point::ZERO,
        cubic: false,
    };
}

fn flatten(p: [Point; 4], depth: u8, t0: f64, t1: f64, emit: &mut impl FnMut(Point, f64)) {
    let chord = p[0].distance(p[3]);
    let polygon = p[0].distance(p[1]) + p[1].distance(p[2]) + p[2].distance(p[3]);
    // Distance from controls to the chord also bounds geometric deviation.
    let deviation = if chord > 0.0 {
        let d = p[3].minus(p[0]);
        let a = p[1].minus(p[0]);
        let b = p[2].minus(p[0]);
        (d.x * a.y - d.y * a.x)
            .abs()
            .max((d.x * b.y - d.y * b.x).abs())
            / chord
    } else {
        polygon
    };
    if depth == 9 || (polygon - chord <= TOLERANCE && deviation <= TOLERANCE) {
        emit(p[3], t1);
        return;
    }
    let a = p[0].plus(p[1]).scaled(0.5);
    let b = p[1].plus(p[2]).scaled(0.5);
    let c = p[2].plus(p[3]).scaled(0.5);
    let d = a.plus(b).scaled(0.5);
    let e = b.plus(c).scaled(0.5);
    let m = d.plus(e).scaled(0.5);
    let tm = (t0 + t1) * 0.5;
    flatten([p[0], a, d, m], depth + 1, t0, tm, emit);
    flatten([m, e, c, p[3]], depth + 1, tm, t1, emit);
}

/// Emits (x, y, cumulative distance, segment-index + parameter). The parameter
/// lets authoring map fitted spatial points back to the SAME distance table.
pub fn compile(nodes: &[Node], mut emit: impl FnMut([f64; 4])) -> (Point, Point) {
    let Some(first) = nodes.first() else {
        return (Point::ZERO, Point::ZERO);
    };
    let mut last = first.point;
    let mut length = 0.0;
    emit([last.x, last.y, 0.0, 0.0]);
    for (index, pair) in nodes.windows(2).enumerate() {
        let a = pair[0];
        let b = pair[1];
        let mut append = |p: Point, t: f64| {
            length += last.distance(p);
            last = p;
            emit([p.x, p.y, length, index as f64 + t]);
        };
        if a.cubic {
            flatten(
                [
                    a.point,
                    a.point.plus(a.outgoing),
                    b.point.plus(b.incoming),
                    b.point,
                ],
                0,
                0.0,
                1.0,
                &mut append,
            );
        } else {
            append(b.point, 1.0);
        }
    }
    let mut start = Point::ZERO;
    let mut end = Point::ZERO;
    for pair in nodes.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let candidates = if a.cubic {
            [
                a.outgoing,
                b.point.plus(b.incoming).minus(a.point),
                b.point.minus(a.point),
            ]
        } else {
            [b.point.minus(a.point); 3]
        };
        for d in candidates {
            if d != Point::ZERO {
                start = d.unit();
                break;
            }
        }
        if start != Point::ZERO {
            break;
        }
    }
    for pair in nodes.windows(2).rev() {
        let a = pair[0];
        let b = pair[1];
        let candidates = if a.cubic {
            [
                b.incoming.scaled(-1.0),
                b.point.minus(a.point.plus(a.outgoing)),
                b.point.minus(a.point),
            ]
        } else {
            [b.point.minus(a.point); 3]
        };
        for d in candidates {
            if d != Point::ZERO {
                end = d.unit();
                break;
            }
        }
        if end != Point::ZERO {
            break;
        }
    }
    (start, end)
}

pub fn evaluate(samples: &[[f64; 4]], progress: f64, start: Point, end: Point) -> Point {
    let Some(a) = samples.first() else {
        return Point::ZERO;
    };
    let b = samples[samples.len() - 1];
    let total = b[2];
    let first = Point { x: a[0], y: a[1] };
    let last = Point { x: b[0], y: b[1] };
    if total == 0.0 || !progress.is_finite() {
        return first;
    }
    if progress <= 0.0 {
        return first.plus(start.scaled(progress * total));
    }
    if progress >= 1.0 {
        return last.plus(end.scaled((progress - 1.0) * total));
    }
    let distance = progress * total;
    let mut lo = 1;
    let mut hi = samples.len() - 1;
    while lo < hi {
        let m = (lo + hi) / 2;
        if samples[m][2] < distance {
            lo = m + 1;
        } else {
            hi = m;
        }
    }
    let a = samples[lo - 1];
    let b = samples[lo];
    let u = if b[2] > a[2] {
        (distance - a[2]) / (b[2] - a[2])
    } else {
        0.0
    };
    Point {
        x: a[0] + (b[0] - a[0]) * u,
        y: a[1] + (b[1] - a[1]) * u,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(x: f64, y: f64) -> Node {
        Node {
            point: Point { x, y },
            ..Node::ZERO
        }
    }
    #[test]
    fn samples_retain_ordered_segment_parameters() {
        let mut nodes = [line(0.0, 0.0), line(100.0, 0.0), line(150.0, 0.0)];
        nodes[0].cubic = true;
        nodes[0].outgoing = Point { x: 0.0, y: 100.0 };
        nodes[1].incoming = Point { x: 0.0, y: 100.0 };
        let mut samples = Vec::new();
        compile(&nodes, |p| samples.push(p));
        assert_eq!(samples[0][3], 0.0);
        assert_eq!(samples.last().unwrap()[3], 2.0);
        assert!(samples.iter().any(|s| s[3] > 0.0 && s[3] < 1.0));
        assert!(samples.iter().any(|s| s[3] == 1.0));
        assert!(samples
            .windows(2)
            .all(|p| p[0][3] < p[1][3] && p[0][2] <= p[1][2]));
    }
    #[test]
    fn distance_uses_length_not_node_count() {
        let mut s = Vec::new();
        let (a, b) = compile(&[line(0.0, 0.0), line(10.0, 0.0), line(10.0, 90.0)], |p| {
            s.push(p)
        });
        assert_eq!(evaluate(&s, 0.5, a, b), Point { x: 10.0, y: 40.0 });
        assert_eq!(evaluate(&s, -0.1, a, b), Point { x: -10.0, y: 0.0 });
        assert!((evaluate(&s, 1.1, a, b).y - 100.0).abs() < 1e-12);
    }
    #[test]
    fn loop_with_coincident_endpoints_does_not_collapse() {
        let mut a = line(0.0, 0.0);
        a.cubic = true;
        a.outgoing = Point { x: 100.0, y: 100.0 };
        let mut b = a;
        b.incoming = Point {
            x: -100.0,
            y: 100.0,
        };
        let mut s = Vec::new();
        let (u, v) = compile(&[a, b], |p| s.push(p));
        assert!(s.last().unwrap()[2] > 100.0);
        assert!(evaluate(&s, 0.5, u, v).y > 70.0);
    }
    #[test]
    fn repeated_points_stay_finite() {
        let mut s = Vec::new();
        let (a, b) = compile(&[line(3.0, 4.0); 3], |p| s.push(p));
        assert_eq!(evaluate(&s, 2.0, a, b), Point { x: 3.0, y: 4.0 });
    }
}

/// Admitted immutable lookup table; rebuilding occurs on authored path changes.
#[derive(Clone, Debug)]
pub struct CompiledMotionPath {
    samples: Vec<[f64; 4]>,
    start: Point,
    end: Point,
}
impl CompiledMotionPath {
    pub fn new(nodes: &[Node]) -> Result<Self, String> {
        if nodes.is_empty() || nodes.len() > MAX_NODES {
            return Err("Motion path requires 1–128 nodes".into());
        }
        if nodes.iter().any(|node| {
            [node.point, node.incoming, node.outgoing]
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
        }) {
            return Err("Motion path coordinates must be finite".into());
        }
        let mut samples = Vec::new();
        let (start, end) = compile(nodes, |sample| samples.push(sample));
        if samples.len() > MAX_SAMPLES
            || samples.iter().flatten().any(|value| !value.is_finite())
            || ![start.x, start.y, end.x, end.y]
                .iter()
                .all(|value| value.is_finite())
        {
            return Err("Motion path geometry exceeds finite range".into());
        }
        Ok(Self {
            samples,
            start,
            end,
        })
    }
    pub fn sample(&self, progress: f64) -> Point {
        evaluate(&self.samples, progress, self.start, self.end)
    }
    pub fn length(&self) -> f64 {
        self.samples.last().map_or(0.0, |sample| sample[2])
    }
    pub fn samples(&self) -> &[[f64; 4]] {
        &self.samples
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn rejects_invalid_geometry_before_runtime_sampling() {
        assert!(CompiledMotionPath::new(&[]).is_err());
        assert!(CompiledMotionPath::new(&[Node::ZERO; MAX_NODES + 1]).is_err());
        let mut node = Node::ZERO;
        node.outgoing.x = f64::NAN;
        assert!(CompiledMotionPath::new(&[node]).is_err());
        let nodes = [
            Node {
                point: Point {
                    x: -f64::MAX,
                    y: 0.0,
                },
                ..Node::ZERO
            },
            Node {
                point: Point {
                    x: f64::MAX,
                    y: 0.0,
                },
                ..Node::ZERO
            },
        ];
        assert!(CompiledMotionPath::new(&nodes).is_err());
    }
    #[test]
    fn admitted_lookup_matches_distance_and_endpoint_contract() {
        let nodes = [
            Node::ZERO,
            Node {
                point: Point { x: 10.0, y: 0.0 },
                ..Node::ZERO
            },
            Node {
                point: Point { x: 10.0, y: 90.0 },
                ..Node::ZERO
            },
        ];
        let path = CompiledMotionPath::new(&nodes).unwrap();
        assert_eq!(path.length(), 100.0);
        assert_eq!(path.sample(0.5), Point { x: 10.0, y: 40.0 });
        assert_eq!(path.sample(f64::NAN), Point::ZERO);
        assert_eq!(path.sample(1.0), nodes[2].point);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialTangentMode {
    Corner,
    Smooth,
    Auto,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSegment {
    Line,
    Cubic,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathNode {
    pub id: String,
    pub point: Point,
    pub in_handle: Point,
    pub out_handle: Point,
    pub segment: PathSegment,
    pub tangent_mode: SpatialTangentMode,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionPath {
    pub nodes: Vec<PathNode>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct PathLocation {
    pub segment: usize,
    pub t: f64,
    pub distance: f64,
}

impl MotionPath {
    pub fn compile(&self) -> Result<CompiledMotionPath, String> {
        let mut ids = std::collections::HashSet::new();
        for node in &self.nodes {
            if node.id.is_empty() || !ids.insert(&node.id) {
                return Err("Path node ids must be unique and nonempty".into());
            }
            if [node.point, node.in_handle, node.out_handle]
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1e7 || p.y.abs() > 1e7)
            {
                return Err("Path coordinates must be within ten million pixels".into());
            }
        }
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| Node {
                point: node.point,
                incoming: node.in_handle,
                outgoing: node.out_handle,
                cubic: node.segment == PathSegment::Cubic,
            })
            .collect();
        CompiledMotionPath::new(&nodes)
    }
    /// Authoring only: persisted explicit handles are the playback truth.
    pub fn solve(&self) -> Result<Self, String> {
        self.compile()?;
        fn length(p: Point) -> f64 {
            p.x.hypot(p.y)
        }
        fn unit(p: Point) -> Point {
            let n = length(p);
            if n > 1e-12 {
                p.scaled(1.0 / n)
            } else {
                Point::ZERO
            }
        }
        let mut solved = self.clone();
        for (index, node) in solved.nodes.iter_mut().enumerate() {
            match node.tangent_mode {
                SpatialTangentMode::Corner => {}
                SpatialTangentMode::Smooth => {
                    if length(node.out_handle) > 1e-12 {
                        node.in_handle = unit(node.out_handle).scaled(-length(node.in_handle));
                    }
                }
                SpatialTangentMode::Auto => {
                    let before = if index > 0 {
                        node.point.minus(self.nodes[index - 1].point)
                    } else {
                        Point::ZERO
                    };
                    let after = self
                        .nodes
                        .get(index + 1)
                        .map_or(Point::ZERO, |next| next.point.minus(node.point));
                    let direction = unit(unit(before).plus(unit(after)));
                    node.in_handle = direction.scaled(-length(before) / 3.0);
                    node.out_handle = direction.scaled(length(after) / 3.0);
                }
            }
        }
        solved.compile()?;
        Ok(solved)
    }
}

#[cfg(test)]
mod authored_tests {
    use super::*;
    fn node(id: &str, x: f64, y: f64, mode: SpatialTangentMode) -> PathNode {
        PathNode {
            id: id.into(),
            point: Point { x, y },
            in_handle: Point::ZERO,
            out_handle: Point::ZERO,
            segment: PathSegment::Cubic,
            tangent_mode: mode,
        }
    }
    #[test]
    fn auto_bisects_unit_chords_preserving_each_neighbour_length() {
        let path = MotionPath {
            nodes: vec![
                node("a", 0.0, 0.0, SpatialTangentMode::Auto),
                node("b", 3.0, 0.0, SpatialTangentMode::Auto),
                node("c", 3.0, 30.0, SpatialTangentMode::Auto),
            ],
        };
        let solved = path.solve().unwrap();
        let middle = &solved.nodes[1];
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert!((middle.in_handle.x + s).abs() < 1e-14);
        assert!((middle.in_handle.y + s).abs() < 1e-14);
        assert!((middle.out_handle.x - 10.0 * s).abs() < 1e-13);
        assert_eq!(solved.solve().unwrap(), solved);
        assert_eq!(path.nodes[1].out_handle, Point::ZERO);
        let mut reversal = path.clone();
        reversal.nodes[2].point = Point::ZERO;
        assert_eq!(reversal.solve().unwrap().nodes[1].out_handle, Point::ZERO);
    }
    #[test]
    fn smooth_keeps_lengths_and_serde_and_admission_are_strict() {
        let mut n = node("a", 0.0, 0.0, SpatialTangentMode::Smooth);
        n.in_handle = Point { x: 3.0, y: 4.0 };
        n.out_handle = Point { x: 0.0, y: 2.0 };
        let path = MotionPath { nodes: vec![n] };
        assert_eq!(
            path.solve().unwrap().nodes[0].in_handle,
            Point { x: 0.0, y: -5.0 }
        );
        let wire = serde_json::to_value(&path).unwrap();
        assert_eq!(serde_json::from_value::<MotionPath>(wire).unwrap(), path);
        let mut invalid = path.clone();
        invalid.nodes.push(invalid.nodes[0].clone());
        assert!(invalid.compile().is_err());
        invalid.nodes.pop();
        invalid.nodes[0].point.x = 1e7 + 1.0;
        assert!(invalid.compile().is_err());
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathNodePart {
    Point,
    InHandle,
    OutHandle,
}
impl MotionPath {
    pub fn set_node_mode(&self, index: usize, mode: SpatialTangentMode) -> Result<Self, String> {
        self.compile()?;
        if index >= self.nodes.len() {
            return Err("Path node does not exist".into());
        }
        let mut result = self.clone();
        result.nodes[index].tangent_mode = mode;
        if mode != SpatialTangentMode::Corner {
            if index > 0 {
                result.nodes[index - 1].segment = PathSegment::Cubic;
            }
            if index + 1 < result.nodes.len() {
                result.nodes[index].segment = PathSegment::Cubic;
            }
            let node = &result.nodes[index];
            if mode == SpatialTangentMode::Smooth
                && node.in_handle.x.hypot(node.in_handle.y)
                    + node.out_handle.x.hypot(node.out_handle.y)
                    < 1e-12
            {
                let mut auto = result.clone();
                auto.nodes[index].tangent_mode = SpatialTangentMode::Auto;
                let solved = auto.solve()?;
                result.nodes[index].in_handle = solved.nodes[index].in_handle;
                result.nodes[index].out_handle = solved.nodes[index].out_handle;
            }
        }
        result.solve()
    }
    /// WeftCut hit-test search: 64 samples per span and 28 ternary refinements.
    pub fn nearest_location(&self, point: Point) -> Result<Option<PathLocation>, String> {
        self.compile()?;
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > 1e7
            || point.y.abs() > 1e7
        {
            return Err("Finite hit-test point required".into());
        }
        let mut best: Option<PathLocation> = None;
        for segment in 0..self.nodes.len().saturating_sub(1) {
            let distance = |t| self.point_at(segment, t).unwrap().distance(point);
            let mut sample = 0;
            for index in 1..=64 {
                if distance(index as f64 / 64.0) < distance(sample as f64 / 64.0) {
                    sample = index;
                }
            }
            let mut low = ((sample as f64 - 1.0) / 64.0).max(0.0);
            let mut high = ((sample as f64 + 1.0) / 64.0).min(1.0);
            for _ in 0..28 {
                let a = low + (high - low) / 3.0;
                let b = high - (high - low) / 3.0;
                if distance(a) < distance(b) {
                    high = b;
                } else {
                    low = a;
                }
            }
            let t = (low + high) / 2.0;
            let candidate = PathLocation {
                segment,
                t,
                distance: distance(t),
            };
            if best
                .as_ref()
                .is_none_or(|old| candidate.distance < old.distance)
            {
                best = Some(candidate);
            }
        }
        Ok(best)
    }
    pub fn translate(&self, delta: Point) -> Result<Self, String> {
        self.compile()?;
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return Err("Finite translation required".into());
        }
        let mut result = self.clone();
        for node in &mut result.nodes {
            node.point.x += delta.x;
            node.point.y += delta.y;
        }
        result.compile()?;
        Ok(result)
    }
    pub fn remove_node(&self, index: usize) -> Result<Self, String> {
        self.compile()?;
        if self.nodes.len() < 2 || index >= self.nodes.len() {
            return Err("Cannot remove path node".into());
        }
        let mut result = self.clone();
        result.nodes.remove(index);
        result.solve()
    }
    pub fn toggle_segment(&self, index: usize) -> Result<Self, String> {
        self.compile()?;
        if index >= self.nodes.len().saturating_sub(1) {
            return Err("Path segment not found".into());
        }
        let mut result = self.clone();
        let cubic = result.nodes[index].segment == PathSegment::Line;
        result.nodes[index].tangent_mode = SpatialTangentMode::Corner;
        result.nodes[index].segment = if cubic {
            PathSegment::Cubic
        } else {
            PathSegment::Line
        };
        if cubic {
            result.nodes[index].out_handle = Point { x: 60.0, y: -60.0 };
            result.nodes[index + 1].in_handle = Point { x: -60.0, y: -60.0 };
        }
        result.solve()
    }
    pub fn edit_node(
        &self,
        index: usize,
        part: PathNodePart,
        value: Point,
    ) -> Result<Self, String> {
        self.compile()?;
        if index >= self.nodes.len() {
            return Err("Path node does not exist".into());
        }
        if !value.x.is_finite()
            || !value.y.is_finite()
            || value.x.abs() > 1e7
            || value.y.abs() > 1e7
        {
            return Err("Path coordinates must be within ten million pixels".into());
        }
        let mut result = self.clone();
        let node = &mut result.nodes[index];
        match part {
            PathNodePart::Point => node.point = value,
            PathNodePart::InHandle => node.in_handle = value,
            PathNodePart::OutHandle => node.out_handle = value,
        }
        if !matches!(part, PathNodePart::Point) && node.tangent_mode != SpatialTangentMode::Corner {
            node.tangent_mode = SpatialTangentMode::Smooth;
            let opposite = if matches!(part, PathNodePart::InHandle) {
                &mut node.out_handle
            } else {
                &mut node.in_handle
            };
            let n = value.x.hypot(value.y);
            *opposite = if n > 1e-12 {
                value.scaled(-opposite.x.hypot(opposite.y) / n)
            } else {
                Point::ZERO
            };
        }
        result.solve()
    }
}

#[cfg(test)]
mod edit_tests {
    use super::*;
    fn fixture() -> MotionPath {
        MotionPath {
            nodes: [("a", 0.0, 0.0), ("b", 30.0, 0.0), ("c", 30.0, 30.0)]
                .into_iter()
                .map(|(id, x, y)| PathNode {
                    id: id.into(),
                    point: Point { x, y },
                    in_handle: Point::ZERO,
                    out_handle: Point::ZERO,
                    segment: PathSegment::Line,
                    tangent_mode: SpatialTangentMode::Corner,
                })
                .collect(),
        }
    }
    #[test]
    fn mode_converts_adjacent_segments_and_manual_drag_frees_auto() {
        let original = fixture();
        let auto = original.set_node_mode(1, SpatialTangentMode::Auto).unwrap();
        assert_eq!(auto.nodes[0].segment, PathSegment::Cubic);
        assert_eq!(auto.nodes[1].segment, PathSegment::Cubic);
        let dragged = auto
            .edit_node(1, PathNodePart::OutHandle, Point { x: 0.0, y: 20.0 })
            .unwrap();
        assert_eq!(dragged.nodes[1].tangent_mode, SpatialTangentMode::Smooth);
        assert_eq!(dragged.nodes[1].out_handle, Point { x: 0.0, y: 20.0 });
        assert!((dragged.nodes[1].in_handle.y + 10.0).abs() < 1e-12);
        assert_eq!(dragged.nodes[1].in_handle.x, 0.0);
        assert_eq!(original.nodes[0].segment, PathSegment::Line);
        let smooth = original
            .set_node_mode(1, SpatialTangentMode::Smooth)
            .unwrap();
        assert_eq!(smooth.nodes[1].in_handle, auto.nodes[1].in_handle);
    }
    #[test]
    fn incoming_edits_keep_opposite_length_and_corner_does_not_align() {
        let auto = fixture()
            .set_node_mode(1, SpatialTangentMode::Auto)
            .unwrap();
        let changed = auto
            .edit_node(1, PathNodePart::InHandle, Point { x: -20.0, y: 0.0 })
            .unwrap();
        assert!((changed.nodes[1].out_handle.x - 10.0).abs() < 1e-12);
        assert_eq!(changed.nodes[1].out_handle.y, 0.0);
        let corner = auto.set_node_mode(1, SpatialTangentMode::Corner).unwrap();
        let changed = corner
            .edit_node(1, PathNodePart::InHandle, Point { x: -20.0, y: 0.0 })
            .unwrap();
        assert_eq!(changed.nodes[1].out_handle, corner.nodes[1].out_handle);
        assert!(auto.edit_node(4, PathNodePart::Point, Point::ZERO).is_err());
        assert!(auto
            .edit_node(
                1,
                PathNodePart::Point,
                Point {
                    x: f64::INFINITY,
                    y: 0.0
                }
            )
            .is_err());
    }
}

fn mix(a: Point, b: Point, t: f64) -> Point {
    a.scaled(1.0 - t).plus(b.scaled(t))
}
impl MotionPath {
    pub fn point_at(&self, segment: usize, t: f64) -> Result<Point, String> {
        if !t.is_finite() || !(0.0..=1.0).contains(&t) {
            return Err("Path parameter must be within 0–1".into());
        }
        let a = self
            .nodes
            .get(segment)
            .ok_or("Path segment does not exist")?;
        let b = self
            .nodes
            .get(segment + 1)
            .ok_or("Path segment does not exist")?;
        if a.segment == PathSegment::Line {
            return Ok(mix(a.point, b.point, t));
        }
        let q0 = mix(a.point, a.point.plus(a.out_handle), t);
        let q1 = mix(a.point.plus(a.out_handle), b.point.plus(b.in_handle), t);
        let q2 = mix(b.point.plus(b.in_handle), b.point, t);
        Ok(mix(mix(q0, q1, t), mix(q1, q2, t), t))
    }
    pub fn insert_node(&self, segment: usize, t: f64, id: String) -> Result<Self, String> {
        self.compile()?;
        if segment >= self.nodes.len() - 1 || !t.is_finite() || t <= 0.0 || t >= 1.0 {
            return Err("Insert inside an existing segment".into());
        }
        if self.nodes.len() >= MAX_NODES
            || id.is_empty()
            || self.nodes.iter().any(|node| node.id == id)
        {
            return Err("Inserted node needs capacity and a unique id".into());
        }
        let mut result = self.clone();
        let a = &self.nodes[segment];
        let b = &self.nodes[segment + 1];
        let mut inserted = PathNode {
            id,
            point: mix(a.point, b.point, t),
            in_handle: Point::ZERO,
            out_handle: Point::ZERO,
            segment: a.segment,
            tangent_mode: SpatialTangentMode::Corner,
        };
        if a.segment == PathSegment::Cubic {
            let q0 = mix(a.point, a.point.plus(a.out_handle), t);
            let q1 = mix(a.point.plus(a.out_handle), b.point.plus(b.in_handle), t);
            let q2 = mix(b.point.plus(b.in_handle), b.point, t);
            let r0 = mix(q0, q1, t);
            let r1 = mix(q1, q2, t);
            inserted.point = mix(r0, r1, t);
            inserted.in_handle = r0.minus(inserted.point);
            inserted.out_handle = r1.minus(inserted.point);
            inserted.tangent_mode = SpatialTangentMode::Smooth;
            result.nodes[segment].out_handle = q0.minus(a.point);
            result.nodes[segment + 1].in_handle = q2.minus(b.point);
        }
        for index in [segment, segment + 1] {
            if result.nodes[index].tangent_mode == SpatialTangentMode::Auto {
                result.nodes[index].tangent_mode = SpatialTangentMode::Smooth;
            }
        }
        result.nodes.insert(segment + 1, inserted);
        result.compile()?;
        Ok(result)
    }
}

#[cfg(test)]
mod insertion_tests {
    use super::*;
    #[test]
    fn cubic_subdivision_preserves_geometry_after_authoring_solve() {
        let path = MotionPath {
            nodes: vec![
                PathNode {
                    id: "a".into(),
                    point: Point::ZERO,
                    in_handle: Point::ZERO,
                    out_handle: Point { x: 30.0, y: 80.0 },
                    segment: PathSegment::Cubic,
                    tangent_mode: SpatialTangentMode::Auto,
                },
                PathNode {
                    id: "b".into(),
                    point: Point { x: 100.0, y: 0.0 },
                    in_handle: Point { x: -30.0, y: 80.0 },
                    out_handle: Point::ZERO,
                    segment: PathSegment::Line,
                    tangent_mode: SpatialTangentMode::Auto,
                },
            ],
        };
        let before = serde_json::to_string(&path).unwrap();
        for split in [0.1, 0.37, 0.9] {
            let inserted = path.insert_node(0, split, "new".into()).unwrap();
            assert!(inserted
                .nodes
                .iter()
                .all(|node| node.tangent_mode != SpatialTangentMode::Auto));
            let solved = inserted.solve().unwrap();
            for i in 0..=1000 {
                let t = i as f64 / 1000.0;
                let (segment, u) = if t <= split {
                    (0, t / split)
                } else {
                    (1, (t - split) / (1.0 - split))
                };
                let expected = path.point_at(0, t).unwrap();
                for candidate in [&inserted, &solved] {
                    assert!(candidate.point_at(segment, u).unwrap().distance(expected) < 1e-10);
                }
            }
        }
        assert_eq!(serde_json::to_string(&path).unwrap(), before);
        assert!(path.insert_node(0, 0.0, "new".into()).is_err());
        assert!(path.insert_node(0, 0.5, "a".into()).is_err());
        assert!(path.insert_node(1, 0.5, "new".into()).is_err());
    }
    #[test]
    fn line_insertion_has_zero_controls_and_exact_affine_point() {
        let node = |id: &str, x| PathNode {
            id: id.into(),
            point: Point { x, y: 0.0 },
            in_handle: Point::ZERO,
            out_handle: Point::ZERO,
            segment: PathSegment::Line,
            tangent_mode: SpatialTangentMode::Corner,
        };
        let path = MotionPath {
            nodes: vec![node("a", 0.0), node("b", 100.0)],
        };
        let inserted = path.insert_node(0, 0.25, "new".into()).unwrap();
        assert_eq!(inserted.nodes[1].point, Point { x: 25.0, y: 0.0 });
        assert_eq!(inserted.nodes[1].in_handle, Point::ZERO);
        assert_eq!(inserted.compile().unwrap().length(), 100.0);
    }
}

/// Mutually exclusive scalar XY or distance-driven spatial path position.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum PositionAnimation {
    Xy {
        x: super::composition::AnimatableValue,
        y: super::composition::AnimatableValue,
    },
    Path {
        path: MotionPath,
        progress: super::composition::AnimatableValue,
    },
}
#[derive(Clone, Debug)]
pub enum CompiledPositionAnimation {
    Xy {
        x: super::composition::AnimatableValue,
        y: super::composition::AnimatableValue,
    },
    Path {
        path: CompiledMotionPath,
        progress: super::composition::AnimatableValue,
    },
}
impl PositionAnimation {
    pub fn compile(&self) -> Result<CompiledPositionAnimation, String> {
        use super::composition::AnimatableValue;
        let prepare = |value: &AnimatableValue| -> Result<AnimatableValue, String> {
            value.validate().map_err(|error| error.to_string())?;
            let mut admitted = value.clone();
            if let AnimatableValue::Keyframes { track } = &mut admitted {
                track.keyframes.sort_by_key(|key| key.tick);
            }
            Ok(admitted)
        };
        match self {
            Self::Xy { x, y } => Ok(CompiledPositionAnimation::Xy {
                x: prepare(x)?,
                y: prepare(y)?,
            }),
            Self::Path { path, progress } => Ok(CompiledPositionAnimation::Path {
                path: path.compile()?,
                progress: prepare(progress)?,
            }),
        }
    }
}
impl CompiledPositionAnimation {
    pub fn sample_seconds(&self, local_seconds: f64) -> Result<Point, String> {
        use super::composition::AnimatableValue;
        if !local_seconds.is_finite() {
            return Err("Position time must be finite".into());
        }
        let sample = |value: &AnimatableValue| match value {
            AnimatableValue::Constant { value } => Ok(*value),
            AnimatableValue::Keyframes { track } => track
                .sample_seconds(local_seconds)
                .map_err(|error| error.to_string()),
        };
        self.sample_with(sample)
    }
    /// Preserve integer/source time policies before rounding a clocked progress/XY track.
    pub fn sample_at(&self, time: super::animation_sample_time::AnimationSampleTime) -> Result<Point, String> {
        use super::composition::AnimatableValue;
        self.sample_with(|value| match value {
            AnimatableValue::Constant { value } => Ok(*value),
            AnimatableValue::Keyframes { track } => time.sample_track(track).map_err(|error|error.to_string()),
        })
    }
    fn sample_with(&self, sample: impl Fn(&super::composition::AnimatableValue) -> Result<f64, String>) -> Result<Point, String> {
        let point = match self {
            Self::Xy { x, y } => Point {
                x: sample(x)?,
                y: sample(y)?,
            },
            Self::Path { path, progress } => path.sample(sample(progress)?),
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("Sampled position exceeds finite range".into());
        }
        Ok(point)
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;
    use crate::domain::{
        composition::AnimatableValue,
        keyframes::{Interpolation, Keyframe, KeyframeTrack, ProceduralInterpolation},
    };
    #[test]
    fn temporal_easing_drives_distance_with_endpoint_extrapolation() {
        let nodes = [("a", 0.0, 0.0), ("b", 10.0, 0.0), ("c", 10.0, 90.0)]
            .into_iter()
            .map(|(id, x, y)| PathNode {
                id: id.into(),
                point: Point { x, y },
                in_handle: Point::ZERO,
                out_handle: Point::ZERO,
                segment: PathSegment::Line,
                tangent_mode: SpatialTangentMode::Corner,
            })
            .collect();
        let keys = vec![
            Keyframe {
                tick: 0,
                value: 0.0,
                interpolation: None,
                tangents: None,
            },
            Keyframe {
                tick: 1000,
                value: 1.0,
                interpolation: None,
                tangents: None,
            },
        ];
        let track = KeyframeTrack::new(
            1000,
            Interpolation::Procedural(ProceduralInterpolation::Bezier {
                p1: [1.0 / 3.0, 0.0],
                p2: [2.0 / 3.0, 1.0 / 3.0],
            }),
            keys,
        )
        .unwrap();
        let position = PositionAnimation::Path {
            path: MotionPath { nodes },
            progress: AnimatableValue::Keyframes { track },
        };
        let compiled = position.compile().unwrap();
        let midpoint = compiled.sample_seconds(0.5).unwrap();
        assert!((midpoint.x - 10.0).abs() < 1e-10 && (midpoint.y - 15.0).abs() < 1e-10);
        assert_eq!(
            compiled.sample_seconds(1.0).unwrap(),
            Point { x: 10.0, y: 90.0 }
        );
        let PositionAnimation::Path { path, .. } = position else {
            panic!()
        };
        let overshoot = PositionAnimation::Path {
            path,
            progress: AnimatableValue::constant(1.1),
        }
        .compile()
        .unwrap();
        assert!((overshoot.sample_seconds(0.0).unwrap().y - 100.0).abs() < 1e-12);
    }
    #[test]
    fn strict_modes_static_xy_and_invalid_progress() {
        let xy = PositionAnimation::Xy {
            x: AnimatableValue::constant(3.0),
            y: AnimatableValue::constant(4.0),
        };
        assert_eq!(
            xy.compile().unwrap().sample_seconds(2.0).unwrap(),
            Point { x: 3.0, y: 4.0 }
        );
        let mut wire = serde_json::to_value(&xy).unwrap();
        wire["path"] = serde_json::json!({"nodes":[]});
        assert!(serde_json::from_value::<PositionAnimation>(wire).is_err());
        assert!(PositionAnimation::Xy {
            x: AnimatableValue::constant(f64::NAN),
            y: AnimatableValue::constant(4.0)
        }
        .compile()
        .is_err());
        assert!(xy.compile().unwrap().sample_seconds(f64::INFINITY).is_err());
    }
}

impl CompiledMotionPath {
    /// Balanced piecewise interpolation over the exact playback distance table.
    /// Register0 is local to this expression; temporal easing uses registers3–9.
    pub fn ffmpeg_expression(&self, progress: &str, axis: usize) -> Result<String, String> {
        if axis > 1 || progress.is_empty() {
            return Err("Invalid motion path expression input".into());
        }
        let first = self.samples[0][axis];
        let last = self.samples.last().unwrap()[axis];
        let total = self.length();
        if total == 0.0 {
            return Ok(format!("{first:.17}"));
        }
        let spans: Vec<_> = self
            .samples
            .windows(2)
            .filter(|pair| pair[1][2] > pair[0][2])
            .map(|pair| (pair[0], pair[1]))
            .collect();
        fn lookup(spans: &[([f64; 4], [f64; 4])], axis: usize) -> String {
            if spans.len() == 1 {
                let (a, b) = spans[0];
                return format!(
                    "({:.17}+({:.17})*(ld(0)-{:.17})/{:.17})",
                    a[axis],
                    b[axis] - a[axis],
                    a[2],
                    b[2] - a[2]
                );
            }
            let mid = spans.len() / 2;
            format!(
                "if(lte(ld(0),{:.17}),{},{})",
                spans[mid - 1].1[2],
                lookup(&spans[..mid], axis),
                lookup(&spans[mid..], axis)
            )
        }
        let start = if axis == 0 {
            self.start.x
        } else {
            self.start.y
        };
        let end = if axis == 0 { self.end.x } else { self.end.y };
        Ok(format!("st(0,({progress})*{total:.17});if(isnan(ld(0)),{first:.17},if(lte(ld(0),0),({first:.17}+{start:.17}*ld(0)),if(gte(ld(0),{total:.17}),({last:.17}+{end:.17}*(ld(0)-{total:.17})),{})))",lookup(&spans,axis)))
    }
}
#[cfg(test)]
mod ffmpeg_path_tests {
    use super::*;
    #[test]
    fn real_ffmpeg_distance_lookup_matches_native_sampling_with_overshoot() {
        let path = CompiledMotionPath::new(&[
            Node::ZERO,
            Node {
                point: Point { x: 10.0, y: 0.0 },
                ..Node::ZERO
            },
            Node {
                point: Point { x: 10.0, y: 90.0 },
                ..Node::ZERO
            },
        ])
        .unwrap();
        let cubic = CompiledMotionPath::new(&[
            Node {
                outgoing: Point { x: 30.0, y: 80.0 },
                cubic: true,
                ..Node::ZERO
            },
            Node {
                point: Point { x: 100.0, y: 0.0 },
                incoming: Point { x: -30.0, y: 80.0 },
                ..Node::ZERO
            },
        ])
        .unwrap();
        for path in [path, cubic] {
            for axis in [0, 1] {
                let expression = path.ffmpeg_expression("T-0.1", axis).unwrap();
                let filter = format!("format=gray,geq=lum='128+({expression})'");
                let output = std::process::Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        "color=black:s=16x16:r=100:d=1.21",
                        "-vf",
                        &filter,
                        "-frames:v",
                        "121",
                        "-pix_fmt",
                        "gray",
                        "-f",
                        "rawvideo",
                        "pipe:1",
                    ])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout.len(), 121 * 256);
                for i in 0..121 {
                    let expected = path.sample(i as f64 / 100.0 - 0.1);
                    let coordinate = if axis == 0 { expected.x } else { expected.y };
                    assert!(
                        (output.stdout[i * 256] as f64 - 128.0 - coordinate).abs() <= 1.01,
                        "sample {i}"
                    );
                }
            }
        }
    }
}

impl CompiledMotionPath {
    /// Conservative sampled-polyline bounds including endpoint-ray extension.
    pub fn coordinate_bounds(
        &self,
        minimum_progress: f64,
        maximum_progress: f64,
        axis: usize,
    ) -> Result<(f64, f64), String> {
        if axis > 1
            || !minimum_progress.is_finite()
            || !maximum_progress.is_finite()
            || minimum_progress > maximum_progress
        {
            return Err("Invalid path bounds interval".into());
        }
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for sample in &self.samples {
            low = low.min(sample[axis]);
            high = high.max(sample[axis]);
        }
        for progress in [minimum_progress, maximum_progress] {
            let point = self.sample(progress);
            let coordinate = if axis == 0 { point.x } else { point.y };
            if !coordinate.is_finite() {
                return Err("Motion path bounds exceed finite range".into());
            }
            low = low.min(coordinate);
            high = high.max(coordinate);
        }
        Ok((low, high))
    }
}
