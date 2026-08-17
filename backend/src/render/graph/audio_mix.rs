//! Stage 11: audio mixing and dynamics. Owned by the audio agent.
//!
//! Reads `spec.audio_tracks()` and `spec.audio_dynamics()`. It adds one input
//! per audio asset, places and loops each track on the output timeline, ducks
//! music against dialogue with `sidechaincompress`, sums everything with
//! `amix`, then applies the master chain: gate, denoise, dereverb, de-esser,
//! band filters, compressor, volume envelope and limiter. This is the only
//! stage that may replace the plan's terminal audio label with a mixed pad.
//!
//! `loudnorm` is deliberately not emitted here. `args.rs::audio_filters` still
//! appends it after this stage, which keeps normalization the very last thing
//! that touches the master, exactly as the contract orders it.
//!
//! Three knobs of this feature live in `args.rs`: the helpers
//! `bitrate_argument`, `legacy_highpass_filter` and `master_tail_filters` below
//! are called from `audio_filters` and `push_audio` there.

use crate::domain::audio_mix::{AudioDynamicsSpec, AudioTrackSpec, DuckingSpec};
use crate::domain::edit::EditSpec;
use crate::domain::filter_graph::{FilterGraph, MediaKind};
use crate::domain::keyframes::FfmpegKeyframeAdapter;

use super::overlays::OverlayAudioPad;
use super::{ComplexPlan, InputSpec, RenderContext};

/// `-b:a` for an edit that carries no `audioDynamics` block. Mirrors the wire
/// default in `model::default_audio_bitrate`, so nothing changes by default.
pub const DEFAULT_BITRATE_KBPS: u32 = 128;

/// Common mixing format. `amix` refuses to sum branches whose sample rate or
/// channel layout disagree, and an uploaded asset can be anything.
const MIX_FORMAT: &str = "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo";

/// `aloop` counts samples, not seconds. Ten minutes at the mix rate bounds the
/// buffer; the `atrim` that follows is what actually ends the looped track.
const LOOP_WINDOW_SAMPLES: u64 = 48_000 * 600;

/// `afftdn` expresses noise reduction in dB over 0.01..97; the wire value is a
/// 0..1 slider.
const MAX_NOISE_REDUCTION_DB: f64 = 97.0;

/// Loudness target of the legacy `normalizeAudio` switch, repeated here only so
/// `master_tail_filters` can put it in the right place.
const LOUDNESS_TARGET: &str = "loudnorm=I=-14:TP=-1.5:LRA=11";

pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext) -> anyhow::Result<()> {
    apply_with_overlays(plan, spec, ctx, &[])
}

/// Same as `apply`, plus the picture-in-picture audio pads that `overlays::
/// apply_with_audio` registered. The overlay branches join the same `amix`, so
/// a PiP clip is audible on the output.
pub fn apply_with_overlays(
    plan: &mut ComplexPlan,
    spec: &EditSpec,
    ctx: &RenderContext,
    overlay_pads: &[OverlayAudioPad],
) -> anyhow::Result<()> {
    let tracks = spec.audio_tracks();
    let dynamics = spec.audio_dynamics();
    if tracks.is_empty() && dynamics.is_none() && overlay_pads.is_empty() {
        return Ok(());
    }
    // A muted edit compiles to an output without an audio stream: there is
    // nothing to mix into and nothing to process.
    if spec.audio().muted {
        return Ok(());
    }
    mix_extra_tracks(plan, tracks, overlay_pads, ctx)?;
    if let Some(dynamics) = dynamics {
        let filters = dynamics_filters(dynamics)?;
        plan.chain_audio(&filters, "dynamics")?;
    }
    Ok(())
}

/// Number of kbit/s the encoder should use for the audio stream.
pub fn bitrate_kbps(spec: &EditSpec) -> u32 {
    spec.audio_dynamics()
        .map_or(DEFAULT_BITRATE_KBPS, AudioDynamicsSpec::bitrate_kbps)
}

/// The same value in the form FFmpeg expects after `-b:a`.
pub fn bitrate_argument(spec: &EditSpec) -> String {
    format!("{}k", bitrate_kbps(spec))
}

