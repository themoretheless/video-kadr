use video_kadr_backend::domain::{
    composition::AudioClip, concat_retime::expand_audio, concat_retime_edit::Clock,
};
fn clock(index: usize, speed: f64) -> Clock {
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
        speed_curve: None,
    }
}
fn audio(clock: &Clock) -> AudioClip {
    serde_json::from_value(serde_json::json!({"id":"constant-document","sourceId":"source","placement":clock.placement(25,1).unwrap(),"gain":{"mode":"constant","value":1},"pan":{"mode":"constant","value":0},"fadeInTicks":350000,"fadeOutTicks":270000,"concatGainKeys":[{"at":0.25,"value":0.2},{"at":0.75,"value":1.8,"ease":[0.42,-0.2,0.58,1.2]}]})).unwrap()
}
#[test]
fn constant_document_ownership_is_explicit_and_keeps_whole_gain_track() {
    let raw = clock(0, 1.23456789);
    let p = raw.placement(25, 1).unwrap();
    let retained = p.concat_retime.as_ref().unwrap();
    assert!(retained.points.is_empty());
    assert!(retained.retime_curve().unwrap().is_none());
    assert!(retained.curve().is_err());
    let decoded: video_kadr_backend::domain::composition::ClipPlacement =
        serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(
        decoded.concat_retime.as_ref().unwrap().document_clock,
        Some(raw.clone())
    );
    assert!(decoded.timeline_duration_ticks().is_ok());
    let clip = audio(&raw);
    let pieces = expand_audio(&clip).unwrap();
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0].clip.concat_gain_keys, clip.concat_gain_keys);
    assert_eq!(pieces[0].source.from, 0.);
    assert_eq!(pieces[0].source.to, 1.);
    let mut stale = p.clone();
    stale.concat_retime.as_mut().unwrap().document_clock = None;
    assert!(stale.timeline_duration_ticks().is_err());
    let mut stale = p;
    stale.concat_retime.as_mut().unwrap().points.push((0., 1.));
    assert!(stale.timeline_duration_ticks().is_err());
}
#[test]
#[ignore = "requires updated original constant document clock oracle"]
fn constant_video_and_audio_match_actual_original_clocks() {
    let text = std::fs::read_to_string(std::env::var("CONCAT_RETIME_ORACLE").unwrap()).unwrap();
    let speeds = [0.0625, 1.75, 1.23456789, 16.];
    let mut video = 0;
    let mut audio_count = 0;
    let mut rounded_changes = 0;
    for line in text.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        match f[0] {
            "11" => {
                let index: usize = f[1].parse().unwrap();
                let speed = speeds[f[2].parse::<usize>().unwrap()];
                let raw = clock(index, speed);
                let rate_num = f[3].parse().unwrap();
                let rate_den = f[4].parse().unwrap();
                let placement = raw.placement(rate_num, rate_den).unwrap();
                let retained = placement.concat_retime.as_ref().unwrap();
                let exact = f[6].parse().unwrap();
                let expected = f[7].parse().unwrap();
                assert_eq!(
                    retained
                        .sample(
                            placement.timeline_start_tick,
                            placement.source_in_tick,
                            exact
                        )
                        .unwrap(),
                    Some(expected),
                    "{line}"
                );
                let rounded = Clock {
                    start: placement.timeline_start_tick as f64 / 1e6,
                    duration: placement.timeline_duration_ticks().unwrap() as f64 / 1e6,
                    source_start: placement.source_in_tick as f64 / 1e6,
                    ..raw
                };
                let rounded = rounded.placement(rate_num, rate_den).unwrap();
                rounded_changes += usize::from(
                    rounded
                        .concat_retime
                        .as_ref()
                        .unwrap()
                        .sample(rounded.timeline_start_tick, rounded.source_in_tick, exact)
                        .unwrap()
                        != Some(expected),
                );
                video += 1;
            }
            "12" => {
                let speed = speeds[f[1].parse::<usize>().unwrap()];
                let raw = clock(f[2].parse().unwrap(), speed);
                let pieces = expand_audio(&audio(&raw)).unwrap();
                assert_eq!(pieces.len(), 1);
                let piece = pieces[0].source;
                for (actual, expected) in [
                    piece.start_seconds,
                    piece.duration_seconds,
                    piece.source_start_seconds,
                    piece.speed,
                    piece.fade_in_seconds,
                    piece.fade_out_seconds,
                ]
                .into_iter()
                .zip(&f[4..])
                {
                    assert_eq!(
                        actual.to_bits(),
                        expected.parse::<f64>().unwrap().to_bits(),
                        "{line}: {actual}"
                    );
                }
                audio_count += 1;
            }
            _ => {}
        }
    }
    assert_eq!(video, 1908);
    assert_eq!(audio_count, 12);
    assert!(rounded_changes > 0);
    println!("{video} original constant-speed video samples and {audio_count} single audio pieces exact; {rounded_changes} premature-rounding negative-control mismatches");
}
