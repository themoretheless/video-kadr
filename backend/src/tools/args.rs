//! Pure ffmpeg argument construction: maps an `EditRequest` to a `Vec<String>`
//! command line. No I/O and no process spawning, so it is fully unit-testable.

use std::path::Path;

use crate::model::EditRequest;

/// Map a named look preset to an ffmpeg filter string.
fn filter_preset(name: &str) -> Option<&'static str> {
    match name {
        "grayscale" => Some("hue=s=0"),
        "sepia" => Some("colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131"),
        "warm" => Some("colorbalance=rs=0.2:gs=0.05:bs=-0.2"),
        "cold" => Some("colorbalance=rs=-0.2:gs=0:bs=0.2"),
        // Shadows toward teal, highlights toward orange (the blockbuster look).
        "teal-orange" => Some("colorbalance=rs=-0.15:bs=0.15:rm=0.1:bm=-0.05:rh=0.15:bh=-0.15"),
        // Lifted blacks + lowered whites for a flat, matte film look.
        "faded" => Some("curves=all='0/0.08 1/0.92'"),
        // High-contrast black and white.
        "noir" => Some("hue=s=0,eq=contrast=1.4"),
        // Warm, slightly faded vintage.
        "vintage" => Some("curves=all='0/0.06 1/0.95',colorbalance=rs=0.15:gs=0.05:bs=-0.1"),
        _ => None,
    }
}

/// Whitelist a censor-box colour to a safe token (default black).
fn sanitize_color(name: Option<&str>) -> &'static str {
    match name.unwrap_or("black") {
        "white" => "white",
        "gray" | "grey" => "gray",
        "red" => "red",
        _ => "black",
    }
}

/// Parse an aspect like "9:16" into (w, h), only allowing small sane values.
fn parse_aspect(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once(':')?;
    let w: u32 = a.trim().parse().ok()?;
    let h: u32 = b.trim().parse().ok()?;
    if w == 0 || h == 0 || w > 100 || h > 100 {
        return None;
    }
    Some((w, h))
}

/// Output file extension for a requested export format.
pub fn output_ext(format: Option<&str>) -> &'static str {
    match format.unwrap_or("mp4") {
        "webm" => "webm",
        "gif" => "gif",
        "png" => "png",
        "jpg" => "jpg",
        "mp3" => "mp3",
        "prores" => "mov",
        // av1 lives in an mp4 container.
        _ => "mp4",
    }
}