/// High-pass contributed by the legacy `highpass` boolean. `highpassHz` in
/// `audioDynamics` overrides it: this stage has then already emitted the
/// configured cutoff and the fixed 100 Hz filter must not be stacked on top.
pub fn legacy_highpass_filter(spec: &EditSpec) -> Option<String> {
    if spec
        .audio_dynamics()
        .and_then(AudioDynamicsSpec::highpass_hz)
        .is_some()
    {
        return None;
    }
    spec.audio().highpass.then(|| "highpass=f=100".to_owned())
}

/// Ordered tail of the master chain.
///
/// `args.rs` emits `volume` before `loudnorm`, so turning normalization on
/// silently discards the user's volume slider: `loudnorm` drives the signal to
/// a fixed target regardless of the gain in front of it. The order is
/// normalize first, then the slider on the normalized signal, which makes the
/// slider a deliberate offset from the target instead of dead UI.
pub fn master_tail_filters(spec: &EditSpec) -> Vec<String> {
    let audio = spec.audio();
    let mut filters = Vec::new();
    if audio.normalize {
        filters.push(LOUDNESS_TARGET.to_owned());
    }
    if (audio.volume - 1.0).abs() > 1e-6 {
        filters.push(format!("volume={:.3}", audio.volume));
    }
    filters
}

/// Lay every extra track on the output timeline and sum it with the primary
/// branch. Leaves the plan untouched when no track survives placement.
fn mix_extra_tracks(
    plan: &mut ComplexPlan,
    tracks: &[AudioTrackSpec],
    overlay_pads: &[OverlayAudioPad],
    ctx: &RenderContext,
) -> anyhow::Result<()> {
    if tracks.is_empty() && overlay_pads.is_empty() {
        return Ok(());
    }

    // Register the inputs first: a track that lands entirely past the end of
    // the render contributes nothing and must not add an input either.
    let mut placed: Vec<(usize, &AudioTrackSpec, f64)> = Vec::new();
    for track in tracks {
        let Some(length) = track_length_seconds(track, ctx) else {
            continue;
        };
        let path = ctx.require_asset(track.asset_id())?;
        let index = plan.add_input(InputSpec::source(path));
        placed.push((index, track, length));
    }
    // Overlay pads reference inputs the overlays stage already registered, so
    // they only need placement on the output timeline.
    let overlays: Vec<(&OverlayAudioPad, f64)> = overlay_pads
        .iter()
        .filter_map(|pad| overlay_length_seconds(pad, ctx).map(|length| (pad, length)))
        .collect();
    if placed.is_empty() && overlays.is_empty() {
        return Ok(());
    }

    // The primary (voice) branch is both a mix member and the ducking key, so
    // it is normalized to the mix format once and then split.
    if plan.audio_label().is_some() {
        plan.chain_audio(&[MIX_FORMAT.to_owned()], "voice")?;
    }
    let ducked = placed
        .iter()
        .filter(|(_, track, _)| track.ducking().is_some_and(DuckingSpec::enabled))
        .count();
    let mut keys: Vec<String> = Vec::new();
    let mut branches: Vec<String> = Vec::new();
    if let Some(primary) = plan.audio_label().map(str::to_owned) {
        if ducked == 0 {
            branches.push(primary);
        } else {
            let main = plan.next_label("voice");
            let mut pads = format!("[{main}]");
            for _ in 0..ducked {
                let key = plan.next_label("duckkey");
                pads.push_str(&format!("[{key}]"));
                keys.push(key);
            }
            plan.push(format!("[{primary}]asplit={}{pads}", ducked + 1));
            branches.push(main);
        }
    }

    let mut keys = keys.into_iter();
    for (index, track, length) in placed {
        let label = plan.next_label("track");
        let chain = serialize_audio_chain(&track_filters(track, length))?;
        plan.push(format!("[{index}:a]{chain}[{label}]"));
        // sidechaincompress takes the signal first and the key second, so the
        // music branch drops while the voice branch is above the threshold.
        let label = match (track.ducking().filter(|duck| duck.enabled()), keys.next()) {
            (Some(duck), Some(key)) => {
                let ducked_label = plan.next_label("ducked");
                plan.push(format!(
                    "[{label}][{key}]{}[{ducked_label}]",
                    ducking_filter(duck)
                ));
                ducked_label
            }
            _ => label,
        };
        branches.push(label);
    }

    for (pad, length) in overlays {
        let label = plan.next_label("pipaudio");
        let chain = serialize_audio_chain(&overlay_filters(pad, length))?;
        plan.push(format!("[{}]{chain}[{label}]", pad.pad));
        branches.push(label);
    }

    // A silent primary source leaves a single branch, which needs no amix.
    if branches.len() == 1 {
        plan.set_audio_label(branches.pop());
        return Ok(());
    }
    let mixed = plan.next_label("amix");
    let pads: String = branches
        .iter()
        .map(|label| format!("[{label}]"))
        .collect::<Vec<_>>()
        .join("");
    // normalize=0 keeps the primary at unity instead of scaling every branch
    // by 1/inputs, which is what makes gain staging predictable.
    plan.push(format!(
        "{pads}amix=inputs={}:duration=longest:dropout_transition=0:normalize=0[{mixed}]",
        branches.len()
    ));
    plan.set_audio_label(Some(mixed));
    Ok(())
}

