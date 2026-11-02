//! Explicit private qualification helper: no fixture or personal image enters git.
use video_kadr_backend::domain::{
    group_media, insta360_camera_profile, insta360_metadata,
    linear_color_frame::LinearColorFrame,
    spherical_video::{Input, Output, SphericalVideo, StabilizationMode},
    video_clock::FrameTime,
};
#[test]
#[ignore = "explicit local packed video, embedded source calibration footer and private output directory"]
fn prepare_private_pose_panorama_sequence() {
    let input = std::env::var("VIDEO_KADR_REID_POSE_PACKED").unwrap();
    let source = std::env::var("VIDEO_KADR_REID_POSE_SOURCE").unwrap();
    let folder = std::path::PathBuf::from(std::env::var("VIDEO_KADR_REID_POSE_OUTPUT").unwrap());
    std::fs::create_dir_all(&folder).unwrap();
    let metadata = insta360_metadata::inspect(&mut std::fs::File::open(source).unwrap())
        .unwrap()
        .unwrap();
    let profile = insta360_camera_profile::inspect(&metadata).profile.unwrap();
    let p = SphericalVideo {
        input: Input::DualFisheye,
        output: Output::Equirectangular,
        stabilization_mode: StabilizationMode::Disabled,
        camera_profile: Some(profile),
        ..Default::default()
    };
    for i in 0..17 {
        let raw = group_media::decode_source_frame_policy_layout(
            std::path::Path::new("/opt/homebrew/bin/ffmpeg"),
            std::path::Path::new(&input),
            FrameTime::Microseconds(i * 250000),
            512,
            256,
            std::time::Duration::from_secs(30),
            &std::sync::atomic::AtomicBool::new(false),
            true,
            false,
        )
        .unwrap();
        let mut linear = LinearColorFrame::from_surface(&raw, &mut || Ok(())).unwrap();
        p.apply_linear(&mut linear, i as f64 / 4., &mut || Ok(()))
            .unwrap();
        let frame = linear.to_surface(&mut || Ok(())).unwrap();
        image::save_buffer(
            folder.join(format!("pose-{i:02}.png")),
            &frame.rgba,
            512,
            256,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}
