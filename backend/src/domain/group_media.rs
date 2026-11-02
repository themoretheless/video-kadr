//! Decode a previously rendered flat composition frame for native group assembly.
//! File authorization and process scheduling belong to the calling service.
use super::group_surface::GroupSurface;
use anyhow::{ensure, Context, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

/// Assemble nested groups from already-rendered local composition movies.
/// Each movie's timestamp origin must match its document's local time zero.
pub fn render_project_frame(
    project: &super::nested_composition::NestedCompositionProject,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    time_us: u64,
) -> Result<GroupSurface> {
    render_project_frame_controlled(
        project,
        ffmpeg,
        rendered,
        time_us,
        std::time::Duration::from_secs(30),
        &std::sync::atomic::AtomicBool::new(false),
    )
}

pub fn render_project_frame_controlled(
    project: &super::nested_composition::NestedCompositionProject,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<GroupSurface> {
    render_project_frame_from(
        project,
        &project.root,
        ffmpeg,
        rendered,
        time_us,
        timeout,
        cancelled,
    )
}

pub fn render_project_frame_from(
    project: &super::nested_composition::NestedCompositionProject,
    composition: &str,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<GroupSurface> {
    render_project_frame_sources(
        project,
        composition,
        ffmpeg,
        rendered,
        &std::collections::BTreeMap::new(),
        time_us,
        timeout,
        cancelled,
    )
}

pub fn render_project_frame_sources(
    project: &super::nested_composition::NestedCompositionProject,
    composition: &str,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    images: &std::collections::BTreeMap<super::composition::SourceId, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<GroupSurface> {
    render_project_frame_sources_with_sample(project,composition,ffmpeg,rendered,images,time_us,timeout,cancelled,None)
}

pub fn render_project_frame_sources_with_sample(
    project: &super::nested_composition::NestedCompositionProject,
    composition: &str,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    images: &std::collections::BTreeMap<super::composition::SourceId, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
    sample:Option<super::caption_frame_clock::CaptionFrameSample>,
) -> Result<GroupSurface> {
    render_project_frame_sources_with_filter(project,composition,ffmpeg,rendered,images,time_us,timeout,cancelled,sample,None)
}

/// Apply an admitted source-frame filter before crop, transforms, effects and transitions.
/// Source ACL, filter provenance and enabled-cutout policy belong to the caller.
pub fn render_project_frame_sources_with_filter(
    project: &super::nested_composition::NestedCompositionProject,
    composition: &str,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    images: &std::collections::BTreeMap<super::composition::SourceId, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
    sample: Option<super::caption_frame_clock::CaptionFrameSample>,
    mut filter: Option<&mut dyn FnMut(&super::composition::SourceId,u64,&mut GroupSurface)->Result<()>>,
) -> Result<GroupSurface> {
    let present=filter.is_some();
    let mut contextual=|_composition:&str,_clip:&super::composition::CompositionClipId,source:&super::composition::SourceId,time,frame:&mut GroupSurface|{if let Some(filter)=filter.as_mut(){filter(source,time,frame)?;}Ok(())};
    render_project_frame_sources_with_clip_filter(project,composition,ffmpeg,rendered,images,time_us,timeout,cancelled,sample,if present{Some(&mut contextual)}else{None})
}

/// Source filtering with exact composition and clip ownership, including reused media.
pub fn render_project_frame_sources_with_clip_filter(
    project: &super::nested_composition::NestedCompositionProject,
    composition: &str,
    ffmpeg: &Path,
    rendered: &std::collections::BTreeMap<String, std::path::PathBuf>,
    images: &std::collections::BTreeMap<super::composition::SourceId, std::path::PathBuf>,
    time_us: u64,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
    sample: Option<super::caption_frame_clock::CaptionFrameSample>,
    mut filter: Option<&mut dyn FnMut(&str,&super::composition::CompositionClipId,&super::composition::SourceId,u64,&mut GroupSurface)->Result<()>>,
) -> Result<GroupSurface> {
    if let Some(sample)=sample {ensure!(sample.host_us==time_us,"caption and media frame clocks disagree");}
    use std::{sync::atomic::Ordering, time::Instant};
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "nested frame render cancelled"
    );
    ensure!(!timeout.is_zero(), "nested frame render timeout");
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("invalid nested render timeout")?;
    let surface = project.render_surfaces_from(
        time_us,
        composition,
        |id, content, local_time| {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "nested frame render cancelled"
            );
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .context("nested frame render timeout")?;
            if !rendered.contains_key(id) {
                let mut decode = |source: &super::composition::SourceId| {
                    ensure!(
                        !cancelled.load(Ordering::Relaxed),
                        "nested frame render cancelled"
                    );
                    ensure!(Instant::now() < deadline, "nested frame render timeout");
                    decode_image_surface(
                        images
                            .get(source)
                            .context("missing authorized image source")?,
                    )
                };
                let mut decode_video =
                    |source: &super::composition::SourceId,
                     source_time: super::video_clock::FrameTime,
                     hold_presented: bool,
                     clip: &super::composition::CompositionClipId| {
                        let bound=content.tracks.iter().find_map(|track|match track{super::composition::CompositionTrack::Video{clips,..}=>clips.iter().find(|entry|entry.id==*clip).and_then(|entry|entry.concat_cutout.as_ref()),_=>None});
                        ensure!(bound.is_none()||filter.is_some(),"enabled video cutout requires a verified clip mask provider");
                        let metadata = content
                            .sources
                            .get(source)
                            .context("missing video source metadata")?;
                        let remaining = deadline
                            .checked_duration_since(Instant::now())
                            .context("nested frame render timeout")?;
                        let dual_streams = content.tracks.iter().any(|track| match track {
                            super::composition::CompositionTrack::Video{clips,..} => clips.iter().any(|entry| entry.id == *clip && entry.effects.iter().any(|effect| matches!(effect,super::composition::VideoEffect::SphericalVideo{parameters,..} if parameters.source_layout == super::spherical_video::SourceLayout::DualVideoStreams))),
                            _ => false,
                        });
                        let mut frame = decode_source_frame_policy_layout(
                            ffmpeg,
                            images
                                .get(source)
                                .context("missing authorized video source")?,
                            source_time,
                            metadata.width as usize,
                            metadata.height as usize,
                            remaining,
                            cancelled,
                            hold_presented,
                            dual_streams,
                        )?;
                        if let Some(filter) = filter.as_mut() { filter(id, clip, source, source_time.microseconds()?, &mut frame)?; }
                        Ok(frame)
                    };
                if let Some(surface) = super::flat_surface::render_media_layers_with_library_at_contextual(
                    content,
                    local_time,
                    &mut || {
                        ensure!(
                            !cancelled.load(Ordering::Relaxed),
                            "nested frame render cancelled"
                        );
                        ensure!(Instant::now() < deadline, "nested frame render timeout");
                        Ok(())
                    },
                    Some(&mut decode),
                    Some(&mut decode_video),
                    &project.luts,
                    match sample {Some(sample)=>sample.local_ticks(local_time).map_err(anyhow::Error::msg)?,None=>super::caption_time::CaptionTime::Microseconds(local_time).exact_ticks()},
                )? {
                    return Ok(surface);
                }
            }
            let input = rendered
                .get(id)
                .context("missing rendered composition movie")?;
            decode_rendered_frame_controlled(
                ffmpeg,
                input,
                local_time,
                content.canvas.width as usize,
                content.canvas.height as usize,
                remaining,
                cancelled,
            )
        },
        &mut || {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "nested frame render cancelled"
            );
            ensure!(Instant::now() < deadline, "nested frame render timeout");
            Ok(())
        },
    )?;
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "nested frame render cancelled"
    );
    ensure!(Instant::now() < deadline, "nested frame render timeout");
    Ok(surface)
}