/// How long a picture-in-picture overlay contributes audio, clamped to the
/// render length. An overlay that starts past the end contributes nothing.
fn overlay_length_seconds(pad: &OverlayAudioPad, ctx: &RenderContext) -> Option<f64> {
    let end = pad
        .end_seconds
        .unwrap_or(ctx.output_duration_seconds)
        .min(ctx.output_duration_seconds.max(0.0));
    let length = end - pad.start_seconds;
    (length.is_finite() && length > 0.0).then_some(length)
}

/// Per-overlay chain: mix format, window from the start of the overlay media,
/// gain, then the `adelay` that places it on the output timeline.
fn overlay_filters(pad: &OverlayAudioPad, length: f64) -> Vec<String> {
    let mut filters = vec![
        MIX_FORMAT.to_owned(),
        format!("atrim=start=0.000:end={length:.3}"),
        "asetpts=PTS-STARTPTS".to_owned(),
    ];
    if (pad.volume - 1.0).abs() > 1e-6 {
        filters.push(format!("volume={:.3}", pad.volume));
    }
    if pad.start_seconds > 0.0 {
        filters.push(format!(
            "adelay={}:all=1",
            (pad.start_seconds * 1000.0).round() as u64
        ));
    }
    filters
}

/// How long the track occupies the OUTPUT timeline. `end` is an output-timeline
/// time, so the window is `end - start`; without it the track runs to the end
/// of the render.
fn track_length_seconds(track: &AudioTrackSpec, ctx: &RenderContext) -> Option<f64> {
    let end = track
        .end_seconds()
        .unwrap_or(ctx.output_duration_seconds)
        .min(ctx.output_duration_seconds.max(0.0));
    let length = end - track.start_seconds();
    (length.is_finite() && length > 0.0).then_some(length)
}

/// Per-track chain: mix format, in-point, optional loop, gain, fades, and the
/// `adelay` that finally moves the track onto the output timeline. Everything
/// before the delay is expressed in track-local time.
fn track_filters(track: &AudioTrackSpec, length: f64) -> Vec<String> {
    let mut filters = vec![MIX_FORMAT.to_owned()];
    let source_start = track.source_start_seconds();
    if track.looping() {
        if source_start > 0.0 {
            filters.push(format!("atrim=start={source_start:.3}"));
            filters.push("asetpts=PTS-STARTPTS".to_owned());
        }
        filters.push(format!("aloop=loop=-1:size={LOOP_WINDOW_SAMPLES}"));
        filters.push(format!("atrim=end={length:.3}"));
    } else {
        filters.push(format!(
            "atrim=start={source_start:.3}:end={:.3}",
            source_start + length
        ));
    }
    filters.push("asetpts=PTS-STARTPTS".to_owned());
    let gain = track.gain();
    if (gain - 1.0).abs() > 1e-6 {
        filters.push(format!("volume={gain:.3}"));
    }
    let fade_in = track.fade_in_seconds();
    if fade_in > 0.0 {
        filters.push(format!("afade=t=in:st=0:d={fade_in:.3}"));
    }
    let fade_out = track.fade_out_seconds();
    if fade_out > 0.0 && length > fade_out {
        filters.push(format!(
            "afade=t=out:st={:.3}:d={fade_out:.3}",
            length - fade_out
        ));
    }
    let start = track.start_seconds();
    if start > 0.0 {
        filters.push(format!("adelay={}:all=1", (start * 1000.0).round() as u64));
    }
    filters
}

