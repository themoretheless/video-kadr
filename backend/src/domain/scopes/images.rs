// Source Filmcraft scopes image composition; MIT notice docs/licenses/filmcraft-MIT.txt.
use super::WaveformType;
const RED: [f32; 3] = [1.0, 0.16, 0.16];
const GREEN: [f32; 3] = [0.16, 1.0, 0.16];
const BLUE: [f32; 3] = [0.3, 0.38, 1.0];
const WHITE: [f32; 3] = [0.85, 0.85, 0.85];
const LUMA: [f32; 3] = [0.55, 1.0, 0.7];

pub(super) fn trace_color(name: &str, wt: Option<WaveformType>) -> [f32; 3] {
    match (name, wt) {
        ("R", _) => RED,
        ("G", _) => GREEN,
        ("B", _) | ("Cb", _) | ("C", _) => BLUE,
        ("Cr", _) => [1.0, 0.45, 0.45],
        ("Y", Some(WaveformType::Luma)) => WHITE,
        ("Y", Some(_)) => LUMA,
        _ => WHITE,
    }
}
pub(super) fn parade_image(w: &super::Waveform, gain: f32) -> (usize, usize, Vec<u8>) {
    let n = w.traces.len();
    let Some(first) = w.traces.first() else {
        return (0, 0, Vec::new());
    };
    let (cw, rows) = (first.cols, first.rows);
    let gap = (cw / 24).max(2);
    let width = cw * n + gap * (n - 1);
    let mut out = vec![0u8; width * rows * 4];
    for (i, g) in w.traces.iter().enumerate() {
        let (_, _, px) = super::paint::grids(
            &[(g, trace_color(g.name, None))],
            super::paint::waveform_reference(w.per_column),
            gain,
        );
        let x0 = i * (cw + gap);
        for y in 0..rows {
            let src = &px[y * cw * 4..(y + 1) * cw * 4];
            out[(y * width + x0) * 4..(y * width + x0 + cw) * 4].copy_from_slice(src);
        }
    }
    (width, rows, out)
}
pub(super) fn histogram_image(h: &super::Histogram, gain: f32) -> (usize, usize, Vec<u8>) {
    let (w, rows) = (256usize, super::BINS);
    let max =
        h.r.iter()
            .chain(&h.g)
            .chain(&h.b)
            .copied()
            .max()
            .unwrap_or(0)
            .max(1) as f32;
    let len = |c: u32| ((c as f32 / max).sqrt() * w as f32).round() as usize;
    let mut out = vec![0u8; w * rows * 4];
    for bin in 0..rows {
        let y = rows - 1 - bin;
        let (lr, lg, lb) = (len(h.r[bin]), len(h.g[bin]), len(h.b[bin]));
        for x in 0..w {
            let c = [
                if x < lr { RED } else { [0.0; 3] },
                if x < lg { GREEN } else { [0.0; 3] },
                if x < lb { BLUE } else { [0.0; 3] },
            ];
            let s = [
                c[0][0] + c[1][0] + c[2][0],
                c[0][1] + c[1][1] + c[2][1],
                c[0][2] + c[1][2] + c[2][2],
            ]
            .map(|v| (v * 0.75 * gain).min(1.0));
            let a = s[0].max(s[1]).max(s[2]);
            if a > 0.0 {
                let o = &mut out[(y * w + x) * 4..(y * w + x) * 4 + 4];
                o.copy_from_slice(&[
                    (s[0] * 255.0) as u8,
                    (s[1] * 255.0) as u8,
                    (s[2] * 255.0) as u8,
                    (a * 255.0) as u8,
                ]);
            }
        }
    }
    (w, rows, out)
}
