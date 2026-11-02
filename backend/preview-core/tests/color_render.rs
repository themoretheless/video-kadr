//! Explicit external-renderer qualification, separate from host-independent tests.
use std::process::Command;
use video_kadr_preview_core::{ColorExportFrames, ColorSampleRequest};

#[test]
#[ignore = "requires VIDEO_KADR_COLOR_FFMPEG and VIDEO_KADR_COLOR_FONT"]
fn native_commands_render_fractional_frames_byte_exact() {
    let ffmpeg = std::env::var("VIDEO_KADR_COLOR_FFMPEG").expect("set renderer path");
    let font = std::env::var("VIDEO_KADR_COLOR_FONT").expect("set font path");
    let request: ColorSampleRequest = serde_json::from_value(serde_json::json!({
        "track":{"keys":[
            {"t_us":-0.5,"value":{"r":255,"g":0,"b":0,"a":128},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}},
            {"t_us":1000000.5,"value":{"r":0,"g":0,"b":255,"a":255},"in_":[0.6666666666666666,0.6666666666666666],"out":[0.3333333333333333,0.3333333333333333],"segment":{"kind":"Linear"}}
        ],"extrapolate":{"before":"Hold","after":"Hold"}},
        "default":{"r":0,"g":0,"b":0,"a":255},"times_us":[]
    })).unwrap();
    let expected = ColorExportFrames::new(request.clone(), 30000, 1001, 1000000)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    struct PrivateDirectory(std::path::PathBuf);
    impl Drop for PrivateDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "video-kadr-native-color-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let directory = PrivateDirectory(path);
    let path = directory.0.join("commands");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .unwrap();
    }
    ColorExportFrames::new(request.clone(), 30000, 1001, 1000000)
        .unwrap()
        .write_drawtext_commands(&mut file, "text_1")
        .unwrap();
    file.sync_all().unwrap();
    let escape = |value: &str| {
        format!(
            "'{}'",
            value
                .replace('\\', "\\\\")
                .replace('\'', "'\\''")
                .replace(':', "\\:")
        )
    };
    let graph = format!("color=black@0.0:s=64x64:r=30000/1001,format=rgba,sendcmd=f={},drawtext@text_1=fontfile={}:text=M:fontsize=48:x=0:y=0:fontcolor=white,colorchannelmixer@text_1=aa=1",
        escape(path.to_str().unwrap()), escape(&font));
    let result = Command::new(&ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &graph,
            "-frames:v",
            "30",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let size = 64 * 64 * 4;
    assert_eq!(result.stdout.len(), size * expected.len());
    let interior = (0..size)
        .step_by(4)
        .max_by_key(|&i| result.stdout[i + 3])
        .unwrap();
    for (index, (_, color)) in expected.iter().enumerate() {
        let start = index * size + interior;
        assert_eq!(
            &result.stdout[start..start + 4],
            &[color.r, color.g, color.b, color.a],
            "frame {index}"
        );
    }
    let solid_path = directory.0.join("solid_commands");
    ColorExportFrames::new(request, 30000, 1001, 1000000)
        .unwrap()
        .write_solid_commands(&mut std::fs::File::create(&solid_path).unwrap(), "solid_1")
        .unwrap();
    let graph = format!("color=white:s=2x2:r=30000/1001,format=rgba,sendcmd=f={},colorchannelmixer@solid_1=rr=1:gg=1:bb=1:aa=1", escape(solid_path.to_str().unwrap()));
    let result = Command::new(&ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            &graph,
            "-frames:v",
            "30",
            "-pix_fmt",
            "rgba",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.len(), 16 * expected.len());
    for (index, (_, color)) in expected.iter().enumerate() {
        for pixel in result.stdout[index * 16..(index + 1) * 16].chunks_exact(4) {
            assert_eq!(
                pixel,
                &[color.r, color.g, color.b, color.a],
                "solid frame {index}"
            );
        }
    }
}
