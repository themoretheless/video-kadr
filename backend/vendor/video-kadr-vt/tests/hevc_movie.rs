#![cfg(target_os = "macos")]
use std::{io::Cursor, process::Command};
use video_kadr_vt::movie::HevcMovie;

#[test]
fn real_mp4_and_mov_probe_and_independent_decode() {
    for mov in [false, true] {
        let mut movie = HevcMovie::new(
            Cursor::new(Vec::new()),
            mov,
            64,
            48,
            30000,
            1001,
            1_000_000,
            2,
        )
        .unwrap();
        for _ in 0..3 {
            movie.frame(&[255, 0, 0, 255].repeat(64 * 48)).unwrap();
        }
        let data = movie.finish().unwrap().into_inner();
        let path = std::env::temp_dir().join(format!(
            "kadr-hevc-{}-{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            if mov { "mov" } else { "mp4" }
        ));
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let cleanup = Cleanup(path);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&cleanup.0)
            .unwrap();
        file.write_all(&data).unwrap();
        drop(file);
        let probe=Command::new("ffprobe").args(["-v","error","-count_frames","-show_entries","stream=codec_name,codec_tag_string,width,height,r_frame_rate,time_base,nb_read_frames,color_space,color_transfer,color_primaries","-of","default=noprint_wrappers=1"]).arg(&cleanup.0).output().unwrap();
        assert!(
            probe.status.success(),
            "{}",
            String::from_utf8_lossy(&probe.stderr)
        );
        let info = String::from_utf8(probe.stdout).unwrap();
        for expected in [
            "codec_name=hevc",
            "codec_tag_string=hvc1",
            "width=64",
            "height=48",
            "r_frame_rate=30000/1001",
            "time_base=1/30000",
            "nb_read_frames=3",
            "color_space=bt709",
            "color_transfer=bt709",
            "color_primaries=bt709",
        ] {
            assert!(
                info.lines().any(|line| line == expected),
                "missing {expected}: {info}"
            );
        }
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-threads", "1", "-i"])
            .arg(&cleanup.0)
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 64 * 48 * 3 * 3);
        for rgb in decoded.stdout.chunks_exact(3) {
            assert!(rgb[0] >= 245 && rgb[1] <= 10 && rgb[2] <= 10, "{rgb:?}");
        }
    }
}

#[test]
fn invalid_or_failed_stream_cannot_finish() {
    assert!(HevcMovie::new(Cursor::new(Vec::new()), false, 63, 48, 30, 1, 1_000_000, 1).is_err());
    let movie =
        HevcMovie::new(Cursor::new(Vec::new()), false, 64, 48, 30, 1, 1_000_000, 1).unwrap();
    assert!(movie.finish().is_err());
    let mut movie =
        HevcMovie::new(Cursor::new(Vec::new()), false, 64, 48, 30, 1, 1_000_000, 1).unwrap();
    assert!(movie.frame(&[0; 10]).is_err());
    assert!(movie.frame(&[255; 64 * 48 * 4]).is_err());
    assert!(movie.finish().is_err());
}
