//! Replacement properties derived from a fresh registered-source probe.
use super::{
    composition::{CompositionSource, SourceId, SourceKind},
    media_probe::{ProbeResult, StreamKind},
};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
pub fn replacement(
    id: SourceId,
    name: &str,
    kind: SourceKind,
    probe: &ProbeResult,
) -> Result<(Value, CompositionSource)> {
    ensure!(
        !name.trim().is_empty() && name.chars().count() <= 512,
        "invalid media name"
    );
    ensure!(
        probe.duration.is_finite() && probe.duration >= 0.,
        "invalid probed duration"
    );
    let video = probe.streams.iter().find(|s| s.kind == StreamKind::Video);
    let audio = probe.streams.iter().find(|s| s.kind == StreamKind::Audio);
    let duration = if kind == SourceKind::Image {
        0.
    } else {
        probe.duration
    };
    ensure!(
        duration * 1e6 <= 9_007_199_254_740_991.,
        "source duration exceeds exact range"
    );
    let source = CompositionSource {
        id: id.clone(),
        kind,
        duration_ticks: (duration * 1e6).round() as u64,
        width: if kind == SourceKind::Audio {
            0
        } else {
            probe.width
        },
        height: if kind == SourceKind::Audio {
            0
        } else {
            probe.height
        },
        has_audio: audio.is_some(),
        audio_channels: audio.and_then(|s| s.channels),
    };
    source.validate()?;
    let mut tracks = vec![];
    for stream in probe.streams.iter().filter(|s| s.kind == StreamKind::Audio) {
        let mut track = json!({"index":stream.index,"codec":stream.codec_name.as_deref().unwrap_or(""),"channels":stream.channels.unwrap_or(0),"sampleRate":stream.sample_rate.unwrap_or(0)});
        for key in ["title", "language"] {
            if let Some(value) = stream.tags.get(key).filter(|v| !v.is_empty()) {
                track[key] = json!(value);
            }
        }
        tracks.push(track);
    }
    let fraction = video
        .and_then(|s| s.frame_rate)
        .filter(|r| r.numerator > 0 && r.denominator > 0)
        .map(|r| format!("{}/{}", r.numerator, r.denominator));
    let color = match video.and_then(|s| s.color.transfer.as_deref()) {
        Some("arib-std-b67") => "hlg",
        Some("smpte2084") => "pq",
        _ => "sdr",
    };
    let item = json!({"path":format!("library:{}",id.as_str()),"name":name,"duration":if kind==SourceKind::Image{None}else{Some(duration)},"kind":kind,"width":if source.width==0{None}else{Some(source.width)},"height":if source.height==0{None}else{Some(source.height)},"frameRate":probe.fps,"frameRateFraction":fraction,"videoCodec":probe.vcodec,"audioCodec":probe.acodec,"hasAudio":source.has_audio,"audioTracks":tracks,"colorSpace":color});
    Ok((item, source))
}
