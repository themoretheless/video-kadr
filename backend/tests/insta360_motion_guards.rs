//! Explicit read-only corpus audit: no source video or gyro payload decoding.
use std::io::{Read, Seek, SeekFrom};
#[test]
#[ignore = "requires the explicitly authorized local media inventory"]
fn local_x6_first_and_last_imu_bracket_motion_guards() {
    let inventory =
        std::env::var("VIDEO_KADR_INSTA360_INVENTORY").expect("supply authorized inventory");
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(inventory).unwrap()).unwrap();
    let (mut count, mut absent, mut unqualified, mut insufficient) = (0, 0, 0, 0);
    let (mut min_pre, mut min_post) = (f64::INFINITY, f64::INFINITY);
    for row in data["files"].as_array().unwrap() {
        if row["bytes"].as_u64().unwrap_or(0) == 0 {
            continue;
        }
        let mut file = std::fs::File::open(row["path"].as_str().unwrap()).unwrap();
        let Some(meta) = video_kadr_backend::domain::insta360_metadata::inspect(&mut file).unwrap()
        else {
            absent += 1;
            continue;
        };
        let Some(gyro) = meta
            .blocks
            .iter()
            .find(|b| b.kind == 3 && b.format == 0 && b.length >= 40 && b.length % 20 == 0)
        else {
            unqualified += 1;
            continue;
        };
        let Some(exposure) = meta
            .blocks
            .iter()
            .find(|b| b.kind == 4 && b.format == 0 && b.length >= 32 && b.length % 16 == 0)
        else {
            unqualified += 1;
            continue;
        };
        let clock = match video_kadr_backend::domain::insta360_video_clock::inspect(&mut file) {
            Ok(Some(clock)) => clock,
            _ => {
                unqualified += 1;
                continue;
            }
        };
        let fps = clock.numerator as f64 / clock.denominator as f64;
        let stamp = |file: &mut std::fs::File, at: u64| {
            file.seek(SeekFrom::Start(at)).unwrap();
            let mut b = [0; 8];
            file.read_exact(&mut b).unwrap();
            u64::from_le_bytes(b) as f64 / 1e6
        };
        let first_raw = stamp(&mut file, gyro.offset);
        let last_raw = stamp(&mut file, gyro.offset + gyro.length as u64 - 20);
        let read = |file: &mut std::fs::File, index: usize| {
            file.seek(SeekFrom::Start(exposure.offset + index as u64 * 16))
                .unwrap();
            let mut b = [0; 16];
            file.read_exact(&mut b).unwrap();
            (
                u64::from_le_bytes(b[..8].try_into().unwrap()) as f64 / 1e6,
                f64::from_le_bytes(b[8..].try_into().unwrap()),
            )
        };
        let Some(first) = meta.first_frame_timestamp else {
            unqualified += 1;
            continue;
        };
        let first = first as f64 / 1e6;
        let (mut lo, mut hi) = (0usize, exposure.length as usize / 16);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if read(&mut file, mid).0 < first {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        if lo + clock.sample_count as usize > exposure.length as usize / 16
            || clock.sample_count < 2
        {
            unqualified += 1;
            continue;
        }
        let (a, shutter_a) = read(&mut file, lo);
        let (b, _) = read(&mut file, lo + 1);
        let (last, shutter_last) = read(&mut file, lo + clock.sample_count as usize - 1);
        let (before, _) = read(&mut file, lo + clock.sample_count as usize - 2);
        if !shutter_a.is_finite()
            || !shutter_last.is_finite()
            || shutter_a <= 0.
            || shutter_last <= 0.
            || shutter_a > 1.05 / fps
            || shutter_last > 1.05 / fps
            || b <= a
            || last <= before
        {
            unqualified += 1;
            continue;
        }
        let pre = (a - 0.5 * shutter_a - first_raw) / ((b - a) * fps);
        let post =
            (last_raw - (last - 0.5 * shutter_last + (last - before))) / ((last - before) * fps);
        min_pre = min_pre.min(pre);
        min_post = min_post.min(post);
        count += 1;
        if pre < 0.1 || post < 0.1 {
            insufficient += 1;
        }
    }
    println!("read-only motion guard audit: qualified={count}, no_trailer={absent}, unsupported={unqualified}, insufficient_100ms={insufficient}, minimum_pre_clip_seconds={min_pre:.9}, minimum_post_clip_seconds={min_post:.9}");
    assert!(count > 0);
}
