//! Bounded SDR raster analysis for native Scopes API.
use super::*;
use crate::domain::{basic_grade, group_surface::GroupSurface};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub scopes: Vec<ScopeKind>,
    pub matrix: Matrix,
    pub color_space: Option<ColorSpace>,
    pub clamp: bool,
    pub rows: usize,
    pub vector_size: usize,
    pub waveform_type: WaveformType,
    pub parade_type: ParadeType,
    pub brightness: Brightness,
    pub columns: usize,
    pub peaks: usize,
    pub targets: Targets,
    pub scale: Scale,
    pub layout_width: usize,
    pub layout_height: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            scopes: ScopeKind::ALL.to_vec(),
            matrix: Matrix::Bt709,
            color_space: None,
            clamp: true,
            rows: 256,
            vector_size: 255,
            waveform_type: WaveformType::Rgb,
            parade_type: ParadeType::Rgb,
            brightness: Brightness::Normal,
            columns: 8,
            peaks: 8,
            targets: Targets::default(),
            scale: Scale::Bits8,
            layout_width: 600,
            layout_height: 440,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeImage {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}
fn image((width, height, mut rgba): (usize, usize, Vec<u8>)) -> ScopeImage {
    // Native paint outputs premultiplied pixels; Canvas ImageData consumes straight RGBA.
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel[3] > 0 {
            for c in 0..3 {
                pixel[c] = ((pixel[c] as f32 * 255.0 / pixel[3] as f32)
                    .round()
                    .min(255.0)) as u8;
            }
        }
    }
    ScopeImage {
        width,
        height,
        rgba,
    }
}
pub fn analyze(
    frame: &GroupSurface,
    options: &Options,
    checkpoint: &mut dyn FnMut() -> Result<()>,
) -> Result<Value> {
    frame.validate()?;
    // This endpoint receives the current SDR display raster; native HDR input is separate.
    let matrix = options
        .color_space
        .map(|space| space.resolve(false).matrix())
        .unwrap_or(options.matrix);
    ensure!(
        (1..=16384).contains(&options.layout_width) && (1..=16384).contains(&options.layout_height),
        "scope layout dimensions must be 1..16384"
    );
    ensure!(
        !options.scopes.is_empty() && options.scopes.len() <= 5,
        "choose 1..5 scopes"
    );
    let mut selected = std::collections::HashSet::new();
    ensure!(
        options.scopes.iter().all(|k| selected.insert(*k)),
        "duplicate scopes"
    );
    ensure!(
        (64..=256).contains(&options.rows),
        "scope rows must be64..256"
    );
    ensure!(
        (3..=255).contains(&options.vector_size) && options.vector_size % 2 == 1,
        "vectorscope size must be odd3..255"
    );
    ensure!(
        (1..=512).contains(&options.columns) && (1..=64).contains(&options.peaks),
        "invalid scope summary limits"
    );
    checkpoint()?;
    let mut signal = Signal::from_rgba8(frame.width, frame.height, &frame.rgba);
    let (sx, sy) = super::steps(frame.width, frame.height, MAX_W, MAX_H);
    for y in 0..signal.h {
        checkpoint()?;
        for x in 0..signal.w {
            let a = frame.rgba[((y * sy) * frame.width + x * sx) * 4 + 3] as f32 / 255.0;
            signal.rgb[y * signal.w + x] = signal.rgb[y * signal.w + x].map(|v| {
                (basic_grade::display(basic_grade::linear(v) * a).clamp(0.0, 1.0) * 255.0).round()
                    / 255.0
            });
        }
    }
    let params = Params {
        matrix: matrix,
        clamp: options.clamp,
        rows: options.rows,
        vector_size: options.vector_size,
    };
    let resolved_space = options
        .color_space
        .map(|space| space.resolve(false))
        .unwrap_or(match matrix {
            Matrix::Bt601 => ColorSpace::Rec601,
            Matrix::Bt2020Ncl => ColorSpace::Rec2100,
            _ => ColorSpace::Rec709,
        });
    let mut out = json!({"resolvedColorSpace":resolved_space,"layout":super::layout::choose(options.layout_width as f32,options.layout_height as f32,options.scopes.len()),"samples":[signal.w,signal.h],"matrix":matrix,"clamp":options.clamp,"unit":"%","stats":summary::channel_stats(&signal,matrix)});
    // Source plotted(): SDR levels on an HDR axis use BT.1886, 100-nit white.
    // Vectorscopes and reported SDR channel statistics retain the original signal.
    let pq_signal = if options.scale == Scale::Hdr {
        Some(signal.map(|c| c.map(sdr_to_pq)))
    } else {
        None
    };
    for kind in &options.scopes {
        checkpoint()?;
        let mut result = match compute(
            *kind,
            if matches!(
                kind,
                ScopeKind::Waveform | ScopeKind::Parade | ScopeKind::Histogram
            ) {
                pq_signal.as_ref().unwrap_or(&signal)
            } else {
                &signal
            },
            &params,
            options.waveform_type,
            options.parade_type,
        ) {
            Computed::Waveform(w) | Computed::Parade(w) => {
                let traces: serde_json::Map<String, Value> = w
                    .traces
                    .iter()
                    .map(|g| {
                        (
                            g.name.into(),
                            json!(summary::trace_columns(&w, g, options.columns)),
                        )
                    })
                    .collect();
                let pixels = if *kind == ScopeKind::Parade {
                    super::images::parade_image(&w, options.brightness.gain())
                } else {
                    let layers: Vec<_> = w
                        .traces
                        .iter()
                        .map(|g| {
                            (
                                g,
                                super::images::trace_color(g.name, Some(options.waveform_type)),
                            )
                        })
                        .collect();
                    paint::grids(
                        &layers,
                        paint::waveform_reference(w.per_column),
                        options.brightness.gain(),
                    )
                };
                json!({"foreground":if *kind==ScopeKind::Parade {super::graticule::parade_dividers(pixels.0,pixels.1,options.parade_type)}else{vec![]},"axis":super::graticule::labels(w.lo,w.hi,options.scale),"axisUnit":if options.scale==Scale::Hdr{"cd/m²"}else{"%"},"overlay":super::graticule::level_grid(pixels.0,pixels.1,w.lo,w.hi,options.scale),"lo":w.lo*100.0,"hi":w.hi*100.0,"traces":traces,"image":image(pixels)})
            }
            Computed::Histogram(h) => {
                let pixels = image(super::images::histogram_image(
                    &h,
                    options.brightness.gain(),
                ));
                json!({"axis":super::graticule::labels(0.0,1.0,options.scale),"axisUnit":if options.scale==Scale::Hdr{"cd/m²"}else{"%"},"overlay":super::graticule::level_grid(pixels.width,pixels.height,0.0,1.0,options.scale),"histogram":h,"image":pixels})
            }
            Computed::Vectorscope(v) => {
                json!({"overlay":super::graticule::vector(v.size,matrix,options.targets,*kind==ScopeKind::VectorscopeHls),"samples":v.samples,"mean":v.mean.map(|v|(v as f64*10000.0).round()/100.0),"meanAngleDeg":(angle_deg(v.mean[0],v.mean[1]) as f64*100.0).round()/100.0,"peaks":summary::peaks(&v,options.peaks,0.0),"image":image(paint::vectorscope(&v,[0.85;3],options.brightness.gain(),true))})
            }
        };
        result["title"] = json!(match kind {
            ScopeKind::Waveform => format!("Waveform ({})", options.waveform_type.label()),
            ScopeKind::Parade => format!("Parade ({})", options.parade_type.label()),
            _ => kind.label().to_owned(),
        });
        out[kind.name()] = result;
    }
    checkpoint()?;
    Ok(out)
}