pub fn decode_rendered_frame(
    ffmpeg: &Path,
    input: &Path,
    time_us: u64,
    width: usize,
    height: usize,
) -> Result<GroupSurface> {
    decode_rendered_frame_controlled(
        ffmpeg,
        input,
        time_us,
        width,
        height,
        std::time::Duration::from_secs(30),
        &std::sync::atomic::AtomicBool::new(false),
    )
}

pub fn decode_rendered_frame_controlled(
    ffmpeg: &Path,
    input: &Path,
    time_us: u64,
    width: usize,
    height: usize,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<GroupSurface> {
    decode_frame_policy(
        ffmpeg, input, time_us, width, height, timeout, cancelled, false,
    )
}

pub(crate) fn decode_frame_policy(
    ffmpeg: &Path,
    input: &Path,
    time_us: u64,
    width: usize,
    height: usize,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
    hold_presented: bool,
) -> Result<GroupSurface> {
    decode_source_frame_policy(ffmpeg,input,super::video_clock::FrameTime::Microseconds(time_us),width,height,timeout,cancelled,hold_presented)
}

/// Decode a source timestamp without reducing Concat ticks to integer microseconds.
pub fn decode_source_frame_policy(
    ffmpeg: &Path,
    input: &Path,
    source_time: super::video_clock::FrameTime,
    width: usize,
    height: usize,
    timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool,
    hold_presented: bool,
) -> Result<GroupSurface> {
    decode_source_frame_policy_layout(ffmpeg,input,source_time,width,height,timeout,cancelled,hold_presented,false)
}
#[allow(clippy::too_many_arguments)]
pub fn decode_source_frame_policy_layout(
    ffmpeg: &Path, input: &Path, source_time: super::video_clock::FrameTime,
    width: usize, height: usize, timeout: std::time::Duration,
    cancelled: &std::sync::atomic::AtomicBool, hold_presented: bool, dual_streams: bool,
) -> Result<GroupSurface> {
    ensure!(hold_presented || matches!(source_time, super::video_clock::FrameTime::Microseconds(_)), "exact source decoding requires held presentation selection");
    let time_us = source_time.microseconds()?;
    use std::{
        sync::{atomic::Ordering, mpsc},
        time::{Duration, Instant},
    };
    ensure!(
        !cancelled.load(Ordering::Relaxed),
        "group frame decode cancelled"
    );
    ensure!(!timeout.is_zero(), "group frame decode timeout");
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("invalid group decode timeout")?;
    let mut surface = GroupSurface::transparent(width, height)?;
    ensure!(
        time_us <= 9_007_199_254_740_991,
        "frame time exceeds exact wire range"
    );
    ensure!(input.is_file(), "rendered composition file is missing");
    let time = format!("{}.{:06}", time_us / 1_000_000, time_us % 1_000_000);
    let clock = if hold_presented {Some(super::held_video_clock::resolve(ffmpeg,input,deadline,cancelled)?)} else {None};
    let seek = match clock {Some(clock)=>clock.seek_time(time_us)?,None=>time.clone()};
    let mut command = Command::new(ffmpeg);
    command.args(["-nostdin", "-v", "error", "-protocol_whitelist", crate::tools::OFFLINE_PROTOCOLS, "-ss", &seek]);
    if hold_presented {
        command.args(["-noaccurate_seek", "-seek_timestamp", "1", "-copyts"]);
    }
    command.arg("-i").arg(input);
    if !dual_streams { command.args(["-map", "0:v:0"]); }
    if hold_presented {
        let end_us = time_us + 1_000_000;
        let end = format!("{}.{:06}", end_us / 1_000_000, end_us % 1_000_000);
        let select = if let super::video_clock::FrameTime::SourceTicks(_) = source_time {
            let base = clock.unwrap().base;
            let last = source_time.last_presented_pts(base.numerator, base.denominator)?;
            format!("setpts=PTS-({}),trim=end={end},select='lte(pts,{last})',scale={width}:{height}",clock.unwrap().origin)
        } else {
            format!("setpts=PTS-({}),trim=end={end},select='lte(pts,{time}/TB+0.000001)',scale={width}:{height}",clock.unwrap().origin)
        };
        if dual_streams {
            command.args(["-filter_complex", &format!("[0:v:0][0:v:1]hstack=inputs=2:shortest=1,{select}[packed]"), "-map", "[packed]", "-fps_mode", "passthrough"]);
        } else { command.args(["-vf", &select, "-fps_mode", "passthrough"]); }
    } else {
        if dual_streams {
            command.args(["-frames:v", "1", "-filter_complex", &format!("[0:v:0][0:v:1]hstack=inputs=2:shortest=1,scale={width}:{height}[packed]"), "-map", "[packed]"]);
        } else { command.args(["-frames:v", "1", "-vf", &format!("scale={width}:{height}")]); }
    }
    let mut process = command
        .args(["-an", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("start group frame decoder")?;
    let stdout = process
        .stdout
        .take()
        .context("group frame decoder stdout")?;
    let limit = surface.rgba.len();
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let result = if hold_presented {
            let mut stream = stdout;
            let mut last = Vec::new();
            let mut next = vec![0; limit];
            let mut frames = 0;
            (|| -> std::io::Result<Vec<u8>> {
                loop {
                    let mut count = 0;
                    while count < limit {
                        let read = stream.read(&mut next[count..])?;
                        if read == 0 {
                            break;
                        }
                        count += read;
                    }
                    if count == 0 {
                        return Ok(last);
                    }
                    if count != limit {
                        return Err(std::io::Error::other("incomplete video frame"));
                    }
                    frames += 1;
                    if frames > 4096 {
                        return Err(std::io::Error::other("video seek frame budget exceeded"));
                    }
                    std::mem::swap(&mut last, &mut next);
                    if next.len() != limit {
                        next.resize(limit, 0);
                    }
                }
            })()
        } else {
            let mut bytes = Vec::with_capacity(limit);
            stdout
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        };
        let _ = sender.send(result);
    });
    let outcome = (|| -> Result<_> {
        let bytes = loop {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "group frame decode cancelled"
            );
            ensure!(Instant::now() < deadline, "group frame decode timeout");
            match receiver.recv_timeout(Duration::from_millis(10)) {
                Ok(result) => break result.context("read group frame")?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("group frame reader stopped")
                }
            }
        };
        ensure!(
            bytes.len() <= limit,
            "group decoder exceeded frame byte budget"
        );
        loop {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "group frame decode cancelled"
            );
            ensure!(Instant::now() < deadline, "group frame decode timeout");
            if let Some(status) = process.try_wait().context("wait for group frame decoder")? {
                return Ok((bytes, status));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if outcome.is_err() {
        let _ = process.kill();
        let _ = process.wait();
    }
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("group frame reader panicked"))?;
    let (bytes, status) = outcome?;
    ensure!(status.success(), "group frame decoder failed");
    // A seek past EOF has no output and represents transparent source overhang.
    if bytes.is_empty() {
        return Ok(surface);
    }
    ensure!(
        bytes.len() == surface.rgba.len(),
        "incomplete decoded group frame"
    );
    surface.rgba = bytes;
    Ok(surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_video_frames_respect_fractional_rate_pts_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let raw: Vec<u8> = (0..8u8).flat_map(|k| [k * 29, 255 - k * 23, k * 17, 255].repeat(4)).collect();
        let raw_path = directory.path().join("frames.rgba");
        std::fs::write(&raw_path, &raw).unwrap();
        for rate in ["24000/1001", "30000/1001", "60000/1001"] {
            let input = directory.path().join("fractional.nut");
            let encoded = Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pixel_format", "rgba", "-video_size", "2x2", "-framerate", rate, "-i"]).arg(&raw_path).args(["-c:v", "ffv1", "-pix_fmt", "bgra"]).arg(&input).output().unwrap();
            assert!(encoded.status.success(), "{}", String::from_utf8_lossy(&encoded.stderr));
            let probe = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=time_base:frame=pts", "-of", "json"]).arg(&input).output().unwrap();
            assert!(probe.status.success());
            let metadata: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
            let base: Vec<u64> = metadata["streams"][0]["time_base"].as_str().unwrap().split('/').map(|v| v.parse().unwrap()).collect();
            let frames = metadata["frames"].as_array().unwrap();
            assert_eq!(frames.len(), 8);
            for (index, frame) in frames.iter().enumerate().skip(1) {
                let numerator = frame["pts"].as_u64().unwrap() * base[0] * 1_000_000;
                let first_us = numerator.div_ceil(base[1]);
                for (time_us, expected_index) in [(first_us - 1, index - 1), (first_us, index)] {
                    let actual = decode_frame_policy(Path::new("ffmpeg"), &input, time_us, 2, 2, std::time::Duration::from_secs(5), &std::sync::atomic::AtomicBool::new(false), true).unwrap();
                    assert_eq!(actual.rgba, raw[expected_index * 16..(expected_index + 1) * 16], "{rate}: frame{index} at {time_us}us");
                }
            }
        }
    }
    #[test]
    fn raster_formats_decode_real_files_with_alpha_and_dimension_limits() {
        let directory = tempfile::tempdir().unwrap();
        for extension in ["png", "jpg", "webp", "gif", "bmp", "tiff"] {
            let path = directory.path().join(format!("image.{extension}"));
            let alpha = if matches!(extension, "png" | "webp" | "tiff") {
                128
            } else {
                255
            };
            if extension == "jpg" {
                image::RgbImage::from_raw(2, 2, [10, 100, 200].repeat(4))
                    .unwrap()
                    .save(&path)
                    .unwrap();
            } else {
                image::RgbaImage::from_raw(2, 2, [10, 100, 200, alpha].repeat(4))
                    .unwrap()
                    .save(&path)
                    .unwrap();
            }
            let frame = decode_image_surface(&path)
                .unwrap_or_else(|error| panic!("{extension}: {error:#}"));
            assert_eq!((frame.width, frame.height), (2, 2));
            for pixel in frame.rgba.chunks_exact(4) {
                for (channel, expected) in pixel[..3].iter().zip([10u8, 100, 200]) {
                    assert!(channel.abs_diff(expected) <= 10, "{extension}: {pixel:?}");
                }
                assert_eq!(pixel[3], alpha, "{extension}");
            }
        }
        let oversized = directory.path().join("oversized.png");
        image::RgbaImage::new(3841, 1).save(&oversized).unwrap();
        assert!(decode_image_surface(&oversized).is_err());
        let corrupt = directory.path().join("corrupt.jpg");
        std::fs::write(&corrupt, b"not an image").unwrap();
        assert!(decode_image_surface(&corrupt).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn decoder_timeout_reaps_process_and_cancellation_prevents_spawn() {
        use std::{
            os::unix::fs::PermissionsExt,
            sync::atomic::AtomicBool,
            time::{Duration, Instant},
        };
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("slow-decoder");
        let input = directory.path().join("input");
        std::fs::write(&input, b"fixture").unwrap();
        std::fs::write(&executable, b"#!/bin/sh\nexec sleep 5\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let start = Instant::now();
        let error = decode_rendered_frame_controlled(
            &executable,
            &input,
            0,
            2,
            2,
            Duration::from_millis(50),
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timeout"));
        assert!(start.elapsed() < Duration::from_secs(2));
        let error = decode_rendered_frame_controlled(
            Path::new("/missing-decoder"),
            &input,
            0,
            2,
            2,
            Duration::from_secs(1),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        let cancelled = AtomicBool::new(false);
        let start = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(50));
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            let error = decode_rendered_frame_controlled(
                &executable,
                &input,
                0,
                2,
                2,
                Duration::from_secs(4),
                &cancelled,
            )
            .unwrap_err();
            assert!(error.to_string().contains("cancelled"));
        });
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn real_nested_movies_preserve_source_offset_and_group_boundary() {
        use crate::domain::{
            composition::{AnimatableValue, BlendMode, CanvasSpec, Composition, TransformSpec},
            composition_graph::CompositionReferenceTiming,
            nested_composition::{
                CompositionInstance, NestedCompositionDocument, NestedCompositionProject,
            },
        };
        use std::{collections::BTreeMap, io::Write};
        let ffmpeg = Path::new("/opt/homebrew/bin/ffmpeg");
        if !ffmpeg.is_file() {
            eprintln!("FFmpeg unavailable: nested movie fixture not run");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let root_file = directory.path().join("root.nut");
        let child_file = directory.path().join("child.nut");
        for (file, frames) in [
            (&root_file, [[255, 0, 0, 255]; 3]),
            (
                &child_file,
                [[0, 255, 0, 255], [0, 0, 255, 128], [0, 0, 255, 255]],
            ),
        ] {
            let mut process = Command::new(ffmpeg)
                .args([
                    "-nostdin", "-v", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", "2x2",
                    "-r", "2", "-i", "pipe:0", "-c:v", "rawvideo", "-f", "nut",
                ])
                .arg(file)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = process.stdin.take().unwrap();
            for pixel in frames {
                input.write_all(&pixel.repeat(4)).unwrap();
            }
            drop(input);
            assert!(process.wait().unwrap().success());
        }
        let mut canvas = CanvasSpec::default();
        canvas.width = 2;
        canvas.height = 2;
        let child = NestedCompositionDocument {
            duration_us: Some(1500000),
            content: Composition::new(canvas),
            instances: vec![],
        };
        let mut root = child.clone();
        root.instances.push(CompositionInstance {
            id: "group".into(),
            timing: CompositionReferenceTiming {
                composition: "child".into(),
                timeline_start_us: 500000,
                source_in_us: 500000,
                source_out_us: 1500000,
            },
            animation_offset_us: 0,
            transform: TransformSpec::default(),
            opacity: AnimatableValue::constant(0.5),
            blend_mode: BlendMode::Normal,
        });
        let project = NestedCompositionProject {
            project_media: Default::default(),
            fonts:vec![],
            timeline_order: vec![],
            luts:vec![],
            schema_version: 1,
            root: "root".into(),
            compositions: BTreeMap::from([("root".into(), root), ("child".into(), child)]),
        };
        let files = BTreeMap::from([("root".into(), root_file), ("child".into(), child_file)]);
        assert_eq!(
            render_project_frame(&project, ffmpeg, &files, 0)
                .unwrap()
                .rgba,
            [255, 0, 0, 255].repeat(4)
        );
        assert_eq!(
            render_project_frame(&project, ffmpeg, &files, 500000)
                .unwrap()
                .rgba,
            [191, 0, 64, 255].repeat(4)
        );
        assert_eq!(
            render_project_frame(&project, ffmpeg, &files, 1000000)
                .unwrap()
                .rgba,
            [128, 0, 128, 255].repeat(4)
        );
        assert_eq!(
            render_project_frame(&project, ffmpeg, &files, 1500000)
                .unwrap()
                .rgba,
            vec![0; 16]
        );
        assert_eq!(
            render_project_frame(&project, ffmpeg, &BTreeMap::new(), 0)
                .unwrap()
                .rgba,
            [0, 0, 0, 255].repeat(4)
        );
    }
    #[test]
    fn real_decoder_retains_rgba_and_returns_blank_after_eof() {
        let ffmpeg = Path::new("/opt/homebrew/bin/ffmpeg");
        if !ffmpeg.is_file() {
            eprintln!("FFmpeg unavailable: decoder fixture not run");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("group.nut");
        assert!(Command::new(ffmpeg)
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=2x2:d=1:r=2,format=rgba",
                "-c:v",
                "rawvideo",
                "-f",
                "nut"
            ])
            .arg(&file)
            .status()
            .unwrap()
            .success());
        let frame = decode_rendered_frame(ffmpeg, &file, 500000, 2, 2).unwrap();
        for pixel in frame.rgba.chunks_exact(4) {
            assert!(pixel[0] > 240 && pixel[1] < 10 && pixel[2] < 10 && pixel[3] == 255);
        }
        assert_eq!(
            decode_rendered_frame(ffmpeg, &file, 2000000, 2, 2)
                .unwrap()
                .rgba,
            vec![0; 16]
        );
        assert!(decode_rendered_frame(ffmpeg, &file, u64::MAX, 2, 2).is_err());
    }
}

/// Bounded raster image decoder; source authorization remains the caller's responsibility.
pub fn decode_image_surface(path: &Path) -> Result<GroupSurface> {
    let mut reader = image::ImageReader::open(path)
        .context("open group image")?
        .with_guessed_format()
        .context("detect group image format")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(3840);
    limits.max_image_height = Some(2160);
    limits.max_alloc = Some(3840 * 2160 * 4);
    reader.limits(limits);
    let image = reader.decode().context("decode group image")?.into_rgba8();
    let mut surface = GroupSurface::transparent(image.width() as usize, image.height() as usize)?;
    surface.rgba = image.into_raw();
    Ok(surface)
}
