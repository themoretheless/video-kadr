use serde_json::json;
use video_kadr_backend::domain::{
    composition::{CanvasSpec, TransformSpec, VideoClip},
    concat_clip_animation::{Pose, SourceAnimation},
    concat_project_transitions::{
        effect_clock, source_pose, visual_layers, visual_range_clip_indices,
    },
    concat_retime_edit::{Clock, Point},
};
fn raw(index: usize, speed: f64, curved: bool) -> Clock {
    let (start, duration, source_start) = [
        (0.0199996, 2.01900049, 0.0199996),
        (0.65065049, 2.01900049, 1.01900049),
        (0.02100049, 2.0199996, 1.01900049),
    ][index];
    Clock {
        start,
        duration,
        source_start,
        speed,
        speed_curve: curved.then(|| vec![Point { at: 0., speed: 1. }, Point { at: 1., speed: 3. }]),
    }
}
fn canvas(num: u32, den: u32) -> CanvasSpec {
    serde_json::from_value(json!({"width":16,"height":16,"fpsMilli":(u64::from(num)*1000/u64::from(den))as u32,"fpsRate":{"numerator":num,"denominator":den},"background":{"red":0,"green":0,"blue":0,"alpha":0}})).unwrap()
}
fn clip(raw: &Clock, num: u32, den: u32) -> VideoClip {
    let mut clip:VideoClip=serde_json::from_value(json!({"id":"clock","sourceId":"source","placement":raw.placement(num,den).unwrap(),"transform":TransformSpec::default(),"opacity":{"mode":"constant","value":1},"blendMode":"normal","sourceAudioEnabled":false})).unwrap();
    clip.transform.concat_animation=Some(SourceAnimation{duration_ticks:(raw.duration*1e6).round()as i64,keys:serde_json::from_value(json!({"opacity":[{"at":0.,"value":0.2},{"at":1.,"value":0.8,"ease":[0.42,0.,0.58,1.]}]})).unwrap(),rest:Pose{scale_x:1.,scale_y:1.,x:0.,y:0.,rotation_degrees:0.,opacity:1.},stretch:[1.,1.],canvas:[16.,16.]});
    clip
}
#[test]
#[ignore = "requires original raw clock, constant clock and opacity oracle records"]
fn active_layers_pose_and_effect_time_match_original_frame_grid() {
    let text = std::fs::read_to_string(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).unwrap();
    let mut frames = 0;
    let mut boundaries = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        let (index, num, den, exact, source, start, duration, opacity, curved, speed) = match f[0] {
            "8" => (
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
                f[5].parse::<i128>().unwrap(),
                f[6].parse::<i128>().unwrap(),
                f[7].parse::<i128>().unwrap(),
                f[8].parse::<i128>().unwrap(),
                f[9].parse::<u32>().unwrap(),
                true,
                1.75,
            ),
            "11" => (
                f[1].parse().unwrap(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
                f[6].parse().unwrap(),
                f[7].parse().unwrap(),
                f[8].parse().unwrap(),
                f[9].parse().unwrap(),
                f[10].parse().unwrap(),
                false,
                [0.0625, 1.75, 1.23456789, 16.][f[2].parse::<usize>().unwrap()],
            ),
            _ => continue,
        };
        let canvas = canvas(num, den);
        let clip = clip(&raw(index, speed, curved), num, den);
        let clips = [clip];
        let layers = visual_layers(&clips, &[], &canvas, exact)
            .unwrap()
            .expect("retained source stage");
        assert_eq!(layers.len(), 1, "{line}");
        assert_eq!(layers[0].source_tick, source, "{line}");
        assert_eq!(layers[0].clip_index, 0);
        assert!(!layers[0].held);
        let filter = effect_clock(&clips[0], &clips, &[], &canvas, exact, false)
            .unwrap()
            .unwrap();
        assert_eq!(filter.start_tick as i128, start, "{line}");
        assert_eq!(filter.duration_tick as i128, duration, "{line}");
        assert_eq!(filter.local_tick() as i128, exact - start, "{line}");
        let clocks = video_kadr_backend::domain::video_effect_timing::sample(&clips[0], exact)
            .unwrap()
            .unwrap();
        assert_eq!(clocks.frame as i128, source, "{line}");
        assert_eq!(clocks.generator as i128, exact - start, "{line}");
        assert_eq!(clocks.parameters, clocks.generator);
        let pose = source_pose(&clips[0], &clips, &[], &canvas, exact, false)
            .unwrap()
            .unwrap();
        assert_eq!((pose.opacity as f32).to_bits(), opacity, "{line}");
        let required = visual_range_clip_indices(&clips, &[], &canvas, exact, exact + 1)
            .unwrap()
            .unwrap();
        assert!(required.contains(&0));
        for outside in [start - 1, start + duration, start + duration + 1] {
            assert!(
                visual_layers(&clips, &[], &canvas, outside)
                    .unwrap()
                    .unwrap()
                    .is_empty(),
                "{line} boundary {outside}"
            );
            assert!(
                visual_range_clip_indices(&clips, &[], &canvas, outside, outside + 1)
                    .unwrap()
                    .unwrap()
                    .is_empty()
            );
            boundaries += 1;
        }
        frames += 1;
    }
    assert_eq!(frames, 2385);
    assert_eq!(boundaries, 7155);
    println!("{frames} original frame selections/opacity/effect clocks and {boundaries} half-open boundary checks passed");
}
#[test]
fn zero_frame_clips_are_skipped_and_stale_fps_refuses() {
    let mut raw = raw(0, 1.75, false);
    raw.duration = 1. / 60.;
    let clips = [clip(&raw, 25, 1)];
    let canvas = canvas(25, 1);
    for time in [-1, 0, 1, 254016 * 20_000] {
        assert!(visual_layers(&clips, &[], &canvas, time)
            .unwrap()
            .unwrap()
            .is_empty());
    }
    assert!(
        visual_range_clip_indices(&clips, &[], &canvas, -1, 254016 * 100_000)
            .unwrap()
            .unwrap()
            .is_empty()
    );
    assert!(effect_clock(&clips[0], &clips, &[], &canvas, 0, false)
        .unwrap()
        .is_none());
    assert!(source_pose(&clips[0], &clips, &[], &canvas, 0, false)
        .unwrap()
        .is_none());
    let mismatched = serde_json::from_value(
        json!({"width":16,"height":16,"fpsMilli":30000,"fpsRate":{"numerator":30,"denominator":1},"background":{"red":0,"green":0,"blue":0,"alpha":0}}),
    )
    .unwrap();
    assert!(visual_layers(&clips, &[], &mismatched, 0).is_err());
}