/// The wire threshold is already a linear amplitude, the envelope times are
/// milliseconds, and both are range-clamped by the domain type.
fn ducking_filter(duck: DuckingSpec) -> String {
    format!(
        "sidechaincompress=threshold={:.6}:ratio={:.3}:attack={:.3}:release={:.3}",
        duck.threshold(),
        duck.ratio(),
        duck.attack_ms(),
        duck.release_ms()
    )
}

/// Master dynamics, in the fixed contract order:
///
/// 1. `agate` removes room tone under the gate threshold
/// 2. `afftdn` broadband denoise, `nr` derived from the 0..1 wire value
/// 3. `afftdn` with noise tracking as the dereverb approximation
/// 4. `deesser` tames sibilance before any gain is added
/// 5. `highpass` / `lowpass` at the configured cutoffs
/// 6. `acompressor`
/// 7. `volume` with the keyframed envelope over the output timeline
/// 8. `alimiter` at the configured ceiling
///
/// `loudnorm` comes after all of this, emitted by `args.rs`.
fn dynamics_filters(dynamics: &AudioDynamicsSpec) -> anyhow::Result<Vec<String>> {
    let mut filters: Vec<String> = Vec::new();
    if let Some(gate) = dynamics.gate() {
        filters.push(format!(
            "agate=threshold={:.6}:ratio={:.3}",
            db_to_linear(gate.threshold_db()),
            gate.ratio()
        ));
    }
    let denoise = dynamics.denoise();
    if denoise > 0.0 {
        let reduction = (denoise * MAX_NOISE_REDUCTION_DB).clamp(0.01, MAX_NOISE_REDUCTION_DB);
        filters.push(format!("afftdn=nr={reduction:.2}:nf=-25"));
    }
    if dynamics.dereverb() {
        // FFmpeg ships no dedicated dereverb. Adaptive noise tracking on a
        // short window is the closest built-in approximation.
        filters.push("afftdn=nr=10.00:nf=-30:tn=1".to_owned());
    }
    if dynamics.deesser() {
        filters.push("deesser=i=0.400:m=0.500:f=0.500:s=o".to_owned());
    }
    if let Some(hz) = dynamics.highpass_hz() {
        filters.push(format!("highpass=f={hz:.1}"));
    }
    if let Some(hz) = dynamics.lowpass_hz() {
        filters.push(format!("lowpass=f={hz:.1}"));
    }
    if let Some(compressor) = dynamics.compressor() {
        filters.push(format!(
            "acompressor=threshold={:.6}:ratio={:.3}:attack={:.3}:release={:.3}:makeup={:.3}",
            db_to_linear(compressor.threshold_db()),
            compressor.ratio(),
            compressor.attack_ms(),
            compressor.release_ms(),
            compressor.makeup()
        ));
    }
    if let Some(envelope) = dynamics.volume_envelope() {
        // Quoted so the commas of the expression do not split the chain, and
        // per-frame so the expression is re-evaluated as `t` advances.
        let expression = FfmpegKeyframeAdapter::new(envelope)
            .expression("t")
            .map_err(|error| anyhow::anyhow!("invalid volume envelope: {error}"))?;
        filters.push(format!("volume=volume='{expression}':eval=frame"));
    }
    if let Some(limiter) = dynamics.limiter() {
        filters.push(format!(
            "alimiter=limit={:.6}:level=disabled",
            db_to_linear(limiter.ceiling_db())
        ));
    }
    Ok(filters)
}

