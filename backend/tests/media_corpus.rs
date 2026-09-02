//! Tiny, source-generated media corpus checked against the real FFmpeg and
//! ffprobe binaries. The committed manifest is the stable semantic contract;
//! generated files stay inside a disposable temp directory.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use video_kadr_backend::domain::media_probe::{ProbeResult, StreamKind};

const MANIFEST: &str = include_str!("fixtures/media-corpus-v1/manifest.json");

fn tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run(mut command: Command, context: &str) -> Output {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{context}: {error}"));
    assert!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn ffmpeg(args: &[&str], output: &Path) {
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-y"]);
    command.args(args).arg(output);
    run(command, &format!("generate {}", output.display()));
}

fn probe(path: &Path, packets: bool) -> Value {
    let mut command = Command::new("ffprobe");
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ]);
    if packets {
        command.arg("-show_packets");
    }
    command.arg(path);
    let output = run(command, &format!("probe {}", path.display()));
    serde_json::from_slice(&output.stdout).expect("ffprobe returned JSON")
}

fn normalized(path: &Path) -> ProbeResult {
    ProbeResult::from_ffprobe_json(&probe(path, false)).expect("normalize ffprobe JSON")
}

#[test]
fn generated_media_corpus_matches_ffprobe_reference_semantics() {
    if !tool_available("ffmpeg") || !tool_available("ffprobe") {
        eprintln!("skipping generated media corpus: ffmpeg/ffprobe not on PATH");
        return;
    }

    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    let documented: BTreeSet<_> = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        documented,
        BTreeSet::from([
            "corrupt",
            "hdr",
            "rotation",
            "subtitles",
            "surround51",
            "vfr"
        ])
    );

    let directory = tempfile::tempdir().unwrap();

    let vfr = directory.path().join("vfr.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=10:duration=1",
            "-vf",
            "setpts=if(lt(N\\,5)\\,N/(10*TB)\\,(N-4)/(2*TB))",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
        ],
        &vfr,
    );
    let vfr_raw = probe(&vfr, true);
    let packet_times: Vec<f64> = vfr_raw["packets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|packet| packet["codec_type"] == "video")
        .filter_map(|packet| packet["pts_time"].as_str()?.parse().ok())
        .collect();
    let deltas: BTreeSet<_> = packet_times
        .windows(2)
        .map(|window| ((window[1] - window[0]) * 1_000.0).round() as i64)
        .collect();
    assert!(
        deltas.len() >= 2,
        "expected VFR packet deltas, got {deltas:?}"
    );
    assert!(normalized(&vfr)
        .streams
        .iter()
        .any(|stream| stream.kind == StreamKind::Video));

    let hdr = directory.path().join("hdr.mkv");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=64x48:r=5:d=0.2",
            "-frames:v",
            "1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-x264-params",
            "colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc",
        ],
        &hdr,
    );
    let hdr_video = normalized(&hdr)
        .streams
        .into_iter()
        .find(|stream| stream.kind == StreamKind::Video)
        .unwrap();
    assert_eq!(hdr_video.color.primaries.as_deref(), Some("bt2020"));
    assert_eq!(hdr_video.color.transfer.as_deref(), Some("smpte2084"));
    assert_eq!(hdr_video.color.space.as_deref(), Some("bt2020nc"));

    let rotation_base = directory.path().join("rotation-base.mp4");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48:rate=5:duration=0.2",
            "-frames:v",
            "1",
            "-c:v",
            "mpeg4",
        ],
        &rotation_base,
    );
    let rotation = directory.path().join("rotation.mp4");
    let mut rotate_command = Command::new("ffmpeg");
    rotate_command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-display_rotation",
            "90",
            "-i",
        ])
        .arg(&rotation_base)
        .args(["-c", "copy"])
        .arg(&rotation);
    run(rotate_command, "add rotation display matrix");
    let rotated_video = normalized(&rotation)
        .streams
        .into_iter()
        .find(|stream| stream.kind == StreamKind::Video)
        .unwrap();
    assert_eq!(rotated_video.rotation_degrees, 90);

    let surround = directory.path().join("surround51.m4a");
    ffmpeg(
        &[
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=5.1:sample_rate=48000",
            "-t",
            "0.1",
            "-c:a",
            "aac",
        ],
        &surround,
    );
    let surround_audio = normalized(&surround)
        .streams
        .into_iter()
        .find(|stream| stream.kind == StreamKind::Audio)
        .unwrap();
    assert_eq!(surround_audio.channels, Some(6));
    assert_eq!(surround_audio.channel_layout.as_deref(), Some("5.1"));

    let captions = directory.path().join("captions.srt");
    std::fs::write(&captions, "1\n00:00:00,000 --> 00:00:00,150\nПривет\n").unwrap();
    let subtitles = directory.path().join("subtitles.mkv");
    let mut subtitle_command = Command::new("ffmpeg");
    subtitle_command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x48:r=5:d=0.2",
            "-f",
            "srt",
            "-i",
        ])
        .arg(&captions)
        .args(["-c:v", "ffv1", "-c:s", "srt", "-shortest"])
        .arg(&subtitles);
    run(subtitle_command, "generate subtitle fixture");
    assert!(normalized(&subtitles)
        .streams
        .iter()
        .any(|stream| stream.kind == StreamKind::Subtitle));

    let corrupt = directory.path().join("corrupt.mp4");
    std::fs::write(&corrupt, b"not-a-media-container").unwrap();
    let corrupt_output = Command::new("ffprobe")
        .args(["-hide_banner", "-loglevel", "error"])
        .arg(&corrupt)
        .output()
        .unwrap();
    assert!(!corrupt_output.status.success());
}
