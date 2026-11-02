//! Scalar/layout negotiation for linear Concat audio graphs. No sample processing.
//! Capabilities and preference rules track the qualified FFmpeg n9.0.2 graph.
use anyhow::{ensure, Result};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    U8,
    S16,
    S32,
    F32,
    F64,
    U8P,
    S16P,
    S32P,
    F32P,
    F64P,
    S64,
    S64P,
}
use Format::*;
const ALL: &[Format] = &[
    U8, S16, S32, F32, F64, U8P, S16P, S32P, F32P, F64P, S64, S64P,
];
const PLANAR: &[Format] = &[S16P, S32P, F32P, F64P];
impl Format {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        ALL.iter().copied().find(|f| f.name() == name)
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            U8 => "u8",
            S16 => "s16",
            S32 => "s32",
            F32 => "flt",
            F64 => "dbl",
            U8P => "u8p",
            S16P => "s16p",
            S32P => "s32p",
            F32P => "fltp",
            F64P => "dblp",
            S64 => "s64",
            S64P => "s64p",
        }
    }
    pub(crate) fn packed(self) -> Self {
        match self {
            U8P => U8,
            S16P => S16,
            S32P => S32,
            F32P => F32,
            F64P => F64,
            S64P => S64,
            v => v,
        }
    }
    pub(crate) fn layout(self, planar: bool) -> Self {
        if !planar {
            return self.packed();
        }
        match self.packed() {
            U8 => U8P,
            S16 => S16P,
            S32 => S32P,
            F32 => F32P,
            F64 => F64P,
            S64 => S64P,
            _ => unreachable!(),
        }
    }
    fn planar(self) -> bool {
        self != self.packed()
    }
    fn bytes(self) -> i32 {
        match self.packed() {
            U8 => 1,
            S16 => 2,
            S32 | F32 => 4,
            _ => 8,
        }
    }
    fn score(self, source: Self) -> i32 {
        let delta = self.bytes() - source.bytes();
        i32::from(self.planar() != source.planar())
            + if delta < 0 { -delta * 100 } else { delta * 10 }
            + if self.packed() == S32 && source.packed() == F32 {
                20
            } else if self.packed() == F32 && source.packed() == S32 {
                2
            } else {
                0
            }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Filter {
    Resample,
    Pass,
    Tempo,
    Biquad,
    Echo,
    Limiter,
    Compand,
    Deesser,
    Denoise,
    Crusher,
    Compressor,
    Phaser,
    Flanger,
    Volume,
    Loudness,
}
impl Filter {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "aresample" => Self::Resample,
            "asetrate" => Self::Pass,
            "atempo" => Self::Tempo,
            "highpass" | "lowpass" | "equalizer" | "treble" | "bass" => Self::Biquad,
            "aecho" => Self::Echo,
            "alimiter" => Self::Limiter,
            "compand" => Self::Compand,
            "deesser" => Self::Deesser,
            "afftdn" => Self::Denoise,
            "acrusher" => Self::Crusher,
            "acompressor" => Self::Compressor,
            "aphaser" => Self::Phaser,
            "flanger" => Self::Flanger,
            "volume" => Self::Volume,
            "loudnorm" => Self::Loudness,
            _ => return None,
        })
    }
    fn formats(self) -> &'static [Format] {
        match self {
            Self::Resample | Self::Pass => ALL,
            Self::Tempo => &[U8, S16, S32, F32, F64],
            Self::Biquad | Self::Echo => PLANAR,
            Self::Limiter | Self::Crusher | Self::Compressor | Self::Loudness => &[F64],
            Self::Compand | Self::Deesser | Self::Flanger => &[F64P],
            Self::Denoise => &[F32P, F64P],
            Self::Volume => &[F32, F32P],
            Self::Phaser => &[F64, F64P, F32, F32P, S32, S32P, S16, S16P],
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stage {
    pub input: Format,
    pub output: Format,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Conversion {
    pub before: usize,
    pub input: Format,
    pub output: Format,
}
#[derive(Debug)]
pub(crate) struct Plan {
    pub stages: Vec<Stage>,
    pub conversions: Vec<Conversion>,
}
struct Sets {
    parent: Vec<usize>,
    values: Vec<Vec<Format>>,
}
impl Sets {
    fn add(&mut self, values: &[Format]) -> usize {
        let i = self.parent.len();
        self.parent.push(i);
        self.values.push(values.to_vec());
        i
    }
    fn root(&self, mut i: usize) -> usize {
        while self.parent[i] != i {
            i = self.parent[i];
        }
        i
    }
    fn get(&self, i: usize) -> &[Format] {
        &self.values[self.root(i)]
    }
    fn set(&mut self, i: usize, values: Vec<Format>) {
        let i = self.root(i);
        self.values[i] = values;
    }
    fn merge(&mut self, a: usize, b: usize) -> bool {
        let a = self.root(a);
        let b = self.root(b);
        if a == b {
            return true;
        }
        let values: Vec<_> = self.values[a]
            .iter()
            .copied()
            .filter(|f| self.values[b].contains(f))
            .collect();
        if values.is_empty() {
            return false;
        }
        self.parent[b] = a;
        self.values[a] = values;
        true
    }
}
#[derive(Clone, Copy)]
struct Node {
    input: usize,
    output: usize,
}
impl Plan {
    /// Default Concat filter precisions, one linear input/output, no channel/rate
    /// negotiation. Explicit precision overrides require separate capabilities.
    pub(crate) fn build(source: Format, filters: &[Filter], sink: Format) -> Result<Self> {
        ensure!(filters.len() <= 256, "audio format graph exceeds budget");
        let mut sets = Sets {
            parent: vec![],
            values: vec![],
        };
        let source = sets.add(&[source]);
        let mut nodes = Vec::new();
        let mut converters = Vec::new();
        let mut previous = source;
        for (index, &filter) in filters.iter().enumerate() {
            let input = sets.add(filter.formats());
            let output = if matches!(filter, Filter::Resample) {
                sets.add(ALL)
            } else {
                input
            };
            if !sets.merge(previous, input) {
                converters.push((
                    index,
                    Node {
                        input: previous,
                        output: input,
                    },
                ));
            }
            nodes.push(Node { input, output });
            previous = output;
        }
        let sink = sets.add(&[sink]);
        if !sets.merge(previous, sink) {
            converters.push((
                filters.len(),
                Node {
                    input: previous,
                    output: sink,
                },
            ));
        }
        // Auto converters are appended to the original node list in graph order.
        let mut all = nodes.clone();
        all.extend(converters.iter().map(|(_, n)| *n));
        loop {
            let mut changed = false;
            for node in &all {
                if sets.get(node.input).len() == 1 && sets.get(node.output).len() > 1 {
                    let format = sets.get(node.input)[0];
                    if sets.get(node.output).contains(&format) {
                        sets.set(node.output, vec![format]);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        // Reorder admissible choices before final picking, including lossless
        // packed/planar counterparts and the source graph's width preference.
        for node in &all {
            if sets.get(node.input).len() != 1 || sets.get(node.output).len() < 2 {
                continue;
            }
            let input = sets.get(node.input)[0];
            let values = sets.get(node.output);
            let mut best = 0;
            let mut score = i32::MIN;
            for (i, &output) in values.iter().enumerate() {
                if output.packed() == input.packed() || (input.bytes() == 4 && output.bytes() == 8)
                {
                    best = i;
                    break;
                }
                let mut current = -(output.bytes() - input.bytes()).abs();
                if output.bytes() >= input.bytes() {
                    current += i32::MAX / 2;
                }
                if current > score {
                    score = current;
                    best = i;
                }
            }
            let mut values = values.to_vec();
            values.swap(0, best);
            sets.set(node.output, values);
        }
        loop {
            let mut changed = false;
            for node in &all {
                if sets.get(node.input).len() == 1 && sets.get(node.output).len() > 1 {
                    let input = sets.get(node.input)[0];
                    let mut best = sets.get(node.output)[0];
                    for &candidate in &sets.get(node.output)[1..] {
                        if candidate.score(input) <= best.score(input) {
                            best = candidate;
                        }
                    }
                    sets.set(node.output, vec![best]);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let chosen = |id| sets.get(id)[0];
        Ok(Self {
            stages: nodes
                .iter()
                .map(|n| Stage {
                    input: chosen(n.input),
                    output: chosen(n.output),
                })
                .collect(),
            conversions: converters
                .iter()
                .map(|(before, n)| Conversion {
                    before: *before,
                    input: chosen(n.input),
                    output: chosen(n.output),
                })
                .collect(),
        })
    }
}
