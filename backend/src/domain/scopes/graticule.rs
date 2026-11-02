//! Native vectorscope graticule geometry adapted from Filmcraft ui-egui/panels/scopes.rs (MIT).
use super::*;
use serde_json::{json, Value};
/// Source SDR percentage grid; labels outside the plot are supplied separately.
pub fn levels(width: usize, height: usize, lo: f32, hi: f32) -> Vec<Value> {
    (0..=10)
        .map(|i| {
            let value = i as f32 / 10.0;
            let y = height as f32 - (value - lo) / (hi - lo) * height as f32;
            json!({"kind":"line","x1":0.0,"y1":y,"x2":width,"y2":y,"color":"#404040"})
        })
        .collect()
}
const NITS_LEVELS: [f32; 7] = [0.0, 10.0, 100.0, 203.0, 1000.0, 4000.0, 10000.0];
pub fn level_grid(width: usize, height: usize, lo: f32, hi: f32, scale: Scale) -> Vec<Value> {
    if scale != Scale::Hdr {
        return levels(width, height, lo, hi);
    }
    NITS_LEVELS
        .into_iter()
        .map(|nits| {
            let y = height as f32 - (nits_to_pq(nits) - lo) / (hi - lo) * height as f32;
            let color = if nits == 203.0 {
                "rgba(255,200,80,0.353)"
            } else {
                "#404040"
            };
            json!({"kind":"line","x1":0.0,"y1":y,"x2":width,"y2":y,"color":color})
        })
        .collect()
}
pub fn labels(lo: f32, hi: f32, scale: Scale) -> Vec<Value> {
    if scale == Scale::Hdr {
        return NITS_LEVELS.into_iter().map(|nits|json!({"position":(hi-nits_to_pq(nits))/(hi-lo)*100.0,"left":format!("{nits:.0}"),"right":""})).collect();
    }

    (0..=10)
        .map(|i| {
            let value = i as f32 / 10.0;
            let position = (hi - value) / (hi - lo) * 100.0;
            let right = if scale == Scale::Float {
                format!("{value:.1}")
            } else {
                format!("{}", (value * 255.0).round() as i32)
            };
            json!({"position":position,"left":format!("{}",i*10),"right":right})
        })
        .collect()
}
/// Parade dividers are painted after the density image in the upstream UI.
pub fn parade_dividers(width: usize, height: usize, kind: ParadeType) -> Vec<Value> {
    let channels = if kind == ParadeType::RgbWhite { 4 } else { 3 };
    (1..channels)
        .map(|i| {
            let x = width as f32 * i as f32 / channels as f32;
            json!({"kind":"line","x1":x,"y1":0.0,"x2":x,"y2":height,"color":"#282828"})
        })
        .collect()
}
pub fn vector(size: usize, matrix: Matrix, selected: Targets, hls: bool) -> Vec<Value> {
    let center = size as f32 / 2.0;
    let scale = center / VECTOR_EXTENT;
    let ring = 0.5 * scale;
    let at = |cb: f32, cr: f32| [center + cb * scale, center - cr * scale];
    let mut out = vec![];
    for r in [ring, ring * 0.5] {
        out.push(json!({"kind":"circle","cx":center,"cy":center,"r":r,"color":"#404040"}));
    }
    let mut line = |a: [f32; 2], b: [f32; 2], color: &str| {
        out.push(json!({"kind":"line","x1":a[0],"y1":a[1],"x2":b[0],"y2":b[1],"color":color}))
    };
    line([center - ring, center], [center + ring, center], "#404040");
    line([center, center - ring], [center, center + ring], "#404040");
    for d in (0..360).step_by(10) {
        let a = (d as f32).to_radians();
        let l = if d % 30 == 0 { 0.94 } else { 0.97 };
        line(
            [center + a.cos() * ring * l, center - a.sin() * ring * l],
            [center + a.cos() * ring, center - a.sin() * ring],
            "#404040",
        );
    }
    if !hls {
        let a = SKIN_TONE_DEG.to_radians();
        line(
            [center, center],
            [center + a.cos() * ring, center - a.sin() * ring],
            "rgba(220,170,120,0.43)",
        );
    }
    if hls {
        for (name, cb, cr) in hls_targets(matrix) {
            let p = at(cb * 1.12, cr * 1.12);
            out.push(json!({"kind":"text","x":p[0],"y":p[1],"text":name,"color":"#aaa"}));
        }
    } else {
        for (amp, strong) in [
            (0.75, selected == Targets::Percent75),
            (1.0, selected == Targets::Percent100),
        ] {
            for target in targets(matrix, amp) {
                let p = at(target.cb, target.cr);
                let size = if strong { 9.0 } else { 5.0 };
                let rgb = target.rgb.map(|v| (v / amp * 200.0) as u8 + 40);
                let alpha = if strong { 1.0 } else { 0.45 };
                let color = format!("rgba({},{},{},{alpha})", rgb[0], rgb[1], rgb[2]);
                out.push(json!({"kind":"rect","x":p[0]-size/2.0,"y":p[1]-size/2.0,"width":size,"height":size,"color":color}));
                if strong {
                    let length = (target.cb * target.cb + target.cr * target.cr).sqrt();
                    out.push(json!({"kind":"text","x":p[0]+target.cb/length*12.0,"y":p[1]-target.cr/length*12.0,"text":target.name,"color":color}));
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_use_the_signal_range_instead_of_clamping_to_plot() {
        let unit = levels(480, 256, 0.0, 1.0);
        assert_eq!(unit.len(), 11);
        let bits = labels(0.0, 1.0, Scale::Bits8);
        assert_eq!(bits[5]["left"], "50");
        assert_eq!(bits[5]["right"], "128");
        assert_eq!(bits[5]["position"], 50.0);
        assert_eq!(labels(0.0, 1.0, Scale::Float)[10]["right"], "1.0");
        assert_eq!(unit[0]["y1"], 256.0);
        assert_eq!(unit[5]["y1"], 128.0);
        assert_eq!(unit[10]["y1"], 0.0);
        let extended = levels(480, 256, -0.25, 1.25);
        assert!((extended[0]["y1"].as_f64().unwrap() - 213.33333).abs() < 0.001);
        assert!((extended[10]["y1"].as_f64().unwrap() - 42.66667).abs() < 0.001);
    }
    #[test]
    fn hdr_axis_uses_source_nits_levels_and_reference_white_mark() {
        let labels = labels(0.0, 1.0, Scale::Hdr);
        assert_eq!(labels.len(), 7);
        assert_eq!(labels[3]["left"], "203");
        assert_eq!(labels[6]["left"], "10000");
        assert!((labels[2]["position"].as_f64().unwrap() - 49.19216).abs() < 0.01);
        let grid = level_grid(480, 256, 0.0, 1.0, Scale::Hdr);
        assert_eq!(grid.len(), 7);
        assert_eq!(grid[3]["color"], "rgba(255,200,80,0.353)");
        assert!((sdr_to_pq(1.0) - nits_to_pq(100.0)).abs() < 1e-6);
    }
    #[test]
    fn parade_dividers_follow_channel_count_and_paint_width() {
        let rgb = parade_dividers(480, 256, ParadeType::Rgb);
        assert_eq!(rgb.len(), 2);
        assert_eq!(rgb[0]["x1"], 160.0);
        assert_eq!(rgb[1]["x1"], 320.0);
        assert_eq!(parade_dividers(480, 256, ParadeType::Yuv), rgb);
        let white = parade_dividers(480, 256, ParadeType::RgbWhite);
        assert_eq!(white.len(), 3);
        assert_eq!(white[0]["x1"], 120.0);
        assert_eq!(white[2]["x1"], 360.0);
        assert_eq!(white[0]["y2"], 256);
    }
    #[test]
    fn targets_and_skin_line_follow_source_geometry() {
        let yuv = vector(255, Matrix::Bt709, Targets::Percent75, false);
        assert_eq!(yuv.iter().filter(|v| v["kind"] == "rect").count(), 12);
        assert_eq!(yuv.iter().filter(|v| v["kind"] == "text").count(), 6);
        let skin = yuv
            .iter()
            .find(|v| v["color"] == "rgba(220,170,120,0.43)")
            .unwrap();
        assert!(skin["x2"].as_f64().unwrap() < 127.5 && skin["y2"].as_f64().unwrap() < 127.5);
        let red = yuv
            .iter()
            .find(|v| v["kind"] == "rect" && v["width"] == 9.0 && v["color"] == "rgba(240,40,40,1)")
            .unwrap();
        assert!((red["y"].as_f64().unwrap() - 43.3125).abs() < 1e-4);
        let full = vector(255, Matrix::Bt709, Targets::Percent100, false);
        let red = full
            .iter()
            .find(|v| v["kind"] == "rect" && v["width"] == 9.0 && v["color"] == "rgba(240,40,40,1)")
            .unwrap();
        assert!((red["y"].as_f64().unwrap() - 16.75).abs() < 1e-4);
        let hls = vector(255, Matrix::Bt709, Targets::Percent100, true);
        assert_eq!(hls.len(), 46);
        assert!(!hls.iter().any(|v| v["kind"] == "rect"));
    }
}
