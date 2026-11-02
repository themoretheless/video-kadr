use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
use video_kadr_backend::domain::{
    composition::{Composition, SourceId, TransformSpec},
    group_media,
    nested_composition::NestedCompositionProject,
};
fn project() -> NestedCompositionProject {
    let mut content = serde_json::to_value(Composition::new(Default::default())).unwrap();
    content["canvas"]["width"] = json!(2);
    content["canvas"]["height"] = json!(2);
    content["canvas"]["background"]["alpha"] = json!(0);
    content["sources"] = json!({"raw":{"id":"raw","kind":"video","durationTicks":1000000,"width":2,"height":2,"hasAudio":false}});
    content["tracks"] = json!([{"kind":"video","id":"video","name":"Video","clips":[{"id":"clip","sourceId":"raw","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":1000000,"speed":1},"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal"}]}]);
    serde_json::from_value(json!({"schemaVersion":1,"root":"root","compositions":{"root":{"content":content,"instances":[]}}})).unwrap()
}
fn render(
    ffmpeg: &Path,
    source: PathBuf,
    time: u64,
    limit: Duration,
    cancel: &AtomicBool,
) -> anyhow::Result<Vec<u8>> {
    Ok(group_media::render_project_frame_sources(
        &project(),
        "root",
        ffmpeg,
        &BTreeMap::new(),
        &BTreeMap::from([(SourceId::parse("raw").unwrap(), source)]),
        time,
        limit,
        cancel,
    )?
    .rgba)
}
#[test]
#[ignore = "requires actual FFmpeg/ffprobe; independently generated RGB frames"]
fn held_video_uses_video_origin_when_audio_starts_earlier() {
    let root = tempfile::tempdir().unwrap();
    let raw = root.path().join("frames.rgba");
    let frames = [
        [23, 15, 80, 255],
        [100, 130, 150, 255],
        [200, 210, 240, 255],
    ]
    .map(|p| p.repeat(4));
    std::fs::write(&raw, frames.concat()).unwrap();
    let ffmpeg = Path::new("/opt/homebrew/bin/ffmpeg");
    #[cfg(unix)]
    let decoder = {
        use std::os::unix::fs::PermissionsExt;
        let directory = root.path().join("wrappers");
        std::fs::create_dir(&directory).unwrap();
        for(name,script)in [("ffmpeg",b"#!/bin/sh\nexec /opt/homebrew/bin/ffmpeg \"$@\"\n".as_slice()),("ffprobe",b"#!/bin/sh\nprintf x >> \"${0%/*}/clock-count\"\nexec /opt/homebrew/bin/ffprobe \"$@\"\n".as_slice())]{let file=directory.join(name);std::fs::write(&file,script).unwrap();std::fs::set_permissions(file,std::fs::Permissions::from_mode(0o700)).unwrap();}
        directory.join("ffmpeg")
    };
    #[cfg(not(unix))]
    let decoder = ffmpeg.to_owned();
    for (case_index, (rate, offset, audio, vfr)) in [
        ("10", 20, true, true),
        ("30000/1001", 60, true, false),
        ("60000/1001", 120, false, false),
        ("10", 0, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        let file = root.path().join(format!("case-{offset}.nut"));
        let mut command = std::process::Command::new(ffmpeg);
        command
            .args([
                "-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2", "-r",
                rate, "-i",
            ])
            .arg(&raw);
        if audio {
            command.args(["-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono:d=3"]);
        }
        command.args([
            "-vf",
            &format!("setpts={}+{offset}", if vfr { "N*N" } else { "N" }),
            "-map",
            "0:v:0",
        ]);
        if audio {
            command.args(["-map", "1:a:0", "-c:a", "pcm_s16le"]);
        }
        let encoded = command
            .args([
                "-frames:v",
                "3",
                "-fps_mode",
                "passthrough",
                "-c:v",
                "ffv1",
                "-pix_fmt",
                "bgra",
                "-avoid_negative_ts",
                "disabled",
            ])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let probed = std::process::Command::new("/opt/homebrew/bin/ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=time_base,start_pts:frame=best_effort_timestamp",
                "-of",
                "json",
            ])
            .arg(&file)
            .output()
            .unwrap();
        assert!(probed.status.success());
        let metadata: serde_json::Value = serde_json::from_slice(&probed.stdout).unwrap();
        let base: Vec<u64> = metadata["streams"][0]["time_base"]
            .as_str()
            .unwrap()
            .split('/')
            .map(|v| v.parse().unwrap())
            .collect();
        let first = metadata["frames"][0]["best_effort_timestamp"]
            .as_u64()
            .unwrap();
        if offset > 0 {
            assert!(first * base[0] * 1000000 / base[1] >= 2000000)
        } else {
            assert_eq!(first, 0)
        };
        let active = root.path().join("active.nut");
        std::fs::copy(&file, &active).unwrap();
        let mut samples = vec![(0, 0)];
        for index in 1..3 {
            let pts = metadata["frames"][index]["best_effort_timestamp"]
                .as_u64()
                .unwrap();
            let boundary = ((pts - first) * base[0] * 1000000).div_ceil(base[1]);
            samples.extend([(boundary - 1, index - 1), (boundary, index)]);
        }
        for (time, ordinal) in samples {
            let actual = render(
                &decoder,
                active.clone(),
                time,
                Duration::from_secs(5),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(
                actual, frames[ordinal],
                "{rate} audio={audio} source={time}"
            );
        }
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(root.path().join("wrappers/clock-count"))
                .unwrap()
                .len(),
            case_index as u64 + 1,
            "one probe per stable file, fresh probe after replacement"
        );
    }
}

#[cfg(unix)]
#[test]
fn clock_probe_timeout_and_cancellation_reap_the_owned_process() {
    use std::{os::unix::fs::PermissionsExt, sync::atomic::Ordering, time::Instant};
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    std::fs::write(&source, b"private test source").unwrap();
    let ffmpeg = root.path().join("ffmpeg");
    std::fs::write(&ffmpeg, b"unused").unwrap();
    let probe = root.path().join("ffprobe");
    let pid = root.path().join("probe.pid");
    std::fs::write(
        &probe,
        format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec sleep 5\n",
            pid.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cancelled = AtomicBool::new(false);
    for explicit_cancel in [false, true] {
        let _ = std::fs::remove_file(&pid);
        let started = Instant::now();
        let result = std::thread::scope(|scope| {
            if explicit_cancel {
                scope.spawn(|| {
                    let until = Instant::now() + Duration::from_secs(1);
                    while !pid.exists() && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                    cancelled.store(true, Ordering::Relaxed);
                });
            }
            render(
                &ffmpeg,
                source.clone(),
                0,
                if explicit_cancel {
                    Duration::from_secs(4)
                } else {
                    Duration::from_millis(500)
                },
                &cancelled,
            )
        });
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains(if explicit_cancel {
                "cancelled"
            } else {
                "timeout"
            }),
            "{error}"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        let child = std::fs::read_to_string(&pid).expect("owned probe must have started");
        assert!(!std::process::Command::new("kill")
            .args(["-0", child.trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success());
    }
}
