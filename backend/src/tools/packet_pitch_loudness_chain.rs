//! Global scalar/rate plan for ordered Pitch/Loudness graphs.
use super::{packet_pcm::Decoded, packet_timeline::Source};
use crate::domain::concat_format_plan::{Filter, Format, Plan};
use anyhow::{ensure, Result};
#[derive(Clone, Copy)]
pub(crate) enum Setting {
    Pitch(f32),
    Loudness(f32),
}
pub(crate) fn build(decoded: Decoded, settings: &[Setting]) -> Result<Box<dyn Source>> {
    ensure!(
        !settings.is_empty() && settings.len() <= 16,
        "invalid mixed chain length"
    );
    let (format, channels, planar) = match &decoded {
        Decoded::U8(s) => (Format::U8, s.spec().channels, s.spec().source_planar),
        Decoded::S16(s) => (Format::S16, s.spec().channels, s.spec().source_planar),
        Decoded::S32(s) => (Format::S32, s.spec().channels, s.spec().source_planar),
        Decoded::F32(s) => (Format::F32, s.spec().channels, s.spec().source_planar),
        Decoded::F64(s) => (Format::F64, s.spec().channels, s.spec().source_planar),
    };
    let mut filters = Vec::new();
    let mut ends = Vec::new();
    let mut clock = 0;
    for setting in settings {
        match *setting {
            Setting::Pitch(p) => {
                ensure!(
                    p.is_finite() && (-12. ..=12.).contains(&p),
                    "invalid mixed Pitch"
                );
                filters.extend([
                    Filter::Resample,
                    Filter::Pass,
                    Filter::Resample,
                    Filter::Tempo,
                ]);
                clock = 48000;
            }
            Setting::Loudness(t) => {
                ensure!(
                    t.is_finite() && (-30. ..=-12.).contains(&t),
                    "invalid mixed Loudness"
                );
                if clock != 192000 {
                    filters.push(Filter::Resample);
                }
                filters.push(Filter::Loudness);
                clock = 192000;
            }
        }
        ends.push(filters.len());
    }
    if clock != 48000 {
        filters.push(Filter::Resample);
    }
    let plan = Plan::build(format.layout(planar), &filters, Format::F32)?;
    let mut source: Box<dyn Source> = match settings[0] {
        Setting::Pitch(p) => Box::new(super::packet_pitch_frames::Reader::new(
            decoded,
            &filters[4..],
            Format::F32,
            p,
        )?),
        Setting::Loudness(t) => {
            let up = super::packet_rate_frames::reader(decoded, 192000)?;
            Box::new(super::packet_loudness::Reader::new(
                up,
                192000,
                channels,
                f64::from(t),
            )?)
        }
    };
    let mut current = plan.stages[ends[0] - 1].output;
    let mut clock = match settings[0] {
        Setting::Pitch(_) => 48000,
        Setting::Loudness(_) => 192000,
    };
    for (i, setting) in settings.iter().enumerate().skip(1) {
        match *setting {
            Setting::Pitch(p) => {
                if clock != 48000 {
                    ensure!(
                        current.packed() == Format::F64,
                        "mixed downsample must be double"
                    );
                    source = Box::new(super::packet_double_rate::Reader::new(
                        source, clock, 48000, channels,
                    )?);
                }
                let output = plan.stages[ends[i] - 1].output.packed();
                source = Box::new(super::packet_pitch_node::Node::new(
                    source,
                    current.packed(),
                    output,
                    channels,
                    p,
                )?);
                current = output;
                clock = 48000;
            }
            Setting::Loudness(t) => {
                if clock != 192000 {
                    source =
                        super::packet_typed_rate::reader(source, current, clock, 192000, channels)?;
                }
                ensure!(
                    clock != 192000 || current.packed() == Format::F64,
                    "mixed Loudness scalar mismatch"
                );
                source = Box::new(super::packet_loudness::Reader::new(
                    source,
                    192000,
                    channels,
                    f64::from(t),
                )?);
                current = Format::F64;
                clock = 192000;
            }
        }
    }
    if clock != 48000 {
        source = Box::new(super::packet_double_rate::Reader::new(
            source, clock, 48000, channels,
        )?);
    }
    Ok(source)
}