/// Build the geometry/colour (and optionally temporal) video filter chain.
/// Order matters: crop -> rotate -> flip -> scale -> eq -> preset -> reverse
/// -> setpts -> fade. `temporal=false` skips speed/fade (used for still frames).
fn video_filters(edit: &EditRequest, out_dur: f64, temporal: bool) -> Vec<String> {
    let mut vf: Vec<String> = Vec::new();
    // Censor box first, in source coordinates (matches the on-video selection).
    if let Some(c) = &edit.censor {
        let color = sanitize_color(edit.censor_color.as_deref());
        vf.push(format!(
            "drawbox=x={}:y={}:w={}:h={}:color={color}:t=fill",
            c.x, c.y, c.w, c.h
        ));
    }
    if let Some(c) = &edit.crop {
        // Force even dimensions; libx264 + yuv420p requires them.
        let w = c.w & !1;
        let h = c.h & !1;
        vf.push(format!("crop={w}:{h}:{}:{}", c.x, c.y));
    }
    match edit.rotate.rem_euclid(360) {
        90 => vf.push("transpose=1".into()),
        180 => {
            vf.push("transpose=1".into());
            vf.push("transpose=1".into());
        }
        270 => vf.push("transpose=2".into()),
        _ => {}
    }
    if edit.flip_h {
        vf.push("hflip".into());
    }
    if edit.flip_v {
        vf.push("vflip".into());
    }
    if let Some(s) = &edit.scale {
        vf.push(format!("scale={}:{}", s.w, s.h));
    }
    // Letterbox/pillarbox to a target aspect (adds bars, keeps whole frame).
    if let Some((tw, th)) = edit.pad.as_deref().and_then(parse_aspect) {
        vf.push(format!(
            "pad=w='ceil(max(iw,ih*{tw}/{th})/2)*2':h='ceil(max(ih,iw*{th}/{tw})/2)*2':x='(ow-iw)/2':y='(oh-ih)/2':color=black"
        ));
    }
    if edit.denoise {
        vf.push("hqdn3d".into());
    }
    let eq_changed = edit.brightness.abs() > 1e-6
        || (edit.contrast - 1.0).abs() > 1e-6
        || (edit.saturation - 1.0).abs() > 1e-6;
    if eq_changed {
        vf.push(format!(
            "eq=brightness={:.3}:contrast={:.3}:saturation={:.3}",
            edit.brightness, edit.contrast, edit.saturation
        ));
    }
    if let Some(f) = edit.filter.as_deref().and_then(filter_preset) {
        vf.push(f.into());
    }
    if edit.sharpen > 1e-6 {
        vf.push(format!(
            "unsharp=5:5:{:.3}:5:5:0.0",
            edit.sharpen.clamp(0.0, 5.0)
        ));
    }
    if edit.vignette {
        vf.push("vignette".into());
    }
    if edit.grain > 1e-6 {
        vf.push(format!(
            "noise=alls={:.0}:allf=t",
            edit.grain.clamp(0.0, 100.0)
        ));
    }
    if edit.reverse {
        vf.push("reverse".into());
    }
    if temporal {
        let speed = edit.speed;
        if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
            vf.push(format!("setpts={:.6}*PTS", 1.0 / speed));
        }
        if edit.fade_in > 0.0 {
            vf.push(format!("fade=t=in:st=0:d={:.3}", edit.fade_in));
        }
        if edit.fade_out > 0.0 && out_dur > edit.fade_out {
            vf.push(format!(
                "fade=t=out:st={:.3}:d={:.3}",
                out_dur - edit.fade_out,
                edit.fade_out
            ));
        }
    }
    vf
}

/// Build the audio filter chain: areverse -> volume -> atempo -> afade.
fn audio_filters(edit: &EditRequest, out_dur: f64) -> Vec<String> {
    let mut af: Vec<String> = Vec::new();
    if edit.reverse {
        af.push("areverse".into());
    }
    if edit.highpass {
        af.push("highpass=f=100".into());
    }
    if (edit.volume - 1.0).abs() > 1e-6 {
        af.push(format!("volume={:.3}", edit.volume.max(0.0)));
    }
    let speed = edit.speed;
    if (speed - 1.0).abs() > 1e-6 && speed > 0.0 {
        // atempo only accepts 0.5..=2.0; the frontend clamps speed to that range.
        af.push(format!("atempo={:.6}", speed.clamp(0.5, 2.0)));
    }
    if edit.fade_in > 0.0 {
        af.push(format!("afade=t=in:st=0:d={:.3}", edit.fade_in));
    }
    if edit.fade_out > 0.0 && out_dur > edit.fade_out {
        af.push(format!(
            "afade=t=out:st={:.3}:d={:.3}",
            out_dur - edit.fade_out,
            edit.fade_out
        ));
    }
    // Loudness normalization is applied last, on the finished chain.
    if edit.normalize_audio {
        af.push("loudnorm=I=-14:TP=-1.5:LRA=11".into());
    }
    af
}

/// Append audio options: `-an` when muted, otherwise the filter chain + codec.
fn push_audio(args: &mut Vec<String>, edit: &EditRequest, out_dur: f64, codec: &str) {
    if edit.mute {
        args.push("-an".into());
        return;
    }
    let af = audio_filters(edit, out_dur);
    if !af.is_empty() {
        args.push("-af".into());
        args.push(af.join(","));
    }
    args.push("-c:a".into());
    args.push(codec.into());
    args.push("-b:a".into());
    args.push("128k".into());
}

fn push_fps(args: &mut Vec<String>, edit: &EditRequest) {
    if let Some(fps) = edit.fps {
        if fps > 0.0 {
            args.push("-r".into());
            args.push(format!("{fps:.3}"));
        }
    }
}