/// `agate`, `acompressor` and `alimiter` all take linear amplitudes while the
/// wire speaks dB. The domain clamps every input, so this stays finite.
fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Validate a track chain through the shared DAG builder before it is spliced
/// into a raw statement, the same way `ComplexPlan::chain_audio` does.
fn serialize_audio_chain(filters: &[String]) -> anyhow::Result<String> {
    FilterGraph::linear(MediaKind::Audio, filters)
        .and_then(|graph| graph.ffmpeg_linear_chain())
        .map_err(|error| anyhow::anyhow!("invalid audio filter chain: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MUSIC: &str = "ast_0123456789abcdef";
    const SFX: &str = "ast_fedcba9876543210";

    fn spec_from(value: serde_json::Value) -> EditSpec {
        let request: crate::model::EditRequest = serde_json::from_value(value).unwrap();
        let extensions = crate::domain::edit::EditExtensions::from_request(&request).unwrap();
        crate::services::render::EditPlan::compile(
            crate::domain::artifact_graph::Fingerprint::digest(b"source"),
            request,
            crate::services::render::SourceMediaMetadata::new(1920, 1080, 10.0).unwrap(),
        )
        .unwrap()
        .edit
        .with_extensions(extensions)
        .unwrap()
    }

    fn context(has_audio: bool) -> RenderContext {
        RenderContext::new(1920, 1080, 10.0, has_audio, Some(30.0), 10.0)
            .with_asset(MUSIC, "/private/assets/music.mp3")
            .with_asset(SFX, "/private/assets/sfx.wav")
    }

    fn compose(value: serde_json::Value, has_audio: bool) -> ComplexPlan {
        let spec = spec_from(value);
        let ctx = context(has_audio);
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), has_audio);
        apply(&mut plan, &spec, &ctx).unwrap();
        plan
    }

    fn music_track() -> serde_json::Value {
        serde_json::json!({ "assetId": MUSIC, "role": "music" })
    }

    #[test]
    fn an_edit_without_audio_extensions_emits_todays_chain_unchanged() {
        let plan = compose(serde_json::json!({ "videoId": "x" }), true);
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
        assert_eq!(plan.audio_label(), Some("0:a"));
    }

    #[test]
    fn a_muted_edit_never_builds_an_audio_branch() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "mute": true,
                "audioTracks": [music_track()],
            }),
            false,
        );
        assert!(plan.is_trivial());
        assert_eq!(plan.inputs().len(), 1);
    }

    #[test]
    fn a_music_track_is_trimmed_placed_faded_and_mixed() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{
                    "assetId": MUSIC,
                    "role": "music",
                    "gain": 0.4,
                    "start": 2.0,
                    "sourceStart": 30.0,
                    "end": 8.0,
                    "fadeIn": 1.0,
                    "fadeOut": 1.5,
                }],
            }),
            true,
        );
        let graph = plan.render();
        assert_eq!(plan.inputs().len(), 2);
        assert_eq!(
            plan.inputs()[1].path,
            std::path::PathBuf::from("/private/assets/music.mp3")
        );
        // 6 seconds of output timeline starting 30 s into the asset.
        assert!(graph.contains("[1:a]"), "{graph}");
        assert!(graph.contains("atrim=start=30.000:end=36.000"), "{graph}");
        assert!(graph.contains("volume=0.400"), "{graph}");
        assert!(graph.contains("afade=t=in:st=0:d=1.000"), "{graph}");
        assert!(graph.contains("afade=t=out:st=4.500:d=1.500"), "{graph}");
        assert!(graph.contains("adelay=2000:all=1"), "{graph}");
        assert!(
            graph.contains("amix=inputs=2:duration=longest:dropout_transition=0:normalize=0"),
            "{graph}"
        );
        assert_eq!(plan.audio_label(), Some("amix_3"));
        assert!(!graph.contains("sidechaincompress"), "{graph}");
    }

    #[test]
    fn a_looping_track_repeats_the_in_point_and_is_cut_to_the_window() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{
                    "assetId": MUSIC, "role": "music",
                    "sourceStart": 4.0, "loop": true,
                }],
            }),
            true,
        );
        let graph = plan.render();
        assert!(
            graph.contains("atrim=start=4.000,asetpts=PTS-STARTPTS"),
            "{graph}"
        );
        assert!(
            graph.contains(&format!("aloop=loop=-1:size={LOOP_WINDOW_SAMPLES}")),
            "{graph}"
        );
        assert!(graph.contains("atrim=end=10.000"), "{graph}");
    }

    #[test]
    fn ducking_splits_the_voice_branch_and_keys_sidechaincompress() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{
                    "assetId": MUSIC, "role": "music",
                    "ducking": { "enabled": true, "threshold": 0.05, "ratio": 8.0,
                                 "attack": 20.0, "release": 300.0 },
                }],
            }),
            true,
        );
        let graph = plan.render();
        assert!(graph.contains("[0:a]aformat="), "{graph}");
        assert!(graph.contains("asplit=2[voice_2][duckkey_3]"), "{graph}");
        assert!(
            graph.contains(
                "[track_4][duckkey_3]sidechaincompress=threshold=0.050000:ratio=8.000:attack=20.000:release=300.000[ducked_5]"
            ),
            "{graph}"
        );
        assert!(
            graph.contains("[voice_2][ducked_5]amix=inputs=2"),
            "{graph}"
        );
    }

    #[test]
    fn a_disabled_ducking_block_leaves_the_voice_branch_unsplit() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{
                    "assetId": MUSIC, "role": "music",
                    "ducking": { "enabled": false, "threshold": 0.05, "ratio": 8.0,
                                 "attack": 20.0, "release": 300.0 },
                }],
            }),
            true,
        );
        let graph = plan.render();
        assert!(!graph.contains("asplit"), "{graph}");
        assert!(!graph.contains("sidechaincompress"), "{graph}");
        assert!(graph.contains("amix=inputs=2"), "{graph}");
    }

    #[test]
    fn a_silent_primary_source_mixes_only_the_extra_tracks() {
        let single = compose(
            serde_json::json!({ "videoId": "x", "audioTracks": [music_track()] }),
            false,
        );
        let graph = single.render();
        assert!(!graph.contains("0:a"), "{graph}");
        assert!(!graph.contains("amix"), "{graph}");
        assert_eq!(single.audio_label(), Some("track_1"));

        let pair = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [
                    music_track(),
                    { "assetId": SFX, "role": "sfx", "start": 3.0 },
                ],
            }),
            false,
        );
        assert!(pair.render().contains("amix=inputs=2"), "{}", pair.render());
        assert_eq!(pair.inputs().len(), 3);
    }

    #[test]
    fn a_track_that_starts_past_the_end_adds_no_input() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioTracks": [{ "assetId": MUSIC, "role": "music", "start": 40.0 }],
            }),
            true,
        );
        assert!(plan.is_trivial());
        assert_eq!(plan.inputs().len(), 1);
    }

    #[test]
    fn an_unresolved_asset_fails_the_render_instead_of_dropping_the_track() {
        let spec = spec_from(serde_json::json!({
            "videoId": "x",
            "audioTracks": [{ "assetId": "ast_unregistered000", "role": "music" }],
        }));
        let ctx = context(true);
        let mut plan = ComplexPlan::new(InputSpec::source("/in.mp4"), true);
        assert!(apply(&mut plan, &spec, &ctx).is_err());
    }

    #[test]
    fn every_dynamics_stage_is_emitted_in_the_documented_order() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioDynamics": {
                    "denoise": 0.5,
                    "dereverb": true,
                    "deesser": true,
                    "gate": { "threshold": -40.0, "ratio": 2.0 },
                    "highpassHz": 80.0,
                    "lowpassHz": 12000.0,
                    "compressor": { "threshold": -20.0, "ratio": 3.0, "attack": 20.0,
                                    "release": 250.0, "makeup": 1.5 },
                    "limiter": { "ceiling": -1.0 },
                },
            }),
            true,
        );
        let graph = plan.render();
        let order = [
            "agate=threshold=0.010000:ratio=2.000",
            "afftdn=nr=48.50:nf=-25",
            "afftdn=nr=10.00:nf=-30:tn=1",
            "deesser=i=0.400:m=0.500:f=0.500:s=o",
            "highpass=f=80.0",
            "lowpass=f=12000.0",
            "acompressor=threshold=0.100000:ratio=3.000:attack=20.000:release=250.000:makeup=1.500",
            "alimiter=limit=0.891251:level=disabled",
        ];
        let mut cursor = 0usize;
        for filter in order {
            let found = graph[cursor..]
                .find(filter)
                .unwrap_or_else(|| panic!("missing or out of order: {filter} in {graph}"));
            cursor += found + filter.len();
        }
        assert_eq!(plan.audio_label(), Some("dynamics_1"));
        assert!(!graph.contains("loudnorm"), "{graph}");
    }

    #[test]
    fn an_all_default_dynamics_block_leaves_the_chain_untouched() {
        let plan = compose(
            serde_json::json!({ "videoId": "x", "audioDynamics": { "bitrateKbps": 256 } }),
            true,
        );
        assert!(plan.is_trivial());
        assert_eq!(plan.render(), "");
    }

    #[test]
    fn the_volume_envelope_becomes_a_keyframed_expression() {
        let plan = compose(
            serde_json::json!({
                "videoId": "x",
                "audioDynamics": {
                    "volumeEnvelope": [
                        { "t": 0.0, "v": 1.0, "interp": "linear" },
                        { "t": 4.0, "v": 0.25, "interp": "linear" },
                    ],
                },
            }),
            true,
        );
        assert_eq!(
            plan.render(),
            "[0:a]volume=volume='if(lt(t,0),1,if(lt(t,4),1+(0.25-1)*((t-0)/4),0.25))':eval=frame[dynamics_1]"
        );
    }

    #[test]
    fn the_bitrate_comes_from_the_wire_and_falls_back_to_the_default() {
        let default = spec_from(serde_json::json!({ "videoId": "x" }));
        assert_eq!(bitrate_kbps(&default), DEFAULT_BITRATE_KBPS);
        assert_eq!(bitrate_argument(&default), "128k");

        let chosen = spec_from(serde_json::json!({
            "videoId": "x",
            "audioDynamics": { "bitrateKbps": 256 },
        }));
        assert_eq!(bitrate_kbps(&chosen), 256);
        assert_eq!(bitrate_argument(&chosen), "256k");

        // Out-of-range values are clamped by the domain, never passed through.
        let absurd = spec_from(serde_json::json!({
            "videoId": "x",
            "audioDynamics": { "bitrateKbps": 4000 },
        }));
        assert_eq!(bitrate_argument(&absurd), "320k");
    }

    #[test]
    fn the_configured_cutoff_replaces_the_legacy_fixed_highpass() {
        let legacy = spec_from(serde_json::json!({ "videoId": "x", "highpass": true }));
        assert_eq!(
            legacy_highpass_filter(&legacy),
            Some("highpass=f=100".to_owned())
        );

        let overridden = spec_from(serde_json::json!({
            "videoId": "x",
            "highpass": true,
            "audioDynamics": { "highpassHz": 60.0 },
        }));
        assert_eq!(legacy_highpass_filter(&overridden), None);

        let off = spec_from(serde_json::json!({ "videoId": "x" }));
        assert_eq!(legacy_highpass_filter(&off), None);
    }

    #[test]
    fn the_master_tail_normalizes_before_applying_the_volume_slider() {
        let both = spec_from(serde_json::json!({
            "videoId": "x", "normalizeAudio": true, "volume": 1.5,
        }));
        assert_eq!(
            master_tail_filters(&both),
            vec![LOUDNESS_TARGET.to_owned(), "volume=1.500".to_owned()]
        );

        let volume_only = spec_from(serde_json::json!({ "videoId": "x", "volume": 0.5 }));
        assert_eq!(master_tail_filters(&volume_only), vec!["volume=0.500"]);

        let untouched = spec_from(serde_json::json!({ "videoId": "x" }));
        assert!(master_tail_filters(&untouched).is_empty());
    }
}
