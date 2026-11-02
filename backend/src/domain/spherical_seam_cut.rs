//! Bounded binary cut over the body-sphere overlap. Labels choose a lens;
//! unavailable coverage is a hard constraint, never an invented pixel.
use anyhow::Result;
use std::collections::VecDeque;
#[derive(Clone, Copy, Default)]
pub(super) struct Observation {
    pub color: [f32; 3],
    pub priority: f64,
    pub valid: bool,
}
#[derive(Clone)]
struct Edge {
    to: usize,
    reverse: usize,
    capacity: u64,
}
struct Graph {
    edges: Vec<Vec<Edge>>,
}
impl Graph {
    fn new(n: usize) -> Self {
        Self {
            edges: vec![Vec::new(); n],
        }
    }
    fn edge(&mut self, a: usize, b: usize, capacity: u64, reverse_capacity: u64) {
        let (ra, rb) = (self.edges[a].len(), self.edges[b].len());
        self.edges[a].push(Edge {
            to: b,
            reverse: rb,
            capacity,
        });
        self.edges[b].push(Edge {
            to: a,
            reverse: ra,
            capacity: reverse_capacity,
        });
    }
}
struct Budget<'a> {
    work: usize,
    checkpoint: &'a mut dyn FnMut() -> Result<()>,
}
impl Budget<'_> {
    fn tick(&mut self) -> Result<bool> {
        self.work += 1;
        if self.work & 4095 == 0 {
            (self.checkpoint)()?;
        }
        Ok(self.work <= 40_000_000)
    }
}
// Iterative Dinic: no recursion proportional to the number of image pixels.
fn cut(
    graph: &mut Graph,
    source: usize,
    sink: usize,
    budget: &mut Budget<'_>,
) -> Result<Option<Vec<bool>>> {
    let n = graph.edges.len();
    loop {
        let mut level = vec![usize::MAX; n];
        level[source] = 0;
        let mut queue = VecDeque::from([source]);
        while let Some(v) = queue.pop_front() {
            for edge in &graph.edges[v] {
                if !budget.tick()? {
                    return Ok(None);
                }
                if edge.capacity > 0 && level[edge.to] == usize::MAX {
                    level[edge.to] = level[v] + 1;
                    queue.push_back(edge.to);
                }
            }
        }
        if level[sink] == usize::MAX {
            return Ok(Some(level.iter().map(|v| *v != usize::MAX).collect()));
        }
        let mut next = vec![0usize; n];
        let mut nodes = vec![source];
        let mut path: Vec<(usize, usize)> = Vec::new();
        while let Some(&v) = nodes.last() {
            if !budget.tick()? {
                return Ok(None);
            }
            if v == sink {
                let amount = path
                    .iter()
                    .map(|(a, e)| graph.edges[*a][*e].capacity)
                    .min()
                    .unwrap();
                let mut first_saturated = path.len();
                for (i, &(a, e)) in path.iter().enumerate() {
                    if !budget.tick()? {
                        return Ok(None);
                    }
                    let (b, reverse) = (graph.edges[a][e].to, graph.edges[a][e].reverse);
                    graph.edges[a][e].capacity -= amount;
                    graph.edges[b][reverse].capacity += amount;
                    if graph.edges[a][e].capacity == 0 {
                        first_saturated = first_saturated.min(i);
                    }
                }
                let (a, e) = path[first_saturated];
                next[a] = e + 1;
                path.truncate(first_saturated);
                nodes.truncate(first_saturated + 1);
                continue;
            }
            while next[v] < graph.edges[v].len() {
                if !budget.tick()? {
                    return Ok(None);
                }
                let e = &graph.edges[v][next[v]];
                if e.capacity > 0 && level[e.to] == level[v] + 1 {
                    break;
                }
                next[v] += 1;
            }
            if next[v] == graph.edges[v].len() {
                level[v] = usize::MAX;
                nodes.pop();
                if let Some((parent, edge)) = path.pop() {
                    next[parent] = edge + 1;
                }
            } else {
                let edge = next[v];
                let to = graph.edges[v][edge].to;
                path.push((v, edge));
                nodes.push(to);
            }
        }
    }
}
fn disagreement(a: Observation, b: Observation) -> f64 {
    if !a.valid || !b.valid {
        return 0.;
    }
    (0..3)
        .map(|k| (a.color[k] - b.color[k]).abs() as f64)
        .sum::<f64>()
        / 3.
}
pub(super) fn solve(
    images: &[Vec<Observation>; 2],
    w: usize,
    h: usize,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Option<Vec<f32>>> {
    if w < 2
        || h < 2
        || w.checked_mul(h)
            .filter(|n| *n <= 256 * 128 && *n > 0)
            .is_none()
        || images.iter().any(|p| p.len() != w * h)
    {
        return Ok(None);
    }
    let n = w * h;
    // Eliminate hard labels exactly. Edges to single-lens coverage become unary
    // costs on the adjacent overlap vertex; large fixed hemisphere interiors
    // do not belong in the flow network or consume augmentation work.
    let mut vertex = vec![usize::MAX; n];
    let mut pixels = Vec::new();
    let mut ownership = vec![0.5; n];
    for p in 0..n {
        match (images[0][p].valid, images[1][p].valid) {
            (true, true) => {
                vertex[p] = pixels.len();
                pixels.push(p);
            }
            (true, false) => ownership[p] = 1.,
            (false, true) => ownership[p] = 0.,
            _ => {}
        }
    }
    if pixels.is_empty() {
        return Ok(Some(ownership));
    }
    let source = pixels.len();
    let sink = source + 1;
    let mut graph = Graph::new(source + 2);
    let mut costs = vec![[0u64; 2]; source];
    let mut budget = Budget {
        work: 0,
        checkpoint,
    };
    for (v, &p) in pixels.iter().enumerate() {
        let (a, b) = (images[0][p], images[1][p]);
        let sum = a.priority + b.priority;
        let preference = if sum > 1e-12 { a.priority / sum } else { 0.5 };
        costs[v] = [
            ((1. - preference) * 0.002 * 4096.).round() as u64,
            (preference * 0.002 * 4096.).round() as u64,
        ];
    }
    for y in 0..h {
        (budget.checkpoint)()?;
        for x in 0..w {
            let p = y * w + x;
            let (a, b) = (images[0][p], images[1][p]);
            for q in [
                Some(y * w + (x + 1) % w),
                (y + 1 < h).then_some((y + 1) * w + x),
            ]
            .into_iter()
            .flatten()
            {
                if !(a.valid || b.valid) || !(images[0][q].valid || images[1][q].valid) {
                    continue;
                }
                let gradient = if a.valid && b.valid && images[0][q].valid && images[1][q].valid {
                    (0..3)
                        .map(|k| {
                            ((a.color[k] - images[0][q].color[k])
                                - (b.color[k] - images[1][q].color[k]))
                                .abs() as f64
                        })
                        .sum::<f64>()
                        / 3.
                } else {
                    0.
                };
                let c = ((0.02
                    + 4. * (disagreement(a, b) + disagreement(images[0][q], images[1][q]))
                    + 2. * gradient)
                    * 4096.)
                    .round() as u64;
                match (vertex[p], vertex[q]) {
                    (v, u) if v != usize::MAX && u != usize::MAX => graph.edge(v, u, c, c),
                    (v, _) if v != usize::MAX => {
                        costs[v][if ownership[q] > 0.5 { 1 } else { 0 }] += c
                    }
                    (_, u) if u != usize::MAX => {
                        costs[u][if ownership[p] > 0.5 { 1 } else { 0 }] += c
                    }
                    _ => {}
                }
            }
        }
    }
    for (v, c) in costs.into_iter().enumerate() {
        graph.edge(source, v, c[1], 0);
        graph.edge(v, sink, c[0], 0);
    }
    let Some(labels) = cut(&mut graph, source, sink, &mut budget)? else {
        return Ok(None);
    };
    for (v, p) in pixels.into_iter().enumerate() {
        ownership[p] = if labels[v] { 1. } else { 0. };
    }
    Ok(Some(ownership))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iterative_cut_matches_exhaustive_binary_energy() {
        for seed in 0..32u64 {
            let unary: Vec<_> = (0..5)
                .map(|i| {
                    (
                        ((seed * 13 + i * 7) % 11) + 1,
                        ((seed * 3 + i * 5) % 13) + 1,
                    )
                })
                .collect();
            let pairs = [
                (0, 1, 4),
                (1, 2, 7),
                (2, 3, 2),
                (3, 4, 6),
                (0, 4, 3),
                (1, 3, 5),
            ];
            let mut graph = Graph::new(7);
            for (i, (a, b)) in unary.iter().enumerate() {
                graph.edge(5, i, *b, 0);
                graph.edge(i, 6, *a, 0);
            }
            for (a, b, c) in pairs {
                graph.edge(a, b, c, c);
            }
            let labels = cut(
                &mut graph,
                5,
                6,
                &mut Budget {
                    work: 0,
                    checkpoint: &mut || Ok(()),
                },
            )
            .unwrap()
            .unwrap();
            let energy = |mask: u32| {
                let mut e = 0;
                for (i, (a, b)) in unary.iter().enumerate() {
                    e += if mask & (1 << i) != 0 { *a } else { *b };
                }
                for (a, b, c) in pairs {
                    if (mask >> a) & 1 != (mask >> b) & 1 {
                        e += c;
                    }
                }
                e
            };
            let mask = (0..5).fold(0, |m, i| m | ((labels[i] as u32) << i));
            assert_eq!(energy(mask), (0..32).map(energy).min().unwrap());
        }
    }
}