/// Build the ffmpeg argument list for an edit request.
///
/// Trim is applied as an *input* option (`-ss` + `-t`) so it happens before the
/// filter graph; geometry/colour/speed/fade then operate on the trimmed stream.
/// `source_duration` is the probed length of the input, used to time fade-outs.
/// The output container/codecs depend on `edit.format` (mp4/webm/gif/png/mp3).
pub fn build_ffmpeg_args(
    input: &Path,
    output: &Path,
    edit: &EditRequest,
    source_duration: f64,
) -> Vec<String> {
    let format = edit.format.as_deref().unwrap_or("mp4");

    // Multi-segment edits (cut from the middle / stitch ranges) need a concat
    // filter graph; only meaningful for the video containers.
    let segs = valid_segments(edit);
    if !segs.is_empty() && matches!(format, "mp4" | "webm") {
        return build_concat_args(input, output, edit, &segs, source_duration);
    }

    let mut args: Vec<String> = vec!["-y".into()];

    // --- input-side trim ---
    if let Some(t) = &edit.trim {
        let dur = (t.end - t.start).max(0.0);
        args.push("-ss".into());
        args.push(format_secs(t.start));
        args.push("-t".into());
        args.push(format_secs(dur));
    }

    args.push("-i".into());
    args.push(input.to_string_lossy().into_owned());

    let out_dur = expected_output_secs(edit, source_duration);

    match format {
        "mp3" => {
            // Audio-only extraction.
            let af = audio_filters(edit, out_dur);
            if !af.is_empty() {
                args.push("-af".into());
                args.push(af.join(","));
            }
            args.push("-vn".into());
            args.push("-c:a".into());
            args.push("libmp3lame".into());
            args.push("-q:a".into());
            args.push("2".into());
        }
        "png" | "jpg" => {
            // Single still frame at the trim start (positioned by -ss above).
            // The encoder is chosen by the output extension (png / mjpeg).
            let vf = video_filters(edit, out_dur, false);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            args.push("-frames:v".into());
            args.push("1".into());
            args.push("-an".into());
        }
        "gif" => {
            // Generate a per-clip palette for a good-looking gif (single pass
            // via split + palettegen/paletteuse).
            let mut parts = video_filters(edit, out_dur, true);
            let fps = edit.fps.filter(|f| *f > 0.0).unwrap_or(12.0);
            parts.push(format!("fps={fps:.3}"));
            let graph = format!(
                "{},split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
                parts.join(",")
            );
            args.push("-vf".into());
            args.push(graph);
            args.push("-an".into());
        }
        "webm" => {
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            push_audio(&mut args, edit, out_dur, "libopus");
            push_video_codec(&mut args, edit, "webm");
        }
        "av1" => {
            // Modern, compact codec in an mp4 container (needs libsvtav1).
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            push_audio(&mut args, edit, out_dur, "aac");
            args.push("-c:v".into());
            args.push("libsvtav1".into());
            args.push("-crf".into());
            args.push(edit.quality.unwrap_or(32).to_string());
            args.push("-preset".into());
            args.push("6".into());
            args.push("-pix_fmt".into());
            args.push("yuv420p".into());
            push_fps(&mut args, edit);
            args.push("-movflags".into());
            args.push("+faststart".into());
        }
        "prores" => {
            // Intra-only edit codec in a .mov; audio as PCM. prores_ks is built in.
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            if edit.mute {
                args.push("-an".into());
            } else {
                let af = audio_filters(edit, out_dur);
                if !af.is_empty() {
                    args.push("-af".into());
                    args.push(af.join(","));
                }
                args.push("-c:a".into());
                args.push("pcm_s16le".into());
            }
            args.push("-c:v".into());
            args.push("prores_ks".into());
            args.push("-profile:v".into());
            args.push("3".into());
            args.push("-pix_fmt".into());
            args.push("yuv422p10le".into());
            push_fps(&mut args, edit);
        }
        _ => {
            // mp4 (default): H.264 or H.265.
            let vf = video_filters(edit, out_dur, true);
            if !vf.is_empty() {
                args.push("-vf".into());
                args.push(vf.join(","));
            }
            push_audio(&mut args, edit, out_dur, "aac");
            push_video_codec(&mut args, edit, "mp4");
        }
    }

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Push the video encoder and its quality/pixel-format options for `format`
/// (VP9 for webm, otherwise H.264/H.265), then fps and (for mp4) faststart.
/// Shared by the single-pass and concat paths so codec settings live in one place.
fn push_video_codec(args: &mut Vec<String>, edit: &EditRequest, format: &str) {
    if format == "webm" {
        args.push("-c:v".into());
        args.push("libvpx-vp9".into());
        args.push("-crf".into());
        args.push(edit.quality.unwrap_or(32).to_string());
        args.push("-b:v".into());
        args.push("0".into());
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    } else {
        let h265 = edit.codec.as_deref() == Some("h265");
        args.push("-c:v".into());
        args.push(if h265 { "libx265" } else { "libx264" }.into());
        args.push("-preset".into());
        args.push("veryfast".into());
        args.push("-crf".into());
        args.push(
            edit.quality
                .unwrap_or(if h265 { 28 } else { 23 })
                .to_string(),
        );
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
        if h265 {
            // hvc1 tag keeps the result playable in QuickTime/Safari.
            args.push("-tag:v".into());
            args.push("hvc1".into());
        }
    }
    push_fps(args, edit);
    if format != "webm" {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }
}

/// Build args for a multi-segment edit: trim each keep-segment, concat them,
/// then apply the usual geometry/colour/speed/fade filters to the result.
fn build_concat_args(
    input: &Path,
    output: &Path,
    edit: &EditRequest,
    segments: &[&crate::model::Trim],
    source_duration: f64,
) -> Vec<String> {
    let format = edit.format.as_deref().unwrap_or("mp4");
    let muted = edit.mute;
    let n = segments.len();
    let out_dur = expected_output_secs(edit, source_duration);

    let mut graph = String::new();
    for (i, s) in segments.iter().enumerate() {
        graph.push_str(&format!(
            "[0:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS[v{i}];",
            s.start, s.end
        ));
        if !muted {
            graph.push_str(&format!(
                "[0:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS[a{i}];",
                s.start, s.end
            ));
        }
    }
    for i in 0..n {
        graph.push_str(&format!("[v{i}]"));
        if !muted {
            graph.push_str(&format!("[a{i}]"));
        }
    }
    if muted {
        graph.push_str(&format!("concat=n={n}:v=1:a=0[cv]"));
    } else {
        graph.push_str(&format!("concat=n={n}:v=1:a=1[cv][ca]"));
    }

    // Effects apply to the concatenated stream.
    let vf = video_filters(edit, out_dur, true);
    let vmap = if vf.is_empty() {
        "[cv]".to_string()
    } else {
        graph.push_str(&format!(";[cv]{}[vout]", vf.join(",")));
        "[vout]".to_string()
    };
    let amap = if muted {
        None
    } else {
        let af = audio_filters(edit, out_dur);
        if af.is_empty() {
            Some("[ca]".to_string())
        } else {
            graph.push_str(&format!(";[ca]{}[aout]", af.join(",")));
            Some("[aout]".to_string())
        }
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        graph,
        "-map".into(),
        vmap,
    ];
    if let Some(am) = &amap {
        args.push("-map".into());
        args.push(am.clone());
    }

    push_video_codec(&mut args, edit, format);
    if amap.is_some() {
        args.push("-c:a".into());
        args.push(if format == "webm" { "libopus" } else { "aac" }.into());
        args.push("-b:a".into());
        args.push("128k".into());
    }

    args.push(output.to_string_lossy().into_owned());
    args
}

/// Valid keep-segments (positive length), if any were requested.
fn valid_segments(edit: &EditRequest) -> Vec<&crate::model::Trim> {
    match &edit.segments {
        Some(segs) => segs.iter().filter(|s| s.end - s.start > 0.01).collect(),
        None => Vec::new(),
    }
}

/// Expected output duration (seconds) for an edit, used to scale ffmpeg progress.
pub fn expected_output_secs(edit: &EditRequest, source_duration: f64) -> f64 {
    let segs = valid_segments(edit);
    let base = if !segs.is_empty() {
        segs.iter().map(|s| (s.end - s.start).max(0.0)).sum()
    } else {
        match &edit.trim {
            Some(t) => (t.end - t.start).max(0.0),
            None => source_duration,
        }
    };
    let speed = if edit.speed > 0.0 { edit.speed } else { 1.0 };
    base / speed
}

/// Format seconds without scientific notation, trimming trailing noise.
fn format_secs(s: f64) -> String {
    format!("{s:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edit(v: serde_json::Value) -> EditRequest {
        serde_json::from_value(v).expect("valid EditRequest")
    }

    fn args_for(v: serde_json::Value, dur: f64) -> Vec<String> {
        let input = Path::new("/in.mp4");
        let output = Path::new("/out.mp4");
        build_ffmpeg_args(input, output, &edit(v), dur)
    }

    fn vf(args: &[String]) -> String {
        let i = args.iter().position(|a| a == "-vf").expect("has -vf");
        args[i + 1].clone()
    }

    fn af(args: &[String]) -> Option<String> {
        args.iter()
            .position(|a| a == "-af")
            .map(|i| args[i + 1].clone())
    }

    #[test]
    fn default_edit_reencodes_with_audio() {
        let args = args_for(json!({ "videoId": "x" }), 10.0);
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"aac".to_string()));
        assert!(!args.contains(&"-an".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
    }

    #[test]
    fn trim_is_input_side() {
        let args = args_for(
            json!({ "videoId": "x", "trim": { "start": 2.0, "end": 5.0 } }),
            10.0,
        );
        let ss = args.iter().position(|a| a == "-ss").unwrap();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert!(ss < i, "-ss must precede -i");
        assert_eq!(
            args[args.iter().position(|a| a == "-t").unwrap() + 1],
            "3.000"
        );
    }

    #[test]
    fn mute_drops_audio() {
        let args = args_for(json!({ "videoId": "x", "mute": true }), 10.0);
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"aac".to_string()));
        assert!(af(&args).is_none());
    }

    #[test]
    fn geometry_and_colour_order() {
        let args = args_for(
            json!({
                "videoId": "x",
                "crop": { "x": 10, "y": 20, "w": 101, "h": 51 },
                "rotate": 90,
                "flipH": true,
                "scale": { "w": 640, "h": -2 },
                "filter": "grayscale",
                "fadeOut": 1.0
            }),
            10.0,
        );
        let chain = vf(&args);
        // Crop forces even dimensions.
        assert!(chain.contains("crop=100:50:10:20"), "{chain}");
        let order = |s: &str| chain.find(s).unwrap();
        assert!(order("crop") < order("transpose=1"));
        assert!(order("transpose=1") < order("hflip"));
        assert!(order("hflip") < order("scale"));
        assert!(order("scale") < order("hue=s=0"));
        assert!(chain.contains("fade=t=out:st=9.000:d=1.000"), "{chain}");
    }

    #[test]
    fn audio_chain_volume_and_speed() {
        let args = args_for(json!({ "videoId": "x", "volume": 1.5, "speed": 2.0 }), 10.0);
        let chain = af(&args).expect("has -af");
        assert!(chain.contains("volume=1.500"), "{chain}");
        assert!(chain.contains("atempo=2.000000"), "{chain}");
        // Speed halves duration: setpts on the video side.
        assert!(vf(&args).contains("setpts="));
    }

    #[test]
    fn fps_override_present() {
        let args = args_for(json!({ "videoId": "x", "fps": 30.0 }), 10.0);
        let i = args.iter().position(|a| a == "-r").expect("has -r");
        assert_eq!(args[i + 1], "30.000");
    }

    #[test]
    fn output_ext_maps_formats() {
        assert_eq!(output_ext(None), "mp4");
        assert_eq!(output_ext(Some("mp4")), "mp4");
        assert_eq!(output_ext(Some("webm")), "webm");
        assert_eq!(output_ext(Some("gif")), "gif");
        assert_eq!(output_ext(Some("png")), "png");
        assert_eq!(output_ext(Some("jpg")), "jpg");
        assert_eq!(output_ext(Some("mp3")), "mp3");
        assert_eq!(output_ext(Some("prores")), "mov");
        assert_eq!(output_ext(Some("av1")), "mp4");
        assert_eq!(output_ext(Some("weird")), "mp4");
    }

    #[test]
    fn audio_normalize_and_highpass() {
        let args = args_for(
            json!({ "videoId": "x", "normalizeAudio": true, "highpass": true }),
            10.0,
        );
        let chain = af(&args).expect("has -af");
        assert!(chain.contains("loudnorm"), "{chain}");
        assert!(chain.contains("highpass=f=100"), "{chain}");
    }

    #[test]
    fn av1_format_uses_svtav1() {
        let args = args_for(
            json!({ "videoId": "x", "format": "av1", "quality": 30 }),
            10.0,
        );
        assert!(args.contains(&"libsvtav1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "30");
    }

    #[test]
    fn prores_format_uses_prores_ks_and_pcm() {
        let args = args_for(json!({ "videoId": "x", "format": "prores" }), 10.0);
        assert!(args.contains(&"prores_ks".to_string()));
        assert!(args.contains(&"pcm_s16le".to_string())); // not muted -> PCM audio
        assert!(!args.contains(&"+faststart".to_string())); // mov, not mp4
    }

    #[test]
    fn jpg_grabs_single_frame() {
        let args = args_for(json!({ "videoId": "x", "format": "jpg" }), 10.0);
        assert_eq!(
            args[args.iter().position(|a| a == "-frames:v").unwrap() + 1],
            "1"
        );
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn h265_codec_and_quality() {
        let args = args_for(
            json!({ "videoId": "x", "codec": "h265", "quality": 20 }),
            10.0,
        );
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        let crf = args.iter().position(|a| a == "-crf").unwrap();
        assert_eq!(args[crf + 1], "20");
    }

    #[test]
    fn webm_uses_vp9_and_opus() {
        let args = args_for(json!({ "videoId": "x", "format": "webm" }), 10.0);
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.contains(&"libopus".to_string()));
        assert!(!args.contains(&"+faststart".to_string()));
    }

    #[test]
    fn gif_has_palette_graph_and_no_audio() {
        let args = args_for(json!({ "videoId": "x", "format": "gif" }), 10.0);
        let chain = vf(&args);
        assert!(chain.contains("palettegen"), "{chain}");
        assert!(chain.contains("paletteuse"), "{chain}");
        assert!(chain.contains("fps="), "{chain}");
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn png_grabs_single_frame() {
        let args = args_for(
            json!({ "videoId": "x", "format": "png", "trim": { "start": 3.0, "end": 9.0 } }),
            10.0,
        );
        let frames = args.iter().position(|a| a == "-frames:v").unwrap();
        assert_eq!(args[frames + 1], "1");
        assert!(args.contains(&"-an".to_string()));
        // Trim still positions the grab.
        assert_eq!(
            args[args.iter().position(|a| a == "-ss").unwrap() + 1],
            "3.000"
        );
    }

    fn filter_complex(args: &[String]) -> String {
        let i = args
            .iter()
            .position(|a| a == "-filter_complex")
            .expect("has -filter_complex");
        args[i + 1].clone()
    }

    #[test]
    fn censor_vignette_pad_in_chain() {
        let args = args_for(
            json!({
                "videoId": "x",
                "censor": { "x": 10, "y": 20, "w": 100, "h": 50 },
                "censorColor": "white",
                "vignette": true,
                "pad": "9:16",
                "crop": { "x": 0, "y": 0, "w": 320, "h": 240 }
            }),
            10.0,
        );
        let chain = vf(&args);
        assert!(
            chain.contains("drawbox=x=10:y=20:w=100:h=50:color=white:t=fill"),
            "{chain}"
        );
        assert!(chain.contains("vignette"), "{chain}");
        assert!(chain.contains("pad=w="), "{chain}");
        // Censor is applied before crop (source coordinates).
        assert!(chain.find("drawbox").unwrap() < chain.find("crop=").unwrap());
    }

    #[test]
    fn denoise_sharpen_grain_in_chain() {
        let args = args_for(
            json!({ "videoId": "x", "denoise": true, "sharpen": 1.5, "grain": 20.0, "filter": "teal-orange" }),
            10.0,
        );
        let chain = vf(&args);
        assert!(chain.contains("hqdn3d"), "{chain}");
        assert!(chain.contains("unsharp=5:5:1.500"), "{chain}");
        assert!(chain.contains("noise=alls=20"), "{chain}");
        assert!(chain.contains("colorbalance="), "{chain}"); // teal-orange preset
                                                             // Order: denoise -> preset -> sharpen -> grain.
        assert!(chain.find("hqdn3d").unwrap() < chain.find("unsharp").unwrap());
        assert!(chain.find("unsharp").unwrap() < chain.find("noise=alls").unwrap());
    }

    #[test]
    fn look_presets_map_to_filters() {
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "faded" }),
            10.0
        ))
        .contains("curves="));
        assert!(
            vf(&args_for(json!({ "videoId": "x", "filter": "noir" }), 10.0)).contains("hue=s=0")
        );
        assert!(vf(&args_for(
            json!({ "videoId": "x", "filter": "vintage" }),
            10.0
        ))
        .contains("curves="));
    }

    #[test]
    fn censor_color_sanitized() {
        let args = args_for(
            json!({ "videoId": "x", "censor": { "x": 0, "y": 0, "w": 10, "h": 10 }, "censorColor": "; rm -rf" }),
            10.0,
        );
        assert!(vf(&args).contains("color=black"));
    }

    #[test]
    fn segments_concat_graph() {
        let args = args_for(
            json!({
                "videoId": "x",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 4.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("trim=start=0.000:end=1.000"), "{graph}");
        assert!(graph.contains("trim=start=3.000:end=4.000"), "{graph}");
        assert!(graph.contains("concat=n=2:v=1:a=1[cv][ca]"), "{graph}");
        assert!(args.contains(&"libx264".to_string()));
        // The concat path replaces input-side trim.
        assert!(!args.contains(&"-ss".to_string()));
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 2);
    }

    #[test]
    fn segments_muted_drops_audio_streams() {
        let args = args_for(
            json!({
                "videoId": "x",
                "mute": true,
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        let graph = filter_complex(&args);
        assert!(graph.contains("concat=n=2:v=1:a=0[cv]"), "{graph}");
        assert!(!graph.contains("atrim"), "{graph}");
        assert_eq!(args.iter().filter(|a| *a == "-map").count(), 1);
    }

    #[test]
    fn segments_carry_codec_and_faststart() {
        // The concat path must produce the same encoder settings as the simple
        // path (shared via push_video_codec): h265 tag + faststart + mapped audio.
        let args = args_for(
            json!({
                "videoId": "x",
                "codec": "h265",
                "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 2.0, "end": 3.0 }]
            }),
            5.0,
        );
        assert!(args.contains(&"-filter_complex".to_string()));
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"hvc1".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        assert!(args.contains(&"aac".to_string())); // not muted -> audio mapped
    }

    #[test]
    fn segments_ignored_for_gif() {
        // gif is not a concat target, so it falls back to the normal path.
        let args = args_for(
            json!({ "videoId": "x", "format": "gif", "segments": [{ "start": 0.0, "end": 1.0 }] }),
            5.0,
        );
        assert!(!args.contains(&"-filter_complex".to_string()));
        assert!(vf(&args).contains("palettegen"));
    }

    #[test]
    fn expected_output_secs_trim_speed_segments() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Whole clip.
        assert!(close(
            expected_output_secs(&edit(json!({ "videoId": "x" })), 10.0),
            10.0
        ));
        // Trim narrows to its length.
        assert!(close(
            expected_output_secs(
                &edit(json!({ "videoId": "x", "trim": { "start": 2.0, "end": 7.0 } })),
                10.0
            ),
            5.0
        ));
        // Speed shortens proportionally.
        assert!(close(
            expected_output_secs(&edit(json!({ "videoId": "x", "speed": 2.0 })), 10.0),
            5.0
        ));
        // Segments sum their lengths (and override trim).
        assert!(close(
            expected_output_secs(
                &edit(json!({
                    "videoId": "x",
                    "segments": [{ "start": 0.0, "end": 1.0 }, { "start": 3.0, "end": 5.0 }]
                })),
                10.0
            ),
            3.0
        ));
    }

    #[test]
    fn mp3_is_audio_only() {
        let args = args_for(
            json!({ "videoId": "x", "format": "mp3", "volume": 0.5 }),
            10.0,
        );
        assert!(args.contains(&"-vn".to_string()));
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(!args.contains(&"-vf".to_string()));
        assert!(af(&args).unwrap().contains("volume=0.500"));
    }
}
